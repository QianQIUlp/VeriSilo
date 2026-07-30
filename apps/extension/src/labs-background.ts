import {
  createDefaultLabsExperiments,
  LABS_LOCAL_AUTHORIZATION_TTL_MS,
  LABS_RECEIPT_TTL_MS,
  LABS_SILO_AUTHORIZATION_TTL_MS,
  labsExperimentReceiptSchema,
  labsExperimentSchema,
  nativeResponseSchema,
  PROTOCOL_VERSION,
  type ContentMessage,
  type LabsExperiment,
  type LabsExperimentReceipt,
  type LabsExperimentScope,
  type LabsStopConditionCode,
} from "@verisilo/contracts";

import {
  installDedicatedWorkerExperiment,
  observeDedicatedWorkerEnvironment,
  restoreDedicatedWorkerExperiment,
  scanCanaryExposure,
  verifyDedicatedWorkerExperiment,
  type CanaryExposureResult,
  type WorkerLabApplyResult,
  type WorkerLabObservation,
  type WorkerLabVerificationResult,
} from "./labs-main-world.js";
import {
  beginWorkerExperiment,
  completeWorkerVerification,
  expireWorkerExperiment,
  persistentLabsAuthorization,
  recordWorkerApplication,
  stopWorkerExperiment,
  type LabsRun,
} from "./labs-state.js";

const NATIVE_HOST_NAME = "io.verisilo.host";
const LABS_STATUS_KEY = "labs:status";
const LABS_RECEIPT_PREFIX = "labs-receipt:";
const LABS_AUTHORIZATION_PREFIX = "labs-authorization:";
const MAX_LABS_RECEIPTS = 50;
const MAX_RUNTIME_SNAPSHOT_AGE_MS = 45_000;

type ActiveWorkerRun = {
  run: LabsRun;
  canary: string;
  timeout: ReturnType<typeof setTimeout> | null;
};

let activeWorkerRun: ActiveWorkerRun | null = null;
let initialization: Promise<void> | null = null;
let stopping: Promise<LabsExperiment | null> | null = null;

export function initializeLabsBackground(): void {
  void ensureInitialized();
  chrome.permissions.onRemoved.addListener((permissions) => {
    const active = activeWorkerRun?.run.experiment;
    const pattern =
      active?.scope === null || active?.scope === undefined
        ? null
        : `${active.scope.siteOrigin}/*`;
    if (pattern !== null && (permissions.origins ?? []).includes(pattern)) {
      void stopActiveWorkerRun("permission_taken_over");
    }
  });
}

export async function getLabsStatus(): Promise<Record<string, unknown>> {
  await ensureInitialized();
  await expireActiveRunIfNeeded();
  const tab = await activeRegularTab();
  const currentOrigin =
    tab?.url !== undefined && /^https?:/u.test(tab.url)
      ? new URL(tab.url).origin
      : null;
  const stored = await chrome.storage.session.get(LABS_STATUS_KEY);
  const parsed = labsExperimentSchema.safeParse(stored[LABS_STATUS_KEY]);
  const worker =
    parsed.success &&
    parsed.data.scope?.siteOrigin === currentOrigin &&
    parsed.data.scope.tabId === tab?.id
      ? parsed.data
      : createDefaultLabsExperiments()[0]!;
  const defaults = createDefaultLabsExperiments();
  const receipts = await readLabsReceipts();
  return {
    experiments: [worker, defaults[1], defaults[2]],
    receiptCount: receipts.length,
    localTemporary: worker.scope?.mode === "local_temporary",
  };
}

export async function enableDedicatedWorkerExperiment(): Promise<
  Record<string, unknown>
