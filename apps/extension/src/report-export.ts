import type { ObservationReport } from "@verisilo/contracts";

import { summarizeReport } from "./report-summary.js";
import {
  observedSignalName,
  observedSignalSourceLabel,
  observedSignalStatusLabel,
  signalFailureLabel,
} from "./ui-language.js";
import { translateUiText, type UiLocale } from "./locale.js";

export function redactObservationReport(
  report: ObservationReport,
): ObservationReport {
  return {
    ...report,
    signals: report.signals.map((signal) =>
      signal.sensitivity === "high" && signal.value !== undefined
        ? { ...signal, value: "[默认隐藏]" }
        : signal,
    ),
  };
}

export function reportAsJson(report: ObservationReport): string {
  return `${JSON.stringify(redactObservationReport(report), null, 2)}\n`;
}

export function reportAsHtml(
  report: ObservationReport,
  locale: UiLocale = "zh-CN",
): string {
  const safeReport = redactObservationReport(report);
  const rawHuman = summarizeReport(safeReport);
  const human = {
    ...rawHuman,
    headline: translateUiText(rawHuman.headline, locale),
    description: translateUiText(rawHuman.description, locale),
    facts: rawHuman.facts.map((fact) => ({
      ...fact,
      label: translateUiText(fact.label, locale),
      value: translateUiText(fact.value, locale),
      detail: translateUiText(fact.detail, locale),
    })),
    findings: rawHuman.findings.map((finding) => ({
      ...finding,
      title: translateUiText(finding.title, locale),
      detail: translateUiText(finding.detail, locale),
      action:
        finding.action === undefined
          ? undefined
          : translateUiText(finding.action, locale),
    })),
  };
  const copy = reportCopy(locale);
  const facts = human.facts
    .map(
      (fact) => `
        <article class="fact">
          <span>${escapeHtml(fact.label)}</span>
          <strong>${escapeHtml(fact.value)}</strong>
          <p>${escapeHtml(fact.detail)}</p>
        </article>`,
    )
    .join("");
  const findings = human.findings
    .map(
      (finding) => `
        <article class="finding ${escapeHtml(finding.tone)}">
          <strong>${escapeHtml(finding.title)}</strong>
          <p>${escapeHtml(finding.detail)}</p>
          ${finding.action === undefined ? "" : `<small>${escapeHtml(copy.suggestion)}: ${escapeHtml(finding.action)}</small>`}
        </article>`,
    )
    .join("");
  const rows = safeReport.signals
    .map(
      (signal) => `
        <tr>
          <td>${escapeHtml(translateUiText(observedSignalName(signal.id), locale))}</td>
          <td>${escapeHtml(translateUiText(observedSignalStatusLabel(signal.status), locale))}</td>
          <td>${escapeHtml(translateUiText(observedSignalSourceLabel(signal.source), locale))}</td>
          <td><pre>${escapeHtml(
            signal.status === "error" || signal.status === "unsupported"
              ? translateUiText(signalFailureLabel(signal.error), locale)
              : (JSON.stringify(signal.value) ?? ""),
          )}</pre></td>
        </tr>`,
    )
    .join("");
  return `<!doctype html>
<html lang="${locale}">
  <head>
    <meta charset="utf-8">
    <title>${copy.title}</title>
    <style>
      body { max-width: 960px; font-family: system-ui, sans-serif; margin: 2rem auto; padding: 0 1rem; color: #172036; background: #f6f7fb; }
      header, section, details { border: 1px solid #e1e6ef; border-radius: 1rem; padding: 1.25rem; margin-bottom: 1rem; background: white; }
      .meta, p { color: #667085; line-height: 1.55; }
      .facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: .75rem; }
      .fact, .finding { border: 1px solid #e7eaf1; border-radius: .75rem; padding: .85rem; }
      .fact span, .finding small { display: block; color: #667085; font-size: .8rem; }
      .fact strong { display: block; margin-top: .25rem; }
      .fact p, .finding p { margin-bottom: 0; font-size: .85rem; }
      .findings { display: grid; gap: .65rem; }
      .finding.attention { border-color: #f4d6a4; background: #fffaf0; }
      table { border-collapse: collapse; width: 100%; }
      td, th { border: 1px solid #ccd3e3; padding: .5rem; text-align: left; vertical-align: top; }
      pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
    </style>
  </head>
  <body>
    <header>
      <h1>${escapeHtml(human.headline)}</h1>
      <p>${escapeHtml(human.description)}</p>
      <div class="meta">${copy.site}: ${escapeHtml(safeReport.origin)} · ${copy.collected}: ${escapeHtml(safeReport.collectedAt)}</div>
    </header>
    <section>
      <h2>${copy.keySignals}</h2>
      <div class="facts">${facts}</div>
    </section>
    <section>
      <h2>${copy.findings}</h2>
      <div class="findings">${findings}</div>
    </section>
    <details>
      <summary><strong>${copy.technicalData}</strong></summary>
      <p>${copy.disclosure}</p>
      <table>
        <thead><tr><th>${copy.signal}</th><th>${copy.status}</th><th>${copy.source}</th><th>${copy.value}</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </details>
  </body>
</html>`;
}

function reportCopy(locale: UiLocale) {
  if (locale === "en") {
    return {
      title: "VeriSilo Local Browser Signal Report",
      suggestion: "Suggestion",
      site: "Site",
      collected: "Collected",
      keySignals: "Key browser signals",
      findings: "Worth reviewing",
      technicalData: "Technical data (redacted by default)",
      disclosure:
        "High-sensitivity values are hidden by default. This report does not prove that the device is unrecognizable or that a site cannot link accounts.",
      signal: "Signal",
      status: "Status",
      source: "Source",
      value: "Observed value",
    };
  }
  return {
    title: "VeriSilo 本地浏览器信号观测报告",
    suggestion: "建议",
    site: "网站",
    collected: "采集",
    keySignals: "关键浏览器信号",
    findings: "值得关注",
    technicalData: "技术数据（默认已脱敏）",
    disclosure:
      "高敏感值默认隐藏。本报告不证明设备不可识别，也不代表网站无法关联账号。",
    signal: "信号",
    status: "状态",
    source: "来源",
    value: "观察值",
  };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => {
    const replacements: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return replacements[character] ?? character;
  });
}
