import type { ObservationReport } from "@verisilo/contracts";

import { summarizeReport } from "./report-summary.js";

export function redactObservationReport(
  report: ObservationReport,
): ObservationReport {
  return {
    ...report,
    signals: report.signals.map((signal) =>
      signal.sensitivity === "high" && signal.value !== undefined
        ? { ...signal, value: "[redacted by default]" }
        : signal,
    ),
  };
}

export function reportAsJson(report: ObservationReport): string {
  return `${JSON.stringify(redactObservationReport(report), null, 2)}\n`;
}

export function reportAsHtml(report: ObservationReport): string {
  const safeReport = redactObservationReport(report);
  const human = summarizeReport(safeReport);
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
          ${finding.action === undefined ? "" : `<small>建议：${escapeHtml(finding.action)}</small>`}
        </article>`,
    )
    .join("");
  const rows = safeReport.signals
    .map(
      (signal) => `
        <tr>
          <td>${escapeHtml(signal.id)}</td>
          <td>${escapeHtml(signal.status)}</td>
          <td>${escapeHtml(signal.source)}</td>
          <td><pre>${escapeHtml(
            signal.error ?? JSON.stringify(signal.value) ?? "",
          )}</pre></td>
        </tr>`,
    )
    .join("");
  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8">
    <title>VeriSilo 本地浏览器信号观测报告</title>
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
      <div class="meta">网站：${escapeHtml(safeReport.origin)} · 采集：${escapeHtml(safeReport.collectedAt)}</div>
    </header>
    <section>
      <h2>关键浏览器信号</h2>
      <div class="facts">${facts}</div>
    </section>
    <section>
      <h2>值得关注</h2>
      <div class="findings">${findings}</div>
    </section>
    <details>
      <summary><strong>技术数据（默认已脱敏）</strong></summary>
      <p>高敏感值默认隐藏。本报告不证明设备不可识别，也不代表网站无法关联账号。</p>
      <table>
        <thead><tr><th>信号</th><th>状态</th><th>来源</th><th>观察值</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </details>
  </body>
</html>`;
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
