import { z } from "zod";

import {
  engineAdapterIdSchema,
  engineCapabilityEvidenceSchema,
  engineCapabilityStateSchema,
  siloEngineConfigSchema,
} from "./engine";

export const SCHEMA_VERSION = 3 as const;
export const OBSERVATION_REPORT_SCHEMA_VERSION = 1 as const;
export const PROTOCOL_VERSION = 2 as const;

export const browserKindSchema = z.enum(["chrome", "edge"]);
export type BrowserKind = z.infer<typeof browserKindSchema>;

export const capabilityTierSchema = z.enum([
  "reliable",
  "best_effort",
  "unsupported",
]);
export type CapabilityTier = z.infer<typeof capabilityTierSchema>;

export const capabilityControlSchema = z.enum([
  "not_applicable",
  "not_controllable",
  "controlled_by_other_extensions",
  "controllable_by_this_extension",
]);
export type CapabilityControl = z.infer<typeof capabilityControlSchema>;

export const capabilityOperationSchema = z.enum([
  "not_requested",
  "permission_missing",
  "not_controllable",
  "configured",
  "applied",
  "verified",
  "verification_failed",
]);
export type CapabilityOperation = z.infer<typeof capabilityOperationSchema>;

export const runtimeCapabilitySchema = z
  .object({
    id: z.string().min(1).max(80),
    tier: capabilityTierSchema,
    control: capabilityControlSchema,
    operation: capabilityOperationSchema,
    verifiedAt: z.string().datetime().optional(),
    evidence: z.record(z.string(), z.unknown()).optional(),
  })
  .strict()
  .superRefine((capability, context) => {
    if (
      capability.operation === "verified" &&
      (capability.verifiedAt === undefined ||
        capability.evidence === undefined ||
        Object.keys(capability.evidence).length === 0)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          "A verified capability must include a verification timestamp and non-empty evidence.",
        path: ["operation"],
      });
    }
  });
export type RuntimeCapability = z.infer<typeof runtimeCapabilitySchema>;

const directNetworkProfileSchema = z
  .object({
    mode: z.literal("direct"),
    proxyRequired: z.literal(false),
  })
  .strict();

const fixedProxyNetworkProfileSchema = z
  .object({
    mode: z.literal("fixed_proxy"),
    proxyRequired: z.boolean(),
    scheme: z.enum(["http", "https", "socks4", "socks5"]),
    host: z
      .string()
      .min(1)
      .max(253)
      .regex(/^[A-Za-z0-9.:-]+$/),
    port: z.number().int().min(1).max(65535),
    bypassList: z.array(z.string().min(1).max(255)).max(100),
    credentialRef: z.string().uuid().optional(),
    externalMihomo: z
      .object({
        controllerUrl: z.string().url().max(2_048),
        selectorGroup: z.string().trim().min(1).max(128),
        nodeName: z.string().trim().min(1).max(256),
        controllerSecretRef: z.string().uuid().optional(),
      })
      .strict()
      .optional(),
  })
  .strict();

function validateFixedProxyProfile(
  profile: z.infer<typeof fixedProxyNetworkProfileSchema>,
  context: z.RefinementCtx,
): void {
  if (profile.proxyRequired && profile.bypassList.length > 0) {
    context.addIssue({
      code: z.ZodIssueCode.custom,
      message: "A required proxy profile cannot contain direct bypass rules.",
      path: ["bypassList"],
    });
  }
  if (profile.externalMihomo !== undefined) {
    let controller: URL | null = null;
    try {
      controller = new URL(profile.externalMihomo.controllerUrl);
    } catch {
      // The URL schema reports the primary issue.
    }
    const loopbackHttpController =
      controller !== null &&
      controller.protocol === "http:" &&
      ["127.0.0.1", "[::1]"].includes(controller.hostname) &&
      controller.port !== "" &&
      controller.username === "" &&
      controller.password === "" &&
      controller.pathname === "/" &&
      controller.search === "" &&
      controller.hash === "";
    const clashVergePipeController =
      controller !== null &&
      controller.protocol === "pipe:" &&
      controller.hostname === "verge-mihomo" &&
      controller.port === "" &&
      controller.username === "" &&
      controller.password === "" &&
      (controller.pathname === "/" || controller.pathname === "") &&
      controller.search === "" &&
      controller.hash === "";
    if (
      !profile.proxyRequired ||
      profile.scheme !== "socks5" ||
      !["127.0.0.1", "::1"].includes(profile.host) ||
      !(loopbackHttpController || clashVergePipeController)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message:
          "An external Mihomo binding requires a fail-closed loopback SOCKS5 endpoint and a loopback HTTP controller or Clash Verge kernel pipe.",
        path: ["externalMihomo"],
      });
    }
  }
}

