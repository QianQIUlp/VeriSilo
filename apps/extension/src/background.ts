import {
  contentMessageSchema,
  extensionPageMessageSchema,
  nativeResponseSchema,
  observationReportSchema,
  type RuntimeCapability,
  PROTOCOL_VERSION,
} from "@verisilo/contracts";

import { redactObservationReport } from "./report-export.js";
import {
  buildNetworkCheckResult,
  isNetworkCheckResult,
  NETWORK_CHECK_ENDPOINTS,
  NETWORK_CHECK_ORIGINS,
} from "./network-check.js";

const NATIVE_HOST_NAME = "io.verisilo.host";
const REPORT_KEY_PREFIX = "report:";
const LOCAL_REPORT_KEY_PREFIX = "saved-report:";
const NETWORK_CHECK_KEY = "network-check:last";
const WEBRTC_RESTORE_POINT_KEY = "webrtc-restore-point";
const NETWORK_PREDICTION_RESTORE_POINT_KEY = "network-prediction-restore-point";
type WebRtcPolicy =
  | "default"
  | "default_public_and_private_interfaces"
  | "default_public_interface_only"
  | "disable_non_proxied_udp";

void chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
void chrome.storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });

chrome.action.onClicked.addListener((tab) => {
  if (tab.id !== undefined) {
    void chrome.sidePanel.open({ tabId: tab.id }).catch(() => undefined);
  }
});

chrome.runtime.onInstalled.addListener(() => {
  void chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
  void chrome.storage.session.setAccessLevel({
    accessLevel: "TRUSTED_CONTEXTS",
  });
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === "loading" || changeInfo.url !== undefined) {
    void chrome.storage.session.remove(reportKey(tabId));
  }
});

chrome.tabs.onRemoved.addListener((tabId) => {
  void chrome.storage.session.remove(reportKey(tabId));
});

chrome.runtime.onMessage.addListener(
  (rawMessage: unknown, sender, sendResponse) => {
    void handleMessage(rawMessage, sender)
      .then((response) => sendResponse({ ok: true, ...response }))
      .catch((error: unknown) =>
        sendResponse({ ok: false, error: errorMessage(error) }),
      );
    return true;
  },
);

async function handleMessage(
  rawMessage: unknown,
  sender: chrome.runtime.MessageSender,
): Promise<Record<string, unknown>> {
  const contentMessage = contentMessageSchema.safeParse(rawMessage);
  if (contentMessage.success) {
    const tabId = sender.tab?.id;
    const reportOrigin = contentMessage.data.report.origin;
    const senderOrigin = originFromSender(sender);
    if (
      tabId === undefined ||
      (sender.frameId !== undefined && sender.frameId !== 0) ||
      senderOrigin === null
    ) {
      throw new Error(
        "Observation messages must originate from a top-level regular web tab.",
      );
    }
    if (reportOrigin !== senderOrigin) {
      throw new Error("Observation origin does not match the sending tab.");
    }
    await chrome.storage.session.set({
      [reportKey(tabId)]: contentMessage.data.report,
    });
    await chrome.storage.local.set({
      [localReportKey(contentMessage.data.report.reportId)]: {
        report: redactObservationReport(contentMessage.data.report),
        savedAt: new Date().toISOString(),
      },
    });
    return { report: contentMessage.data.report };
  }

  const pageMessage = extensionPageMessageSchema.safeParse(rawMessage);
  if (!pageMessage.success) {
    throw new Error("VeriSilo rejected an unknown message.");
  }

  switch (pageMessage.data.type) {
    case "scan_current_tab":
      return scanCurrentTab();
    case "request_current_site_access":
      return requestCurrentSiteAccess();
    case "get_current_report":
      return getCurrentReport();
    case "get_network_check":
      return getNetworkCheck();
    case "run_network_check":
      return runNetworkCheck();
    case "clear_network_check":
      return clearNetworkCheck();
    case "get_lightweight_isolation_status":
      return getLightweightIsolationStatus();
    case "open_private_workspace":
      return openPrivateWorkspace();
    case "request_optional_privacy_permission":
      return requestPrivacyPermission();
    case "apply_webrtc_leak_reduction":
      return applyWebRtcLeakReduction();
    case "restore_webrtc_leak_reduction":
      return restoreWebRtcLeakReduction();
    case "apply_network_prediction_reduction":
      return applyNetworkPredictionReduction();
    case "restore_network_prediction_reduction":
      return restoreNetworkPredictionReduction();
    case "open_desktop":
      return connectNativeHost();
  }
}

