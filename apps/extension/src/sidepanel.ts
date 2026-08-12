import {
  labsExperimentReceiptSchema,
  labsExperimentSchema,
  observationReportSchema,
  type LabsExperiment,
  type LabsExperimentReceipt,
  type ObservationReport,
  type ObservedSignal,
  PRODUCT_WEBSITE_URL,
  type RuntimeCapability,
} from "@verisilo/contracts";

import { reportAsHtml, reportAsJson } from "./report-export.js";
import {
  isNetworkCheckResult,
  NETWORK_CHECK_ORIGINS,
  type NetworkCheckResult,
} from "./network-check.js";
import {
  isNetworkEvidenceHandoffStatus,
  type NetworkEvidenceHandoffStatus,
} from "./network-evidence-handoff.js";
import {
  summarizeReport,
  type HumanFact,
  type HumanFinding,
} from "./report-summary.js";
import {
  labsBrowserBackgroundLabel,
  labsEmbeddedPageLabel,
  labsEvidenceLabel,
  labsInjectionOrderLabel,
  labsNewBackgroundTaskLabel,
  labsVisibleSiteDataLabel,
  observedSignalName,
  observedSignalStatusLabel,
  observationWorkerCoverageLabel,
  signalFailureLabel,
  userFacingError,
} from "./ui-language.js";

const scanButton = requiredElement<HTMLButtonElement>("scan");
const requestSiteAccessButton = requiredElement<HTMLButtonElement>(
  "request-site-access",
);
const revokeSiteAccessButton =
  requiredElement<HTMLButtonElement>("revoke-site-access");
const openPrivateButton = requiredElement<HTMLButtonElement>("open-private");
const privacyEnableButton =
  requiredElement<HTMLButtonElement>("privacy-enable");
const privacyRestoreButton =
  requiredElement<HTMLButtonElement>("privacy-restore");
const desktopButton = requiredElement<HTMLButtonElement>("desktop");
const desktopProjectButton =
  requiredElement<HTMLButtonElement>("desktop-project");
const exportJsonButton = requiredElement<HTMLButtonElement>("export-json");
const exportHtmlButton = requiredElement<HTMLButtonElement>("export-html");
const clearReportHistoryButton = requiredElement<HTMLButtonElement>(
  "clear-report-history",
);
const reportHistoryStatus = requiredElement<HTMLParagraphElement>(
  "report-history-status",
);
const reportHistoryList = requiredElement<HTMLDivElement>(
  "report-history-list",
);
const notice = requiredElement<HTMLDivElement>("notice");
const reportEmpty = requiredElement<HTMLElement>("report-empty");
const reportContent = requiredElement<HTMLDivElement>("report-content");
const verdict = requiredElement<HTMLElement>("verdict");
const verdictIcon = requiredElement<HTMLDivElement>("verdict-icon");
const summaryHeadline = requiredElement<HTMLHeadingElement>("summary-headline");
const summaryDescription = requiredElement<HTMLParagraphElement>(
  "summary-description",
);
const reportOrigin = requiredElement<HTMLSpanElement>("report-origin");
const reportTime = requiredElement<HTMLSpanElement>("report-time");
const reportCoverage = requiredElement<HTMLSpanElement>("report-coverage");
const factGrid = requiredElement<HTMLDivElement>("fact-grid");
const findingList = requiredElement<HTMLDivElement>("finding-list");
const findingCount = requiredElement<HTMLSpanElement>("finding-count");
const privateStatus = requiredElement<HTMLParagraphElement>("private-status");
const webRtcStatus = requiredElement<HTMLSpanElement>("webrtc-status");
const predictionStatus = requiredElement<HTMLSpanElement>("prediction-status");
const labsEnableButton = requiredElement<HTMLButtonElement>("labs-enable");
const labsStopButton = requiredElement<HTMLButtonElement>("labs-stop");
const labsClearReceiptsButton = requiredElement<HTMLButtonElement>(
  "labs-clear-receipts",
);
const labsWorkerStatus = requiredElement<HTMLSpanElement>("labs-worker-status");
const labsScope = requiredElement<HTMLParagraphElement>("labs-scope");
const labsEvidence = requiredElement<HTMLParagraphElement>("labs-evidence");
const labsReceiptStatus = requiredElement<HTMLParagraphElement>(
  "labs-receipt-status",
);
const labsReceiptList = requiredElement<HTMLDivElement>("labs-receipt-list");
const rawMeta = requiredElement<HTMLDivElement>("raw-meta");
const rawEmpty = requiredElement<HTMLParagraphElement>("raw-empty");
const signals = requiredElement<HTMLDivElement>("signals");
const tabButtons = Array.from(
  document.querySelectorAll<HTMLButtonElement>("[data-tab]"),
);
const panels = Array.from(
  document.querySelectorAll<HTMLElement>("[data-panel]"),
);

let latestReport: ObservationReport | null = null;
let latestNetworkCheck: NetworkCheckResult | null = null;
let latestNetworkHandoff: NetworkEvidenceHandoffStatus | null = null;
let reportRefreshVersion = 0;
let labsRefreshVersion = 0;
let activeSitePermissionRefreshVersion = 0;
let networkHandoffExpiryTimer: ReturnType<typeof setTimeout> | null = null;
let activeSiteOriginPattern: string | null = null;

