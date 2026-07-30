export interface WorkerLabObservation {
  origin: string;
  documentState: DocumentReadyState;
  workerAvailable: boolean;
  constructorRestorable: boolean;
}

export interface WorkerLabApplyResult {
  active: boolean;
  documentState: DocumentReadyState;
  constructorWrapped: boolean;
  wrappedRealmCount: number;
}

export interface WorkerLabVerificationResult {
  active: boolean;
  constructorWrapped: boolean;
  newWorkerHandshake: boolean;
  sameOriginIframeConsistent: boolean;
  injectionOrder: "late_or_unknown";
  wrappedRealmCount: number;
}

export interface CanaryExposureResult {
  locationOrDocument: boolean;
  visibleCookie: boolean;
  localStorage: boolean;
  sessionStorage: boolean;
  serviceWorkerUrl: boolean;
  sameOriginIframe: boolean;
}

export function observeDedicatedWorkerEnvironment(): WorkerLabObservation {
  const descriptor = Object.getOwnPropertyDescriptor(window, "Worker");
  return {
    origin: location.origin,
    documentState: document.readyState,
    workerAvailable: typeof Worker === "function",
    constructorRestorable:
      typeof Worker === "function" &&
      (descriptor === undefined || descriptor.writable === true),
  };
}

export function installDedicatedWorkerExperiment(input: {
  runId: string;
  siteOrigin: string;
  canary: string;
  expiresAtUnixMs: number;
}): WorkerLabApplyResult {
  type RealmWindow = Window & typeof globalThis;
  const stateKey = "__verisiloLabsDedicatedWorkerV1__";
  const root = window as unknown as Record<string, unknown>;
  type LabState = {
    runId: string;
    active: boolean;
    documentState: DocumentReadyState;
    wrappedRealmCount: number;
    handshakeCount: number;
    restore: (notify: boolean, stopCode: string) => boolean;
    wrapAccessibleFrames: () => void;
    isWrapped: (realm: RealmWindow) => boolean;
  };

  const oldState = root[stateKey] as LabState | undefined;
  if (oldState?.active === true) {
    if (oldState.runId === input.runId) {
      return {
        active: true,
        documentState: oldState.documentState,
        constructorWrapped: oldState.isWrapped(window),
        wrappedRealmCount: oldState.wrappedRealmCount,
      };
    }
    oldState.restore(false, "user_requested");
  }

  const restorers: Array<() => boolean> = [];
  const wrappedRealms = new WeakSet<RealmWindow>();
  const objectUrls = new Set<string>();
  const errorListeners: Array<{
    realm: RealmWindow;
    error: EventListener;
    rejection: EventListener;
  }> = [];
  let observer: MutationObserver | null = null;
  let restoring = false;
  let restoreSucceeded = false;
  let timeout = 0;

  const state: LabState = {
    runId: input.runId,
    active: true,
    documentState: document.readyState,
    wrappedRealmCount: 0,
    handshakeCount: 0,
    restore: (notify, stopCode) => {
      if (restoring) {
        return restoreSucceeded;
      }
      restoring = true;
      state.active = false;
      window.clearTimeout(timeout);
      observer?.disconnect();
      for (const listener of errorListeners) {
        listener.realm.removeEventListener("error", listener.error, true);
        listener.realm.removeEventListener(
          "unhandledrejection",
          listener.rejection,
          true,
        );
      }
      restoreSucceeded = restorers.length > 0;
      for (const restore of restorers.reverse()) {
        try {
          restoreSucceeded = restore() && restoreSucceeded;
        } catch {
          // Continue restoring every realm even if a hostile page replaced one.
          restoreSucceeded = false;
        }
      }
      for (const url of objectUrls) {
        try {
          URL.revokeObjectURL(url);
        } catch {
          // URL cleanup is best-effort after the constructor is restored.
        }
      }
      objectUrls.clear();
      if (restoreSucceeded) {
        try {
          delete root[stateKey];
        } catch {
          root[stateKey] = undefined;
        }
      }
      if (notify) {
        window.postMessage(
          {
            source: "verisilo-labs-worker-v1",
            runId: input.runId,
            stopCode,
          },
          input.siteOrigin,
        );
      }
      return restoreSucceeded;
    },
    wrapAccessibleFrames: () => wrapFrames(window),
    isWrapped: (realm) => wrappedRealms.has(realm),
  };

  function failClosed(stopCode: string): void {
    state.restore(true, stopCode);
  }

  function belongsToSiteRealm(realm: RealmWindow): boolean {
    const visited = new Set<RealmWindow>();
    let current = realm;
    while (!visited.has(current)) {
      visited.add(current);
      try {
        if (current.location.origin === input.siteOrigin) {
          return true;
        }
        if (current.location.origin !== "null") {
          return false;
        }

        const hrefWithoutFragment = current.location.href.split("#", 1)[0];
        if (
          hrefWithoutFragment !== "about:blank" &&
          hrefWithoutFragment !== "about:srcdoc"
        ) {
          return false;
        }

        // about:blank and srcdoc expose a null URL origin even when their
        // effective origin is inherited. Access to the document and this
        // owner/parent relationship must both succeed before following that
        // inheritance; sandboxed and cross-origin frames therefore stay out.
        void current.document;
        const owner = current.frameElement;
        const parent = current.parent as RealmWindow;
        if (
          owner === null ||
          parent === current ||
          owner.ownerDocument.defaultView !== parent
        ) {
          return false;
        }
        current = parent;
      } catch {
        return false;
      }
    }
    return false;
  }

  function wrapRealm(realm: RealmWindow): void {
    if (wrappedRealms.has(realm)) {
      return;
    }
    let NativeWorker: typeof Worker;
    try {
      if (!belongsToSiteRealm(realm)) {
        return;
      }
      NativeWorker = realm.Worker;
    } catch {
      return;
    }
    if (typeof NativeWorker !== "function") {
      return;
    }
    const originalDescriptor = Object.getOwnPropertyDescriptor(realm, "Worker");

    function LabsWorker(
      this: Worker,
      scriptURL: string | URL,
      options?: WorkerOptions,
    ): Worker {
      if (!state.active || Date.now() >= input.expiresAtUnixMs) {
        if (state.active) {
          failClosed("timeout");
        }
        return Reflect.construct(
          NativeWorker,
          [scriptURL, options].filter((value) => value !== undefined),
        ) as Worker;
      }

      let original: URL;
      try {
        original = new URL(String(scriptURL), realm.location.href);
      } catch {
        failClosed("scope_violation");
        return Reflect.construct(
          NativeWorker,
          [scriptURL, options].filter((value) => value !== undefined),
        ) as Worker;
      }
      const sameOrigin =
        original.origin === input.siteOrigin &&
        ["http:", "https:", "blob:"].includes(original.protocol);
      if (!sameOrigin || options?.type === "module") {
        failClosed("scope_violation");
        return Reflect.construct(
          NativeWorker,
          [scriptURL, options].filter((value) => value !== undefined),
        ) as Worker;
      }

      const bootstrapSource = [
        `self.postMessage({__verisiloLabsWorkerV1:true,runId:${JSON.stringify(input.runId)},proof:${JSON.stringify(input.canary)}});`,
        `self.importScripts(${JSON.stringify(original.href)});`,
      ].join("\n");
      const bootstrapUrl = realm.URL.createObjectURL(
        new realm.Blob([bootstrapSource], { type: "text/javascript" }),
      );
      objectUrls.add(bootstrapUrl);
      let worker: Worker;
      try {
        worker = Reflect.construct(
          NativeWorker,
          [bootstrapUrl, options].filter((value) => value !== undefined),
        ) as Worker;
      } catch (error) {
        realm.URL.revokeObjectURL(bootstrapUrl);
        objectUrls.delete(bootstrapUrl);
        failClosed("worker_error");
        throw error;
      }

      const receiveHandshake = (event: MessageEvent<unknown>) => {
        const value = event.data;
        if (
          value === null ||
          typeof value !== "object" ||
          (value as Record<string, unknown>).__verisiloLabsWorkerV1 !== true ||
          (value as Record<string, unknown>).runId !== input.runId ||
          (value as Record<string, unknown>).proof !== input.canary
        ) {
          const containsCanary = (candidate: unknown, depth = 0): boolean => {
            if (depth > 4) {
              return false;
            }
            if (typeof candidate === "string") {
              return candidate.slice(0, 64 * 1_024).includes(input.canary);
            }
            if (Array.isArray(candidate)) {
              return candidate
                .slice(0, 100)
                .some((entry) => containsCanary(entry, depth + 1));
            }
            if (candidate !== null && typeof candidate === "object") {
              return Object.entries(candidate)
                .slice(0, 100)
                .some(
                  ([key, entry]) =>
                    key.includes(input.canary) ||
                    containsCanary(entry, depth + 1),
                );
            }
            return false;
          };
          if (containsCanary(value)) {
            event.stopImmediatePropagation();
            failClosed("worker_canary_leak");
          }
          return;
        }
        event.stopImmediatePropagation();
        state.handshakeCount += 1;
        worker.removeEventListener(
          "message",
          receiveHandshake as EventListener,
          true,
        );
        realm.URL.revokeObjectURL(bootstrapUrl);
        objectUrls.delete(bootstrapUrl);
      };
      worker.addEventListener(
        "message",
        receiveHandshake as EventListener,
        true,
      );
      worker.addEventListener("error", () => failClosed("worker_error"), {
        once: true,
        capture: true,
      });
      realm.setTimeout(() => {
        if (objectUrls.delete(bootstrapUrl)) {
          realm.URL.revokeObjectURL(bootstrapUrl);
        }
      }, 30_000);
      return worker;
    }

    try {
      Object.setPrototypeOf(LabsWorker, NativeWorker);
      LabsWorker.prototype = NativeWorker.prototype;
      realm.Worker = LabsWorker as unknown as typeof Worker;
    } catch {
      failClosed("scope_violation");
      return;
    }
    wrappedRealms.add(realm);
    state.wrappedRealmCount += 1;
    restorers.push(() => {
      if (realm.Worker !== (LabsWorker as unknown as typeof Worker)) {
        return realm.Worker === NativeWorker;
      }
      if (originalDescriptor === undefined) {
        delete (realm as unknown as Record<string, unknown>).Worker;
      } else {
        Object.defineProperty(realm, "Worker", originalDescriptor);
      }
      return realm.Worker === NativeWorker;
    });

    const onError = () => failClosed("page_error");
    const onRejection = () => failClosed("page_error");
    realm.addEventListener("error", onError, true);
    realm.addEventListener("unhandledrejection", onRejection, true);
    errorListeners.push({
      realm,
      error: onError,
      rejection: onRejection,
    });
  }

  function wrapFrames(realm: RealmWindow): void {
    wrapRealm(realm);
    let frames: NodeListOf<HTMLIFrameElement>;
    try {
      frames = realm.document.querySelectorAll("iframe");
    } catch {
      return;
    }
    for (const frame of frames) {
      try {
        const child = frame.contentWindow as RealmWindow | null;
        if (child !== null && belongsToSiteRealm(child)) {
          wrapFrames(child);
        }
      } catch {
        // Cross-origin frames are explicitly outside this narrow experiment.
      }
    }
  }

  try {
    Object.defineProperty(root, stateKey, {
      value: state,
      configurable: true,
      enumerable: false,
      writable: true,
    });
    wrapFrames(window);
    observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (!(node instanceof Element)) {
            continue;
          }
          const frames = [
            ...(node instanceof HTMLIFrameElement ? [node] : []),
            ...node.querySelectorAll("iframe"),
          ];
          for (const frame of frames) {
            frame.addEventListener(
              "load",
              () => {
                try {
                  const child = frame.contentWindow as RealmWindow | null;
                  if (child !== null && belongsToSiteRealm(child)) {
                    wrapFrames(child);
                  }
                } catch {
                  // Cross-origin iframe remains outside coverage.
                }
              },
              { once: true },
            );
          }
        }
      }
      wrapFrames(window);
    });
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
    });
    timeout = window.setTimeout(
      () => failClosed("timeout"),
      Math.max(0, input.expiresAtUnixMs - Date.now()),
    );
  } catch {
    state.restore(true, "scope_violation");
  }

  return {
    active: state.active,
    documentState: state.documentState,
    constructorWrapped: state.isWrapped(window),
    wrappedRealmCount: state.wrappedRealmCount,
  };
}