async function scanCurrentTab(): Promise<Record<string, unknown>> {
  const tab = await activeTab();
  if (tab.id === undefined) {
    throw new Error("No active browser tab is available.");
  }
  if (tab.url === undefined) {
    throw new Error(
      "尚未获得当前页面的一次性访问权限。请关闭侧栏，在目标网页点击 VeriSilo 工具栏图标打开侧栏后再扫描。",
    );
  }
  if (!/^https?:/u.test(tab.url)) {
    throw new Error("VeriSilo 只扫描普通 HTTP(S) 页面。");
  }

  try {
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ["content.js"],
      injectImmediately: true,
    });
  } catch {
    throw new Error(
      "无法访问当前页面。请在普通 HTTP(S) 页面点击 VeriSilo 工具栏图标后重新扫描；浏览器内部页面、商店页面和 PDF 不支持扫描。",
    );
  }

  let mainWorldInjected = true;
  try {
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ["main-world.js"],
      world: "MAIN",
      injectImmediately: true,
    });
  } catch {
    // MAIN-world observation is explicitly best-effort. Its failure must not
    // discard an already-started isolated-world scan.
    mainWorldInjected = false;
  }

  return { started: true, mainWorldInjected };
}

async function requestCurrentSiteAccess(): Promise<Record<string, unknown>> {
  const tab = await activeTab();
  if (tab.id === undefined) {
    throw new Error("No active browser tab is available.");
  }
  if (tab.url === undefined || !/^https?:/u.test(tab.url)) {
    throw new Error("只能为普通 HTTP(S) 页面请求站点访问权限。");
  }

  const originPattern = `${new URL(tab.url).origin}/*`;
  if (
    await chrome.permissions.contains({
      origins: [originPattern],
    })
  ) {
    return { requested: false, alreadyGranted: true };
  }

  type HostAccessRequestApi = typeof chrome.permissions & {
    addHostAccessRequest?: (request: { tabId: number }) => Promise<void>;
  };
  const permissions = chrome.permissions as HostAccessRequestApi;
  if (permissions.addHostAccessRequest === undefined) {
    throw new Error(
      "此版本 Edge 不支持逐站点访问请求。请从目标网页点击 VeriSilo 工具栏图标，以授予本页一次性扫描访问权限。",
    );
  }

  try {
    await permissions.addHostAccessRequest({ tabId: tab.id });
    return { requested: true, alreadyGranted: false };
  } catch (error) {
    if (
      /already has access|已有.*访问|已经.*访问/iu.test(errorMessage(error))
    ) {
      return { requested: false, alreadyGranted: true };
    }
    throw error;
  }
}

async function getCurrentReport(): Promise<Record<string, unknown>> {
  const tab = await activeTab();
  if (tab.id === undefined) {
    return { report: null };
  }
  const stored = await chrome.storage.session.get(reportKey(tab.id));
  const parsed = observationReportSchema.safeParse(stored[reportKey(tab.id)]);
  const currentOrigin =
    tab.url !== undefined && /^https?:/u.test(tab.url)
      ? new URL(tab.url).origin
      : null;
  if (
    !parsed.success ||
    (currentOrigin !== null && parsed.data.origin !== currentOrigin)
  ) {
    await chrome.storage.session.remove(reportKey(tab.id));
    return { report: null };
  }
  return {
    report: parsed.data,
  };
}