const pacNetworkProfileSchema = z
  .object({
    mode: z.literal("pac"),
    proxyRequired: z.boolean(),
    pacUrl: z.string().url().max(2_048),
  })
  .strict();

export const networkProfileSchema = z
  .discriminatedUnion("mode", [
    directNetworkProfileSchema,
    fixedProxyNetworkProfileSchema,
    pacNetworkProfileSchema,
  ])
  .superRefine((profile, context) => {
    if (profile.mode === "fixed_proxy") {
      validateFixedProxyProfile(profile, context);
    }
  });
export type NetworkProfile = z.infer<typeof networkProfileSchema>;

const httpsOriginSchema = z
  .string()
  .url()
  .max(2_048)
  .superRefine((value, context) => {
    let origin: URL;
    try {
      origin = new URL(value);
    } catch {
      return;
    }
    if (
      origin.protocol !== "https:" ||
      origin.username !== "" ||
      origin.password !== "" ||
      origin.pathname !== "/" ||
      origin.search !== "" ||
      origin.hash !== ""
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "A remote execution endpoint must be an HTTPS origin.",
      });
    }
  });

export const siloExecutionTargetSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("local") }).strict(),
  z
    .object({
      kind: z.literal("wsl"),
      distribution: z.string().trim().min(1).max(128),
    })
    .strict(),
  z
    .object({
      kind: z.literal("remote"),
      endpointOrigin: httpsOriginSchema,
    })
    .strict(),
]);
export type SiloExecutionTarget = z.infer<typeof siloExecutionTargetSchema>;

export const proxyCredentialsInputSchema = z
  .object({
    username: z.string().trim().min(1).max(512),
    password: z.string().max(1_024),
  })
  .strict();
export type ProxyCredentialsInput = z.infer<typeof proxyCredentialsInputSchema>;

export const mihomoControllerSecretInputSchema = z
  .object({
    secret: z.string().max(1_024),
  })
  .strict();
export type MihomoControllerSecretInput = z.infer<
  typeof mihomoControllerSecretInputSchema
>;

export const runtimeEvidenceStateSchema = z.enum([
  "not_applicable",
  "not_requested",
  "configured",
  "reachable",
  "applied",
  "observed",
  "verified",
  "failed",
  "unavailable",
]);
export type RuntimeEvidenceState = z.infer<typeof runtimeEvidenceStateSchema>;

export const runtimeNetworkEvidenceSchema = z
  .object({
    runtimeId: z.string().uuid(),
    evidenceId: z.string().uuid(),
    observedAt: z.string().datetime(),
    expiresAt: z.string().datetime().nullable(),
    provenance: z.enum([
      "desktop_control_plane",
      "extension_asserted",
      "relay_observed",
    ]),
    provider: z.enum(["direct", "fixed_proxy", "external_mihomo", "pac"]),
    configuration: runtimeEvidenceStateSchema,
    controllerBinding: runtimeEvidenceStateSchema,
    endpoint: runtimeEvidenceStateSchema,
    authentication: runtimeEvidenceStateSchema,
    authenticationProvenance: z.enum([
      "desktop_control_plane",
      "extension_asserted",
      "relay_observed",
    ]),
    browserRouting: runtimeEvidenceStateSchema,
    exit: runtimeEvidenceStateSchema,
    dns: runtimeEvidenceStateSchema,
    webRtc: runtimeEvidenceStateSchema,
    endpointLabel: z.string().max(512).optional(),
    safeguards: z.array(z.string().min(1).max(160)).max(12),
  })
  .strict();
