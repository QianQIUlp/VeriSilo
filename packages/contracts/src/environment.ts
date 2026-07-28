import { z } from "zod";

export const ENVIRONMENT_CONTRACT_VERSION = 1 as const;

export const environmentBackendIdSchema = z.enum([
  "wsl-chromium",
  "windows-sandbox",
  "hyper-v",
]);
export type EnvironmentBackendId = z.infer<typeof environmentBackendIdSchema>;

export const environmentOperationSchema = z.enum([
  "create",
  "start",
  "stop",
  "pause",
  "snapshot",
  "destroy",
  "configureNetwork",
  "health",
  "logs",
]);
export type EnvironmentOperation = z.infer<typeof environmentOperationSchema>;

export const operationAvailabilitySchema = z.discriminatedUnion(
  "availability",
  [
    z.object({ availability: z.literal("available") }).strict(),
    z
      .object({
        availability: z.literal("unavailable"),
        reason: z.string().trim().min(1).max(500),
      })
      .strict(),
  ],
);
export type OperationAvailability = z.infer<typeof operationAvailabilitySchema>;

export const environmentCapabilitySchema = z
  .object({
    operation: environmentOperationSchema,
    availability: operationAvailabilitySchema,
  })
  .strict();
export type EnvironmentCapability = z.infer<typeof environmentCapabilitySchema>;

export const environmentPrerequisiteSchema = z
  .object({
    id: z.string().trim().min(1).max(80),
    state: z.enum([
      "configured",
      "guest_observed",
      "verified",
      "missing",
      "unavailable",
      "unknown",
    ]),
    detail: z.string().trim().min(1).max(500),
  })
  .strict();
export type EnvironmentPrerequisite = z.infer<
  typeof environmentPrerequisiteSchema
>;

export const environmentBackendStatusSchema = z
  .object({
    contractVersion: z.literal(ENVIRONMENT_CONTRACT_VERSION),
    backend: environmentBackendIdSchema,
    capabilities: z.array(environmentCapabilitySchema).length(9),
    prerequisites: z.array(environmentPrerequisiteSchema).max(32),
  })
  .strict()
  .superRefine((status, context) => {
    const operations = status.capabilities.map(({ operation }) => operation);
    for (const operation of environmentOperationSchema.options) {
      if (
        operations.filter((candidate) => candidate === operation).length !== 1
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          path: ["capabilities"],
          message: `Backend capability negotiation must describe ${operation} exactly once.`,
        });
      }
    }
  });
export type EnvironmentBackendStatus = z.infer<
  typeof environmentBackendStatusSchema
>;

export const environmentActionReceiptSchema = z
  .object({
    backend: environmentBackendIdSchema,
    operation: environmentOperationSchema,
    environmentId: z.string().uuid(),
    state: z.enum([
      "configured",
      "started",
      "stopped",
      "destroyed",
      "healthy",
      "logs_exported",
    ]),
    message: z.string().trim().min(1).max(1_024),
    artifactPath: z.string().min(1).max(4_096).optional(),
  })
  .strict();
export type EnvironmentActionReceipt = z.infer<
  typeof environmentActionReceiptSchema
>;

export const guestEvidenceStateSchema = z.enum([
  "not_requested",
  "configured",
  "verified",
  "failed",
  "unavailable",
]);
export type GuestEvidenceState = z.infer<typeof guestEvidenceStateSchema>;

/**
 * The sole accepted source is the backend's authenticated/fixed guest agent.
 * A desktop/WebView request can never satisfy an environment exit or DNS claim.
 */