scanButton.addEventListener("click", () => void scan());
requestSiteAccessButton.addEventListener("click", () => {
  const permissionRequest =
    activeSiteOriginPattern === null
      ? null
      : chrome.permissions.request({ origins: [activeSiteOriginPattern] });
  void requestSiteAccess(permissionRequest);
});
revokeSiteAccessButton.addEventListener("click", () => void revokeSiteAccess());
openPrivateButton.addEventListener("click", () => void openPrivateWorkspace());
privacyEnableButton.addEventListener("click", () => {
  const permissionRequest = chrome.permissions.request({
    permissions: ["privacy"],
  });
  void enableRecommendedProtection(permissionRequest);
});
privacyRestoreButton.addEventListener(
  "click",
  () => void restorePrivacyControls(),
);
labsEnableButton.addEventListener("click", () => {
  if (
    !globalThis.confirm(
      "开启当前站点的网页后台任务检查？它可能让网站短暂异常；发现测试标记泄漏、页面异常、超时或权限变化时会立即恢复并停用。",
    )
  ) {
    return;
  }
  const permissionRequest =
    activeSiteOriginPattern === null
      ? null
      : chrome.permissions.request({ origins: [activeSiteOriginPattern] });
  void enableWorkerExperiment(permissionRequest);
});
labsStopButton.addEventListener("click", () => void stopWorkerExperiment());
labsClearReceiptsButton.addEventListener(
  "click",
  () => void clearLabsReceiptHistory(),
);
desktopButton.addEventListener("click", () => void openDesktop());
desktopProjectButton.addEventListener("click", () => void openDesktopProject());
exportJsonButton.addEventListener("click", () => exportCurrentReport("json"));
exportHtmlButton.addEventListener("click", () => exportCurrentReport("html"));
clearReportHistoryButton.addEventListener(
  "click",
  () => void clearSavedReportHistory(),
);
for (const tab of tabButtons) {
  tab.addEventListener("click", () => selectTab(tab.dataset.tab ?? "overview"));
}

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName === "session") {
    const changedKeys = Object.keys(changes);
    if (changedKeys.some((key) => key.startsWith("report:"))) {
      void refreshReport();
    }
    if (changedKeys.some((key) => key.startsWith("network-check:"))) {
      void refreshNetworkCheck();
    }
    if (changedKeys.includes("labs:status")) {
      void refreshLabsStatus(true);
    }
  }
  if (
    areaName === "local" &&
    Object.keys(changes).some((key) => key.startsWith("saved-report:"))
  ) {
    void refreshSavedReportHistory();
  }
  if (
    areaName === "local" &&
    Object.keys(changes).some((key) => key.startsWith("labs-receipt:"))
  ) {
    void refreshLabsReceipts();
  }
});

chrome.tabs.onActivated.addListener(() => refreshActiveTabContext());
chrome.windows.onFocusChanged.addListener(() => refreshActiveTabContext());
chrome.tabs.onUpdated.addListener((_tabId, changeInfo, tab) => {
  if (
    tab.active &&
    (changeInfo.status === "loading" || changeInfo.url !== undefined)
  ) {
    refreshActiveTabContext();
  }
});

void refreshReport();
void refreshIsolationStatus();
void refreshNetworkCheck();
void refreshSavedReportHistory();
void refreshLabsStatus();
void refreshLabsReceipts();
void refreshActiveSitePermissionTarget();

async function enableWorkerExperiment(
  permissionRequest: Promise<boolean> | null,
): Promise<void> {
  setLabsButtonsBusy(true, "正在开始检查…");
  try {
    if (permissionRequest !== null && !(await permissionRequest)) {
      throw new Error("未授予当前站点权限；实验保持关闭，也没有注入页面。");
    }
    const response = await sendMessage({
      type: "enable_dedicated_worker_experiment",
    });
    const worker = workerExperimentFromResponse(response);
    if (worker === null) {
      throw new Error("实验室返回了无法识别的状态。");
    }
    renderWorkerExperiment(worker);
    if (worker.state === "permission_missing") {
      showNotice("error", "未授予当前站点权限；实验保持关闭，也没有注入页面。");
    } else if (worker.state === "best_effort") {
      showNotice(
        "success",
        worker.scope?.mode === "local_temporary"
          ? "网页后台任务检查已启动；覆盖范围有限，只在当前浏览器中临时运行。"
          : "网页后台任务检查已启动，并关联当前桌面身份与网站；由于无法覆盖所有网页运行区域，状态标为“有限覆盖”。",
      );
    } else if (worker.state === "leak_detected") {
      showNotice(
        "error",
        "检测到随机测试标记泄漏；原网页行为已恢复，当前检查已停用。",
      );
    } else {
      showNotice("error", "检查未能完成确认，已恢复原网页行为并停用当前站点。");
    }
    await refreshLabsReceipts();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法开启网页后台任务检查，请稍后重试。"),
    );
  } finally {
    setLabsButtonsBusy(false);
    await refreshLabsStatus();
  }
}

async function stopWorkerExperiment(): Promise<void> {
  setLabsButtonsBusy(true, "正在恢复…");
  try {
    const response = await sendMessage({
      type: "stop_dedicated_worker_experiment",
    });
    const worker = workerExperimentFromResponse(response);
    if (worker !== null) {
      renderWorkerExperiment(worker);
    }
    showNotice("success", "已恢复原网页行为，并停用当前站点检查。");
    await refreshLabsReceipts();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法停止当前检查，请稍后重试。"),
    );
  } finally {
    setLabsButtonsBusy(false);
    await refreshLabsStatus();
  }
}

async function refreshLabsStatus(announceAutomaticStop = false): Promise<void> {
  const version = ++labsRefreshVersion;
  try {
    const response = await sendMessage({ type: "get_labs_status" });
    if (version !== labsRefreshVersion) {
      return;
    }
    const worker = workerExperimentFromResponse(response);
    renderWorkerExperiment(worker);
    if (
      announceAutomaticStop &&
      worker?.lastReceipt !== null &&
      worker?.lastReceipt !== undefined
    ) {
      if (worker.state === "failed" || worker.state === "leak_detected") {
        showNotice(
          "error",
          `实验室检查已因“${labsStopLabel(worker.lastReceipt.stopCode)}”停止；原网页行为${worker.lastReceipt.restore.succeeded ? "已恢复" : "未能确认恢复"}。`,
        );
      } else if (
        worker.state === "restored" &&
        worker.lastReceipt.stopCode !== "user_requested"
      ) {
        showNotice(
          "success",
          `实验室检查已因“${labsStopLabel(worker.lastReceipt.stopCode)}”自动停止并恢复。`,
        );
      }
    }
  } catch (error) {
    if (version !== labsRefreshVersion) {
      return;
    }
    labsWorkerStatus.textContent = "状态不可用";
    labsWorkerStatus.className = "status-chip warning";
    labsScope.textContent = userFacingError(
      error,
      "暂时无法读取实验状态，请稍后重试。",
    );
  }
}

async function refreshLabsReceipts(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_labs_receipts" });
    const receipts = Array.isArray(response.receipts)
      ? response.receipts.flatMap((value) => {
          const parsed = labsExperimentReceiptSchema.safeParse(value);
          return parsed.success ? [parsed.data] : [];
        })
      : [];
    const maximum =
      typeof response.maximum === "number" ? response.maximum : 50;
    const retentionDays =
      typeof response.retentionDays === "number" ? response.retentionDays : 30;
    labsReceiptStatus.textContent = `本机保存 ${receipts.length}/${maximum} 份脱敏收据；${retentionDays} 天后自动清理；下方显示最近 ${Math.min(receipts.length, 8)} 份。`;
    labsClearReceiptsButton.disabled = receipts.length === 0;
    renderLabsReceipts(receipts);
  } catch {
    labsReceiptStatus.textContent = "暂时无法读取本地实验收据。";
    labsReceiptList.replaceChildren();
  }
}

