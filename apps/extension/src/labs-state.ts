import {
  createDefaultLabsExperiments,
  DEFAULT_LABS_COVERAGE,
  LABS_RECEIPT_TTL_MS,
  LABS_RUNTIME_TTL_MS,
  LABS_SCHEMA_VERSION,
  labsExperimentReceiptSchema,
  labsExperimentSchema,
  labsSiteAuthorizationSchema,
  type LabsCoverage,
  type LabsExperiment,
  type LabsExperimentReceipt,
  type LabsExperimentScope,
  type LabsPhaseReceipt,
  type LabsSiteAuthorization,
  type LabsStopConditionCode,
} from "@verisilo/contracts";

export interface LabsRun {
  experiment: LabsExperiment;
  startedAt: string;
  phases: LabsPhaseReceipt[];
}

export interface LabsVerification {
  constructorWrapped: boolean;
  newWorkerHandshake: boolean;
  sameOriginIframeConsistent: boolean;
  injectionOrder: LabsCoverage["injectionOrder"];
  visibleCookieProbeClear: boolean;
  serviceWorkerUrlProbeClear: boolean;
  crossTabProbeClear: boolean;
}

const leakStopCodes = new Set<LabsStopConditionCode>([
  "cross_tab_canary_leak",
  "iframe_canary_leak",
  "worker_canary_leak",
  "service_worker_canary_leak",
  "cookie_canary_leak",
  "window_canary_leak",
]);

export function defaultWorkerExperiment(now = new Date()): LabsExperiment {
  return createDefaultLabsExperiments(now)[0]!;
}

export function beginWorkerExperiment(input: {
  previous?: LabsExperiment;
  scope: LabsExperimentScope;
  permissionGranted: boolean;
  runId: string;
  now: Date;
}): LabsRun {
  const base = input.previous ?? defaultWorkerExperiment(input.now);
  const updatedAt = input.now.toISOString();
  if (!input.permissionGranted) {
    return {
      experiment: labsExperimentSchema.parse({
        ...base,
        runId: null,
        state: "permission_missing",
        phase: null,
        enabled: false,
        assurance: "unverified",
        scope: input.scope,
        updatedAt,
        expiresAt: null,
        coverage: DEFAULT_LABS_COVERAGE,
      }),
      startedAt: updatedAt,
      phases: [],
    };
  }

  const expiresAt = new Date(
    Math.min(
      input.now.getTime() + LABS_RUNTIME_TTL_MS,
      Date.parse(input.scope.expiresAt),
    ),
  ).toISOString();
  return {
    experiment: labsExperimentSchema.parse({
      ...base,
      runId: input.runId,
      state: "applying",
      phase: "observe",
      enabled: true,
      assurance: "unverified",
      scope: input.scope,
      updatedAt,
      expiresAt,
      coverage: DEFAULT_LABS_COVERAGE,
      lastReceipt: null,
    }),
    startedAt: updatedAt,
    phases: [],
  };
}

export function recordWorkerApplication(
  run: LabsRun,
  input: {
    observed: boolean;
    constructorRestorable: boolean;
    constructorWrapped: boolean;
    now: Date;
  },
): LabsRun {
  const recordedAt = input.now.toISOString();
  const observe: LabsPhaseReceipt = {
    phase: "observe",
    outcome:
      input.observed && input.constructorRestorable ? "passed" : "failed",
    recordedAt,
    evidenceCodes: [
      ...(input.observed ? (["main_world_observed"] as const) : []),
      ...(input.constructorRestorable
        ? (["constructor_restorable"] as const)
        : []),
    ],
  };
  const apply: LabsPhaseReceipt = {
    phase: "apply",
    outcome: input.constructorWrapped ? "passed" : "failed",
    recordedAt,
    evidenceCodes: input.constructorWrapped ? ["constructor_wrapped"] : [],
  };
  return {
    ...run,
    experiment: labsExperimentSchema.parse({
      ...run.experiment,
      phase: "verify",
      updatedAt: recordedAt,
      coverage: {
        ...run.experiment.coverage,
        newDedicatedWorkers: input.constructorWrapped
          ? "same_origin_blob_classic_only"
          : "failed",
      },
    }),
    phases: [...run.phases, observe, apply],
  };
}