async function requestPrivacyPermission(): Promise<Record<string, unknown>> {
  const granted = await chrome.permissions.request({
    permissions: ["privacy"],
  });
  return { granted };
}

async function runNetworkCheck(): Promise<Record<string, unknown>> {
  const hasPermission = await chrome.permissions.contains({
    origins: [...NETWORK_CHECK_ORIGINS],
  });
  if (!hasPermission) {
    throw new Error(
      "尚未授权网络检查服务。VeriSilo 没有发送任何出口或 DNS 检查请求。",
    );
  }

  const [ipProbe, cloudflareProbe, googleProbe] = await Promise.all([
    probeJson("IP 出口", NETWORK_CHECK_ENDPOINTS.ip),
    probeJson("Cloudflare DNS", NETWORK_CHECK_ENDPOINTS.cloudflareDns, {
      Accept: "application/dns-json",
    }),
    probeJson("Google DNS", NETWORK_CHECK_ENDPOINTS.googleDns),
  ]);
  const errors = [
    ipProbe.error,
    cloudflareProbe.error,
    googleProbe.error,
  ].filter((error): error is string => error !== null);
  const result = buildNetworkCheckResult({
    ipPayload: ipProbe.value,
    cloudflareDnsPayload: cloudflareProbe.value,
    googleDnsPayload: googleProbe.value,
    errors,
  });
  await chrome.storage.session.set({ [NETWORK_CHECK_KEY]: result });
  return { result };
}

async function getNetworkCheck(): Promise<Record<string, unknown>> {
  const stored = await chrome.storage.session.get(NETWORK_CHECK_KEY);
  const result = stored[NETWORK_CHECK_KEY];
  if (!isNetworkCheckResult(result)) {
    await chrome.storage.session.remove(NETWORK_CHECK_KEY);
    return { result: null };
  }
  return { result };
}

async function clearNetworkCheck(): Promise<Record<string, unknown>> {
  await chrome.storage.session.remove(NETWORK_CHECK_KEY);
  return { cleared: true };
}

async function probeJson(
  label: string,
  url: string,
  headers?: Record<string, string>,
): Promise<{ value: unknown | null; error: string | null }> {
  try {
    return { value: await fetchBoundedJson(url, headers), error: null };
  } catch (error) {
    return {
      value: null,
      error: `${label}：${errorMessage(error)}`.slice(0, 300),
    };
  }
}

async function fetchBoundedJson(
  url: string,
  headers?: Record<string, string>,
): Promise<unknown> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10_000);
  try {
    const request: RequestInit = {
      method: "GET",
      cache: "no-store",
      credentials: "omit",
      redirect: "error",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    };
    if (headers !== undefined) {
      request.headers = headers;
    }
    const response = await fetch(url, request);
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    const text = await response.text();
    if (text.length > 64 * 1_024) {
      throw new Error("response exceeded 64 KiB");
    }
    return JSON.parse(text) as unknown;
  } finally {
    clearTimeout(timeout);
  }
}

async function openPrivateWorkspace(): Promise<Record<string, unknown>> {
  const incognitoAllowed = await isIncognitoAllowed();
  if (!incognitoAllowed) {
    throw new Error(
      "Edge 尚未允许 VeriSilo 在 InPrivate 中运行。请打开“扩展管理 → VeriSilo Companion → 允许 InPrivate”，然后重试。",
    );
  }

  const tab = await activeTab();
  const url =
    tab.url !== undefined && /^https?:/u.test(tab.url)
      ? tab.url
      : "about:blank";
  const created = await chrome.windows.create({
    incognito: true,
    focused: true,
    url,
  });
  return {
    opened: true,
    windowId: created?.id,
    url,
    storageBoundary: "regular_vs_incognito",
  };
}

async function getLightweightIsolationStatus(): Promise<
  Record<string, unknown>