async function clearLabsReceiptHistory(): Promise<void> {
  if (!globalThis.confirm("清除全部本地脱敏实验记录？")) {
    return;
  }
  setBusy(labsClearReceiptsButton, true, "正在清除…");
  try {
    const response = await sendMessage({ type: "clear_labs_receipts" });
    const cleared = typeof response.cleared === "number" ? response.cleared : 0;
    showNotice("success", `已清除 ${cleared} 份本地脱敏实验记录。`);
    await refreshLabsReceipts();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法清除实验记录，请稍后重试。"),
    );
  } finally {
    setBusy(labsClearReceiptsButton, false);
    await refreshLabsReceipts();
  }
}

function workerExperimentFromResponse(
  response: Record<string, unknown>,
): LabsExperiment | null {
  if (!Array.isArray(response.experiments)) {
    return null;
  }
  for (const value of response.experiments) {
    const parsed = labsExperimentSchema.safeParse(value);
    if (parsed.success && parsed.data.id === "dedicated_worker_constructor") {
      return parsed.data;
    }
  }
  return null;
}

function renderWorkerExperiment(experiment: LabsExperiment | null): void {
  if (experiment === null) {
    labsWorkerStatus.textContent = "默认关闭";
    labsWorkerStatus.className = "status-chip neutral";
    labsScope.textContent = "尚未为当前网站开启检查。";
    labsEvidence.textContent = "尚无实验记录。";
    labsEnableButton.disabled = false;
    labsStopButton.disabled = true;
    return;
  }
  labsWorkerStatus.textContent = labsStateLabel(experiment.state);
  labsWorkerStatus.className = `status-chip ${labsStateTone(experiment.state)}`;
  labsEnableButton.disabled = experiment.enabled;
  labsStopButton.disabled = !experiment.enabled;
  if (experiment.scope === null) {
    labsScope.textContent = "尚未为当前网站开启检查；默认关闭。";
  } else if (experiment.scope.mode === "desktop_silo") {
    labsScope.textContent = `范围：当前桌面身份 / ${experiment.scope.siteHost}。检查权限到期：${formatDate(experiment.scope.expiresAt)}。`;
  } else {
    labsScope.textContent = `范围：${experiment.scope.siteHost} 的本机临时实验，不关联桌面身份，关闭浏览器或到期即失效。`;
  }
  labsEvidence.textContent = [
    `启用时机：${labsInjectionOrderLabel(experiment.coverage.injectionOrder)}`,
    `网页后台任务：${labsNewBackgroundTaskLabel(experiment.coverage.newDedicatedWorkers)}`,
    `内嵌页面：${labsEmbeddedPageLabel(experiment.coverage.windowIframe)}`,
    "不覆盖开启检查前已运行、跨站或浏览器长期运行的后台任务",
  ].join("；");
}

function renderLabsReceipts(receipts: LabsExperimentReceipt[]): void {
  labsReceiptList.replaceChildren();
  for (const receipt of receipts.slice(0, 8)) {
    const item = document.createElement("details");
    item.className = "labs-receipt";
    const summary = document.createElement("summary");
    const title = document.createElement("strong");
    title.textContent = `${labsStateLabel(receipt.state)} · ${receipt.scope.siteHost}`;
    const detail = document.createElement("span");
    const restoreLabel = receipt.restore.attempted
      ? `恢复${receipt.restore.succeeded ? "已确认" : "未确认"}`
      : "尚未执行恢复";
    detail.textContent = `${formatDate(receipt.finalizedAt)} · ${labsStopLabel(receipt.stopCode)} · ${restoreLabel}`;
    summary.append(title, detail);
    const phases = document.createElement("ul");
    phases.className = "labs-stop-list";
    for (const phase of receipt.phases) {
      const row = document.createElement("li");
      const evidence = phase.evidenceCodes.map(labsEvidenceLabel);
      row.textContent = `${labsPhaseLabel(phase.phase)}：${phase.outcome === "passed" ? "通过" : phase.outcome === "failed" ? "失败" : "未执行"} · ${evidence.join("、") || "没有补充记录"}`;
      phases.append(row);
    }
    const coverage = document.createElement("p");
    coverage.className = "labs-evidence";
    coverage.textContent = `覆盖：${labsNewBackgroundTaskLabel(receipt.coverage.newDedicatedWorkers)}；${labsEmbeddedPageLabel(receipt.coverage.windowIframe)}；启用时机：${labsInjectionOrderLabel(receipt.coverage.injectionOrder)}；网站数据：${labsVisibleSiteDataLabel(receipt.coverage.cookies)}；浏览器后台任务：${labsBrowserBackgroundLabel(receipt.coverage.serviceWorkers)}。`;
    item.append(summary, phases, coverage);
    labsReceiptList.append(item);
  }
}

function labsPhaseLabel(
  phase: LabsExperimentReceipt["phases"][number]["phase"],
): string {
  return {
    observe: "开始前检查",
    apply: "启用检查",
    verify: "结果确认",
    restore: "恢复网页",
  }[phase];
}

function labsStopLabel(code: LabsExperimentReceipt["stopCode"]): string {
  if (code === null) {
    return "运行中无停止条件";
  }
  const labels: Record<
    Exclude<LabsExperimentReceipt["stopCode"], null>,
    string
  > = {
    cross_tab_canary_leak: "随机测试标记传播到其他标签页",
    iframe_canary_leak: "随机测试标记传播到内嵌页面",
    worker_canary_leak: "随机测试标记传播到网页后台任务",
    service_worker_canary_leak: "随机测试标记出现在浏览器后台任务地址中",
    cookie_canary_leak: "随机测试标记出现在页面可见网站数据中",
    window_canary_leak: "随机测试标记传播到页面环境",
    page_error: "页面异常",
    worker_error: "网页后台任务异常",
    timeout: "运行超时",
    permission_taken_over: "站点权限已撤销或被接管",
    site_navigation: "页面已切换",
    scope_violation: "超出实验范围",
    verification_failed: "验证失败",
    extension_context_lost: "扩展上下文丢失",
    user_requested: "用户手动停止",
    expired: "授权已过期",
  };
  return labels[code];
}

