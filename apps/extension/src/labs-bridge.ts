declare global {
  interface Window {
    __verisiloLabsBridgeInstalled?: boolean;
  }
}

export {};

if (!window.__verisiloLabsBridgeInstalled) {
  window.__verisiloLabsBridgeInstalled = true;
  window.addEventListener("message", (event: MessageEvent<unknown>) => {
    if (event.source !== window || event.origin !== location.origin) {
      return;
    }
    const message = parseLabsStopMessage(event.data);
    if (message !== null) {
      void chrome.runtime.sendMessage(message).catch(() => undefined);
    }
  });
}

function parseLabsStopMessage(value: unknown): {
  type: "verisilo_labs_stop";
  runId: string;
  stopCode:
    | "page_error"
    | "worker_error"
    | "worker_canary_leak"
    | "timeout"
    | "scope_violation";
} | null {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const candidate = value as Record<string, unknown>;
  if (
    candidate.source !== "verisilo-labs-worker-v1" ||
    typeof candidate.runId !== "string" ||
    !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(
      candidate.runId,
    ) ||
    ![
      "page_error",
      "worker_error",
      "worker_canary_leak",
      "timeout",
      "scope_violation",
    ].includes(String(candidate.stopCode))
  ) {
    return null;
  }
  return {
    type: "verisilo_labs_stop",
    runId: candidate.runId,
    stopCode: candidate.stopCode as
      | "page_error"
      | "worker_error"
      | "worker_canary_leak"
      | "timeout"
      | "scope_violation",
  };
}