export type RuntimeNetworkEvidence = z.infer<
  typeof runtimeNetworkEvidenceSchema
>;

const currentSiloSchema = z
  .object({
    id: z.string().uuid(),
    schemaVersion: z.literal(SCHEMA_VERSION),
    name: z.string().trim().min(1).max(64),
    color: z.string().regex(/^#[0-9a-fA-F]{6}$/),
    browser: z
      .object({
        kind: browserKindSchema,
        executablePath: z.string().min(1).max(4_096),
        version: z.string().min(1).max(128).optional(),
      })
      .strict()
      .nullable(),
    profileDirectory: z.string().min(1).max(4_096),
    networkProfile: networkProfileSchema,
    engine: siloEngineConfigSchema.default({ adapter: "stock" }),
    executionTarget: siloExecutionTargetSchema.default({ kind: "local" }),
    identityLockedAt: z.string().datetime().nullable().default(null),
    seedReference: z.string().uuid(),
    createdAt: z.string().datetime(),
    archivedAt: z.string().datetime().nullable(),
  })
  .strict()
  .superRefine((silo, context) => {
    if (silo.engine.adapter === "camoufox" && silo.browser !== null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browser"],
        message:
          "Managed Camoufox Silos must not carry a stock browser descriptor.",
      });
    }
    if (silo.engine.adapter !== "camoufox" && silo.browser === null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["browser"],
        message:
          "Stock and controlled Chromium Silos require a browser descriptor.",
      });
    }
    if (
      silo.engine.adapter === "controlled-chromium" &&
      silo.engine.identityTemplate.network.proxyRequired !==
        silo.networkProfile.proxyRequired
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["engine", "identityTemplate", "network", "proxyRequired"],
        message:
          "The engine identity template must match the Silo proxy requirement.",
      });
    }
  });

function migrateLegacySilo(input: unknown): unknown {
  if (typeof input !== "object" || input === null || Array.isArray(input)) {
    return input;
  }
  const legacy = input as Record<string, unknown>;
  if (legacy.schemaVersion !== 1 && legacy.schemaVersion !== 2) {
    return input;
  }
  const migrated: Record<string, unknown> = {
    ...legacy,
    schemaVersion: SCHEMA_VERSION,
    executionTarget: legacy.executionTarget ?? { kind: "local" },
    identityLockedAt: legacy.identityLockedAt ?? null,
  };
  const engine = migrated.engine;
  if (
    typeof engine === "object" &&
    engine !== null &&
    !Array.isArray(engine) &&
    (engine as Record<string, unknown>).adapter === "camoufox"
  ) {
    const legacyEngine = engine as Record<string, unknown>;
    const artifactBinding = legacyEngine.artifactBinding;
    migrated.browser = null;
    migrated.engine = {
      adapter: "camoufox",
      ...(artifactBinding === undefined || artifactBinding === null
        ? {}
        : { artifactBinding }),
    };
  }
  return migrated;
}

export const siloSchema = z.preprocess(migrateLegacySilo, currentSiloSchema);
export type Silo = z.infer<typeof siloSchema>;

export const browserVerificationSchema = z
  .object({
    state: z.enum([
      "verified",
      "baseline_missing",
      "version_drift",
      "missing",
      "path_changed",
      "kind_mismatch",
      "publisher_mismatch",
      "probe_failed",
    ]),
    expectedKind: browserKindSchema,
    expectedVersion: z.string().nullable(),
    actualVersion: z.string().nullable(),
    executablePath: z.string().min(1).max(32_768),
    checkedAt: z.string().datetime(),
    message: z.string(),
  })
  .strict();