function labsStateLabel(state: LabsExperiment["state"]): string {
  return {
    disabled: "默认关闭",
    permission_missing: "缺少站点权限",
    applying: "正在开启",
    best_effort: "有限覆盖",
    verified: "检查通过",
    failed: "失败并停用",
    leak_detected: "泄漏即停",
    restored: "已恢复",
    unsupported: "不支持",
  }[state];
}

function labsStateTone(state: LabsExperiment["state"]): string {
  if (state === "best_effort") {
    return "warning";
  }
  if (["failed", "leak_detected", "permission_missing"].includes(state)) {
    return "danger";
  }
  if (state === "restored") {
    return "good";
  }
  return "neutral";
}

function setLabsButtonsBusy(busy: boolean, busyText?: string): void {
  setBusy(labsEnableButton, busy, busyText);
  setBusy(labsStopButton, busy, busyText);
}

async function scan(): Promise<void> {
  setBusy(scanButton, true, "正在扫描…");
  try {
    const startedAt = Date.now();
    const response = await sendMessage({ type: "scan_current_tab" });
    const report = await waitForCompletedReport(
      startedAt,
      typeof response.origin === "string" ? response.origin : null,
    );
    if (report === null) {
      throw new Error(
        "扫描未在 7 秒内完成。页面可能阻止了某项浏览器信号；请重试或查看扩展错误日志。",
      );
    }
    renderReport(report);
    showNotice(
      "success",
      response.mainWorldInjected === false
        ? "基础扫描已完成。页面主环境观察不可用，结论已明确标注覆盖边界。"
        : "扫描已完成，结果已整理为身份结论。",
    );
  } catch (error) {
    showNotice("error", userFacingError(error, "扫描失败，请稍后重试。"));
  } finally {
    setBusy(scanButton, false);
  }
}

async function waitForCompletedReport(
  startedAt: number,
  expectedOrigin: string | null,
): Promise<ObservationReport | null> {
  const deadline = startedAt + 7_000;
  while (Date.now() < deadline) {
    const response = await sendMessage({ type: "get_current_report" });
    const parsed = observationReportSchema.safeParse(response.report);
    if (
      parsed.success &&
      Date.parse(parsed.data.collectedAt) >= startedAt - 1_000 &&
      (expectedOrigin === null || parsed.data.origin === expectedOrigin)
    ) {
      return parsed.data;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 100));
  }
  return null;
}

async function requestSiteAccess(
  directPermissionRequest: Promise<boolean> | null,
): Promise<void> {
  setBusy(requestSiteAccessButton, true, "正在检查…");
  try {
    if (directPermissionRequest !== null) {
      const granted = await directPermissionRequest;
      showNotice(
        granted ? "success" : "error",
        granted
          ? "当前站点权限已授予，可以扫描或运行明确开启的实验室检查。"
          : "未授予当前站点权限；VeriSilo 没有注入或扫描页面。",
      );
      return;
    }
    const response = await sendMessage({ type: "request_current_site_access" });
    if (response.temporaryAccess === true) {
      showNotice(
        "success",
        "工具栏已为当前标签页授予一次性访问权限，可以直接扫描；跨站导航或关闭标签页后会自动失效。",
      );
      return;
    }
    if (response.alreadyGranted === true) {
      showNotice(
        "success",
        "当前站点已有长期访问权限，无需重复请求。可以直接扫描。",
      );
      return;
    }
    showNotice(
      response.requested === true ? "success" : "error",
      response.requested === true
        ? "已向浏览器发起站点访问请求。请点击地址栏中的“允许”提示，授权后再扫描。"
        : "未能发起当前站点访问请求。",
    );
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法更改当前站点权限，请稍后重试。"),
    );
  } finally {
    setBusy(requestSiteAccessButton, false);
  }
}

async function revokeSiteAccess(): Promise<void> {
  setBusy(revokeSiteAccessButton, true, "正在撤销…");
  try {
    const response = await sendMessage({ type: "revoke_current_site_access" });
    showNotice(
      "success",
      response.removed === true
        ? "已撤销当前站点的长期访问权限；正在运行的该站点实验室检查也会停止并恢复。"
        : "当前站点没有长期访问权限。工具栏授予的一次性权限会在跨站导航或关闭标签页后失效。",
    );
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法撤销当前站点权限，请稍后重试。"),
    );
  } finally {
    setBusy(revokeSiteAccessButton, false);
    await refreshActiveSitePermissionTarget();
  }
}

async function openPrivateWorkspace(): Promise<void> {
  setBusy(openPrivateButton, true, "正在打开…");
  try {
    const response = await sendMessage({ type: "open_private_workspace" });
    if (response.opened !== true) {
      throw new Error("浏览器没有创建隐私窗口。");
    }
    // Creating a focused privacy window triggers tabs.onActivated. Let the
    // context refresh finish before publishing the operation result so it is
    // not immediately cleared as stale feedback from the previous tab.
    await new Promise((resolve) => window.setTimeout(resolve, 100));
    showNotice(
      "success",
      "已在 Chrome 无痕 / Edge InPrivate 中打开当前网站。它与普通窗口的网站数据分开；关闭全部隐私窗口后临时网站数据会被清除。",
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法打开隐私窗口，请检查扩展设置。"),
    );
  } finally {
    setBusy(openPrivateButton, false);
  }
}

async function enableRecommendedProtection(
  permissionRequest: Promise<boolean>,
): Promise<void> {
  setPrivacyButtonsBusy(true, "正在验证…");
  try {
    if (!(await permissionRequest)) {
      throw new Error("未授予隐私控制权限，VeriSilo 没有更改浏览器设置。");
    }

    const [webRtcResponse, predictionResponse] = await Promise.all([
      sendMessage({ type: "apply_webrtc_leak_reduction" }),
      sendMessage({ type: "apply_network_prediction_reduction" }),
    ]);
    const capabilities = [
      runtimeCapability(webRtcResponse.capability),
      runtimeCapability(predictionResponse.capability),
    ];
    const verified = capabilities.filter(
      (capability) => capability?.operation === "verified",
    ).length;
    showNotice(
      verified === capabilities.length ? "success" : "error",
      verified === capabilities.length
        ? "推荐保护已应用并复查：部分直接网络连接已限制，网络预测已关闭。"
        : `已确认 ${verified}/${capabilities.length} 项；其余设置可能被策略或其他扩展接管。`,
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法启用推荐保护，请稍后重试。"),
    );
  } finally {
    setPrivacyButtonsBusy(false);
  }
}

