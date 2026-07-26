import { describe, expect, it } from "vitest";

import type { ObservationReport } from "@verisilo/contracts";

import { redactObservationReport, reportAsHtml } from "./report-export.js";

const report: ObservationReport = {
  schemaVersion: 1,
  reportId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
  origin: "https://example.test",
  collectedAt: "2026-01-01T00:00:00.000Z",
  coverage: { mainWorld: "partial", worker: "self_test_only" },
  signals: [
    {
      id: "canvas_hash",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt: "2026-01-01T00:00:00.000Z",
      durationMs: 1,
      value: "<sensitive>",
    },
  ],
};

describe("report export", () => {
  it("redacts high-sensitivity signal values by default", () => {
    expect(redactObservationReport(report).signals[0]?.value).toBe(
      "[redacted by default]",
    );
  });

  it("escapes report values in the HTML export", () => {
    const html = reportAsHtml(report);
    expect(html).not.toContain("<sensitive>");
    expect(html).toContain("关键身份");
    expect(html).toContain("技术数据（默认已脱敏）");
  });
});