export async function verifyDedicatedWorkerExperiment(input: {
  runId: string;
}): Promise<WorkerLabVerificationResult> {
  type RealmWindow = Window & typeof globalThis;
  const stateKey = "__verisiloLabsDedicatedWorkerV1__";
  const root = window as unknown as Record<string, unknown>;
  type LabState = {
    runId: string;
    active: boolean;
    wrappedRealmCount: number;
    handshakeCount: number;
    restore: (notify: boolean, stopCode: string) => boolean;
    wrapAccessibleFrames: () => void;
    isWrapped: (realm: RealmWindow) => boolean;
  };
  const state = root[stateKey] as LabState | undefined;
  if (state?.active !== true || state.runId !== input.runId) {
    return {
      active: false,
      constructorWrapped: false,
      newWorkerHandshake: false,
      sameOriginIframeConsistent: false,
      injectionOrder: "late_or_unknown",
      wrappedRealmCount: 0,
    };
  }

  const iframe = document.createElement("iframe");
  iframe.hidden = true;
  iframe.src = "about:blank";
  document.documentElement.append(iframe);
  let sameOriginIframeConsistent = false;
  try {
    state.wrapAccessibleFrames();
    const frameWindow = iframe.contentWindow as RealmWindow | null;
    sameOriginIframeConsistent =
      frameWindow !== null && state.isWrapped(frameWindow);
  } finally {
    iframe.remove();
  }

  const handshakeCountBefore = state.handshakeCount;
  const testSource = `self.postMessage({__verisiloLabsSelfTest:${JSON.stringify(input.runId)}});`;
  const testUrl = URL.createObjectURL(
    new Blob([testSource], { type: "text/javascript" }),
  );
  let newWorkerHandshake = false;
  let worker: Worker | null = null;
  try {
    worker = new Worker(testUrl);
    newWorkerHandshake = await new Promise<boolean>((resolve) => {
      const timeout = window.setTimeout(() => resolve(false), 2_000);
      worker!.addEventListener("message", (event: MessageEvent<unknown>) => {
        const value = event.data;
        if (
          value !== null &&
          typeof value === "object" &&
          (value as Record<string, unknown>).__verisiloLabsSelfTest ===
            input.runId
        ) {
          window.clearTimeout(timeout);
          resolve(true);
        }
      });
      worker!.addEventListener(
        "error",
        () => {
          window.clearTimeout(timeout);
          resolve(false);
        },
        { once: true },
      );
    });
    newWorkerHandshake =
      newWorkerHandshake && state.handshakeCount > handshakeCountBefore;
  } catch {
    newWorkerHandshake = false;
  } finally {
    worker?.terminate();
    URL.revokeObjectURL(testUrl);
  }

  if (!newWorkerHandshake || !sameOriginIframeConsistent) {
    state.restore(true, "verification_failed");
  }
  return {
    active: state.active,
    constructorWrapped: state.isWrapped(window),
    newWorkerHandshake,
    sameOriginIframeConsistent,
    injectionOrder: "late_or_unknown",
    wrappedRealmCount: state.wrappedRealmCount,
  };
}