export type BrowserVerification = z.infer<typeof browserVerificationSchema>;

export const engineControlPhaseReceiptSchema = z
  .object({
    phase: z.enum(["observe", "apply", "verify", "restore"]),
    recordedAt: z.string().datetime(),
    capabilities: z.array(engineCapabilityEvidenceSchema).max(17),
  })
  .strict();
export type EngineControlPhaseReceipt = z.infer<
  typeof engineControlPhaseReceiptSchema
>;

export const siteFallbackReceiptSchema = z
  .object({
    site: z.string().trim().min(1).max(253),
    matchedPattern: z.string().trim().min(1).max(255),
    action: z.enum(["restore_experimental_controls", "restore_then_reload"]),
    restoredAt: z.string().datetime(),
    capabilities: z.array(engineCapabilityEvidenceSchema).min(1).max(17),
  })
  .strict();
export type SiteFallbackReceipt = z.infer<typeof siteFallbackReceiptSchema>;

export const runtimePackageVerificationSchema = z
  .object({
    verifierId: z.string().trim().min(1).max(100),
    artifactSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    digestVerified: z.boolean(),
    signatureVerified: z.boolean(),
    packageManifestSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    packageTreeSha256: z
      .string()
      .regex(/^[a-f0-9]{64}$/u)
      .nullable(),
    hostSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    signerCertificateSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    engineRevision: z.string().trim().min(1).max(128).nullable(),
    verifiedAt: z.string().datetime(),
  })
  .strict();
export type RuntimePackageVerification = z.infer<
  typeof runtimePackageVerificationSchema
>;

export const runtimeEngineEvidenceSchema = z
  .object({
    configuredAdapter: engineAdapterIdSchema,
    launchedAdapter: engineAdapterIdSchema.nullable(),
    verifiedAdapter: engineAdapterIdSchema.nullable(),
    packageVerification: runtimeEvidenceStateSchema,
    packageVerificationDetails: runtimePackageVerificationSchema
      .nullable()
      .default(null),
    bootstrapDelivery: runtimeEvidenceStateSchema,
    hostLaunch: runtimeEvidenceStateSchema,
    runtimeReceipts: runtimeEvidenceStateSchema,
    restoreReceipt: runtimeEvidenceStateSchema,
    capabilities: z.array(engineCapabilityStateSchema).max(17),
    phaseReceipts: z.array(engineControlPhaseReceiptSchema).max(4),
    fallbackReceipts: z.array(siteFallbackReceiptSchema).max(128),
  })
  .strict()
  .superRefine((evidence, context) => {
    if (
      evidence.launchedAdapter !== null &&
      evidence.launchedAdapter !== evidence.configuredAdapter
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["launchedAdapter"],
        message: "The launched adapter must match the configured adapter.",
      });
    }
    if (
      evidence.verifiedAdapter !== null &&
      evidence.verifiedAdapter !== evidence.launchedAdapter
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["verifiedAdapter"],
        message:
          "Runtime adapter verification requires a matching launch and protocol evidence.",
      });
    }
    if (
      evidence.packageVerification === "verified" &&
      evidence.packageVerificationDetails !== null &&
      (!evidence.packageVerificationDetails.digestVerified ||
        !evidence.packageVerificationDetails.signatureVerified)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["packageVerificationDetails"],
        message:
          "Verified package evidence must include verified digest and signature details.",
      });
    }
    if (evidence.verifiedAdapter === "controlled-chromium") {
      if (
        evidence.bootstrapDelivery !== "verified" ||
        evidence.runtimeReceipts !== "verified"
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["verifiedAdapter"],
          message:
            "Controlled Chromium verification requires verified bootstrap delivery and runtime receipts as protocol evidence.",
        });
      }
    } else if (evidence.verifiedAdapter === "camoufox") {
      if (
        evidence.packageVerification !== "verified" ||
        evidence.hostLaunch !== "verified" ||
        evidence.bootstrapDelivery !== "not_applicable" ||
        evidence.runtimeReceipts !== "not_applicable"
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["verifiedAdapter"],
          message:
            "Production Camoufox verification requires a verified package and Host launch with bootstrap and runtime receipts not applicable.",
        });
      }
    } else if (evidence.verifiedAdapter !== null) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["verifiedAdapter"],
        message:
          "Only controlled Chromium and production Camoufox can be verified adapters.",
      });
    }
    const phases = evidence.phaseReceipts.map((receipt) => receipt.phase);
    if (
      evidence.runtimeReceipts === "verified" &&
      (phases.length < 3 ||
        phases[0] !== "observe" ||
        phases[1] !== "apply" ||
        phases[2] !== "verify")
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["runtimeReceipts"],
        message:
          "Verified runtime receipts require ordered observe/apply/verify phase receipts.",
      });
    }
    if (
      evidence.restoreReceipt === "verified" &&
      (evidence.runtimeReceipts !== "verified" ||
        phases.at(-1) !== "restore" ||
        evidence.capabilities.some((capability) =>
          ["configured", "applied", "verified"].includes(capability.operation),
        ))
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["restoreReceipt"],
        message:
          "Verified restore requires a final Restore receipt and no capability left active.",
      });
    }
    if (
      new Set(evidence.capabilities.map((capability) => capability.id)).size !==
      evidence.capabilities.length
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["capabilities"],
        message: "Runtime capability identifiers must be unique.",
      });
    }
  });
