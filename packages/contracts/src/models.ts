import { z } from "zod";

export const SCHEMA_VERSION = 1 as const;
export const PROTOCOL_VERSION = 1 as const;

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
  })
  .strict();

const pacNetworkProfileSchema = z
  .object({
    mode: z.literal("pac"),
    proxyRequired: z.boolean(),
    pacUrl: z.string().url().max(2_048),
  })
  .strict();

export const networkProfileSchema = z.discriminatedUnion("mode", [
  directNetworkProfileSchema,
  fixedProxyNetworkProfileSchema,
  pacNetworkProfileSchema,
]);
export type NetworkProfile = z.infer<typeof networkProfileSchema>;

export const siloSchema = z
  .object({
    id: z.string().uuid(),
    schemaVersion: z.literal(SCHEMA_VERSION),
    name: z.string().trim().min(1).max(64),
    color: z.string().regex(/^#[0-9a-fA-F]{6}$/),
    browser: z.object({
      kind: browserKindSchema,
      executablePath: z.string().min(1).max(4_096),
      version: z.string().min(1).max(128).optional(),
    }),
    profileDirectory: z.string().min(1).max(4_096),
    networkProfile: networkProfileSchema,
    seedReference: z.string().uuid(),
    createdAt: z.string().datetime(),
    archivedAt: z.string().datetime().nullable(),
  })
  .strict();
export type Silo = z.infer<typeof siloSchema>;

export const runtimeActivationSchema = z
  .object({
    activeSiloId: z.string().uuid().nullable(),
    state: z.enum([
      "idle",
      "preflight",
      "launching",
      "running",
      "stopped",
      "failed",
    ]),
    updatedAt: z.string().datetime(),
    message: z.string().max(512).optional(),
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
    schemaVersion: z.literal(SCHEMA_VERSION),
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