export function restoreDedicatedWorkerExperiment(input: { runId: string }): {
  restored: boolean;
  constructorNative: boolean;
} {
  const stateKey = "__verisiloLabsDedicatedWorkerV1__";
  const root = window as unknown as Record<string, unknown>;
  const state = root[stateKey] as
    | {
        runId: string;
        active: boolean;
        restore: (notify: boolean, stopCode: string) => boolean;
      }
    | undefined;
  if (state === undefined || state.runId !== input.runId) {
    return { restored: true, constructorNative: true };
  }
  const restored = state.restore(false, "user_requested");
  return {
    restored: restored && state.active === false,
    constructorNative: restored,
  };
}

export async function scanCanaryExposure(input: {
  canary: string;
}): Promise<CanaryExposureResult> {
  const contains = (value: string | null | undefined) =>
    typeof value === "string" && value.includes(input.canary);
  let visibleCookie = false;
  let local = false;
  let session = false;
  let locationOrDocument = false;
  let serviceWorkerUrl = false;
  let sameOriginIframe = false;

  try {
    visibleCookie = contains(document.cookie);
  } catch {
    // A blocked cookie read is not treated as affirmative leak evidence.
  }
  try {
    for (
      let index = 0;
      index < Math.min(localStorage.length, 100);
      index += 1
    ) {
      const key = localStorage.key(index);
      if (
        contains(key) ||
        (key !== null && contains(localStorage.getItem(key)))
      ) {
        local = true;
        break;
      }
    }
  } catch {
    // Storage may be denied by page/browser policy.
  }
  try {
    for (
      let index = 0;
      index < Math.min(sessionStorage.length, 100);
      index += 1
    ) {
      const key = sessionStorage.key(index);
      if (
        contains(key) ||
        (key !== null && contains(sessionStorage.getItem(key)))
      ) {
        session = true;
        break;
      }
    }
  } catch {
    // Storage may be denied by page/browser policy.
  }
  try {
    const boundedText = document.documentElement?.textContent?.slice(
      0,
      1_000_000,
    );
    locationOrDocument = contains(location.href) || contains(boundedText);
  } catch {
    // DOM observation is deliberately bounded and best-effort.
  }
  try {
    if (navigator.serviceWorker !== undefined) {
      const registrations = await navigator.serviceWorker.getRegistrations();
      serviceWorkerUrl = registrations
        .slice(0, 100)
        .some((registration) =>
          [
            registration.active?.scriptURL,
            registration.installing?.scriptURL,
            registration.waiting?.scriptURL,
          ].some(contains),
        );
    }
  } catch {
    // MV3 cannot inspect Service Worker memory; only exposed URLs are observed.
  }
  try {
    const frames = document.querySelectorAll("iframe");
    for (const frame of frames) {
      const realm = frame.contentWindow;
      if (realm === null || realm.location.origin !== location.origin) {
        continue;
      }
      const frameText = realm.document.documentElement?.textContent?.slice(
        0,
        1_000_000,
      );
      if (
        contains(realm.location.href) ||
        contains(frameText) ||
        contains(realm.document.cookie)
      ) {
        sameOriginIframe = true;
        break;
      }
      for (
        let index = 0;
        index < Math.min(realm.sessionStorage.length, 100);
        index += 1
      ) {
        const key = realm.sessionStorage.key(index);
        if (
          contains(key) ||
          (key !== null && contains(realm.sessionStorage.getItem(key)))
        ) {
          sameOriginIframe = true;
          break;
        }
      }
      if (sameOriginIframe) {
        break;
      }
    }
  } catch {
    // Cross-origin and sandboxed frames remain outside observable coverage.
  }

  return {
    locationOrDocument,
    visibleCookie,
    localStorage: local,
    sessionStorage: session,
    serviceWorkerUrl,
    sameOriginIframe,
  };
}
