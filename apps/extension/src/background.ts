import {
  contentMessageSchema,
  extensionPageMessageSchema,
  NETWORK_EVIDENCE_COVERAGE,
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
import {
  eligibleSiloForNetworkEvidence,
  isNetworkEvidenceHandoffStatus,
  type NetworkEvidenceHandoffReason,
  type NetworkEvidenceHandoffStatus,
  type RuntimeEvidenceBinding,
} from "./network-evidence-handoff.js";
import {
  MAX_SAVED_REPORTS,
  planSavedReportPrune,
  SAVED_REPORT_KEY_PREFIX,
  SAVED_REPORT_TTL_MS,
  shouldPersistSavedReport,
} from "./saved-report-history.js";
import {
  clearLabsReceipts,
  enableDedicatedWorkerExperiment,
  getLabsReceipts,
  getLabsStatus,
  handleLabsContentStop,
  initializeLabsBackground,
  labsTabNavigated,
  labsTabRemoved,
  stopDedicatedWorkerExperiment,
} from "./labs-background.js";
import { sendNativeMessageWithTimeout } from "./native-messaging.js";

const NATIVE_HOST_NAME = "io.verisilo.host";
const REPORT_KEY_PREFIX = "report:";
const NETWORK_CHECK_KEY = "network-check:last";
const NETWORK_CHECK_HANDOFF_KEY = "network-check:handoff";
const WEBRTC_RESTORE_POINT_KEY = "webrtc-restore-point";
const NETWORK_PREDICTION_RESTORE_POINT_KEY = "network-prediction-restore-point";
type WebRtcPolicy =
  | "default"
  | "default_public_and_private_interfaces"
  | "default_public_interface_only"
  | "disable_non_proxied_udp";

void chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
void chrome.storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
void pruneSavedReports(Date.now()).catch(() => undefined);
initializeLabsBackground();

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
  void pruneSavedReports(Date.now()).catch(() => undefined);
});

chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.status === "loading" || changeInfo.url !== undefined) {
    void chrome.storage.session.remove(reportKey(tabId));
    labsTabNavigated(tabId);
  }
});

