import { describe, expect, it } from "vitest";

import type { RuntimeEngineEvidence } from "@verisilo/contracts";

import {
  describeActivation,
  describeEngineCapabilityOperation,
  describeEnginePhaseReceipt,
  describeNetwork,
  describeRuntimeEngineReceipts,
  describeSiteFallbackReceipt,
} from "./formatters.js";

describe("desktop formatters", () => {
  it("labels a terminal runtime failure as blocked instead of running", () => {
    expect(
      describeActivation({
        activeSiloId: "11111111-1111-4111-8111-111111111111",
        state: "verification_failed",
        updatedAt: "2026-07-28T12:00:00.000Z",
        message: null,
        engineEvidence: null,
        networkEvidence: null,
      }),
    ).toContain("已 fail-closed 阻断");
  });

  it("never describes a direct Silo as proxy protected", () => {
    expect(describeNetwork({ mode: "direct", proxyRequired: false })).toContain(
      "直连",
    );
  });

  it("shows phase, fallback, restore, and per-capability receipt state", () => {
    const evidence: RuntimeEngineEvidence = {
      configuredAdapter: "controlled-chromium",
      launchedAdapter: "controlled-chromium",
      verifiedAdapter: "controlled-chromium",
      packageVerification: "verified",
      bootstrapDelivery: "verified",
      runtimeReceipts: "verified",
      restoreReceipt: "not_requested",
      capabilities: [
        {
          id: "canvas",
          availability: "experimental",
          operation: "verified",
          reason: "Controlled package surface.",
          verifiedAt: "2026-07-28T12:00:02.000Z",
          evidence: ["canvas probe matched"],
        },
      ],
      phaseReceipts: ["observe", "apply", "verify"].map((phase, index) => ({
        phase: phase as "observe" | "apply" | "verify",
        recordedAt: `2026-07-28T12:00:0${index}.000Z`,
        capabilities: [
          { id: "canvas", evidence: [`${phase} canvas evidence`] },
        ],
      })),
      fallbackReceipts: [
        {
          site: "login.example.test",
          matchedPattern: "*.example.test",
          action: "restore_then_reload",
          restoredAt: "2026-07-28T12:00:03.000Z",
          capabilities: [{ id: "canvas", evidence: ["compatibility restore"] }],
        },
      ],
    };

    expect(describeEngineCapabilityOperation("verified")).toContain("收据");
    expect(describeRuntimeEngineReceipts(evidence)).toContain(
      "observe → apply → verify",
    );
    expect(describeRuntimeEngineReceipts(evidence)).toContain("站点回退：1 条");
    expect(describeRuntimeEngineReceipts(evidence)).not.toContain("token");
    expect(describeEnginePhaseReceipt(evidence.phaseReceipts[0]!)).toContain(
      "canvas [observe canvas evidence]",
    );
    expect(
      describeSiteFallbackReceipt(evidence.fallbackReceipts[0]!),
    ).toContain("login.example.test ↔ *.example.test");
  });
});