export type RuntimeEngineEvidence = z.infer<typeof runtimeEngineEvidenceSchema>;

export const runtimeStateSchema = z.enum([
  "idle",
  "preflight",
  "launching",
  "running",
  "verification_failed",
  "recovery_required",
  "stopped",
  "failed",
]);
export type RuntimeState = z.infer<typeof runtimeStateSchema>;

export const runtimeActivationSchema = z
  .object({
    activeSiloId: z.string().uuid().nullable(),
    state: runtimeStateSchema,
    updatedAt: z.string().datetime(),
    message: z.string().nullable(),
    browserVerification: browserVerificationSchema.optional(),
    engineEvidence: runtimeEngineEvidenceSchema.nullable(),
    networkEvidence: runtimeNetworkEvidenceSchema.nullable(),
  })
  .strict();
export type RuntimeActivation = z.infer<typeof runtimeActivationSchema>;

export const vaultStateSchema = z
  .object({
    state: z.enum(["uninitialized", "locked", "unlocked"]),
    autoLockAt: z.string().datetime().nullable(),
  })
  .strict();
export type VaultState = z.infer<typeof vaultStateSchema>;

export const observedSignalSchema = z
  .object({
    id: z.string().min(1).max(100),
    source: z.enum(["window", "iframe", "worker", "header", "extension"]),
    status: z.enum(["ok", "blocked", "unsupported", "error"]),
    stability: z.enum(["stable", "session", "volatile"]),
    sensitivity: z.enum(["low", "medium", "high"]),
    collectedAt: z.string().datetime(),
    durationMs: z.number().finite().min(0).max(60_000),
    value: z.unknown().optional(),
    error: z.string().max(512).optional(),
  })
  .strict();
export type ObservedSignal = z.infer<typeof observedSignalSchema>;

export const observationReportSchema = z
  .object({
    schemaVersion: z.literal(OBSERVATION_REPORT_SCHEMA_VERSION),
    reportId: z.string().uuid(),
    origin: z.string().url(),
    collectedAt: z.string().datetime(),
    coverage: z.object({
      mainWorld: z.enum([
        "not_attempted",
        "observed",
        "partial",
        "unavailable",
      ]),
      worker: z.enum(["self_test_only", "not_attempted"]),
    }),
    signals: z.array(observedSignalSchema).max(100),
  })
  .strict();
export type ObservationReport = z.infer<typeof observationReportSchema>;
