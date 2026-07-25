import {
  contentMessageSchema,
  extensionPageMessageSchema,
  nativeResponseSchema,
  observationReportSchema,
  type RuntimeCapability,
  PROTOCOL_VERSION,
} from "@verisilo/contracts";

import { redactObservationReport } from "./report-export.js";

const NATIVE_HOST_NAME = "io.verisilo.host";
const REPORT_KEY_PREFIX = "report:";
const LOCAL_REPORT_KEY_PREFIX = "saved-report:";
const WEBRTC_RESTORE_POINT_KEY = "webrtc-restore-point";
type WebRtcPolicy =
  | "default"
  | "default_public_and_private_interfaces"
  | "default_public_interface_only"
  | "disable_non_proxied_udp";

void chrome.storage.local.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
void chrome.storage.session.setAccessLevel({ accessLevel: "TRUSTED_CONTEXTS" });
void chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: true });

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
    const tabUrl = sender.tab?.url;
    if (tabId === undefined || tabUrl === undefined) {
      throw new Error(
        "Observation messages must originate from an active tab.",
      );
    }
    const tabOrigin = new URL(tabUrl).origin;
    if (contentMessage.data.report.origin !== tabOrigin) {
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
    case "get_current_report":
      return getCurrentReport();
    case "request_optional_privacy_permission":
      return requestPrivacyPermission();
    case "apply_webrtc_leak_reduction":
      return applyWebRtcLeakReduction();
    case "restore_webrtc_leak_reduction":
      return restoreWebRtcLeakReduction();
    case "open_desktop":
      return connectNativeHost();
  }
}

async function scanCurrentTab(): Promise<Record<string, unknown>> {
  const tab = await activeTab();
  if (tab.id === undefined) {
    throw new Error("No active browser tab is available.");
  }
  if (tab.url === undefined || !/^https?:/u.test(tab.url)) {
    throw new Error(
      "VeriSilo only scans regular HTTP(S) pages after you request it.",
    );
  }

  await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    files: ["content.js"],
    injectImmediately: true,
  });
  await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    files: ["main-world.js"],
    world: "MAIN",
    injectImmediately: true,
  });

  return { started: true };
}

async function getCurrentReport(): Promise<Record<string, unknown>> {
  const tab = await activeTab();
  if (tab.id === undefined || tab.url === undefined) {
    return { report: null };
  }
  const stored = await chrome.storage.session.get(reportKey(tab.id));
  const parsed = observationReportSchema.safeParse(stored[reportKey(tab.id)]);
  if (!parsed.success || parsed.data.origin !== new URL(tab.url).origin) {
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

async function applyWebRtcLeakReduction(): Promise<Record<string, unknown>> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: capability("not_controllable", "permission_missing", {
        reason: "privacy permission is not granted",
      }),
    };
  }

  const setting = chrome.privacy.network.webRTCIPHandlingPolicy;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: capability(beforeControl, "not_controllable", {
        levelOfControl: before.levelOfControl,
        value: before.value,
      }),
    };
  }

  if (before.value === "disable_non_proxied_udp") {
    return {
      capability: capability("controllable_by_this_extension", "verified", {
        alreadyEnabled: true,
        levelOfControl: before.levelOfControl,
        value: before.value,
      }),
    };
  }

  await chrome.storage.session.set({
    [WEBRTC_RESTORE_POINT_KEY]: { value: before.value },
  });
  await setting.set({ value: "disable_non_proxied_udp" });
  const after = await setting.get({});
  const verified =
    after.levelOfControl === "controllable_by_this_extension" &&
    after.value === "disable_non_proxied_udp";
  return {
    capability: verified
      ? capability("controllable_by_this_extension", "verified", {
          levelOfControl: after.levelOfControl,
          value: after.value,
        })
      : capability("controllable_by_this_extension", "verification_failed", {
          levelOfControl: after.levelOfControl,
          value: after.value,
        }),
  };
}

async function restoreWebRtcLeakReduction(): Promise<Record<string, unknown>> {
  const hasPermission = await chrome.permissions.contains({
    permissions: ["privacy"],
  });
  if (!hasPermission) {
    return {
      capability: capability("not_controllable", "permission_missing", {
        reason: "privacy permission is not granted",
      }),
    };
  }

  const setting = chrome.privacy.network.webRTCIPHandlingPolicy;
  const before = await setting.get({});
  const beforeControl = controlFromLevel(before.levelOfControl);
  if (beforeControl !== "controllable_by_this_extension") {
    return {
      capability: capability(beforeControl, "not_controllable", {
        levelOfControl: before.levelOfControl,
        value: before.value,
      }),
    };
  }

  const stored = await chrome.storage.session.get(WEBRTC_RESTORE_POINT_KEY);
  const restoreValue = storedWebRtcRestoreValue(
    stored[WEBRTC_RESTORE_POINT_KEY],
  );
  if (restoreValue === null) {
    return {
      capability: capability(
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

  if (restoreValue === "default") {
    await setting.clear({});
  } else {
    await setting.set({ value: restoreValue });
  }
  const after = await setting.get({});
  const restored = after.value === restoreValue;
  if (restored) {
    await chrome.storage.session.remove(WEBRTC_RESTORE_POINT_KEY);
  }
  return {
    capability: restored
      ? capability("controllable_by_this_extension", "verified", {
          action: "restored",
          restoredValue: restoreValue,
          levelOfControl: after.levelOfControl,
          value: after.value,
        })
      : capability("controllable_by_this_extension", "verification_failed", {
          action: "restore",
          restoredValue: restoreValue,
          levelOfControl: after.levelOfControl,
          value: after.value,
        }),
  };
}

function capability(
  control: RuntimeCapability["control"],
  operation: RuntimeCapability["operation"],
  evidence: Record<string, unknown>,
): RuntimeCapability {
  const base = {
    id: "webrtc_non_proxied_udp",
    tier: "best_effort" as const,
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

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
