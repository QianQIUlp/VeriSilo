import {
  observationReportSchema,
  type ObservedSignal,
  type ObservationReport,
} from "@verisilo/contracts";

declare global {
  interface Window {
    __verisiloContentInstalled?: boolean;
  }
}

let currentReport: ObservationReport | null = null;
let pendingMainWorldSignal: ObservedSignal | null = null;
let scanGeneration = 0;
const COLLECTOR_TIMEOUT_MS = 5_000;

if (!window.__verisiloContentInstalled) {
  window.__verisiloContentInstalled = true;
  window.addEventListener("message", receiveMainWorldObservation);
}
void collectAndSendReport();

async function collectAndSendReport(): Promise<void> {
  const generation = ++scanGeneration;
  currentReport = null;
  pendingMainWorldSignal = null;
  const signals = await Promise.all([
    collect("navigator", "window", "stable", "medium", () => ({
      userAgent: navigator.userAgent,
      platform: navigator.platform,
      language: navigator.language,
      languages: [...navigator.languages],
      hardwareConcurrency: navigator.hardwareConcurrency,
      deviceMemory:
        (navigator as Navigator & { deviceMemory?: number }).deviceMemory ??
        null,
      maxTouchPoints: navigator.maxTouchPoints,
    })),
    collect("ua_ch", "window", "stable", "medium", userAgentClientHints),
    collect(
      "timezone",
      "window",
      "stable",
      "medium",
      () => Intl.DateTimeFormat().resolvedOptions().timeZone,
    ),
    collect("screen", "window", "session", "medium", () => ({
      width: screen.width,
      height: screen.height,
      availWidth: screen.availWidth,
      availHeight: screen.availHeight,
      colorDepth: screen.colorDepth,
      pixelRatio: window.devicePixelRatio,
    })),
    collect("canvas_hash", "window", "session", "high", canvasHash),
    collect("webgl", "window", "session", "high", webglSummary),
    collect("webgpu", "window", "session", "high", webgpuSummary),
    collect("audio", "window", "session", "high", audioHash),
    collect("fonts", "window", "session", "high", fontsSummary),
    collect("media_devices", "window", "session", "high", mediaDevicesSummary),
    collect("permissions", "window", "session", "medium", permissionsSummary),
    collect("storage", "window", "session", "medium", () => ({
      cookiesEnabled: navigator.cookieEnabled,
      localStorage: storageAvailable("localStorage"),
      sessionStorage: storageAvailable("sessionStorage"),
      indexedDb: "indexedDB" in window,
    })),
    collect("webrtc", "window", "session", "high", webRtcSummary),
    collect("window_iframe", "iframe", "session", "medium", iframeSummary),
    collect(
      "dedicated_worker",
      "worker",
      "session",
      "medium",
      dedicatedWorkerSelfTest,
    ),
  ]);

  if (generation !== scanGeneration) {
    return;
  }

  const isolatedReport = observationReportSchema.parse({
    schemaVersion: 1,
    reportId: crypto.randomUUID(),
    origin: location.origin,
    collectedAt: new Date().toISOString(),
    coverage: { mainWorld: "not_attempted", worker: "self_test_only" },
    signals,
  });
  currentReport = mergeReports(isolatedReport, pendingMainWorldSignal);
  await sendReport(currentReport);
}

function receiveMainWorldObservation(event: MessageEvent<unknown>): void {
  if (event.source !== window) {
    return;
  }
  const observation = parseMainWorldObservation(event.data);
  if (observation === null || observation.origin !== location.origin) {
    return;
  }
  // MAIN-world values are visible to and forgeable by the page. They are retained as
  // narrowly scoped, explicitly untrusted audit evidence only.
  pendingMainWorldSignal = {
    id: "main_world_navigator_untrusted",
    source: "window",
    status: "ok",
    stability: "stable",
    sensitivity: "medium",
    collectedAt: new Date().toISOString(),
    durationMs: 0,
    value: {
      ...observation.navigator,
      evidenceTrust: "page_observable_untrusted",
    },
  };
  if (currentReport !== null) {
    currentReport = mergeReports(currentReport, pendingMainWorldSignal);
    void sendReport(currentReport);
  }
}

type MainWorldObservation = {
  origin: string;
  navigator: { userAgent: string; platform: string; languages: string[] };
};

function parseMainWorldObservation(
  value: unknown,
): MainWorldObservation | null {
  if (value === null || typeof value !== "object") {
    return null;
  }
  const candidate = value as { source?: unknown; observation?: unknown };
  if (
    candidate.source !== "verisilo-main-world" ||
    candidate.observation === null ||
    typeof candidate.observation !== "object"
  ) {
    return null;
  }
  const observation = candidate.observation as {
    origin?: unknown;
    navigator?: {
      userAgent?: unknown;
      platform?: unknown;
      languages?: unknown;
    };
  };
  const navigatorValue = observation.navigator;
  if (
    !boundedString(observation.origin, 2_048) ||
    navigatorValue === undefined ||
    !boundedString(navigatorValue.userAgent, 1_024) ||
    !boundedString(navigatorValue.platform, 256) ||
    !boundedStringArray(navigatorValue.languages, 20, 128)
  ) {
    return null;
  }
  return {
    origin: observation.origin,
    navigator: {
      userAgent: navigatorValue.userAgent,
      platform: navigatorValue.platform,
      languages: navigatorValue.languages,
    },
  };
}