export const guestNetworkEvidenceSchema = z
  .object({
    schemaVersion: z.literal(ENVIRONMENT_CONTRACT_VERSION),
    evidenceId: z.string().uuid(),
    environmentId: z.string().uuid(),
    source: z.literal("guest_agent"),
    runtimeId: z.string().uuid(),
    profilePath: z.string().min(1).max(512),
    proxyPort: z.number().int().min(1).max(65_535).nullable(),
    agentSha256: z.string().regex(/^[a-f0-9]{64}$/u),
    proxy: guestEvidenceStateSchema,
    exit: guestEvidenceStateSchema,
    proxyDns: guestEvidenceStateSchema,
    guestResolver: guestEvidenceStateSchema,
    observedAt: z.string().datetime(),
    validUntil: z.string().datetime(),
  })
  .strict();
export type GuestNetworkEvidence = z.infer<typeof guestNetworkEvidenceSchema>;

const environmentDirectNetworkSchema = z
  .object({ mode: z.literal("direct") })
  .strict();

const environmentFixedProxySchema = z
  .object({
    mode: z.literal("fixed_proxy"),
    proxyRequired: z.boolean(),
    scheme: z.enum(["http", "https", "socks5"]),
    host: z
      .string()
      .min(1)
      .max(253)
      .regex(/^[A-Za-z0-9.:[\]-]+$/u),
    port: z.number().int().min(1).max(65_535),
  })
  .strict();

export const environmentNetworkProfileSchema = z.discriminatedUnion("mode", [
  environmentDirectNetworkSchema,
  environmentFixedProxySchema,
]);
export type EnvironmentNetworkProfile = z.infer<
  typeof environmentNetworkProfileSchema
>;

export const environmentOperationRequestSchema = z.discriminatedUnion(
  "operation",
  [
    z
      .object({
        operation: z.literal("create"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
        network: environmentNetworkProfileSchema,
      })
      .strict(),
    z
      .object({
        operation: z.literal("start"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
    z
      .object({
        operation: z.literal("stop"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
    z
      .object({
        operation: z.literal("pause"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
    z
      .object({
        operation: z.literal("snapshot"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
    z
      .object({
        operation: z.literal("destroy"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
        confirmDestroy: z.literal(true),
      })
      .strict(),
    z
      .object({
        operation: z.literal("configureNetwork"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
        network: environmentNetworkProfileSchema,
      })
      .strict(),
    z
      .object({
        operation: z.literal("health"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
    z
      .object({
        operation: z.literal("logs"),
        backend: environmentBackendIdSchema,
        environmentId: z.string().uuid(),
      })
      .strict(),
  ],
);
export type EnvironmentOperationRequest = z.infer<
  typeof environmentOperationRequestSchema
>;

export function requiredProxyHasGuestEvidence(
  environmentId: string,
  network: EnvironmentNetworkProfile,
  evidence: GuestNetworkEvidence | undefined,
  binding: {
    runtimeId: string;
    profilePath: string;
    agentSha256: string;
  },
  now: Date = new Date(),
): boolean {
  if (network.mode !== "fixed_proxy" || !network.proxyRequired) {
    return true;
  }
  const observedAt = Date.parse(evidence?.observedAt ?? "");
  const validUntil = Date.parse(evidence?.validUntil ?? "");
  const nowMilliseconds = now.getTime();
  return (
    evidence !== undefined &&
    evidence.schemaVersion === ENVIRONMENT_CONTRACT_VERSION &&
    evidence.environmentId === environmentId &&
    evidence.source === "guest_agent" &&
    evidence.runtimeId === binding.runtimeId &&
    evidence.profilePath === binding.profilePath &&
    evidence.proxyPort === network.port &&
    evidence.agentSha256.toLowerCase() === binding.agentSha256.toLowerCase() &&
    evidence.proxy === "verified" &&
    evidence.exit === "verified" &&
    evidence.proxyDns === "verified" &&
    evidence.guestResolver === "unavailable" &&
    Number.isFinite(observedAt) &&
    Number.isFinite(validUntil) &&
    observedAt <= nowMilliseconds + 30_000 &&
    observedAt >= nowMilliseconds - 120_000 &&
    validUntil >= nowMilliseconds &&
    validUntil >= observedAt &&
    validUntil <= observedAt + 120_000
  );
}
