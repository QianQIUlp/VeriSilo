import { describe, expect, it } from "vitest";

import { analyzeConsistency } from "./analysis.js";
import { parseNativeMessage } from "./protocol.js";
import { networkProfileSchema, runtimeCapabilitySchema } from "./models.js";

describe("VeriSilo contracts", () => {
  it("accepts an explicit direct network profile", () => {
    expect(
      networkProfileSchema.parse({ mode: "direct", proxyRequired: false }),
    ).toEqual({
      mode: "direct",
      proxyRequired: false,
    });
  });

  it("rejects a direct profile incorrectly marked as proxy required", () => {
    expect(() =>
      networkProfileSchema.parse({ mode: "direct", proxyRequired: true }),
    ).toThrow();
  });

  it("requires runtime evidence before a capability may be called verified", () => {
    expect(() =>
      runtimeCapabilitySchema.parse({
        id: "proxy",
        tier: "reliable",
        control: "controllable_by_this_extension",
        operation: "verified",
      }),
    ).toThrow(/verification timestamp/);

    expect(
      runtimeCapabilitySchema.parse({
        id: "proxy",
        tier: "reliable",
        control: "controllable_by_this_extension",
        operation: "verified",
        verifiedAt: new Date().toISOString(),
        evidence: { endpoint: "https://echo.example.test" },
      }).operation,
    ).toBe("verified");
  });

  it("rejects native messages containing browser-state secrets", () => {
    expect(() =>
      parseNativeMessage({
        protocolVersion: 1,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        type: "handshake",
        cookie: "secret",
      }),
    ).toThrow(/Sensitive browser state/);
  });

  it("explains a mobile declaration without touch capability without claiming fraud", () => {
    const findings = analyzeConsistency({
      schemaVersion: 1,
      reportId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      origin: "https://example.test",
      collectedAt: new Date().toISOString(),
      coverage: { mainWorld: "partial", worker: "self_test_only" },
      signals: [
        {
          id: "navigator",
          source: "window",
          status: "ok",
          stability: "stable",
          sensitivity: "medium",
          collectedAt: new Date().toISOString(),
          durationMs: 1,
          value: { userAgent: "Example Mobile", maxTouchPoints: 0 },
        },
      ],
    });
    expect(findings[0]?.id).toBe("mobile-without-touch");
    expect(findings[0]?.beginnerSummary).toContain("真实设备");
  });
});
