import {
  type ObservationReport,
  type ObservedSignal,
  type RuntimeCapability,
} from "@verisilo/contracts";

import { reportAsHtml, reportAsJson } from "./report-export.js";
import {
  isNetworkCheckResult,
  NETWORK_CHECK_ORIGINS,
  type NetworkCheckResult,
} from "./network-check.js";
import {
  summarizeReport,
  type HumanFact,
  type HumanFinding,
} from "./report-summary.js";

const scanButton = requiredElement<HTMLButtonElement>("scan");
const requestSiteAccessButton = requiredElement<HTMLButtonElement>(
  "request-site-access",
);
const openPrivateButton = requiredElement<HTMLButtonElement>("open-private");
const privacyEnableButton =
  requiredElement<HTMLButtonElement>("privacy-enable");
const privacyRestoreButton =
  requiredElement<HTMLButtonElement>("privacy-restore");
const desktopButton = requiredElement<HTMLButtonElement>("desktop");
const exportJsonButton = requiredElement<HTMLButtonElement>("export-json");
const exportHtmlButton = requiredElement<HTMLButtonElement>("export-html");
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

scanButton.addEventListener("click", () => void scan());
requestSiteAccessButton.addEventListener(
  "click",
  () => void requestSiteAccess(),
);
openPrivateButton.addEventListener("click", () => void openPrivateWorkspace());
privacyEnableButton.addEventListener(
  "click",
  () => void enableRecommendedProtection(),
);
privacyRestoreButton.addEventListener(
  "click",
  () => void restorePrivacyControls(),
);
desktopButton.addEventListener("click", () => void connectDesktop());
exportJsonButton.addEventListener("click", () => exportCurrentReport("json"));
exportHtmlButton.addEventListener("click", () => exportCurrentReport("html"));
for (const tab of tabButtons) {
  tab.addEventListener("click", () => selectTab(tab.dataset.tab ?? "overview"));
}

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (
    areaName === "session" &&
    Object.keys(changes).some((key) => key.startsWith("report:"))
  ) {
    void refreshReport();
  }
});

void refreshReport();
void refreshIsolationStatus();
void refreshNetworkCheck();

