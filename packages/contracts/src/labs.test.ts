import { describe, expect, it } from "vitest";

import {
  canTransitionLabsExperimentState,
  createDefaultLabsExperiments,
  DEFAULT_LABS_COVERAGE,
  isLabsExperimentExpired,
  LABS_EXPERIMENT_DEFINITIONS,
  LABS_RECEIPT_TTL_MS,
  labsExperimentReceiptSchema,
  labsExperimentSchema,
} from "./labs.js";

describe("VeriSilo Labs contracts", () => {
  it("keeps every experiment off by default and unsupported cards unavailable", () => {
    const experiments = createDefaultLabsExperiments(
      new Date("2026-07-28T00:00:00.000Z"),
    );

    expect(experiments).toHaveLength(3);
    expect(experiments.every((experiment) => !experiment.enabled)).toBe(true);
    expect(experiments[0]?.state).toBe("disabled");
    expect(experiments.slice(1).map((experiment) => experiment.state)).toEqual([
      "unsupported",
      "unsupported",
    ]);
    expect(
      LABS_EXPERIMENT_DEFINITIONS[0]?.stopConditions.map(
        (condition) => condition.code,
      ),
    ).toContain("expired");
  });

  it("requires an explicit site and Silo binding for an active run", () => {
    const experiment = createDefaultLabsExperiments()[0]!;
    expect(() =>
      labsExperimentSchema.parse({
        ...experiment,
        runId: crypto.randomUUID(),
        state: "applying",
        phase: "apply",
        enabled: true,
        expiresAt: "2026-07-28T00:02:00.000Z",
      }),
    ).toThrow(/scoped authorization/);
  });

  it("does not allow an unsupported experiment to become verified", () => {
    const experiment = createDefaultLabsExperiments()[1]!;
    expect(() =>
      labsExperimentSchema.parse({
        ...experiment,
        state: "verified",
        assurance: "verified",
        enabled: true,
        runId: crypto.randomUUID(),
        coverage: {
          ...DEFAULT_LABS_COVERAGE,
          injectionOrder: "document_start_guaranteed",
        },
      }),
    ).toThrow(/Unsupported Labs experiments/);
  });

  it("rejects verified receipts when injection order is late or unknown", () => {
    const now = Date.parse("2026-07-28T00:00:00.000Z");
    expect(() =>
      labsExperimentReceiptSchema.parse({
        schemaVersion: 1,
        receiptId: crypto.randomUUID(),
        runId: crypto.randomUUID(),
        experimentId: "dedicated_worker_constructor",
        state: "verified",
        scope: {
          mode: "local_temporary",
          siloId: null,
          siteHost: "example.test",
        },
        startedAt: new Date(now).toISOString(),
        finalizedAt: new Date(now + 1_000).toISOString(),
        expiresAt: new Date(now + LABS_RECEIPT_TTL_MS).toISOString(),
        phases: [
          {
            phase: "verify",
            outcome: "passed",
            recordedAt: new Date(now + 1_000).toISOString(),
            evidenceCodes: ["injection_order_unproven"],
          },
        ],
        stopCode: null,
        restore: { attempted: false, succeeded: false },
        coverage: {
          ...DEFAULT_LABS_COVERAGE,
          injectionOrder: "late_or_unknown",
        },
        sanitized: true,
      }),
    ).toThrow(/document-start ordering/);
  });

  it("models automatic restore paths and expiration", () => {
    expect(canTransitionLabsExperimentState("applying", "leak_detected")).toBe(
      true,
    );
    expect(canTransitionLabsExperimentState("leak_detected", "applying")).toBe(
      true,
    );
    expect(canTransitionLabsExperimentState("unsupported", "verified")).toBe(
      false,
    );

    const experiment = {
      ...createDefaultLabsExperiments(new Date("2026-07-28T00:00:00.000Z"))[0]!,
      expiresAt: "2026-07-28T00:01:00.000Z",
    };
    expect(
      isLabsExperimentExpired(
        experiment,
        Date.parse("2026-07-28T00:01:00.000Z"),
      ),
    ).toBe(true);
  });
});