> {
  const [incognitoAllowed, privacyGranted] = await Promise.all([
    isIncognitoAllowed(),
    chrome.permissions.contains({ permissions: ["privacy"] }),
  ]);
  if (!privacyGranted) {
    return {
      incognitoAllowed,
      privacyGranted,
      webRtc: { effective: false, state: "permission_missing" },
      networkPrediction: { effective: false, state: "permission_missing" },
    };
  }

  const [webRtc, networkPrediction] = await Promise.all([
    chrome.privacy.network.webRTCIPHandlingPolicy.get({}),
    chrome.privacy.network.networkPredictionEnabled.get({}),
  ]);
  return {
    incognitoAllowed,
    privacyGranted,
    webRtc: {
      effective: webRtc.value === "disable_non_proxied_udp",
      state: webRtc.levelOfControl,
    },
    networkPrediction: {
      effective: networkPrediction.value === false,
      state: networkPrediction.levelOfControl,
    },
  };
}

async function applyWebRtcLeakReduction(): Promise<Record<string, unknown>> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: webRtcCapability("not_controllable", "permission_missing", {
        reason: "privacy permission is not granted",
      }),
    };
  }

  const setting = chrome.privacy.network.webRTCIPHandlingPolicy;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: webRtcCapability(beforeControl, "not_controllable", {
        levelOfControl: before.levelOfControl,
        value: before.value,
      }),
    };
  }

  if (before.value === "disable_non_proxied_udp") {
    return {
      capability: webRtcCapability(
        "controllable_by_this_extension",
        "verified",
        {
          alreadyEnabled: true,
          levelOfControl: before.levelOfControl,
          value: before.value,
        },
      ),
    };
  }

  await preserveRestorePoint(WEBRTC_RESTORE_POINT_KEY, {
    value: before.value,
  });
  await setting.set({ value: "disable_non_proxied_udp" });
  const after = await setting.get({});
  const verified =
    controlFromLevel(after.levelOfControl) ===
      "controllable_by_this_extension" &&
    after.value === "disable_non_proxied_udp";
  return {
    capability: verified
      ? webRtcCapability("controllable_by_this_extension", "verified", {
          levelOfControl: after.levelOfControl,
          value: after.value,
        })
      : webRtcCapability(
          "controllable_by_this_extension",
          "verification_failed",
          {
            levelOfControl: after.levelOfControl,
            value: after.value,
          },
        ),
  };
}

async function restoreWebRtcLeakReduction(): Promise<Record<string, unknown>> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: webRtcCapability("not_controllable", "permission_missing", {
        reason: "privacy permission is not granted",
      }),
    };
  }

  const setting = chrome.privacy.network.webRTCIPHandlingPolicy;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: webRtcCapability(beforeControl, "not_controllable", {
        levelOfControl: before.levelOfControl,
        value: before.value,
      }),
    };
  }

  const stored = await restorePoint(WEBRTC_RESTORE_POINT_KEY);
  const restoreValue = storedWebRtcRestoreValue(stored);
  if (restoreValue === null) {
    if (before.levelOfControl === "controlled_by_this_extension") {
      await setting.clear({});
      const after = await setting.get({});
      return {
        capability: webRtcCapability(
          "controllable_by_this_extension",
          after.levelOfControl === "controlled_by_this_extension"
            ? "verification_failed"
            : "verified",
          {
            action: "released_without_saved_baseline",
            levelOfControl: after.levelOfControl,
            value: after.value,
          },
        ),
      };
    }
    return {
      capability: webRtcCapability(
        "controllable_by_this_extension",
        "not_requested",
        {
          action: "nothing_to_restore",
          levelOfControl: before.levelOfControl,
          value: before.value,
        },
      ),
    };
  }

  await setting.clear({});
  const after = await setting.get({});
  const restored = after.levelOfControl !== "controlled_by_this_extension";
  if (restored) {
    await removeRestorePoint(WEBRTC_RESTORE_POINT_KEY);
  }
  return {
    capability: restored
      ? webRtcCapability("controllable_by_this_extension", "verified", {
          action: "restored",
          restoredValue: restoreValue,
          levelOfControl: after.levelOfControl,
          value: after.value,
        })
      : webRtcCapability(
          "controllable_by_this_extension",
          "verification_failed",
          {
            action: "restore",
            restoredValue: restoreValue,
            levelOfControl: after.levelOfControl,
            value: after.value,
          },
        ),
  };
}