> {
  await ensureInitialized();
  if (activeWorkerRun !== null) {
    await stopActiveWorkerRun("user_requested");
  }
  const tab = await activeRegularTab();
  if (
    tab?.id === undefined ||
    tab.url === undefined ||
    !/^https?:/u.test(tab.url)
  ) {
    throw new Error("实验室只能在当前普通 HTTP(S) 页面上运行。");
  }
  const now = new Date();
  const origin = new URL(tab.url).origin;
  const siteHost = new URL(origin).host;
  const siloId = await resolveActiveSiloId(now.getTime());
  const scope = createScope({ tabId: tab.id, origin, siteHost, siloId, now });
  const originPattern = `${origin}/*`;
  let permissionGranted = await chrome.permissions.contains({
    origins: [originPattern],
  });
  if (!permissionGranted) {
    try {
      permissionGranted = await chrome.permissions.request({
        origins: [originPattern],
      });
    } catch {
      permissionGranted = false;
    }
  }

  let run = beginWorkerExperiment({
    scope,
    permissionGranted,
    runId: crypto.randomUUID(),
    now,
  });
  await saveExperiment(run.experiment);
  if (!permissionGranted) {
    return {
      experiments: mergeWithUnsupported(run.experiment),
      permissionGranted: false,
      localTemporary: scope.mode === "local_temporary",
    };
  }

  const canary = randomCanary();
  try {
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ["labs-bridge.js"],
      injectImmediately: true,
    });
    const observed = await executeMain<WorkerLabObservation>(tab.id, {
      func: observeDedicatedWorkerEnvironment,
    });
    if (
      observed === null ||
      observed.origin !== origin ||
      !observed.workerAvailable ||
      !observed.constructorRestorable
    ) {
      run = recordWorkerApplication(run, {
        observed: observed !== null,
        constructorRestorable: observed?.constructorRestorable === true,
        constructorWrapped: false,
        now: new Date(),
      });
      return await stopRunBeforeActivation(run, "verification_failed", tab.id);
    }

    const applied = await executeMain<WorkerLabApplyResult>(tab.id, {
      func: installDedicatedWorkerExperiment,
      args: [
        {
          runId: run.experiment.runId!,
          siteOrigin: origin,
          canary,
          expiresAtUnixMs: Date.parse(run.experiment.expiresAt!),
        },
      ],
    });
    run = recordWorkerApplication(run, {
      observed: true,
      constructorRestorable: true,
      constructorWrapped:
        applied?.active === true && applied.constructorWrapped,
      now: new Date(),
    });
    activeWorkerRun = { run, canary, timeout: null };
    await saveExperiment(run.experiment);
    await saveAuthorization(run.experiment, true);
    if (applied?.active !== true || !applied.constructorWrapped) {
      const stopped = await stopActiveWorkerRun("verification_failed");
      return {
        experiments: mergeWithUnsupported(
          stopped ?? createDefaultLabsExperiments()[0]!,
        ),
        permissionGranted: true,
        localTemporary: scope.mode === "local_temporary",
      };
    }

    const verified = await executeMain<WorkerLabVerificationResult>(tab.id, {
      func: verifyDedicatedWorkerExperiment,
      args: [{ runId: run.experiment.runId! }],
    });
    if (activeWorkerRun?.run.experiment.runId !== run.experiment.runId) {
      return getLabsStatus();
    }
    const permissionStillPresent = await chrome.permissions.contains({
      origins: [originPattern],
    });
    if (!permissionStillPresent) {
      const stopped = await stopActiveWorkerRun("permission_taken_over");
      return {
        experiments: mergeWithUnsupported(stopped ?? run.experiment),
        permissionGranted: false,
        localTemporary: scope.mode === "local_temporary",
      };
    }

    const leak = await findCanaryLeak(tab.id, origin, canary);
    if (leak !== null) {
      const stopped = await stopActiveWorkerRun(leak);
      return {
        experiments: mergeWithUnsupported(stopped ?? run.experiment),
        permissionGranted: true,
        localTemporary: scope.mode === "local_temporary",
      };
    }

    const verification = {
      constructorWrapped:
        verified?.active === true && verified.constructorWrapped,
      newWorkerHandshake: verified?.newWorkerHandshake === true,
      sameOriginIframeConsistent: verified?.sameOriginIframeConsistent === true,
      // This user-triggered executeScript path does not prove document-start
      // ordering, even if document.readyState happened to be "loading".
      injectionOrder: "late_or_unknown" as const,
      visibleCookieProbeClear: true,
      serviceWorkerUrlProbeClear: true,
      crossTabProbeClear: true,
    };
    run = completeWorkerVerification(
      activeWorkerRun.run,
      verification,
      new Date(),
    );
    activeWorkerRun.run = run;
    await saveExperiment(run.experiment);
    await saveReceipt(run.experiment.lastReceipt);
    if (!run.experiment.enabled) {
      await saveAuthorization(run.experiment, false);
      activeWorkerRun = null;
    } else {
      scheduleRuntimeTimeout();
    }
    return {
      experiments: mergeWithUnsupported(run.experiment),
      permissionGranted: true,
      localTemporary: scope.mode === "local_temporary",
    };
  } catch {
    if (activeWorkerRun !== null) {
      const stopped = await stopActiveWorkerRun("verification_failed");
      return {
        experiments: mergeWithUnsupported(stopped ?? run.experiment),
        permissionGranted: true,
        localTemporary: scope.mode === "local_temporary",
      };
    }
    return stopRunBeforeActivation(run, "verification_failed", tab.id);
  }
}

