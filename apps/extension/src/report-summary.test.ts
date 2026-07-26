import { describe, expect, it } from "vitest";

import type { ObservationReport } from "@verisilo/contracts";

import { summarizeReport } from "./report-summary.js";

const collectedAt = "2026-07-26T00:00:00.000Z";
const report: ObservationReport = {
  schemaVersion: 1,
  reportId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
  origin: "https://example.test",
  collectedAt,
  coverage: { mainWorld: "observed", worker: "self_test_only" },
  signals: [
    {
      id: "navigator",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: {
        userAgent:
          "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/150.0.0.0 Safari/537.36 Edg/150.0.0.0",
        platform: "Win32",
        language: "zh-CN",
        deviceMemory: 16,
        hardwareConcurrency: 16,
        maxTouchPoints: 0,
      },
    },
    {
      id: "timezone",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: "Asia/Singapore",
    },
    {
      id: "ua_ch",
      source: "window",
      status: "ok",
      stability: "stable",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { highEntropy: { bitness: "64" } },
    },
    {
      id: "screen",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { width: 1707, height: 960, pixelRatio: 1.5 },
    },
    {
      id: "webgl",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: {
        renderer:
          "ANGLE (NVIDIA, NVIDIA GeForce RTX 4060 Laptop GPU (0x000028E0) Direct3D11)",
      },
    },
    {
      id: "storage",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: {
        cookiesEnabled: true,
        localStorage: true,
        indexedDb: true,
      },
    },
    {
      id: "permissions",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "medium",
      collectedAt,
      durationMs: 1,
      value: { geolocation: "denied", notifications: "granted" },
    },
    {
      id: "media_devices",
      source: "window",
      status: "ok",
      stability: "session",
      sensitivity: "high",
      collectedAt,
      durationMs: 1,
      value: { labelsExposed: true },
    },
  ],
};

describe("human-readable report summary", () => {
  it("turns raw signals into beginner-facing facts", () => {
    const summary = summarizeReport(report);
    expect(summary.facts[0]?.value).toContain("Microsoft Edge 150");
    expect(summary.facts[1]?.value).toContain("Asia/Singapore");
    expect(summary.description).toContain(`${report.signals.length} 组`);
    expect(summary.description).toContain("条解读");
    expect(summary.facts.find((fact) => fact.id === "network")?.value).toBe(
      "本次未验证",
    );
    expect(summary.facts.find((fact) => fact.id === "site-state")?.value).toBe(
      "会保留登录状态",
    );
    expect(summary.facts.find((fact) => fact.id === "graphics")?.value).toBe(
      "NVIDIA GeForce RTX 4060 Laptop GPU",
    );
  });

  it("surfaces exposed media labels as an actionable warning", () => {
    const summary = summarizeReport(report);
    expect(summary.tone).toBe("attention");
    expect(
      summary.findings.some((finding) => finding.id === "media-labels-exposed"),
    ).toBe(true);
    expect(
      summary.findings.some(
        (finding) => finding.id === "notifications-granted",
      ),
    ).toBe(true);
  });
});