async function applyNetworkPredictionReduction(): Promise<
  Record<string, unknown>
> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: networkPredictionCapability(
        "not_controllable",
        "permission_missing",
        { reason: "privacy permission is not granted" },
      ),
    };
  }

  const setting = chrome.privacy.network.networkPredictionEnabled;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: networkPredictionCapability(
        beforeControl,
        "not_controllable",
        { levelOfControl: before.levelOfControl, value: before.value },
      ),
    };
  }
  if (before.value === false) {
    return {
      capability: networkPredictionCapability(
        "controllable_by_this_extension",
        "verified",
        {
          alreadyEnabled: true,
          levelOfControl: before.levelOfControl,
          value: before.value,
        },
      ),
    };
  }

  await preserveRestorePoint(NETWORK_PREDICTION_RESTORE_POINT_KEY, {
    value: before.value,
  });
  await setting.set({ value: false });
  const after = await setting.get({});
  const verified =
    controlFromLevel(after.levelOfControl) ===
      "controllable_by_this_extension" && after.value === false;
  return {
    capability: networkPredictionCapability(
      "controllable_by_this_extension",
      verified ? "verified" : "verification_failed",
      { levelOfControl: after.levelOfControl, value: after.value },
    ),
  };
}

async function restoreNetworkPredictionReduction(): Promise<
  Record<string, unknown>
> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: networkPredictionCapability(
        "not_controllable",
        "permission_missing",
        { reason: "privacy permission is not granted" },
      ),
    };
  }

  const setting = chrome.privacy.network.networkPredictionEnabled;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: networkPredictionCapability(
        beforeControl,
        "not_controllable",
        { levelOfControl: before.levelOfControl, value: before.value },
      ),
    };
  }

  const stored = storedBooleanRestoreValue(
    await restorePoint(NETWORK_PREDICTION_RESTORE_POINT_KEY),
  );
  if (
    stored === null &&
    before.levelOfControl !== "controlled_by_this_extension"
  ) {
    return {
      capability: networkPredictionCapability(
        "controllable_by_this_extension",
        "not_requested",
        {
          action: "nothing_to_restore",
          levelOfControl: before.levelOfControl,
          value: before.value,
        },
      ),
    };
  }

  await setting.clear({});
  const after = await setting.get({});
  const restored = after.levelOfControl !== "controlled_by_this_extension";
  if (restored) {
    await removeRestorePoint(NETWORK_PREDICTION_RESTORE_POINT_KEY);
  }
  return {
    capability: networkPredictionCapability(
      "controllable_by_this_extension",
      restored ? "verified" : "verification_failed",
      {
        action:
          stored === null ? "released_without_saved_baseline" : "restored",
        restoredValue: stored,
        levelOfControl: after.levelOfControl,
        value: after.value,
      },
    ),
  };
}

function webRtcCapability(
  control: RuntimeCapability["control"],
  operation: RuntimeCapability["operation"],
  evidence: Record<string, unknown>,
): RuntimeCapability {
  return capability(
    "webrtc_non_proxied_udp",
    "best_effort",
    control,
    operation,
    evidence,
  );
}

function networkPredictionCapability(
  control: RuntimeCapability["control"],
  operation: RuntimeCapability["operation"],
  evidence: Record<string, unknown>,
): RuntimeCapability {
  return capability(
    "network_prediction",
    "best_effort",
    control,
    operation,
    evidence,
  );
}