async function restorePrivacyControls(): Promise<void> {
  setPrivacyButtonsBusy(true, "正在恢复…");
  try {
    const [webRtcResponse, predictionResponse] = await Promise.all([
      sendMessage({ type: "restore_webrtc_leak_reduction" }),
      sendMessage({ type: "restore_network_prediction_reduction" }),
    ]);
    const capabilities = [
      runtimeCapability(webRtcResponse.capability),
      runtimeCapability(predictionResponse.capability),
    ];
    const completed = capabilities.filter(
      (capability) =>
        capability?.operation === "verified" ||
        capability?.operation === "not_requested",
    ).length;
    const permissionRemoved =
      completed === capabilities.length
        ? await chrome.permissions.remove({ permissions: ["privacy"] })
        : false;
    showNotice(
      completed === capabilities.length ? "success" : "error",
      completed === capabilities.length
        ? `VeriSilo 已恢复原设置，或确认没有需要恢复的设置${permissionRemoved ? "，并撤销了隐私控制权限" : ""}。`
        : "部分设置未能恢复；它可能已被浏览器策略或其他扩展接管。",
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice("error", userFacingError(error, "无法恢复原设置，请稍后重试。"));
  } finally {
    setPrivacyButtonsBusy(false);
  }
}

async function openDesktop(): Promise<void> {
  setBusy(desktopButton, true, "正在打开…");
  try {
    const response = await sendMessage({ type: "open_desktop" });
    if (response.desktopOpened !== true) {
      showNotice(
        "error",
        "未检测到可连接的 VeriSilo 桌面端。扩展仍可独立扫描和使用临时工具；安装并启动兼容的桌面端后才能使用桌面联动。",
      );
      return;
    }
    showNotice("success", "已打开 VeriSilo 桌面端。");
  } catch (error) {
    showNotice("error", userFacingError(error, "无法连接 VeriSilo 桌面端。"));
  } finally {
    setBusy(desktopButton, false);
  }
}

async function openDesktopProject(): Promise<void> {
  setBusy(desktopProjectButton, true, "正在打开…");
  try {
    await chrome.tabs.create({ url: PRODUCT_WEBSITE_URL });
    await new Promise((resolve) => window.setTimeout(resolve, 100));
    showNotice("success", "已打开 VeriSilo 项目页。");
  } catch (error) {
    showNotice("error", userFacingError(error, "无法打开 VeriSilo 项目页。"));
  } finally {
    setBusy(desktopProjectButton, false);
  }
}

async function refreshReport(): Promise<void> {
  const version = ++reportRefreshVersion;
  try {
    const response = await sendMessage({ type: "get_current_report" });
    if (version !== reportRefreshVersion) {
      return;
    }
    renderReport(
      (response.report as ObservationReport | null | undefined) ?? null,
    );
  } catch (error) {
    if (version !== reportRefreshVersion) {
      return;
    }
    showNotice("error", userFacingError(error, "暂时无法读取扫描结果。"));
  }
}

function refreshActiveTabContext(): void {
  notice.textContent = "";
  notice.className = "notice";
  void refreshReport();
  void refreshLabsStatus();
  void refreshActiveSitePermissionTarget();
}

async function refreshActiveSitePermissionTarget(): Promise<void> {
  const version = ++activeSitePermissionRefreshVersion;
  // Clear synchronously so a click immediately after a tab/window change can
  // never reuse the previous site's host pattern while the query is pending.
  activeSiteOriginPattern = null;
  const [tab] = await chrome.tabs.query({
    active: true,
    lastFocusedWindow: true,
  });
  if (version !== activeSitePermissionRefreshVersion) {
    return;
  }
  activeSiteOriginPattern =
    tab?.url !== undefined && /^https?:/u.test(tab.url)
      ? `${new URL(tab.url).origin}/*`
      : null;
}

async function refreshSavedReportHistory(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_saved_report_history" });
    const count =
      typeof response.count === "number" && Number.isSafeInteger(response.count)
        ? response.count
        : 0;
    const maximum =
      typeof response.maximum === "number" &&
      Number.isSafeInteger(response.maximum)
        ? response.maximum
        : 20;
    const retentionDays =
      typeof response.retentionDays === "number" &&
      Number.isSafeInteger(response.retentionDays)
        ? response.retentionDays
        : 30;
    reportHistoryStatus.textContent = `本机已保存 ${count}/${maximum} 份脱敏报告；超过 ${retentionDays} 天自动清理。`;
    clearReportHistoryButton.disabled = count === 0;
    reportHistoryList.replaceChildren();
    const history = Array.isArray(response.history) ? response.history : [];
    for (const value of history) {
      if (typeof value !== "object" || value === null || Array.isArray(value)) {
        continue;
      }
      const item = value as Record<string, unknown>;
      const report = observationReportSchema.safeParse(item.report);
      if (!report.success || typeof item.savedAt !== "string") {
        continue;
      }
      const button = document.createElement("button");
      button.className = "history-item";
      button.type = "button";
      button.textContent = `${displayOrigin(report.data.origin)} · ${formatDate(item.savedAt)}`;
      button.addEventListener("click", () => {
        renderReport(report.data);
        selectTab("overview");
        showNotice("success", "已打开本机保存的脱敏报告。原始高敏值不会恢复。");
      });
      reportHistoryList.append(button);
    }
  } catch {
    reportHistoryStatus.textContent = "暂时无法读取本地报告历史状态。";
    reportHistoryList.replaceChildren();
  }
}

async function clearSavedReportHistory(): Promise<void> {
  if (!globalThis.confirm("清除本机保存的全部脱敏报告历史？此操作不可撤销。")) {
    return;
  }
  setBusy(clearReportHistoryButton, true, "正在清除…");
  try {
    const response = await sendMessage({ type: "clear_saved_report_history" });
    const cleared = typeof response.cleared === "number" ? response.cleared : 0;
    reportHistoryList.replaceChildren();
    showNotice("success", `已清除 ${cleared} 份本地脱敏报告。`);
    await refreshSavedReportHistory();
  } catch (error) {
    showNotice(
      "error",
      userFacingError(error, "无法清除本地报告，请稍后重试。"),
    );
  } finally {
    setBusy(clearReportHistoryButton, false);
  }
}