chrome.tabs.onRemoved.addListener((tabId) => {
  void chrome.storage.session.remove(reportKey(tabId));
  labsTabRemoved(tabId);
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
    if (contentMessage.data.type === "verisilo_labs_stop") {
      return handleLabsContentStop(contentMessage.data, sender);
    }
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
    if (shouldPersistSavedReport(sender.tab?.incognito)) {
      await saveRedactedReport(contentMessage.data.report);
    }
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
    case "get_saved_report_history":
      return getSavedReportHistory();
    case "clear_saved_report_history":
      return clearSavedReportHistory();
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
    case "get_labs_status":
      return getLabsStatus();
    case "enable_dedicated_worker_experiment":
      return enableDedicatedWorkerExperiment();
    case "stop_dedicated_worker_experiment":
      return stopDedicatedWorkerExperiment();
    case "get_labs_receipts":
      return getLabsReceipts();
    case "clear_labs_receipts":
      return clearLabsReceipts();
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

async function saveRedactedReport(report: unknown): Promise<void> {
  const parsed = observationReportSchema.parse(report);
  const now = Date.now();
  await pruneSavedReports(now, 1);
  const record = {
    report: redactObservationReport(parsed),
    savedAt: new Date(now).toISOString(),
  };
  try {
    await chrome.storage.local.set({
      [localReportKey(parsed.reportId)]: record,
    });
  } catch {
    // One additional eviction gives quota pressure a bounded recovery path.
    // If the retry fails, propagate it instead of pretending history was saved.
    await pruneSavedReports(now, 2);
    await chrome.storage.local.set({
      [localReportKey(parsed.reportId)]: record,
    });
  }
}

async function pruneSavedReports(
  nowUnixMs: number,
  reserveSlots = 0,
): Promise<string[]> {
  const stored = await chrome.storage.local.get(null);
  const plan = planSavedReportPrune(stored, nowUnixMs, reserveSlots);
  if (plan.removeKeys.length > 0) {
    await chrome.storage.local.remove(plan.removeKeys);
  }
  return plan.keptKeys;
}

async function getSavedReportHistory(): Promise<Record<string, unknown>> {
  const keptKeys = await pruneSavedReports(Date.now());
  const stored =
    keptKeys.length === 0 ? {} : await chrome.storage.local.get(keptKeys);
  const invalidKeys: string[] = [];
  const history = keptKeys.flatMap((key) => {
    const value = stored[key];
    if (
      typeof value !== "object" ||
      value === null ||
      Array.isArray(value) ||
      typeof (value as Record<string, unknown>).savedAt !== "string"
    ) {
      invalidKeys.push(key);
      return [];
    }
    const report = observationReportSchema.safeParse(
      (value as Record<string, unknown>).report,
    );
    if (!report.success) {
      invalidKeys.push(key);
      return [];
    }
    return [
      {
        savedAt: (value as Record<string, unknown>).savedAt,
        report: report.data,
      },
    ];
  });
  if (invalidKeys.length > 0) {
    await chrome.storage.local.remove(invalidKeys);
  }
  return {
    count: history.length,
    maximum: MAX_SAVED_REPORTS,
    retentionDays: SAVED_REPORT_TTL_MS / (24 * 60 * 60 * 1_000),
    history,
  };
}

async function clearSavedReportHistory(): Promise<Record<string, unknown>> {
  const stored = await chrome.storage.local.get(null);
  const keys = Object.keys(stored).filter((key) =>
    key.startsWith(SAVED_REPORT_KEY_PREFIX),
  );
  if (keys.length > 0) {
    await chrome.storage.local.remove(keys);
  }
  return { cleared: keys.length };
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

  // Bind before issuing any probe. The Native Host checks the same opaque
  // runtime UUID again on submission, so a stop/relaunch during the check is
  // rejected instead of attaching old browser traffic to a new runtime.
  const runtimeBinding = await resolveRuntimeEvidenceBinding();

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
  let handoff: NetworkEvidenceHandoffStatus = {
    state: "local_only",
    checkedAt: result.checkedAt,
    reason: "desktop_unavailable",
  };
  await chrome.storage.session.set({
    [NETWORK_CHECK_KEY]: result,
    [NETWORK_CHECK_HANDOFF_KEY]: handoff,
  });

  handoff = await handoffNetworkEvidence(result, runtimeBinding);
  await chrome.storage.session.set({ [NETWORK_CHECK_HANDOFF_KEY]: handoff });
  return { result, handoff };
}

async function getNetworkCheck(): Promise<Record<string, unknown>> {
  const stored = await chrome.storage.session.get([
    NETWORK_CHECK_KEY,
    NETWORK_CHECK_HANDOFF_KEY,
  ]);
  const result = stored[NETWORK_CHECK_KEY];
  if (!isNetworkCheckResult(result)) {
    await chrome.storage.session.remove([
      NETWORK_CHECK_KEY,
      NETWORK_CHECK_HANDOFF_KEY,
    ]);
    return { result: null, handoff: null };
  }
  const handoff = stored[NETWORK_CHECK_HANDOFF_KEY];
  if (!isNetworkEvidenceHandoffStatus(handoff, result.checkedAt)) {
    await chrome.storage.session.remove(NETWORK_CHECK_HANDOFF_KEY);
    return { result, handoff: null };
  }
  return { result, handoff };
}

async function clearNetworkCheck(): Promise<Record<string, unknown>> {
  await chrome.storage.session.remove([
    NETWORK_CHECK_KEY,
    NETWORK_CHECK_HANDOFF_KEY,
  ]);
  return { cleared: true };
}

async function handoffNetworkEvidence(
  result: ReturnType<typeof buildNetworkCheckResult>,
  runtimeBinding: RuntimeBindingResolution,
): Promise<NetworkEvidenceHandoffStatus> {
  const localOnly = (
    reason: NetworkEvidenceHandoffReason,
  ): NetworkEvidenceHandoffStatus => ({
    state: "local_only",
    checkedAt: result.checkedAt,
    reason,
  });

  if (runtimeBinding.binding === null) {
    return localOnly(runtimeBinding.reason);
  }
  const { siloId, runtimeId } = runtimeBinding.binding;

  const submitRequestId = crypto.randomUUID();
  try {
    const submitRaw = await sendNativeMessageWithTimeout(NATIVE_HOST_NAME, {
      type: "submit_network_evidence",
      protocolVersion: PROTOCOL_VERSION,
      requestId: submitRequestId,
      siloId,
      runtimeId,
      networkCheck: result,
      coverage: NETWORK_EVIDENCE_COVERAGE,
    });
    const submitResponse = nativeResponseSchema.safeParse(submitRaw);
    if (
      !submitResponse.success ||
      submitResponse.data.type !== "evidence_accepted" ||
      submitResponse.data.requestId !== submitRequestId
    ) {
      return localOnly("submission_rejected");
    }
    return {
      state: "submitted",
      checkedAt: result.checkedAt,
      siloId,
      runtimeId,
      evidenceId: submitResponse.data.evidenceId,
      acceptedAt: submitResponse.data.acceptedAt,
      expiresAt: submitResponse.data.expiresAt,
    };
  } catch {
    return localOnly("submission_rejected");
  }
}

type RuntimeBindingResolution =
  | { binding: RuntimeEvidenceBinding; reason: null }
  | { binding: null; reason: NetworkEvidenceHandoffReason };

async function resolveRuntimeEvidenceBinding(): Promise<RuntimeBindingResolution> {
  const requestId = crypto.randomUUID();
  let statusRaw: unknown;
  try {
    statusRaw = await sendNativeMessageWithTimeout(NATIVE_HOST_NAME, {
      type: "get_runtime_status",
      protocolVersion: PROTOCOL_VERSION,
      requestId,
    });
  } catch {
    return { binding: null, reason: "desktop_unavailable" };
  }
  const statusResponse = nativeResponseSchema.safeParse(statusRaw);
  if (!statusResponse.success) {
    return { binding: null, reason: "desktop_unavailable" };
  }
  const binding = eligibleSiloForNetworkEvidence(
    statusResponse.data,
    requestId,
  );
  return binding === null
    ? { binding: null, reason: "runtime_not_ready" }
    : { binding, reason: null };
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
  try {
    const requestId = crypto.randomUUID();
    const raw = await sendNativeMessageWithTimeout(NATIVE_HOST_NAME, {
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

    const openRequestId = crypto.randomUUID();
    const openRaw = await sendNativeMessageWithTimeout(NATIVE_HOST_NAME, {
      type: "open_desktop",
      protocolVersion: PROTOCOL_VERSION,
      requestId: openRequestId,
    });
    const openResponse = nativeResponseSchema.parse(openRaw);
    if (
      openResponse.type !== "desktop_opened" ||
      openResponse.requestId !== openRequestId
    ) {
      const reason =
        openResponse.type === "error"
          ? openResponse.message
          : "The VeriSilo Native Host returned an unexpected open response.";
      throw new Error(reason);
    }
    return { connected: true, desktopOpened: true };
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
  return `${SAVED_REPORT_KEY_PREFIX}${reportId}`;
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
