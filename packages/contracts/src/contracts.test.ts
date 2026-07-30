import { describe, expect, it } from "vitest";

import { analyzeConsistency } from "./analysis.js";
import { parseNativeMessage } from "./protocol.js";
import {
  networkProfileSchema,
  runtimeActivationSchema,
  runtimeCapabilitySchema,
} from "./models.js";

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

  it("rejects direct bypass rules on a fail-closed proxy", () => {
    expect(() =>
      networkProfileSchema.parse({
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: "socks5",
        host: "127.0.0.1",
        port: 7890,
        bypassList: ["localhost"],
      }),
    ).toThrow(/bypass/);
  });

  it("accepts only a fail-closed loopback binding for an external Mihomo controller", () => {
    const base = {
      mode: "fixed_proxy" as const,
      proxyRequired: true,
      scheme: "socks5" as const,
      host: "127.0.0.1",
      port: 7890,
      bypassList: [],
      externalMihomo: {
        controllerUrl: "http://127.0.0.1:9090/",
        selectorGroup: "GLOBAL",
        nodeName: "Tokyo 01",
      },
    };
    expect(networkProfileSchema.parse(base)).toEqual(base);
    expect(() =>
      networkProfileSchema.parse({
        ...base,
        externalMihomo: {
          ...base.externalMihomo,
          controllerUrl: "http://192.0.2.20:9090/",
        },
      }),
    ).toThrow(/loopback/);
  });

  it("keeps configured, applied, and verified network stages distinct", () => {
    expect(
      runtimeActivationSchema.parse({
        activeSiloId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        state: "running",
        updatedAt: new Date().toISOString(),
        networkEvidence: {
          provider: "fixed_proxy",
          configuration: "configured",
          controllerBinding: "not_applicable",
          endpoint: "reachable",
          authentication: "configured",
          browserRouting: "applied",
          exit: "not_requested",
          dns: "not_requested",
          webRtc: "not_requested",
          safeguards: ["no_direct_fallback"],
        },
      }).networkEvidence?.exit,
    ).toBe("not_requested");
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