async function refreshIsolationStatus(): Promise<void> {
  try {
    const response = await sendMessage({
      type: "get_lightweight_isolation_status",
    });
    const incognitoAllowed = response.incognitoAllowed === true;
    privateStatus.textContent = incognitoAllowed
      ? "已允许隐私窗口。注意：所有 Chrome 无痕 / Edge InPrivate 窗口共享同一个临时空间，不等于多个独立账号容器。"
      : "首次使用需要在“扩展管理 → VeriSilo Companion”中允许扩展在 Chrome 无痕 / Edge InPrivate 中运行。";

    const privacyGranted = response.privacyGranted === true;
    renderControlStatus(
      webRtcStatus,
      "直接连接保护",
      privacyGranted,
      response.webRtc,
    );
    renderControlStatus(
      predictionStatus,
      "网络预测",
      privacyGranted,
      response.networkPrediction,
    );
  } catch (error) {
    privateStatus.textContent = userFacingError(
      error,
      "暂时无法检查隐私窗口权限。",
    );
  }
}

async function refreshNetworkCheck(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_network_check" });
    latestNetworkCheck = isNetworkCheckResult(response.result)
      ? response.result
      : null;
    latestNetworkHandoff =
      latestNetworkCheck !== null &&
      isNetworkEvidenceHandoffStatus(
        response.handoff,
        latestNetworkCheck.checkedAt,
      )
        ? response.handoff
        : null;
    scheduleNetworkHandoffExpiry();
    renderNetworkFactState();
  } catch {
    latestNetworkCheck = null;
    latestNetworkHandoff = null;
    scheduleNetworkHandoffExpiry();
    renderNetworkFactState();
  }
}

async function runNetworkCheck(
  button: HTMLButtonElement,
  permissionRequest: Promise<boolean>,
): Promise<void> {
  setBusy(button, true, "正在检查…");
  try {
    const granted = await permissionRequest;
    if (!granted) {
      throw new Error("未授予三方检测端点权限，没有发送网络检查请求。");
    }

    const response = await sendMessage({ type: "run_network_check" });
    if (!isNetworkCheckResult(response.result)) {
      throw new Error("网络检查返回了无法识别的结果。");
    }
    latestNetworkCheck = response.result;
    latestNetworkHandoff = isNetworkEvidenceHandoffStatus(
      response.handoff,
      latestNetworkCheck.checkedAt,
    )
      ? response.handoff
      : null;
    scheduleNetworkHandoffExpiry();
    renderNetworkFactState();
    const hasUsefulResult =
      latestNetworkCheck.ip !== null ||
      latestNetworkCheck.dns.providers.length > 0;
    showNotice(
      hasUsefulResult ? "success" : "error",
      hasUsefulResult
        ? latestNetworkHandoff?.state === "submitted"
          ? "当前浏览器环境的出口检查完成，结果已交给正在运行的桌面身份。两家公共域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。"
          : "当前浏览器环境的出口检查完成，结果仅在扩展本地显示。两家公共域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。"
        : "网络检查没有获得有效结果，请查看网络或扩展权限后重试。",
    );
  } catch (error) {
    showNotice("error", userFacingError(error, "网络检查失败，请稍后重试。"));
  } finally {
    setBusy(button, false);
  }
}

async function clearNetworkCheck(): Promise<void> {
  try {
    await sendMessage({ type: "clear_network_check" });
    const permissionRemoved = await chrome.permissions.remove({
      origins: [...NETWORK_CHECK_ORIGINS],
    });
    latestNetworkCheck = null;
    latestNetworkHandoff = null;
    scheduleNetworkHandoffExpiry();
    renderNetworkFactState();
    showNotice(
      "success",
      `已从本次浏览器会话中清除网络检查结果${permissionRemoved ? "，并撤销检测服务访问权限" : ""}。`,
    );
  } catch (error) {
    showNotice("error", userFacingError(error, "无法清除网络检查结果。"));
  }
}

function renderReport(report: ObservationReport | null): void {
  latestReport = report;
  exportJsonButton.disabled = report === null;
  exportHtmlButton.disabled = report === null;
  reportEmpty.hidden = report !== null;
  reportContent.hidden = report === null;
  rawEmpty.hidden = report !== null;
  factGrid.replaceChildren();
  findingList.replaceChildren();
  signals.replaceChildren();
  rawMeta.replaceChildren();

  if (report === null) {
    return;
  }

  const human = summarizeReport(report);
  verdict.className = `card verdict ${human.tone}`;
  verdictIcon.textContent = human.tone === "attention" ? "!" : "✓";
  summaryHeadline.textContent = human.headline;
  summaryDescription.textContent = human.description;
  reportOrigin.textContent = `网站：${displayOrigin(report.origin)}`;
  reportTime.textContent = `扫描：${formatDate(report.collectedAt)}`;
  reportCoverage.textContent = coverageLabel(report);

  for (const fact of human.facts) {
    factGrid.append(renderFact(fact));
  }
  renderNetworkFactState();
  const sortedFindings = [...human.findings].sort(
    (left, right) => findingRank(left.tone) - findingRank(right.tone),
  );
  findingCount.textContent = `${sortedFindings.length} 条解读`;
  for (const finding of sortedFindings) {
    findingList.append(renderFinding(finding));
  }

  rawMeta.append(
    textLine(`网站：${report.origin}`),
    textLine(`采集时间：${report.collectedAt}`),
    textLine(
      `页面环境：${coverageLabel(report)} · 网页后台任务：${observationWorkerCoverageLabel(report.coverage.worker)}`,
    ),
    textLine(`信号组数：${report.signals.length}`),
  );
  for (const signal of report.signals) {
    signals.append(renderSignal(signal));
  }
}

function renderFact(fact: HumanFact): HTMLElement {
  const element = document.createElement("article");
  element.className = `fact${fact.id === "network" ? " network-fact" : ""}`;
  const label = document.createElement("span");
  label.className = "fact-label";
  label.textContent = fact.label;
  const value = document.createElement("strong");
  value.className = "fact-value";
  value.id = `fact-value-${fact.id}`;
  value.textContent = fact.value;
  const detail = document.createElement("small");
  detail.className = "fact-detail";
  detail.id = `fact-detail-${fact.id}`;
  detail.textContent = fact.detail;
  element.append(label, value, detail);
  if (fact.id === "network") {
    const badges = document.createElement("div");
    badges.className = "network-badges";
    badges.id = "network-badges";
    const actions = document.createElement("div");
    actions.className = "fact-actions";
    const checkButton = document.createElement("button");
    checkButton.className = "button subtle";
    checkButton.id = "network-check";
    checkButton.type = "button";
    checkButton.textContent = "同意并检查当前环境出口";
    checkButton.addEventListener("click", () => {
      // Chromium requires this API call to happen directly in the click callback,
      // before any confirmation dialog, message round-trip, or awaited work.
      const permissionRequest = chrome.permissions.request({
        origins: [...NETWORK_CHECK_ORIGINS],
      });
      void runNetworkCheck(checkButton, permissionRequest);
    });
    const clearButton = document.createElement("button");
    clearButton.className = "button subtle";
    clearButton.id = "network-clear";
    clearButton.type = "button";
    clearButton.textContent = "清除结果并撤销权限";
    clearButton.hidden = latestNetworkCheck === null;
    clearButton.addEventListener("click", () => void clearNetworkCheck());
    actions.append(checkButton, clearButton);
    const disclosure = document.createElement("small");
    disclosure.className = "network-disclosure";
    disclosure.textContent =
      "结果属于当前浏览器环境；成功交给正在运行的桌面身份后，桌面端会显示这次结果。点击后会连接 ipwho.is、Cloudflare 1.1.1.1 和 Google Public DNS，这些服务会看到请求的公网地址。两家域名解析服务只做答案对比，不能证明浏览器实际使用的解析路径。不会自动运行。";
    element.append(badges, actions, disclosure);
  }
  return element;
}

