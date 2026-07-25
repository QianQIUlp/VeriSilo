import type { ObservationReport } from "@verisilo/contracts";

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
<html lang="en">
  <head>
    <meta charset="utf-8">
    <title>VeriSilo local observation report</title>
    <style>
      body { font-family: system-ui, sans-serif; margin: 2rem; color: #1f2a44; }
      table { border-collapse: collapse; width: 100%; }
      td, th { border: 1px solid #ccd3e3; padding: .5rem; text-align: left; vertical-align: top; }
      pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
    </style>
  </head>
  <body>
    <h1>VeriSilo local observation report</h1>
    <p>Origin: ${escapeHtml(safeReport.origin)}</p>
    <p>Collected: ${escapeHtml(safeReport.collectedAt)}</p>
    <p>High-sensitivity values are redacted by default. This report does not prove a device identity or detectability outcome.</p>
    <table>
      <thead><tr><th>Signal</th><th>Status</th><th>Source</th><th>Observed value</th></tr></thead>
      <tbody>${rows}</tbody>
    </table>
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
