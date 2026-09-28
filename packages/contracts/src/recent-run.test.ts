import { describe, expect, it } from "vitest";

import { recentRunRecordSchema } from "./models.js";

const SILO = "11111111-1111-4111-8111-111111111111";
const RUNTIME = "22222222-2222-4222-8222-222222222222";
const OTHER = "33333333-3333-4333-8333-333333333333";
const SHA = "a".repeat(64);

const base = {
  siloId: SILO,
  runId: OTHER,
  runtimeId: RUNTIME,
  startedAt: "2026-09-28T00:00:00.000Z",
  updatedAt: "2026-09-28T00:01:00.000Z",
  endedAt: null,
  profileSiloId: SILO,
  artifactBinding: {
    artifactId: "identity-test",
    artifactFileSha256: SHA,
    schema: "verisilo-camoufox-resolved-identity/v6",
  },
  engineAdapter: "camoufox",
  networkPolicy: {
    mode: "direct",
    proxyRequired: false,
    endpointLabel: null,
    externalMihomo: false,
  },
  state: "running",
  reason: null,
  identityEvidence: {
    siloId: SILO,
    runtimeId: RUNTIME,
    sessionId: "session-test",
    artifactId: "identity-test",
    artifactFileSha256: SHA,
    engineAdapter: "camoufox",
    observedAt: "2026-09-28T00:01:00.000Z",
    state: "stale",
    signals: [],
  },
  engineEvidence: null,
  networkEvidence: null,
} as const;

describe("recent run snapshot", () => {
  it("accepts historical stale evidence without promoting its verdict", () => {
    expect(recentRunRecordSchema.parse(base).identityEvidence?.state).toBe("stale");
  });

  it("rejects evidence borrowed from another Silo, runtime or Artifact", () => {
    for (const identityEvidence of [
      { ...base.identityEvidence, siloId: OTHER },
      { ...base.identityEvidence, runtimeId: OTHER },
      { ...base.identityEvidence, artifactId: "identity-other" },
    ]) {
      expect(recentRunRecordSchema.safeParse({ ...base, identityEvidence }).success).toBe(false);
    }
    expect(recentRunRecordSchema.safeParse({ ...base, profileSiloId: OTHER }).success).toBe(false);
  });

  it("rejects network evidence for a different runtime", () => {
    const networkEvidence = {
      runtimeId: OTHER,
      evidenceId: OTHER,
      observedAt: "2026-09-28T00:01:00.000Z",
      expiresAt: null,
      provenance: "desktop_control_plane",
      provider: "direct",
      configuration: "configured",
      controllerBinding: "not_applicable",
      endpoint: "not_applicable",
      authentication: "not_applicable",
      authenticationProvenance: "desktop_control_plane",
      browserRouting: "not_requested",
      exit: "unavailable",
      dns: "unavailable",
      webRtc: "unavailable",
      safeguards: [],
    };
    expect(recentRunRecordSchema.safeParse({ ...base, networkEvidence }).success).toBe(false);
  });

  it("rejects engine evidence declared for another adapter", () => {
    const engineEvidence = {
      configuredAdapter: "stock-edge",
      launchedAdapter: null,
      verifiedAdapter: null,
      packageVerification: "not_applicable",
      packageVerificationDetails: null,
      bootstrapDelivery: "not_applicable",
      hostLaunch: "not_applicable",
      runtimeReceipts: "not_applicable",
      restoreReceipt: "not_applicable",
      capabilities: [],
      phaseReceipts: [],
      fallbackReceipts: [],
    };
    expect(recentRunRecordSchema.safeParse({ ...base, engineEvidence }).success).toBe(false);
  });
});