function mergeReports(
  isolatedReport: ObservationReport,
  mainWorldSignal: ObservedSignal | null,
): ObservationReport {
  if (mainWorldSignal === null) {
    return isolatedReport;
  }
  return observationReportSchema.parse({
    ...isolatedReport,
    coverage: {
      mainWorld: "observed",
      worker: isolatedReport.coverage.worker,
    },
    signals: [
      ...isolatedReport.signals.filter(
        (signal) => signal.id !== mainWorldSignal.id,
      ),
      mainWorldSignal,
    ],
  });
}

async function sendReport(report: ObservationReport): Promise<void> {
  await chrome.runtime.sendMessage({ type: "verisilo_observation", report });
}

async function collect(
  id: string,
  source: ObservedSignal["source"],
  stability: ObservedSignal["stability"],
  sensitivity: ObservedSignal["sensitivity"],
  callback: () => unknown | Promise<unknown>,
): Promise<ObservedSignal> {
  const startedAt = performance.now();
  try {
    const value = await withTimeout(
      Promise.resolve().then(callback),
      COLLECTOR_TIMEOUT_MS,
    );
    return {
      id,
      source,
      status: "ok",
      stability,
      sensitivity,
      collectedAt: new Date().toISOString(),
      durationMs: Math.round(performance.now() - startedAt),
      value,
    };
  } catch (error) {
    return {
      id,
      source,
      status: "error",
      stability,
      sensitivity,
      collectedAt: new Date().toISOString(),
      durationMs: Math.round(performance.now() - startedAt),
      error:
        error instanceof Error
          ? error.message.slice(0, 512)
          : String(error).slice(0, 512),
    };
  }
}

async function withTimeout<T>(
  operation: Promise<T>,
  timeoutMs: number,
): Promise<T> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation,
      new Promise<never>((_resolve, reject) => {
        timeout = window.setTimeout(
          () => reject(new Error("Signal collector timed out.")),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timeout !== undefined) {
      window.clearTimeout(timeout);
    }
  }
}

function storageAvailable(name: "localStorage" | "sessionStorage"): boolean {
  try {
    const storage = window[name];
    return typeof storage.length === "number";
  } catch {
    return false;
  }
}

function boundedString(value: unknown, maximumLength: number): value is string {
  return typeof value === "string" && value.length <= maximumLength;
}

function boundedStringArray(
  value: unknown,
  maximumItems: number,
  maximumItemLength: number,
): value is string[] {
  return (
    Array.isArray(value) &&
    value.length <= maximumItems &&
    value.every((item) => boundedString(item, maximumItemLength))
  );
}

async function userAgentClientHints(): Promise<Record<string, unknown>> {
  type UserAgentData = {
    brands?: Array<{ brand: string; version: string }>;
    mobile?: boolean;
    platform?: string;
    getHighEntropyValues?: (
      hints: string[],
    ) => Promise<Record<string, unknown>>;
  };
  const data = (navigator as Navigator & { userAgentData?: UserAgentData })
    .userAgentData;
  if (data === undefined) {
    throw new Error("User-Agent Client Hints are unavailable.");
  }

  const summary: Record<string, unknown> = {
    brands: data.brands ?? [],
    mobile: data.mobile ?? null,
    platform: data.platform ?? null,
  };
  if (data.getHighEntropyValues !== undefined) {
    summary.highEntropy = await data.getHighEntropyValues([
      "architecture",
      "bitness",
      "model",
      "platformVersion",
      "uaFullVersion",
      "fullVersionList",
      "wow64",
    ]);
  }
  return summary;
}

function canvasHash(): string {
  const canvas = document.createElement("canvas");
  canvas.width = 220;
  canvas.height = 30;
  const context = canvas.getContext("2d");
  if (context === null) {
    throw new Error("Canvas 2D context is unavailable.");
  }
  context.textBaseline = "alphabetic";
  context.font = "16px Arial";
  context.fillStyle = "#2f4f4f";
  context.fillText("VeriSilo observation", 2, 20);
  return fnv1a(canvas.toDataURL());
}

function webglSummary(): Record<string, string | number | null> {
  const canvas = document.createElement("canvas");
  const context =
    canvas.getContext("webgl") ?? canvas.getContext("experimental-webgl");
  if (context === null || !(context instanceof WebGLRenderingContext)) {
    return { available: 0, renderer: null, vendor: null };
  }
  const debug = context.getExtension("WEBGL_debug_renderer_info");
  return {
    available: 1,
    renderer:
      debug === null
        ? null
        : String(context.getParameter(debug.UNMASKED_RENDERER_WEBGL)),
    vendor:
      debug === null
        ? null
        : String(context.getParameter(debug.UNMASKED_VENDOR_WEBGL)),
  };
}

