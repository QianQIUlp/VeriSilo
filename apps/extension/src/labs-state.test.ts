import { describe, expect, it } from "vitest";

import {
  LABS_LOCAL_AUTHORIZATION_TTL_MS,
  LABS_SILO_AUTHORIZATION_TTL_MS,
  labsExperimentReceiptSchema,
  type LabsExperimentScope,
} from "@verisilo/contracts";

import {
  beginWorkerExperiment,
  completeWorkerVerification,
  defaultWorkerExperiment,
  expireWorkerExperiment,
  persistentLabsAuthorization,
  recordWorkerApplication,
  stopWorkerExperiment,
} from "./labs-state.js";

const NOW = new Date("2026-07-28T00:00:00.000Z");

function scope(
  mode: "desktop_silo" | "local_temporary" = "desktop_silo",
): LabsExperimentScope {
  const common = {
    tabId: 7,
    siteOrigin: "https://example.test",
    siteHost: "example.test",
    authorizedAt: NOW.toISOString(),
    expiresAt: new Date(
      NOW.getTime() +
        (mode === "desktop_silo"
          ? LABS_SILO_AUTHORIZATION_TTL_MS
          : LABS_LOCAL_AUTHORIZATION_TTL_MS),
    ).toISOString(),
  };
  return mode === "desktop_silo"
    ? {
        ...common,
        mode,
        siloId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
      }
    : { ...common, mode, siloId: null };
}

function applyingRun() {
  return recordWorkerApplication(
    beginWorkerExperiment({
      scope: scope(),
      permissionGranted: true,
      runId: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
      now: NOW,
    }),
    {
      observed: true,
      constructorRestorable: true,
      constructorWrapped: true,
      now: new Date(NOW.getTime() + 100),
    },
  );
}

describe("Labs experiment state machine", () => {
  it("is disabled by default", () => {
    expect(defaultWorkerExperiment(NOW)).toMatchObject({
      state: "disabled",
      enabled: false,
      scope: null,
    });
  });

  it("requires a site permission before applying", () => {
    const run = beginWorkerExperiment({
      scope: scope(),
      permissionGranted: false,
      runId: crypto.randomUUID(),
      now: NOW,
    });
    expect(run.experiment).toMatchObject({
      state: "permission_missing",
      enabled: false,
    });
  });

  it("persists only Silo-bound authorization, never a local temporary gate", () => {
    expect(persistentLabsAuthorization(scope(), true, NOW)).toMatchObject({
      enabled: true,
      scope: { mode: "desktop_silo", siteHost: "example.test" },
    });
    expect(
      persistentLabsAuthorization(scope("local_temporary"), true, NOW),
    ).toBeNull();
  });

  it("auto-restores and disables the site after any detected leak", () => {
    const stopped = stopWorkerExperiment(
      applyingRun(),
      "cross_tab_canary_leak",
      true,
      new Date(NOW.getTime() + 500),
    );
    expect(stopped.experiment).toMatchObject({
      state: "leak_detected",
      enabled: false,
      lastReceipt: {
        stopCode: "cross_tab_canary_leak",
        restore: { attempted: true, succeeded: true },
      },
    });
  });

  it("never records a raw canary, cookie, or token in its receipt", () => {
    const secret = "vsl-secret-cookie-token-123";
    const stopped = stopWorkerExperiment(
      applyingRun(),
      "cookie_canary_leak",
      true,
      new Date(NOW.getTime() + 500),
    );
    const serialized = JSON.stringify(stopped.experiment.lastReceipt);
    expect(serialized).not.toContain(secret);
    expect(serialized).not.toContain("document.cookie=");
    expect(serialized).not.toContain("Authorization");
    expect(serialized).toContain('"sanitized":true');
    expect(
      labsExperimentReceiptSchema.safeParse({
        ...stopped.experiment.lastReceipt,
        canary: secret,
      }).success,
    ).toBe(false);
  });

  it("expires into a restored, disabled state", () => {
    const run = applyingRun();
    const expired = expireWorkerExperiment(
      run,
      new Date(Date.parse(run.experiment.expiresAt!) + 1),
      true,
    );
    expect(expired.experiment).toMatchObject({
      state: "restored",
      enabled: false,
      lastReceipt: { stopCode: "expired" },
    });
  });

  it("reports a successful late injection as best-effort, never verified", () => {
    const completed = completeWorkerVerification(
      applyingRun(),
      {
        constructorWrapped: true,
        newWorkerHandshake: true,
        sameOriginIframeConsistent: true,
        injectionOrder: "late_or_unknown",
        visibleCookieProbeClear: true,
        serviceWorkerUrlProbeClear: true,
        crossTabProbeClear: true,
      },
      new Date(NOW.getTime() + 500),
    );
    expect(completed.experiment).toMatchObject({
      state: "best_effort",
      assurance: "best_effort",
    });
    expect(completed.experiment.state).not.toBe("verified");
  });
});