async function scan(): Promise<void> {
  setBusy(scanButton, true, "正在扫描…");
  try {
    const response = await sendMessage({ type: "scan_current_tab" });
    showNotice(
      "success",
      response.mainWorldInjected === false
        ? "基础扫描已启动。页面主环境观察不可用，结论会明确标注覆盖边界。"
        : "扫描已启动，结果完成后会自动整理为身份结论。",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(scanButton, false);
  }
}

async function requestSiteAccess(): Promise<void> {
  setBusy(requestSiteAccessButton, true, "正在检查…");
  try {
    const response = await sendMessage({ type: "request_current_site_access" });
    if (response.alreadyGranted === true) {
      showNotice(
        "success",
        "当前站点已有访问权限，无需重复请求。可以直接扫描。",
      );
      return;
    }
    showNotice(
      response.requested === true ? "success" : "error",
      response.requested === true
        ? "已向 Edge 发起站点访问请求。允许后再次扫描即可。"
        : "未能发起当前站点访问请求。",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(requestSiteAccessButton, false);
  }
}

async function openPrivateWorkspace(): Promise<void> {
  setBusy(openPrivateButton, true, "正在打开…");
  try {
    const response = await sendMessage({ type: "open_private_workspace" });
    if (response.opened !== true) {
      throw new Error("Edge 没有创建 InPrivate 窗口。");
    }
    showNotice(
      "success",
      "已在 InPrivate 中打开当前网站。它与普通窗口的网站数据分开；关闭全部 InPrivate 窗口后临时数据会被清除。",
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(openPrivateButton, false);
  }
}

async function enableRecommendedProtection(): Promise<void> {
  setPrivacyButtonsBusy(true, "正在验证…");
  try {
    const permission = await sendMessage({
      type: "request_optional_privacy_permission",
    });
    if (permission.granted !== true) {
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
        ? "推荐保护已应用并复查：WebRTC 非代理 UDP 已限制，网络预测已关闭。"
        : `已验证 ${verified}/${capabilities.length} 项；其余设置可能被策略或其他扩展接管。`,
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice("error", message(error));
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
    showNotice(
      completed === capabilities.length ? "success" : "error",
      completed === capabilities.length
        ? "VeriSilo 已恢复原设置，或确认没有需要恢复的设置。"
        : "部分设置未能恢复；它可能已被浏览器策略或其他扩展接管。",
    );
    await refreshIsolationStatus();
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setPrivacyButtonsBusy(false);
  }
}

async function connectDesktop(): Promise<void> {
  setBusy(desktopButton, true, "连接中…");
  try {
    const response = await sendMessage({ type: "open_desktop" });
    showNotice(
      response.connected === true ? "success" : "error",
      response.connected === true
        ? "已连接 VeriSilo 桌面端。"
        : `桌面端尚未连接：${String(response.reason ?? "未安装或未运行")}`,
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(desktopButton, false);
  }
}

async function refreshReport(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_current_report" });
    renderReport(
      (response.report as ObservationReport | null | undefined) ?? null,
    );
  } catch (error) {
    showNotice("error", message(error));
  }
}

async function refreshIsolationStatus(): Promise<void> {
  try {
    const response = await sendMessage({
      type: "get_lightweight_isolation_status",
    });
    const incognitoAllowed = response.incognitoAllowed === true;
    privateStatus.textContent = incognitoAllowed
      ? "已允许 InPrivate。注意：所有 InPrivate 窗口共享同一个临时空间，不等于多个独立账号容器。"
      : "首次使用需要在“扩展管理 → VeriSilo Companion”中打开“允许 InPrivate”。";

    const privacyGranted = response.privacyGranted === true;
    renderControlStatus(
      webRtcStatus,
      "WebRTC",
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
    privateStatus.textContent = `无法检查当前状态：${message(error)}`;
  }
}

async function refreshNetworkCheck(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_network_check" });
    latestNetworkCheck = isNetworkCheckResult(response.result)
      ? response.result
      : null;
    renderNetworkFactState();
  } catch {
    latestNetworkCheck = null;
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
    renderNetworkFactState();
    const hasUsefulResult =
      latestNetworkCheck.ip !== null ||
      latestNetworkCheck.dns.providers.length > 0;
    showNotice(
      hasUsefulResult ? "success" : "error",
      hasUsefulResult
        ? "出口检查完成。DNS 结果只表示两家公共 DoH 的一致性，不证明本机或运营商 DNS 一定没有劫持。"
        : "网络检查没有获得有效结果，请查看网络或扩展权限后重试。",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(button, false);
  }
}

async function clearNetworkCheck(): Promise<void> {
  try {
    await sendMessage({ type: "clear_network_check" });
    latestNetworkCheck = null;
    renderNetworkFactState();
    showNotice("success", "已从本次浏览器会话中清除网络检查结果。");
  } catch (error) {
    showNotice("error", message(error));
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
      `覆盖：MAIN world ${report.coverage.mainWorld} · Worker ${report.coverage.worker}`,
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
    checkButton.textContent = "同意并检查出口 IP 与公共 DNS";
    checkButton.addEventListener("click", () => {
      // Edge requires this API call to happen directly in the click callback,
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
    clearButton.textContent = "清除结果";
    clearButton.hidden = latestNetworkCheck === null;
    clearButton.addEventListener("click", () => void clearNetworkCheck());
    actions.append(checkButton, clearButton);
    const disclosure = document.createElement("small");
    disclosure.className = "network-disclosure";
    disclosure.textContent =
      "点击“同意并检查”后会连接 ipwho.is、Cloudflare 1.1.1.1 和 Google Public DNS；三方会看到你的请求 IP。DNS 仅比较固定域名的公共 DoH 结果，不能证明本机 DNS 未被劫持。不会自动运行。";
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
    latestNetworkCheck === null
      ? "同意并检查出口 IP 与公共 DNS"
      : "同意并重新检查";
  if (latestNetworkCheck === null) {
    value.textContent = "本次未验证";
    detail.textContent =
      "点击后可查看公网 IP、出口地区、ASN、运营商、时区组合及公共 DNS 一致性。";
    return;
  }

  const result = latestNetworkCheck;
  if (result.ip === null) {
    value.textContent = "出口 IP 获取失败";
    detail.textContent =
      result.errors.join("；") || "第三方 IP 服务没有返回有效数据。";
    badges.append(networkChip("IP 未确认", "attention"));
  } else {
    const location = [
      result.ip.countryCode ?? result.ip.country,
      result.ip.city,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ");
    value.textContent = `${result.ip.address}${location === "" ? "" : ` · ${location}`}`;
    const network = [result.ip.asn, result.ip.organization ?? result.ip.isp]
      .filter((part): part is string => part !== null)
      .join(" · ");
    detail.textContent = `${network === "" ? "运营商未知" : network}${
      result.ip.timezone === null ? "" : ` · 出口时区 ${result.ip.timezone}`
    }`;
    badges.append(networkChip("IP 已确认", "good"));
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
  }

  const dnsLabels: Record<
    NetworkCheckResult["dns"]["state"],
    { text: string; tone: "good" | "attention" | "neutral" }
  > = {
    consistent: { text: "公共 DNS 结果一致", tone: "good" },
    different: { text: "公共 DNS 结果有差异", tone: "attention" },
    resolver_error: { text: "公共 DNS 返回错误", tone: "attention" },
    partial: { text: "仅一家公共 DNS 可用", tone: "attention" },
    failed: { text: "公共 DNS 检查失败", tone: "attention" },
  };
  const dnsLabel = dnsLabels[result.dns.state];
  badges.append(networkChip(dnsLabel.text, dnsLabel.tone));
  badges.append(
    result.dns.dnssec === "validated"
      ? networkChip("DNSSEC 两家均验证", "good")
      : networkChip("DNSSEC 未完整验证", "attention"),
  );
  badges.append(networkChip("IP 信誉/黑名单未评分", "neutral"));
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
  title.textContent = humanSignalName(signal.id);
  const status = document.createElement("span");
  status.className = `signal-status ${signal.status === "ok" ? "" : "error"}`;
  status.textContent = signalStatusLabel(signal.status);
  summary.append(title, status);
  const detail = document.createElement("pre");
  detail.textContent = signal.error ?? safeStringify(signal.value, 2);
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

function humanSignalName(signalId: string): string {
  const names: Record<string, string> = {
    navigator: "浏览器与设备",
    ua_ch: "浏览器客户端提示（UA-CH）",
    timezone: "时区",
    screen: "屏幕",
    canvas_hash: "Canvas 特征摘要",
    webgl: "WebGL 显卡信息",
    webgpu: "WebGPU 可用性",
    audio: "音频特征摘要",
    fonts: "字体可见性",
    media_devices: "摄像头与麦克风",
    permissions: "网站权限",
    storage: "Cookie 与本地存储",
    webrtc: "WebRTC",
    window_iframe: "iframe 环境",
    dedicated_worker: "Dedicated Worker 环境",
    main_world_navigator: "页面主环境浏览器信息",
  };
  return names[signalId] ?? signalId;
}

function signalStatusLabel(status: ObservedSignal["status"]): string {
  const labels: Record<ObservedSignal["status"], string> = {
    ok: "已采集",
    blocked: "被阻止",
    unsupported: "不支持",
    error: "失败",
  };
  return labels[status];
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

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
