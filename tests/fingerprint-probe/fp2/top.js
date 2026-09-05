(function installFp2Top() {
  "use strict";

  const query = new URLSearchParams(window.location.search);
  const nonce = query.get("nonce") || "";
  const primaryOrigin = window.location.origin;
  const secondaryOrigin = `http://localhost:${window.location.port}`;
  const headerEndpoint = `${window.location.origin}/fp2/header-observation`;
  let inputResolver;
  let inputReceived = false;
  let activeTrace;
  const inputPromise = new Promise((resolve) => {
    inputResolver = resolve;
  });

  window.__fp2Result = undefined;
  window.__fp2Error = undefined;
  window.__fp2State = { status: "waiting-for-input" };
  window.__fp2ProvideInput = (value) => {
    if (inputReceived) return;
    inputReceived = true;
    inputResolver(value);
  };
  window.__fp2GetResult = () => window.__fp2Result;

  function assertProtocol(condition, category) {
    if (!condition) throw new Error(category);
  }

  function waitForMessage(predicate, timeoutLabel) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`${timeoutLabel}_timeout`)),
        15000,
      );
      const handler = (event) => {
        try {
          const value = predicate(event);
          if (value === undefined) return;
          clearTimeout(timer);
          window.removeEventListener("message", handler);
          resolve(value);
        } catch (error) {
          clearTimeout(timer);
          window.removeEventListener("message", handler);
          reject(error);
        }
      };
      window.addEventListener("message", handler);
    });
  }

  async function collectFrame(role, frameOrigin, input) {
    const frame = document.createElement("iframe");
    frame.title = `FP2 ${role}`;
    frame.src = `${frameOrigin}/fp2/frame.html?role=${encodeURIComponent(role)}&nonce=${encodeURIComponent(nonce)}`;
    const resultPromise = waitForMessage((event) => {
      if (event.origin !== frameOrigin || event.source !== frame.contentWindow)
        return undefined;
      const data = event.data;
      if (
        !data ||
        data.kind !== "fp2-frame-result" ||
        data.nonce !== nonce ||
        data.realm !== role
      ) {
        throw new Error(`${role}_message_protocol_mismatch`);
      }
      if (data.failure) throw FP2Realm.failureError(data.failure);
      if (data.error) throw FP2Realm.failureError(data.error);
      if (!Object.prototype.hasOwnProperty.call(data, "result")) {
        throw new Error(`${role}_result_missing`);
      }
      return data.result;
    }, `${role}_message`);
    document.body.append(frame);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`${role}_load_timeout`)),
        15000,
      );
      frame.addEventListener(
        "load",
        () => {
          clearTimeout(timer);
          try {
            frame.contentWindow.postMessage(
              {
                kind: "fp2-frame-init",
                nonce,
                parentOrigin: primaryOrigin,
                input,
              },
              frameOrigin,
            );
            resolve();
          } catch (error) {
            reject(error);
          }
        },
        { once: true },
      );
    });
    const result = await resultPromise;
    frame.remove();
    return result;
  }

  function collectDedicatedWorker() {
    return new Promise((resolve, reject) => {
      const worker = new Worker("/fp2/dedicated-worker.js");
      const timer = setTimeout(() => {
        worker.terminate();
        reject(new Error("dedicated_worker_timeout"));
      }, 15000);
      worker.onmessage = (event) => {
        const data = event.data;
        if (
          !data ||
          data.kind !== "fp2-worker-result" ||
          data.nonce !== nonce
        ) {
          clearTimeout(timer);
          worker.terminate();
          reject(new Error("dedicated_worker_message_protocol_mismatch"));
          return;
        }
        if (data.failure || data.error) {
          clearTimeout(timer);
          worker.terminate();
          reject(FP2Realm.failureError(data.failure || data.error));
          return;
        }
        clearTimeout(timer);
        worker.terminate();
        resolve(data.result);
      };
      worker.onerror = () => {
        clearTimeout(timer);
        worker.terminate();
        reject(new Error("dedicated_worker_error"));
      };
      worker.postMessage({
        kind: "fp2-worker-init",
        nonce,
        endpoint: headerEndpoint,
      });
    });
  }

  function collectSharedWorker() {
    return new Promise((resolve, reject) => {
      const worker = new SharedWorker("/fp2/shared-worker.js", {
        name: "verisilo-fp2-shared-v1",
      });
      const port = worker.port;
      const timer = setTimeout(() => {
        port.close();
        reject(new Error("shared_worker_timeout"));
      }, 15000);
      port.onmessage = (event) => {
        const data = event.data;
        if (
          !data ||
          data.kind !== "fp2-worker-result" ||
          data.nonce !== nonce
        ) {
          clearTimeout(timer);
          port.close();
          reject(new Error("shared_worker_message_protocol_mismatch"));
          return;
        }
        if (data.failure || data.error) {
          clearTimeout(timer);
          port.close();
          reject(FP2Realm.failureError(data.failure || data.error));
          return;
        }
        clearTimeout(timer);
        port.close();
        resolve(data.result);
      };
      port.start();
      port.postMessage({
        kind: "fp2-worker-init",
        nonce,
        endpoint: headerEndpoint,
      });
    });
  }

  async function ensureControlledPage() {
    if (navigator.serviceWorker.controller) {
      return { topController: true, controlledPage: false };
    }
    const frame = document.createElement("iframe");
    frame.title = "FP2 service worker controlled page";
    frame.src = `/fp2/controlled.html?nonce=${encodeURIComponent(nonce)}`;
    const controlledPromise = waitForMessage((event) => {
      if (
        event.origin !== primaryOrigin ||
        event.source !== frame.contentWindow
      )
        return undefined;
      const data = event.data;
      if (
        !data ||
        data.kind !== "fp2-controlled-page" ||
        data.nonce !== nonce
      ) {
        throw new Error("service_worker_control_message_mismatch");
      }
      return data;
    }, "service_worker_control");
    document.body.append(frame);
    const result = await controlledPromise;
    frame.remove();
    assertProtocol(
      result.controlled === true,
      "service_worker_controller_missing",
    );
    return { topController: false, controlledPage: true };
  }

  async function serviceWorkerEvidence(activationDeadlineMs) {
    assertProtocol(
      Number.isFinite(activationDeadlineMs) && activationDeadlineMs > 0,
      "service_worker_activation_deadline_invalid",
    );
    const activationDeadline = FP2Realm.deadlineFromNow(activationDeadlineMs);
    const registrationBefore =
      await navigator.serviceWorker.getRegistration("/fp2/");
    const existedBefore = !!registrationBefore;
    const registration =
      registrationBefore ||
      (await navigator.serviceWorker.register("/fp2/service-worker.js", {
        scope: "/fp2/",
        updateViaCache: "none",
      }));
    const ready = await FP2Realm.withTimeout(
      navigator.serviceWorker.ready,
      FP2Realm.remainingDeadlineMs(activationDeadline),
      "service_worker_ready",
    );
    const active = ready.active || registration.active;
    assertProtocol(!!active, "service_worker_active_missing");
    const activeState = await FP2Realm.waitForServiceWorkerActivation(
      active,
      activationDeadline,
    );
    const scriptResponse = await fetch("/fp2/service-worker.js", {
      method: "GET",
      cache: "no-store",
      credentials: "omit",
    });
    const scriptBytes = new Uint8Array(await scriptResponse.arrayBuffer());
    const scriptSha256 = `sha256:${await FP2Realm.sha256Bytes(scriptBytes)}`;
    const controllerState = await ensureControlledPage();
    const channel = new MessageChannel();
    const workerResult = await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("service_worker_message_timeout")),
        15000,
      );
      channel.port1.onmessage = (event) => {
        clearTimeout(timer);
        const data = event.data;
        if (
          !data ||
          data.kind !== "fp2-service-worker-result" ||
          data.nonce !== nonce
        ) {
          reject(new Error("service_worker_message_protocol_mismatch"));
          return;
        }
        if (data.failure || data.error) {
          reject(FP2Realm.failureError(data.failure || data.error));
          return;
        }
        resolve(data.result);
      };
      channel.port1.start();
      active.postMessage(
        {
          kind: "fp2-service-worker-init",
          nonce,
          endpoint: headerEndpoint,
        },
        [channel.port2],
      );
    });
    return {
      existedBefore,
      scriptURLPath: new URL(active.scriptURL).pathname,
      scriptSha256,
      scopePath: new URL(registration.scope).pathname,
      activeState,
      topController: controllerState.topController,
      controlledPage: controllerState.controlledPage,
      workerResult,
    };
  }

  async function collect() {
    const trace = FP2Realm.createStageTracker("top-window");
    activeTrace = trace;
    const input = await FP2Realm.observeStage(
      trace,
      "input",
      "inputReceived",
      () => inputPromise,
    );
    FP2Realm.observeStage(trace, "input", "validateInput", () => {
      assertProtocol(
        input && Array.isArray(input.fonts),
        "probe_input_invalid",
      );
      assertProtocol(nonce.length >= 16, "session_nonce_missing");
    });
    window.__fp2State = { status: "running" };
    const topWindow = await FP2Realm.observeStage(
      trace,
      "top-window",
      "collectWindowRealm",
      () =>
        FP2Realm.collectWindowRealm({
          realm: "top-window",
          endpoint: headerEndpoint,
          nonce,
          fonts: input.fonts,
        }),
    );
    const sameOrigin = await FP2Realm.observeStage(
      trace,
      "same-origin-iframe",
      "collectFrame",
      () => collectFrame("same-origin-iframe", primaryOrigin, input),
    );
    const crossOrigin = await FP2Realm.observeStage(
      trace,
      "cross-origin-iframe",
      "collectFrame",
      () => collectFrame("cross-origin-iframe", secondaryOrigin, input),
    );
    const dedicated = await FP2Realm.observeStage(
      trace,
      "dedicated-worker",
      "collectDedicatedWorker",
      () => collectDedicatedWorker(),
    );
    const shared = await FP2Realm.observeStage(
      trace,
      "shared-worker",
      "collectSharedWorker",
      () => collectSharedWorker(),
    );
    const serviceWorker = await FP2Realm.observeStage(
      trace,
      "service-worker",
      "serviceWorkerEvidence",
      () => serviceWorkerEvidence(input.serviceWorkerActivationDeadlineMs),
    );
    return FP2Realm.observeStage(
      trace,
      "finalize",
      "assembleResult",
      async () => {
        const nonceSha256 = `sha256:${await FP2Realm.sha256Text(nonce)}`;
        const fontInputSha256 = input.fontInputSha256;
        const realmOrder = [
          "top-window",
          "same-origin-iframe",
          "cross-origin-iframe",
          "dedicated-worker",
          "shared-worker",
          "service-worker",
        ];
        const realms = {
          "top-window": topWindow,
          "same-origin-iframe": sameOrigin,
          "cross-origin-iframe": crossOrigin,
          "dedicated-worker": dedicated,
          "shared-worker": shared,
          "service-worker": serviceWorker.workerResult,
        };
        const bundleManifestSha256 = input.bundleManifestSha256;
        const bundleFiles = input.bundleFiles;
        const storage = await FP2Realm.observeStage(
          trace,
          "storage",
          "collectStorageEvidence",
          () => collectStorageEvidence(),
        );
        const scriptPaths = {
          top: [
            "tests/fingerprint-probe/fp2/top.html",
            "tests/fingerprint-probe/fp2/top.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
          sameOriginIframe: [
            "tests/fingerprint-probe/fp2/frame.html",
            "tests/fingerprint-probe/fp2/frame.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
          crossOriginIframe: [
            "tests/fingerprint-probe/fp2/frame.html",
            "tests/fingerprint-probe/fp2/frame.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
          dedicatedWorker: [
            "tests/fingerprint-probe/fp2/dedicated-worker.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
          sharedWorker: [
            "tests/fingerprint-probe/fp2/shared-worker.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
          serviceWorker: [
            "tests/fingerprint-probe/fp2/service-worker.js",
            "tests/fingerprint-probe/fp2/realm-common.js",
          ],
        };
        return {
          verified: false,
          nonceSha256,
          fontInputSha256,
          realmOrder,
          realms,
          serviceWorker,
          bundleManifestSha256,
          bundleFiles,
          storage,
          scriptPaths,
        };
      },
    );
  }

  async function collectStorageEvidence() {
    const cookieName = "verisilo_fp2_cookie";
    const localName = "verisilo_fp2_local";
    const marker = "verisilo-fp2-continuity-marker";
    const cookieBefore = readCookie(cookieName);
    const localBefore = window.localStorage.getItem(localName);
    const bootBefore =
      Number.parseInt(
        window.localStorage.getItem("verisilo_fp2_boot") || "0",
        10,
      ) || 0;
    if (cookieBefore === null)
      document.cookie = `${cookieName}=${marker}; Path=/fp2; SameSite=Lax`;
    if (localBefore === null) window.localStorage.setItem(localName, marker);
    window.localStorage.setItem("verisilo_fp2_boot", String(bootBefore + 1));
    const cookieAfter = readCookie(cookieName);
    const localAfter = window.localStorage.getItem(localName);
    return {
      boot: { before: bootBefore, after: bootBefore + 1 },
      cookie: {
        presentBefore: cookieBefore !== null,
        presentAfter: cookieAfter !== null,
        valueSha256:
          cookieAfter === null
            ? null
            : `sha256:${await FP2Realm.sha256Text(cookieAfter)}`,
      },
      localStorage: {
        presentBefore: localBefore !== null,
        presentAfter: localAfter !== null,
        valueSha256:
          localAfter === null
            ? null
            : `sha256:${await FP2Realm.sha256Text(localAfter)}`,
      },
    };
  }

  function readCookie(name) {
    const prefix = `${name}=`;
    const match = document.cookie
      .split(";")
      .map((part) => part.trim())
      .find((part) => part.startsWith(prefix));
    return match ? match.slice(prefix.length) : null;
  }

  collect()
    .then((result) => {
      window.__fp2State = { status: "complete" };
      window.__fp2Result = result;
      document.getElementById("status").textContent = "FP2 probe complete";
    })
    .catch((error) => {
      window.__fp2State = { status: "failed" };
      window.__fp2Error = FP2Realm.failureFromError(error, {
        realm: "top-window",
        stage: "orchestration",
        operation: "collect",
        lastSuccessfulStage: activeTrace
          ? activeTrace.lastSuccessfulStage
          : null,
      });
      document.getElementById("status").textContent = "FP2 probe failed";
    });
})();
