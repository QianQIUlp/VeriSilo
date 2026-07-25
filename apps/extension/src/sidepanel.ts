import {
  analyzeConsistency,
  type ObservationReport,
  type ObservedSignal,
  type RuntimeCapability,
} from "@verisilo/contracts";

import { reportAsHtml, reportAsJson } from "./report-export.js";

const scanButton = requiredElement<HTMLButtonElement>("scan");
const privacyButton = requiredElement<HTMLButtonElement>("privacy");
const webRtcApplyButton = requiredElement<HTMLButtonElement>("webrtc-apply");
const webRtcRestoreButton =
  requiredElement<HTMLButtonElement>("webrtc-restore");
const desktopButton = requiredElement<HTMLButtonElement>("desktop");
const exportJsonButton = requiredElement<HTMLButtonElement>("export-json");
const exportHtmlButton = requiredElement<HTMLButtonElement>("export-html");
const notice = requiredElement<HTMLDivElement>("notice");
const summary = requiredElement<HTMLParagraphElement>("summary");
const signals = requiredElement<HTMLDivElement>("signals");
let latestReport: ObservationReport | null = null;

scanButton.addEventListener("click", () => void scan());
privacyButton.addEventListener("click", () => void requestPrivacy());
webRtcApplyButton.addEventListener("click", () => void applyWebRtc());
webRtcRestoreButton.addEventListener("click", () => void restoreWebRtc());
desktopButton.addEventListener("click", () => void connectDesktop());
exportJsonButton.addEventListener("click", () => exportCurrentReport("json"));
exportHtmlButton.addEventListener("click", () => exportCurrentReport("html"));
chrome.storage.onChanged.addListener((changes, areaName) => {
  if (
    areaName === "session" &&
    Object.keys(changes).some((key) => key.startsWith("report:"))
  ) {
    void refresh();
  }
});

void refresh();

async function scan(): Promise<void> {
  setBusy(scanButton, true);
  try {
    await sendMessage({ type: "scan_current_tab" });
    showNotice("success", "正在采集页面信号；完成后将自动更新结果。");
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(scanButton, false);
  }
}

async function requestPrivacy(): Promise<void> {
  setBusy(privacyButton, true);
  try {
    const response = await sendMessage({
      type: "request_optional_privacy_permission",
    });
    showNotice(
      response.granted === true ? "success" : "error",
      response.granted === true
        ? "已授予隐私控制权限。尚未代表你更改任何设置。"
        : "未授予隐私控制权限。",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(privacyButton, false);
  }
}

async function connectDesktop(): Promise<void> {
  setBusy(desktopButton, true);
  try {
    const response = await sendMessage({ type: "open_desktop" });
    showNotice(
      response.connected === true ? "success" : "error",
      response.connected === true
        ? "已连接 VeriSilo Native Host。"
        : `未连接桌面端：${String(response.reason ?? "未知错误")}`,
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(desktopButton, false);
  }
}

async function applyWebRtc(): Promise<void> {
  setBusy(webRtcApplyButton, true);
  try {
    const response = await sendMessage({ type: "apply_webrtc_leak_reduction" });
    showCapability(
      response.capability as RuntimeCapability | undefined,
      "apply",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(webRtcApplyButton, false);
  }
}

async function restoreWebRtc(): Promise<void> {
  setBusy(webRtcRestoreButton, true);
  try {
    const response = await sendMessage({
      type: "restore_webrtc_leak_reduction",
    });
    showCapability(
      response.capability as RuntimeCapability | undefined,
      "restore",
    );
  } catch (error) {
    showNotice("error", message(error));
  } finally {
    setBusy(webRtcRestoreButton, false);
  }
}

async function refresh(): Promise<void> {
  try {
    const response = await sendMessage({ type: "get_current_report" });
    renderReport(
      (response.report as ObservationReport | null | undefined) ?? null,
    );
  } catch (error) {
    showNotice("error", message(error));
  }
}

function renderReport(report: ObservationReport | null): void {
  latestReport = report;
  exportJsonButton.disabled = report === null;
  exportHtmlButton.disabled = report === null;
  signals.replaceChildren();
  if (report === null) {
    summary.textContent = "尚未扫描。";
    return;
  }
  const findings = analyzeConsistency(report);
  const warnings = findings.filter(
    (finding) => finding.severity === "warning",
  ).length;
  summary.textContent =
    warnings > 0
      ? `已采集 ${report.signals.length} 个信号；有 ${warnings} 项需要理解的组合。`
      : `已采集 ${report.signals.length} 个信号；本次未发现明显矛盾。`;
  for (const finding of findings) {
    signals.append(
      renderFinding(finding.title, finding.beginnerSummary, finding.severity),
    );
  }
  for (const signal of report.signals) {
    signals.append(renderSignal(signal));
  }
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

function renderFinding(
  title: string,
  explanation: string,
  severity: "normal" | "info" | "warning",
): HTMLDivElement {
  const element = document.createElement("div");
  element.className = "signal";
  const heading = document.createElement("strong");
  heading.textContent = `${severity === "warning" ? "需要注意" : "说明"}：${title}`;
  const detail = document.createElement("small");
  detail.textContent = explanation;
  element.append(heading, detail);
  return element;
}

function renderSignal(signal: ObservedSignal): HTMLDivElement {
  const element = document.createElement("div");
  element.className = "signal";
  const title = document.createElement("strong");
  title.textContent = `${signal.id} · ${signal.status}`;
  const detail = document.createElement("small");
  detail.textContent = signal.error ?? safeStringify(signal.value);
  element.append(title, detail);
  return element;
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

function safeStringify(value: unknown): string {
  try {
    return JSON.stringify(value) ?? "无值";
  } catch {
    return "无法安全显示该值";
  }
}

function showNotice(tone: "error" | "success", text: string): void {
  notice.className = tone;
  notice.textContent = text;
}

function showCapability(
  capability: RuntimeCapability | undefined,
  action: "apply" | "restore",
): void {
  if (capability === undefined) {
    showNotice("error", "扩展没有返回能力状态。");
    return;
  }
  const success =
    capability.operation === "verified" ||
    (action === "restore" && capability.operation === "not_requested");
  const operationLabels: Record<RuntimeCapability["operation"], string> = {
    not_requested: "未请求",
    permission_missing: "权限缺失",
    not_controllable: "不可控制",
    configured: "已配置",
    applied: "已应用",
    verified: "已验证",
    verification_failed: "验证失败",
  };
  if (action === "restore" && capability.operation === "verified") {
    showNotice(
      "success",
      "WebRTC 设置已恢复并验证；不再将非代理 UDP 减少功能显示为开启。",
    );
    return;
  }
  if (action === "restore" && capability.operation === "not_requested") {
    showNotice("success", "没有由本次 VeriSilo 操作保存的 WebRTC 设置可恢复。");
    return;
  }
  showNotice(
    success ? "success" : "error",
    `WebRTC 非代理 UDP：${operationLabels[capability.operation]}（${capability.tier === "best_effort" ? "尽力" : capability.tier}）。`,
  );
}

function setBusy(button: HTMLButtonElement, busy: boolean): void {
  button.disabled = busy;
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