function renderNetworkFactState(): void {
  const value = document.getElementById("fact-value-network");
  const detail = document.getElementById("fact-detail-network");
  const badges = document.getElementById("network-badges");
  const checkButton = document.getElementById("network-check");
  const clearButton = document.getElementById("network-clear");
  if (
    value === null ||
    detail === null ||
    badges === null ||
    !(checkButton instanceof HTMLButtonElement) ||
    !(clearButton instanceof HTMLButtonElement)
  ) {
    return;
  }

  badges.replaceChildren();
  clearButton.hidden = latestNetworkCheck === null;
  checkButton.textContent =
    latestNetworkCheck === null ? "同意并检查当前环境出口" : "同意并重新检查";
  if (latestNetworkCheck === null) {
    value.textContent = "尚未检查";
    detail.textContent =
      "点击后可查看当前浏览器环境的公网地址、出口地区、网络运营商、时区/语言建议，以及两家公共域名解析服务的答案对比。";
    return;
  }

  const result = latestNetworkCheck;
  badges.append(networkHandoffChip(latestNetworkHandoff));
  if (result.ip === null) {
    value.textContent = "公网地址获取失败";
    detail.textContent = "一个或多个检测服务没有返回有效结果。";
    badges.append(networkChip("公网地址未确认", "attention"));
  } else {
    const location = [
      result.ip.countryCode ?? result.ip.country,
      result.ip.city,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ");
    value.textContent = `${result.ip.address}${location === "" ? "" : ` · ${location}`}`;
    const network = [
      result.ip.asn === null ? null : `网络编号 ${result.ip.asn}`,
      result.ip.organization ?? result.ip.isp,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ");
    detail.textContent = `${network === "" ? "运营商未知" : network}${
      result.ip.timezone === null ? "" : ` · 出口时区 ${result.ip.timezone}`
    }`;
    badges.append(networkChip("公网地址已确认", "good"));
    badges.append(
      result.ip.networkHint === "cloud_or_hosting"
        ? networkChip("云/机房线路线索", "attention")
        : networkChip("线路类型未判定", "neutral"),
    );
    const browserTimezone = observedTimezone(latestReport);
    if (browserTimezone !== null && result.ip.timezone !== null) {
      badges.append(
        browserTimezone === result.ip.timezone
          ? networkChip("浏览器与出口时区一致", "good")
          : networkChip("浏览器与出口时区不一致", "attention"),
      );
    }
    const languageRegion = observedLanguageRegion(latestReport);
    if (languageRegion !== null && result.ip.countryCode !== null) {
      badges.append(
        languageRegion === result.ip.countryCode.toUpperCase()
          ? networkChip("语言地区与出口国家一致", "good")
          : networkChip("语言地区与出口国家不同（仅建议）", "attention"),
      );
    }
  }

  const dnsLabels: Record<
    NetworkCheckResult["dns"]["state"],
    { text: string; tone: "good" | "attention" | "neutral" }
  > = {
    consistent: { text: "两家域名解析结果一致", tone: "good" },
    different: { text: "两家域名解析结果有差异", tone: "attention" },
    resolver_error: { text: "域名解析服务返回错误", tone: "attention" },
    partial: { text: "仅一家域名解析服务可用", tone: "attention" },
    failed: { text: "域名解析检查失败", tone: "attention" },
  };
  const dnsLabel = dnsLabels[result.dns.state];
  badges.append(networkChip(dnsLabel.text, dnsLabel.tone));
  badges.append(
    result.dns.dnssec === "validated"
      ? networkChip("两家解析服务均通过安全校验", "good")
      : networkChip("域名解析安全校验不完整", "attention"),
  );
  badges.append(networkChip("公网地址信誉未评分", "neutral"));
}

function networkHandoffChip(
  handoff: NetworkEvidenceHandoffStatus | null,
): HTMLElement {
  if (handoff?.state === "submitted") {
    return Date.parse(handoff.expiresAt) > Date.now()
      ? networkChip("桌面端已接收结果", "good")
      : networkChip("桌面端结果已过期", "attention");
  }
  const reason = handoff?.state === "local_only" ? handoff.reason : null;
  const reasonLabels = {
    desktop_unavailable: "桌面不可用 · 仅本地",
    runtime_not_ready: "桌面身份未运行 · 仅本地",
    submission_rejected: "桌面拒绝接收 · 仅本地",
  } as const;
  return networkChip(
    reason === null ? "仅本地显示" : reasonLabels[reason],
    reason === null ? "neutral" : "attention",
  );
}

function scheduleNetworkHandoffExpiry(): void {
  if (networkHandoffExpiryTimer !== null) {
    clearTimeout(networkHandoffExpiryTimer);
    networkHandoffExpiryTimer = null;
  }
  if (latestNetworkHandoff?.state !== "submitted") {
    return;
  }
  const delay = Date.parse(latestNetworkHandoff.expiresAt) - Date.now();
  if (!Number.isFinite(delay) || delay <= 0) {
    return;
  }
  networkHandoffExpiryTimer = setTimeout(
    () => {
      networkHandoffExpiryTimer = null;
      renderNetworkFactState();
    },
    Math.min(delay + 10, 2_147_483_647),
  );
}

function networkChip(
  text: string,
  tone: "good" | "attention" | "neutral",
): HTMLSpanElement {
  const chip = document.createElement("span");
  chip.className = `network-chip${tone === "neutral" ? "" : ` ${tone}`}`;
  chip.textContent = text;
  return chip;
}

function observedTimezone(report: ObservationReport | null): string | null {
  const value = report?.signals.find(
    (signal) => signal.id === "timezone" && signal.status === "ok",
  )?.value;
  return typeof value === "string" ? value : null;
}

function observedLanguageRegion(
  report: ObservationReport | null,
): string | null {
  const value = recordValue(
    report?.signals.find(
      (signal) => signal.id === "navigator" && signal.status === "ok",
    )?.value,
  );
  const language = typeof value?.language === "string" ? value.language : null;
  if (language === null) {
    return null;
  }
  const region = language
    .split("-")
    .find((part, index) => index > 0 && /^[A-Za-z]{2}$/u.test(part));
  return region?.toUpperCase() ?? null;
}

function renderFinding(finding: HumanFinding): HTMLElement {
  const element = document.createElement("article");
  element.className = `finding ${finding.tone}`;
  const dot = document.createElement("span");
  dot.className = "finding-dot";
  dot.setAttribute("aria-hidden", "true");
  const body = document.createElement("div");
  const heading = document.createElement("h3");
  heading.textContent = finding.title;
  const detail = document.createElement("p");
  detail.textContent = finding.detail;
  body.append(heading, detail);
  if (finding.action !== undefined) {
    const action = document.createElement("span");
    action.className = "finding-action";
    action.textContent = `建议：${finding.action}`;
    body.append(action);
  }
  element.append(dot, body);
  return element;
}

function renderSignal(signal: ObservedSignal): HTMLDetailsElement {
  const element = document.createElement("details");
  element.className = "signal";
  const summary = document.createElement("summary");
  const title = document.createElement("span");
  title.textContent = observedSignalName(signal.id);
  const status = document.createElement("span");
  status.className = `signal-status ${signal.status === "ok" ? "" : "error"}`;
  status.textContent = observedSignalStatusLabel(signal.status);
  summary.append(title, status);
  const detail = document.createElement("pre");
  detail.textContent =
    signal.status === "error" || signal.status === "unsupported"
      ? signalFailureLabel(signal.error)
      : safeStringify(signal.value, 2);
  element.append(summary, detail);
  return element;
}

function exportCurrentReport(format: "json" | "html"): void {
  if (latestReport === null) {
    showNotice("error", "请先扫描当前页面。");
    return;
  }
  if (!window.confirm("导出前会默认脱敏高敏感信号。是否继续？")) {
    return;
  }
  const content =
    format === "json" ? reportAsJson(latestReport) : reportAsHtml(latestReport);
  const blob = new Blob([content], {
    type: format === "json" ? "application/json" : "text/html",
  });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `verisilo-report-${latestReport.reportId}.${format}`;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
  showNotice("success", "已导出默认脱敏的本地报告。");
}

function selectTab(tabName: string): void {
  for (const tab of tabButtons) {
    const selected = tab.dataset.tab === tabName;
    tab.setAttribute("aria-selected", String(selected));
    tab.tabIndex = selected ? 0 : -1;
  }
  for (const panel of panels) {
    panel.hidden = panel.dataset.panel !== tabName;
  }
  if (tabName === "isolation") {
    void refreshIsolationStatus();
  }
  if (tabName === "labs") {
    void refreshLabsStatus();
    void refreshLabsReceipts();
  }
}

function renderControlStatus(
  element: HTMLSpanElement,
  label: string,
  privacyGranted: boolean,
  rawStatus: unknown,
): void {
  const status = recordValue(rawStatus);
  const effective = status?.effective === true;
  const state = typeof status?.state === "string" ? status.state : "unknown";
  element.classList.toggle("on", effective);
  if (!privacyGranted) {
    element.textContent = `${label}：待授权`;
  } else if (effective) {
    element.textContent = `${label}：已生效`;
  } else if (state === "controlled_by_other_extensions") {
    element.textContent = `${label}：被其他扩展控制`;
  } else if (state === "not_controllable") {
    element.textContent = `${label}：被策略锁定`;
  } else {
    element.textContent = `${label}：未启用`;
  }
}

async function sendMessage(
  payload: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const response = await chrome.runtime.sendMessage(payload);
  if (response?.ok !== true) {
    throw new Error(String(response?.error ?? "VeriSilo operation failed."));
  }
  return response as Record<string, unknown>;
}

function runtimeCapability(value: unknown): RuntimeCapability | null {
  if (value === null || typeof value !== "object") {
    return null;
  }
  return value as RuntimeCapability;
}

function recordValue(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function safeStringify(value: unknown, spacing = 0): string {
  try {
    return JSON.stringify(value, null, spacing) ?? "无值";
  } catch {
    return "无法安全显示该值";
  }
}

function displayOrigin(origin: string): string {
  try {
    return new URL(origin).host;
  } catch {
    return origin;
  }
}

function formatDate(isoDate: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(isoDate));
  } catch {
    return isoDate;
  }
}

function coverageLabel(report: ObservationReport): string {
  const mainWorldLabels: Record<
    ObservationReport["coverage"]["mainWorld"],
    string
  > = {
    not_attempted: "页面环境未检查",
    observed: "页面环境已观察",
    partial: "页面环境部分覆盖",
    unavailable: "页面环境不可用",
  };
  return mainWorldLabels[report.coverage.mainWorld];
}

function findingRank(tone: HumanFinding["tone"]): number {
  return { attention: 0, info: 1, normal: 2 }[tone];
}

function textLine(text: string): HTMLSpanElement {
  const line = document.createElement("span");
  line.textContent = text;
  return line;
}

function showNotice(tone: "error" | "success", text: string): void {
  notice.className = `notice ${tone}`;
  notice.textContent = text;
}

function setPrivacyButtonsBusy(busy: boolean, busyText?: string): void {
  setBusy(privacyEnableButton, busy, busyText);
  setBusy(privacyRestoreButton, busy, busyText);
}

function setBusy(
  button: HTMLButtonElement,
  busy: boolean,
  busyText?: string,
): void {
  if (busy) {
    button.dataset.idleText = button.textContent ?? "";
    button.disabled = true;
    if (busyText !== undefined) {
      button.textContent = busyText;
    }
    return;
  }
  button.disabled = false;
  if (button.dataset.idleText !== undefined) {
    button.textContent = button.dataset.idleText;
    delete button.dataset.idleText;
  }
}

function requiredElement<T extends HTMLElement>(id: string): T {
  const element = document.getElementById(id);
  if (element === null) {
    throw new Error(`Missing required element: ${id}`);
  }
  return element as T;
}