export async function stopDedicatedWorkerExperiment(): Promise<
  Record<string, unknown>
> {
  await ensureInitialized();
  const stopped = await stopActiveWorkerRun("user_requested");
  const worker = stopped ?? createDefaultLabsExperiments()[0]!;
  if (stopped === null) {
    await saveExperiment(worker);
  }
  return { experiments: mergeWithUnsupported(worker) };
}

export async function handleLabsContentStop(
  message: Extract<ContentMessage, { type: "verisilo_labs_stop" }>,
  sender: chrome.runtime.MessageSender,
): Promise<Record<string, unknown>> {
  await ensureInitialized();
  const active = activeWorkerRun?.run.experiment;
  if (
    active === undefined ||
    active.runId !== message.runId ||
    sender.tab?.id !== active.scope?.tabId ||
    (sender.frameId !== undefined && sender.frameId !== 0)
  ) {
    return { stopped: false };
  }
  // Page-originated messages are bounded termination signals, not trusted
  // evidence. Keep fail-closed restoration without letting the page assert a
  // leak classification in an extension-owned receipt.
  const stopCode =
    message.stopCode === "worker_canary_leak"
      ? "verification_failed"
      : message.stopCode;
  const stopped = await stopActiveWorkerRun(stopCode);
  return { stopped: stopped !== null };
}

export async function getLabsReceipts(): Promise<Record<string, unknown>> {
  await ensureInitialized();
  const receipts = await readLabsReceipts();
  return {
    receipts,
    count: receipts.length,
    maximum: MAX_LABS_RECEIPTS,
    retentionDays: LABS_RECEIPT_TTL_MS / (24 * 60 * 60 * 1_000),
  };
}

export async function clearLabsReceipts(): Promise<Record<string, unknown>> {
  const stored = await chrome.storage.local.get(null);
  const keys = Object.keys(stored).filter((key) =>
    key.startsWith(LABS_RECEIPT_PREFIX),
  );
  if (keys.length > 0) {
    await chrome.storage.local.remove(keys);
  }
  return { cleared: keys.length };
}

export function labsTabNavigated(tabId: number): void {
  if (activeWorkerRun?.run.experiment.scope?.tabId === tabId) {
    void stopActiveWorkerRun("site_navigation", true);
  }
}

export function labsTabRemoved(tabId: number): void {
  if (activeWorkerRun?.run.experiment.scope?.tabId === tabId) {
    void stopActiveWorkerRun("site_navigation", true);
  }
}