export function completeWorkerVerification(
  run: LabsRun,
  verification: LabsVerification,
  now: Date,
): LabsRun {
  const recordedAt = now.toISOString();
  const passed =
    verification.constructorWrapped &&
    verification.newWorkerHandshake &&
    verification.sameOriginIframeConsistent &&
    verification.visibleCookieProbeClear &&
    verification.serviceWorkerUrlProbeClear &&
    verification.crossTabProbeClear;
  const coverage: LabsCoverage = {
    ...run.experiment.coverage,
    injectionOrder: verification.injectionOrder,
    newDedicatedWorkers:
      verification.constructorWrapped && verification.newWorkerHandshake
        ? "same_origin_blob_classic_only"
        : "failed",
    serviceWorkers: "registration_urls_only",
    windowIframe: verification.sameOriginIframeConsistent
      ? "same_origin_probe_passed"
      : "failed",
    cookies: "visible_canary_observation_only",
  };
  const verifyPhase: LabsPhaseReceipt = {
    phase: "verify",
    outcome: passed ? "passed" : "failed",
    recordedAt,
    evidenceCodes: [
      ...(verification.newWorkerHandshake
        ? (["new_worker_handshake"] as const)
        : []),
      ...(verification.sameOriginIframeConsistent
        ? (["same_origin_iframe_consistent"] as const)
        : []),
      ...(verification.visibleCookieProbeClear
        ? (["visible_cookie_probe_clear"] as const)
        : []),
      ...(verification.serviceWorkerUrlProbeClear
        ? (["service_worker_url_probe_clear"] as const)
        : []),
      ...(verification.crossTabProbeClear
        ? (["cross_tab_probe_clear"] as const)
        : []),
      ...(verification.injectionOrder !== "document_start_guaranteed"
        ? (["injection_order_unproven"] as const)
        : []),
    ],
  };
  const nextRun = {
    ...run,
    phases: [...run.phases, verifyPhase],
    experiment: labsExperimentSchema.parse({
      ...run.experiment,
      coverage,
      updatedAt: recordedAt,
    }),
  };
  if (!passed) {
    return stopWorkerExperiment(nextRun, "verification_failed", true, now);
  }

  const state =
    verification.injectionOrder === "document_start_guaranteed"
      ? "verified"
      : "best_effort";
  const assurance = state === "verified" ? "verified" : "best_effort";
  const receipt = buildReceipt({
    run: nextRun,
    state,
    phases: nextRun.phases,
    stopCode: null,
    restoreAttempted: false,
    restoreSucceeded: false,
    coverage,
    now,
  });
  return {
    ...nextRun,
    experiment: labsExperimentSchema.parse({
      ...nextRun.experiment,
      state,
      phase: null,
      assurance,
      updatedAt: recordedAt,
      lastReceipt: receipt,
    }),
  };
}

export function stopWorkerExperiment(
  run: LabsRun,
  stopCode: LabsStopConditionCode,
  restoreSucceeded: boolean,
  now: Date,
): LabsRun {
  const isLeak = leakStopCodes.has(stopCode);
  const state =
    isLeak && restoreSucceeded
      ? "leak_detected"
      : ["user_requested", "expired"].includes(stopCode) && restoreSucceeded
        ? "restored"
        : "failed";
  const restorePhase: LabsPhaseReceipt = {
    phase: "restore",
    outcome: restoreSucceeded ? "passed" : "failed",
    recordedAt: now.toISOString(),
    evidenceCodes: [
      restoreSucceeded ? "restore_confirmed" : "restore_unconfirmed",
    ],
  };
  const phases = [
    ...run.phases.filter((phase) => phase.phase !== "restore"),
    restorePhase,
  ];
  const receipt = buildReceipt({
    run,
    state,
    phases,
    stopCode,
    restoreAttempted: true,
    restoreSucceeded,
    coverage: run.experiment.coverage,
    now,
  });
  return {
    ...run,
    phases,
    experiment: labsExperimentSchema.parse({
      ...run.experiment,
      state,
      phase: "restore",
      enabled: false,
      assurance: "unverified",
      updatedAt: now.toISOString(),
      expiresAt: null,
      lastReceipt: receipt,
    }),
  };
}

export function expireWorkerExperiment(
  run: LabsRun,
  now: Date,
  restoreSucceeded: boolean,
): LabsRun {
  const expiresAt = run.experiment.expiresAt;
  if (expiresAt === null || Date.parse(expiresAt) > now.getTime()) {
    return run;
  }
  return stopWorkerExperiment(run, "expired", restoreSucceeded, now);
}

export function persistentLabsAuthorization(
  scope: LabsExperimentScope,
  enabled: boolean,
  now: Date,
): LabsSiteAuthorization | null {
  if (scope.mode !== "desktop_silo") {
    return null;
  }
  return labsSiteAuthorizationSchema.parse({
    schemaVersion: LABS_SCHEMA_VERSION,
    experimentId: "dedicated_worker_constructor",
    scope,
    enabled,
    updatedAt: now.toISOString(),
  });
}

function buildReceipt(input: {
  run: LabsRun;
  state: LabsExperiment["state"];
  phases: LabsPhaseReceipt[];
  stopCode: LabsStopConditionCode | null;
  restoreAttempted: boolean;
  restoreSucceeded: boolean;
  coverage: LabsCoverage;
  now: Date;
}): LabsExperimentReceipt {
  const scope = input.run.experiment.scope;
  const runId = input.run.experiment.runId;
  if (scope === null || runId === null) {
    throw new Error("A Labs receipt requires an authorized run scope.");
  }
  return labsExperimentReceiptSchema.parse({
    schemaVersion: LABS_SCHEMA_VERSION,
    receiptId: crypto.randomUUID(),
    runId,
    experimentId: input.run.experiment.id,
    state: input.state,
    scope: {
      mode: scope.mode,
      siloId: scope.siloId,
      siteHost: scope.siteHost,
    },
    startedAt: input.run.startedAt,
    finalizedAt: input.now.toISOString(),
    expiresAt: new Date(
      input.now.getTime() + LABS_RECEIPT_TTL_MS,
    ).toISOString(),
    phases: input.phases,
    stopCode: input.stopCode,
    restore: {
      attempted: input.restoreAttempted,
      succeeded: input.restoreSucceeded,
    },
    coverage: input.coverage,
    sanitized: true,
  });
}
