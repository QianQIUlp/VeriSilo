import { describe, expect, it } from "vitest";

import { analyzeConsistency } from "./analysis.js";
import { buildNetworkCheckResult } from "./network-check.js";
import {
  NETWORK_EVIDENCE_COVERAGE,
  NATIVE_MESSAGE_MAX_BYTES,
  nativeResponseSchema,
  parseNativeMessage,
} from "./protocol.js";
import {
  networkProfileSchema,
  OBSERVATION_REPORT_SCHEMA_VERSION,
  PROTOCOL_VERSION,
  runtimeActivationSchema,
  runtimeCapabilitySchema,
  runtimeEngineEvidenceSchema,
  SCHEMA_VERSION,
  siloExecutionTargetSchema,
  siloSchema,
} from "./models.js";

describe("VeriSilo contracts", () => {
  it("migrates a legacy Silo without engine configuration to stock", () => {
    const silo = siloSchema.parse({
      id: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      schemaVersion: 1,
      name: "Legacy",
      color: "#4f46e5",
      browser: {
        kind: "chrome",
        executablePath: "C:/Program Files/Google/Chrome/Application/chrome.exe",
        version: "150.0.0.0",
      },
      profileDirectory: "C:/VeriSilo/silos/legacy/browser-data",
      networkProfile: { mode: "direct", proxyRequired: false },
      seedReference: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
      createdAt: "2026-07-28T00:00:00.000Z",
      archivedAt: null,
    });
    expect(silo.schemaVersion).toBe(SCHEMA_VERSION);
    expect(silo.engine).toEqual({ adapter: "stock" });
    expect(silo.executionTarget).toEqual({ kind: "local" });
    expect(silo.identityLockedAt).toBeNull();
  });

  it("accepts local, WSL, and remote Silo execution targets", () => {
    expect(siloExecutionTargetSchema.parse({ kind: "local" })).toEqual({
      kind: "local",
    });
    expect(
      siloExecutionTargetSchema.parse({
        kind: "wsl",
        distribution: "Ubuntu-24.04",
      }),
    ).toEqual({ kind: "wsl", distribution: "Ubuntu-24.04" });
    expect(
      siloExecutionTargetSchema.parse({
        kind: "remote",
        endpointOrigin: "https://runtime.example.test:8443",
      }),
    ).toEqual({
      kind: "remote",
      endpointOrigin: "https://runtime.example.test:8443",
    });
  });

  it.each([
    "http://runtime.example.test",
    "https://runtime.example.test/agent",
    "https://runtime.example.test?region=sg",
    "https://user:secret@runtime.example.test",
  ])("rejects a non-HTTPS-origin remote endpoint: %s", (endpointOrigin) => {
    expect(() =>
      siloExecutionTargetSchema.parse({ kind: "remote", endpointOrigin }),
    ).toThrow(/HTTPS origin/);
  });

  it("defaults a current Silo to local execution with an unlocked identity", () => {
    const silo = siloSchema.parse({
      id: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      schemaVersion: SCHEMA_VERSION,
      name: "Current",
      color: "#4f46e5",
      browser: {
        kind: "chrome",
        executablePath: "C:/Program Files/Google/Chrome/Application/chrome.exe",
      },
      profileDirectory: "C:/VeriSilo/silos/current/browser-data",
      networkProfile: { mode: "direct", proxyRequired: false },
      seedReference: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
      createdAt: "2026-07-28T00:00:00.000Z",
      archivedAt: null,
    });
    expect(silo.executionTarget).toEqual({ kind: "local" });
    expect(silo.identityLockedAt).toBeNull();
  });

  it("keeps the current Silo contract strict", () => {
    expect(() =>
      siloSchema.parse({
        id: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        schemaVersion: SCHEMA_VERSION,
        name: "Current",
        color: "#4f46e5",
        browser: {
          kind: "chrome",
          executablePath:
            "C:/Program Files/Google/Chrome/Application/chrome.exe",
        },
        profileDirectory: "C:/VeriSilo/silos/current/browser-data",
        networkProfile: { mode: "direct", proxyRequired: false },
        seedReference: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        createdAt: "2026-07-28T00:00:00.000Z",
        archivedAt: null,
        unexpected: true,
      }),
    ).toThrow();
  });

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
        message: null,
        engineEvidence: null,
        networkEvidence: {
          runtimeId: "11111111-1111-4111-8111-111111111111",
          evidenceId: "0f8fad5b-d9cb-469f-a165-70867728950e",
          observedAt: "2026-07-28T00:00:00.000Z",
          expiresAt: null,
          provenance: "desktop_control_plane",
          provider: "fixed_proxy",
          configuration: "configured",
          controllerBinding: "not_applicable",
          endpoint: "reachable",
          authentication: "configured",
          authenticationProvenance: "desktop_control_plane",
          browserRouting: "applied",
          exit: "not_requested",
          dns: "not_requested",
          webRtc: "not_requested",
          safeguards: ["no_direct_fallback"],
        },
      }).networkEvidence?.exit,
    ).toBe("not_requested");
  });

  it("does not treat a verified engine package as runtime adapter verification", () => {
    const evidence = runtimeEngineEvidenceSchema.parse({
      configuredAdapter: "controlled-chromium",
      launchedAdapter: "controlled-chromium",
      verifiedAdapter: null,
      packageVerification: "verified",
      bootstrapDelivery: "applied",
      hostLaunch: "not_applicable",
      runtimeReceipts: "not_requested",
      restoreReceipt: "not_requested",
      capabilities: [],
      phaseReceipts: [],
      fallbackReceipts: [],
    });
    expect(evidence.packageVerification).toBe("verified");
    expect(evidence.verifiedAdapter).toBeNull();
    expect(() =>
      runtimeEngineEvidenceSchema.parse({
        ...evidence,
        verifiedAdapter: "controlled-chromium",
      }),
    ).toThrow(/protocol evidence/);

    const camoufoxEvidence = runtimeEngineEvidenceSchema.parse({
      configuredAdapter: "camoufox",
      launchedAdapter: "camoufox",
      verifiedAdapter: null,
      packageVerification: "verified",
      bootstrapDelivery: "not_applicable",
      hostLaunch: "observed",
      runtimeReceipts: "not_applicable",
      restoreReceipt: "not_applicable",
      capabilities: [],
      phaseReceipts: [],
      fallbackReceipts: [],
    });
    expect(camoufoxEvidence.hostLaunch).toBe("observed");
    expect(() =>
      runtimeEngineEvidenceSchema.parse({
        ...camoufoxEvidence,
        hostLaunch: "verified",
      }),
    ).toThrow(/M3-0 Camoufox Host/);
  });

  it("strictly exposes sanitized ordered per-capability runtime receipts", () => {
    const capability = {
      id: "canvas" as const,
      availability: "experimental" as const,
      operation: "verified" as const,
      reason: "Controlled package surface.",
      verifiedAt: "2026-07-28T12:00:02.000Z",
      evidence: ["canvas probe matched the applied template"],
    };
    const evidence = runtimeEngineEvidenceSchema.parse({
      configuredAdapter: "controlled-chromium",
      launchedAdapter: "controlled-chromium",
      verifiedAdapter: "controlled-chromium",
      packageVerification: "verified",
      bootstrapDelivery: "verified",
      hostLaunch: "not_applicable",
      runtimeReceipts: "verified",
      restoreReceipt: "not_requested",
      capabilities: [capability],
      phaseReceipts: ["observe", "apply", "verify"].map((phase, index) => ({
        phase,
        recordedAt: `2026-07-28T12:00:0${index}.000Z`,
        capabilities: [
          { id: "canvas", evidence: [`${phase} canvas evidence`] },
        ],
      })),
      fallbackReceipts: [],
    });

    expect(evidence.capabilities[0]?.operation).toBe("verified");
    expect(JSON.stringify(evidence)).not.toContain("tokenId");
    expect(() =>
      runtimeEngineEvidenceSchema.parse({
        ...evidence,
        phaseReceipts: [evidence.phaseReceipts[1]],
      }),
    ).toThrow(/ordered observe\/apply\/verify/);
    expect(() =>
      runtimeEngineEvidenceSchema.parse({
        ...evidence,
        tokenId: "11111111-1111-4111-8111-111111111111",
      }),
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
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        type: "handshake",
        cookie: "secret",
      }),
    ).toThrow(/Sensitive browser state/);
  });

  it("rejects unknown and oversized native messages", () => {
    expect(() =>
      parseNativeMessage({
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        type: "handshake",
        unexpected: true,
      }),
    ).toThrow();

    expect(() =>
      parseNativeMessage({
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        type: "handshake",
        padding: "x".repeat(NATIVE_MESSAGE_MAX_BYTES),
      }),
    ).toThrow(/16 KiB/);
  });

  it("accepts only versioned, non-sensitive Native Host responses", () => {
    expect(
      nativeResponseSchema.parse({
        type: "desktop_opened",
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      }).type,
    ).toBe("desktop_opened");

    expect(() =>
      nativeResponseSchema.parse({
        type: "error",
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        code: "unavailable",
        message: "missing protocol version",
      }),
    ).toThrow();
  });

  it("uses the sanitized Native Host runtime snapshot shape", () => {
    const response = nativeResponseSchema.parse({
      type: "runtime_status",
      protocolVersion: PROTOCOL_VERSION,
      requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      snapshotWrittenAt: "2026-07-28T00:00:00.000Z",
      activation: {
        activeSiloId: null,
        state: "idle",
        updatedAt: "2026-07-28T00:00:00.000Z",
        networkEvidence: null,
      },
      vault: { state: "locked", autoLockAt: null },
    });
    expect(response.type).toBe("runtime_status");
    expect(() =>
      nativeResponseSchema.parse({
        ...(response as object),
        activation: {
          ...(response.type === "runtime_status" ? response.activation : {}),
          message: "private desktop detail",
          engineEvidence: null,
        },
      }),
    ).toThrow();
  });

  it("accepts only bounded user-initiated Silo network evidence", () => {
    const networkCheck = buildNetworkCheckResult({
      ipPayload: null,
      cloudflareDnsPayload: null,
      googleDnsPayload: null,
      checkedAt: "2026-07-28T00:00:00.000Z",
      errors: ["Network probes returned no usable answer."],
    });
    expect(
      parseNativeMessage({
        type: "submit_network_evidence",
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
        runtimeId: "11111111-1111-4111-8111-111111111111",
        networkCheck,
        coverage: NETWORK_EVIDENCE_COVERAGE,
      }).type,
    ).toBe("submit_network_evidence");

    expect(() =>
      parseNativeMessage({
        type: "submit_network_evidence",
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
        runtimeId: "11111111-1111-4111-8111-111111111111",
        networkCheck,
        coverage: {
          ...NETWORK_EVIDENCE_COVERAGE,
          actualDnsPath: "verified",
        },
      }),
    ).toThrow();
  });

  it("parses a versioned evidence receipt without claiming verification", () => {
    expect(
      nativeResponseSchema.parse({
        type: "evidence_accepted",
        protocolVersion: PROTOCOL_VERSION,
        requestId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        evidenceId: "0f8fad5b-d9cb-469f-a165-70867728950e",
        acceptedAt: "2026-07-28T00:00:00.000Z",
        expiresAt: "2026-07-28T00:10:00.000Z",
      }).type,
    ).toBe("evidence_accepted");
  });

  it("explains a mobile declaration without touch capability without claiming fraud", () => {
    const findings = analyzeConsistency({
      schemaVersion: OBSERVATION_REPORT_SCHEMA_VERSION,
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