async function ensureInitialized(): Promise<void> {
  initialization ??= recoverInterruptedRun();
  await initialization;
}

async function recoverInterruptedRun(): Promise<void> {
  await pruneLabsReceipts();
  const stored = await chrome.storage.session.get(LABS_STATUS_KEY);
  const parsed = labsExperimentSchema.safeParse(stored[LABS_STATUS_KEY]);
  if (
    !parsed.success ||
    !parsed.data.enabled ||
    parsed.data.scope === null ||
    parsed.data.runId === null
  ) {
    return;
  }
  const experiment = parsed.data;
  const scope = experiment.scope;
  if (scope === null) {
    return;
  }
  let restored = false;
  try {
    const result = await executeMain<{
      restored: boolean;
      constructorNative: boolean;
    }>(scope.tabId, {
      func: restoreDedicatedWorkerExperiment,
      args: [{ runId: experiment.runId }],
    });
    restored = result?.restored === true && result.constructorNative;
  } catch {
    // If the tab navigated or closed, the old realm no longer owns a wrapper.
    restored = true;
  }
  const recovered = stopWorkerExperiment(
    {
      experiment,
      startedAt: experiment.updatedAt,
      phases: [],
    },
    "extension_context_lost",
    restored,
    new Date(),
  );
  await saveExperiment(recovered.experiment);
  await saveReceipt(recovered.experiment.lastReceipt);
  await saveAuthorization(recovered.experiment, false);
}

async function stopRunBeforeActivation(
  run: LabsRun,
  code: LabsStopConditionCode,
  tabId: number,
): Promise<Record<string, unknown>> {
  let restored = true;
  if (run.experiment.runId !== null) {
    try {
      const result = await executeMain<{
        restored: boolean;
        constructorNative: boolean;
      }>(tabId, {
        func: restoreDedicatedWorkerExperiment,
        args: [{ runId: run.experiment.runId }],
      });
      restored = result?.constructorNative === true;
    } catch {
      restored = true;
    }
  }
  const stopped = stopWorkerExperiment(run, code, restored, new Date());
  await saveExperiment(stopped.experiment);
  await saveReceipt(stopped.experiment.lastReceipt);
  await saveAuthorization(stopped.experiment, false);
  return {
    experiments: mergeWithUnsupported(stopped.experiment),
    permissionGranted: true,
    localTemporary: stopped.experiment.scope?.mode === "local_temporary",
  };
}

async function stopActiveWorkerRun(
  stopCode: LabsStopConditionCode,
  realmGone = false,
): Promise<LabsExperiment | null> {
  if (stopping !== null) {
    return stopping;
  }
  const active = activeWorkerRun;
  if (active === null) {
    return null;
  }
  activeWorkerRun = null;
  if (active.timeout !== null) {
    clearTimeout(active.timeout);
  }
  stopping = (async () => {
    const experiment = active.run.experiment;
    let restored = realmGone;
    if (!realmGone && experiment.scope !== null && experiment.runId !== null) {
      try {
        const result = await executeMain<{
          restored: boolean;
          constructorNative: boolean;
        }>(experiment.scope.tabId, {
          func: restoreDedicatedWorkerExperiment,
          args: [{ runId: experiment.runId }],
        });
        restored = result?.restored === true && result.constructorNative;
      } catch {
        restored = false;
      }
    }
    const stopped = stopWorkerExperiment(
      active.run,
      stopCode,
      restored,
      new Date(),
    );
    await saveExperiment(stopped.experiment);
    await saveReceipt(stopped.experiment.lastReceipt);
    await saveAuthorization(stopped.experiment, false);
    return stopped.experiment;
  })();
  try {
    return await stopping;
  } finally {
    stopping = null;
  }
}

async function expireActiveRunIfNeeded(): Promise<void> {
  const active = activeWorkerRun;
  if (active === null) {
    return;
  }
  const expired = expireWorkerExperiment(active.run, new Date(), true);
  if (expired !== active.run) {
    await stopActiveWorkerRun("expired");
  }
}