function webgpuSummary(): Record<string, boolean> {
  return {
    available: "gpu" in navigator,
  };
}

async function audioHash(): Promise<string> {
  const Context = window.OfflineAudioContext;
  if (Context === undefined) {
    throw new Error("Offline audio rendering is unavailable.");
  }
  const context = new Context(1, 4_410, 44_100);
  const oscillator = context.createOscillator();
  const compressor = context.createDynamicsCompressor();
  oscillator.type = "triangle";
  oscillator.frequency.value = 10_000;
  oscillator.connect(compressor);
  compressor.connect(context.destination);
  oscillator.start(0);
  const rendered = await context.startRendering();
  const samples = rendered.getChannelData(0);
  let value = "";
  for (let index = 0; index < samples.length; index += 32) {
    value += `${samples[index]?.toFixed(8) ?? "0"},`;
  }
  return fnv1a(value);
}

function fontsSummary(): Record<string, boolean> {
  if (!("fonts" in document)) {
    throw new Error("Font Loading API is unavailable.");
  }
  const fonts = ["Arial", "Times New Roman", "Segoe UI", "Noto Sans"];
  return Object.fromEntries(
    fonts.map((font) => [font, document.fonts.check(`12px "${font}"`)]),
  );
}

async function mediaDevicesSummary(): Promise<
  Record<string, boolean | number>
> {
  if (navigator.mediaDevices?.enumerateDevices === undefined) {
    throw new Error("Media Devices API is unavailable.");
  }
  const devices = await navigator.mediaDevices.enumerateDevices();
  const count = (kind: MediaDeviceKind) =>
    devices.filter((device) => device.kind === kind).length;
  return {
    audioInputCount: count("audioinput"),
    audioOutputCount: count("audiooutput"),
    videoInputCount: count("videoinput"),
    labelsExposed: devices.some((device) => device.label.length > 0),
  };
}

async function permissionsSummary(): Promise<Record<string, string>> {
  if (!("permissions" in navigator)) {
    return { status: "unsupported" };
  }
  const result: Record<string, string> = {};
  for (const name of ["geolocation", "notifications"] as const) {
    try {
      const status = await navigator.permissions.query({
        name,
      } as PermissionDescriptor);
      result[name] = status.state;
    } catch {
      result[name] = "unsupported";
    }
  }
  return result;
}

function webRtcSummary(): Record<string, boolean> {
  if (!("RTCPeerConnection" in window)) {
    return { available: false, dataChannelCreated: false };
  }
  const connection = new RTCPeerConnection({ iceServers: [] });
  try {
    connection.createDataChannel("verisilo-observation");
    return { available: true, dataChannelCreated: true };
  } finally {
    connection.close();
  }
}

function iframeSummary(): Record<string, string | boolean | null> {
  const iframe = document.createElement("iframe");
  iframe.setAttribute("aria-hidden", "true");
  iframe.style.display = "none";
  iframe.src = "about:blank";
  document.documentElement.append(iframe);
  try {
    const frameWindow = iframe.contentWindow;
    if (frameWindow === null) {
      throw new Error("Same-origin iframe window is unavailable.");
    }
    return {
      available: true,
      userAgent: frameWindow.navigator.userAgent,
      language: frameWindow.navigator.language,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    };
  } finally {
    iframe.remove();
  }
}

async function dedicatedWorkerSelfTest(): Promise<
  Record<string, string | boolean>
> {
  if (!("Worker" in window)) {
    throw new Error("Dedicated Worker is unavailable.");
  }

  const source = [
    "self.onmessage = () => {",
    "  self.postMessage({ userAgent: self.navigator.userAgent, language: self.navigator.language });",
    "};",
  ].join("\n");
  const workerUrl = URL.createObjectURL(
    new Blob([source], { type: "text/javascript" }),
  );
  const worker = new Worker(workerUrl);

  try {
    return await new Promise<Record<string, string | boolean>>(
      (resolve, reject) => {
        const timeout = window.setTimeout(() => {
          reject(new Error("Dedicated Worker self-test timed out."));
        }, 2_000);
        worker.onmessage = (event: MessageEvent<unknown>) => {
          window.clearTimeout(timeout);
          if (event.data === null || typeof event.data !== "object") {
            reject(
              new Error(
                "Dedicated Worker returned an invalid self-test result.",
              ),
            );
            return;
          }
          const result = event.data as {
            userAgent?: unknown;
            language?: unknown;
          };
          if (
            typeof result.userAgent !== "string" ||
            typeof result.language !== "string"
          ) {
            reject(
              new Error(
                "Dedicated Worker returned an invalid self-test result.",
              ),
            );
            return;
          }
          resolve({
            available: true,
            userAgent: result.userAgent,
            language: result.language,
          });
        };
        worker.onerror = () => {
          window.clearTimeout(timeout);
          reject(
            new Error("Dedicated Worker self-test was blocked or failed."),
          );
        };
        worker.postMessage({ type: "verisilo-self-test" });
      },
    );
  } finally {
    worker.terminate();
    URL.revokeObjectURL(workerUrl);
  }
}

function fnv1a(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}