function capability(
  id: string,
  tier: RuntimeCapability["tier"],
  control: RuntimeCapability["control"],
  operation: RuntimeCapability["operation"],
  evidence: Record<string, unknown>,
): RuntimeCapability {
  const base = {
    id,
    tier,
    control,
    operation,
    evidence,
  };
  return operation === "verified"
    ? { ...base, verifiedAt: new Date().toISOString() }
    : base;
}

function controlFromLevel(
  levelOfControl: string,
): RuntimeCapability["control"] {
  switch (levelOfControl) {
    case "controlled_by_this_extension":
    case "controllable_by_this_extension":
      return "controllable_by_this_extension";
    case "controlled_by_other_extensions":
      return "controlled_by_other_extensions";
    case "not_controllable":
      return "not_controllable";
    default:
      return "not_applicable";
  }
}

function storedWebRtcRestoreValue(value: unknown): WebRtcPolicy | null {
  if (value === null || typeof value !== "object") {
    return null;
  }
  const candidate = value as { value?: unknown };
  const allowedValues = new Set<WebRtcPolicy>([
    "default",
    "default_public_and_private_interfaces",
    "default_public_interface_only",
    "disable_non_proxied_udp",
  ]);
  return typeof candidate.value === "string" && isWebRtcPolicy(candidate.value)
    ? candidate.value
    : null;

  function isWebRtcPolicy(candidate: string): candidate is WebRtcPolicy {
    return allowedValues.has(candidate as WebRtcPolicy);
  }
}

function storedBooleanRestoreValue(value: unknown): boolean | null {
  if (value === null || typeof value !== "object") {
    return null;
  }
  const candidate = value as { value?: unknown };
  return typeof candidate.value === "boolean" ? candidate.value : null;
}

async function preserveRestorePoint(
  key: string,
  value: Record<string, unknown>,
): Promise<void> {
  if ((await restorePoint(key)) === undefined) {
    await chrome.storage.local.set({ [key]: value });
  }
}

async function restorePoint(key: string): Promise<unknown> {
  const local = await chrome.storage.local.get(key);
  if (local[key] !== undefined) {
    return local[key];
  }
  const legacySession = await chrome.storage.session.get(key);
  return legacySession[key];
}

async function removeRestorePoint(key: string): Promise<void> {
  await Promise.all([
    chrome.storage.local.remove(key),
    chrome.storage.session.remove(key),
  ]);
}

function isIncognitoAllowed(): Promise<boolean> {
  return new Promise((resolve) => {
    chrome.extension.isAllowedIncognitoAccess(resolve);
  });
}

async function connectNativeHost(): Promise<Record<string, unknown>> {
  const requestId = crypto.randomUUID();
  try {
    const raw = await chrome.runtime.sendNativeMessage(NATIVE_HOST_NAME, {
      type: "handshake",
      protocolVersion: PROTOCOL_VERSION,
      requestId,
    });
    const response = nativeResponseSchema.parse(raw);
    if (response.type !== "handshake_ack" || response.requestId !== requestId) {
      throw new Error(
        "The VeriSilo Native Host returned an unexpected response.",
      );
    }
    return { connected: true };
  } catch (error) {
    return { connected: false, reason: errorMessage(error) };
  }
}

async function activeTab(): Promise<chrome.tabs.Tab> {
  const [tab] = await chrome.tabs.query({
    active: true,
    lastFocusedWindow: true,
  });
  if (tab === undefined) {
    throw new Error("No active browser tab is available.");
  }
  return tab;
}

function reportKey(tabId: number): string {
  return `${REPORT_KEY_PREFIX}${tabId}`;
}

function localReportKey(reportId: string): string {
  return `${LOCAL_REPORT_KEY_PREFIX}${reportId}`;
}

function originFromSender(sender: chrome.runtime.MessageSender): string | null {
  for (const candidate of [sender.origin, sender.url]) {
    if (candidate === undefined) {
      continue;
    }
    try {
      const origin = new URL(candidate).origin;
      if (/^https?:/u.test(origin)) {
        return origin;
      }
    } catch {
      // Continue to the next browser-provided sender field.
    }
  }
  return null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