function scheduleRuntimeTimeout(): void {
  const active = activeWorkerRun;
  const expiresAt = active?.run.experiment.expiresAt;
  if (active === null || expiresAt === null || expiresAt === undefined) {
    return;
  }
  active.timeout = setTimeout(
    () => void stopActiveWorkerRun("timeout"),
    Math.max(0, Date.parse(expiresAt) - Date.now()),
  );
}

async function findCanaryLeak(
  activeTabId: number,
  origin: string,
  canary: string,
): Promise<LabsStopConditionCode | null> {
  const tabs = await chrome.tabs.query({ url: `${origin}/*` });
  for (const tab of tabs) {
    if (tab.id === undefined) {
      continue;
    }
    let result: CanaryExposureResult | null = null;
    try {
      result = await executeIsolated<CanaryExposureResult>(tab.id, {
        func: scanCanaryExposure,
        args: [{ canary }],
      });
    } catch {
      return "verification_failed";
    }
    if (result === null) {
      return "verification_failed";
    }
    if (result.visibleCookie) {
      return "cookie_canary_leak";
    }
    if (result.serviceWorkerUrl) {
      return "service_worker_canary_leak";
    }
    if (result.sameOriginIframe) {
      return "iframe_canary_leak";
    }
    const exposed =
      result.locationOrDocument || result.localStorage || result.sessionStorage;
    if (exposed) {
      return tab.id === activeTabId
        ? "window_canary_leak"
        : "cross_tab_canary_leak";
    }
  }
  return null;
}

async function executeMain<T>(
  tabId: number,
  injection: {
    func: (...args: never[]) => unknown;
    args?: unknown[];
  },
): Promise<T | null> {
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    world: "MAIN",
    injectImmediately: true,
    func: injection.func,
    args: (injection.args ?? []) as never[],
  });
  return (results[0]?.result as T | undefined) ?? null;
}

async function executeIsolated<T>(
  tabId: number,
  injection: {
    func: (...args: never[]) => unknown;
    args?: unknown[];
  },
): Promise<T | null> {
  const results = await chrome.scripting.executeScript({
    target: { tabId },
    injectImmediately: true,
    func: injection.func,
    args: (injection.args ?? []) as never[],
  });
  return (results[0]?.result as T | undefined) ?? null;
}

function createScope(input: {
  tabId: number;
  origin: string;
  siteHost: string;
  siloId: string | null;
  now: Date;
}): LabsExperimentScope {
  const mode = input.siloId === null ? "local_temporary" : "desktop_silo";
  const expiresAt = new Date(
    input.now.getTime() +
      (mode === "desktop_silo"
        ? LABS_SILO_AUTHORIZATION_TTL_MS
        : LABS_LOCAL_AUTHORIZATION_TTL_MS),
  ).toISOString();
  return mode === "desktop_silo"
    ? {
        mode,
        siloId: input.siloId!,
        tabId: input.tabId,
        siteOrigin: input.origin,
        siteHost: input.siteHost,
        authorizedAt: input.now.toISOString(),
        expiresAt,
      }
    : {
        mode,
        siloId: null,
        tabId: input.tabId,
        siteOrigin: input.origin,
        siteHost: input.siteHost,
        authorizedAt: input.now.toISOString(),
        expiresAt,
      };
}

async function resolveActiveSiloId(nowUnixMs: number): Promise<string | null> {
  const requestId = crypto.randomUUID();
  let raw: unknown;
  try {
    raw = await chrome.runtime.sendNativeMessage(NATIVE_HOST_NAME, {
      type: "get_runtime_status",
      protocolVersion: PROTOCOL_VERSION,
      requestId,
    });
  } catch {
    return null;
  }
  const parsed = nativeResponseSchema.safeParse(raw);
  if (
    !parsed.success ||
    parsed.data.type !== "runtime_status" ||
    parsed.data.requestId !== requestId ||
    parsed.data.vault.state !== "unlocked" ||
    parsed.data.activation.state !== "running" ||
    parsed.data.activation.activeSiloId === null
  ) {
    return null;
  }
  const age = nowUnixMs - Date.parse(parsed.data.snapshotWrittenAt);
  if (
    !Number.isFinite(age) ||
    age < -5_000 ||
    age > MAX_RUNTIME_SNAPSHOT_AGE_MS
  ) {
    return null;
  }
  return parsed.data.activation.activeSiloId;
}

async function activeRegularTab(): Promise<chrome.tabs.Tab | null> {
  const [tab] = await chrome.tabs.query({
    active: true,
    lastFocusedWindow: true,
  });
  return tab ?? null;
}

function mergeWithUnsupported(worker: LabsExperiment): LabsExperiment[] {
  const defaults = createDefaultLabsExperiments();
  return [worker, defaults[1]!, defaults[2]!];
}

function randomCanary(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(24));
  return `vsl_${Array.from(bytes, (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("")}`;
}

async function saveExperiment(experiment: LabsExperiment): Promise<void> {
  await chrome.storage.session.set({
    [LABS_STATUS_KEY]: labsExperimentSchema.parse(experiment),
  });
}

async function saveAuthorization(
  experiment: LabsExperiment,
  enabled: boolean,
): Promise<void> {
  if (experiment.scope === null) {
    return;
  }
  const authorization = persistentLabsAuthorization(
    experiment.scope,
    enabled,
    new Date(),
  );
  if (authorization === null) {
    return;
  }
  const digest = await sha256(
    `${authorization.scope.siloId}:${authorization.scope.siteOrigin}`,
  );
  await chrome.storage.local.set({
    [`${LABS_AUTHORIZATION_PREFIX}${digest}`]: authorization,
  });
}

async function saveReceipt(
  receipt: LabsExperimentReceipt | null,
): Promise<void> {
  if (receipt === null) {
    return;
  }
  await pruneLabsReceipts(1);
  await chrome.storage.local.set({
    [`${LABS_RECEIPT_PREFIX}${receipt.receiptId}`]: receipt,
  });
}

async function readLabsReceipts(): Promise<LabsExperimentReceipt[]> {
  await pruneLabsReceipts();
  const stored = await chrome.storage.local.get(null);
  return Object.entries(stored)
    .filter(([key]) => key.startsWith(LABS_RECEIPT_PREFIX))
    .flatMap(([, value]) => {
      const parsed = labsExperimentReceiptSchema.safeParse(value);
      return parsed.success ? [parsed.data] : [];
    })
    .sort((left, right) => right.finalizedAt.localeCompare(left.finalizedAt));
}

async function pruneLabsReceipts(reserveSlots = 0): Promise<void> {
  const stored = await chrome.storage.local.get(null);
  const now = Date.now();
  const entries = Object.entries(stored)
    .filter(([key]) => key.startsWith(LABS_RECEIPT_PREFIX))
    .map(([key, value]) => ({
      key,
      parsed: labsExperimentReceiptSchema.safeParse(value),
    }));
  const valid = entries
    .filter(
      (entry) =>
        entry.parsed.success && Date.parse(entry.parsed.data.expiresAt) > now,
    )
    .sort((left, right) =>
      right.parsed.success && left.parsed.success
        ? right.parsed.data.finalizedAt.localeCompare(
            left.parsed.data.finalizedAt,
          )
        : 0,
    );
  const keep = new Set(
    valid
      .slice(0, Math.max(0, MAX_LABS_RECEIPTS - reserveSlots))
      .map((entry) => entry.key),
  );
  const remove = entries
    .filter((entry) => !keep.has(entry.key))
    .map((entry) => entry.key);
  if (remove.length > 0) {
    await chrome.storage.local.remove(remove);
  }
}

async function sha256(value: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(value),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}
