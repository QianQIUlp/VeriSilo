import { z } from "zod";

import { environmentOperationSchema } from "./environment.js";

export const REMOTE_ENVIRONMENT_PROTOCOL_VERSION = 1 as const;
export const REMOTE_ENVIRONMENT_MAX_MESSAGE_BYTES = 64 * 1024;
export const REMOTE_ENVIRONMENT_MAX_PAIRING_TOKEN_LIFETIME_MS = 5 * 60 * 1000;
export const REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS = 30 * 24 * 60 * 60;
export const REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS = 8 * 60 * 60;
export const REMOTE_AGENT_MAX_AUTOMATION_SECONDS = 60 * 60;
export const REMOTE_AGENT_MAX_INPUT_EVENTS = 128;
export const REMOTE_AGENT_MAX_ACTIVITY_ENTRIES = 2_000;

const boundedIdSchema = z.string().trim().min(1).max(128);
const boundedTextSchema = z.string().trim().min(1).max(1_024);
const unixMillisecondsSchema = z.number().int().nonnegative();
const nonceSchema = z.string().regex(/^[A-Za-z0-9_-]{32,128}$/u);
const sha256HexSchema = z.string().regex(/^[a-f0-9]{64}$/u);

export const remoteTlsPinSchema = z
  .object({
    kind: z.enum(["certificate_sha256", "spki_sha256"]),
    sha256: sha256HexSchema,
  })
  .strict()
  .refine(({ sha256 }) => !/^0+$/u.test(sha256), "An all-zero pin is invalid.");
export type RemoteTlsPin = z.infer<typeof remoteTlsPinSchema>;

function isPinnedHttpsOrigin(value: string): boolean {
  try {
    const url = new URL(value);
    return (
      url.protocol === "https:" &&
      url.username === "" &&
      url.password === "" &&
      url.pathname === "/" &&
      url.search === "" &&
      url.hash === "" &&
      url.hostname.length > 0
    );
  } catch {
    return false;
  }
}

/**
 * Only a user-entered, self-hosted HTTPS origin is accepted. The pin is
 * mandatory and is checked again by the native transport after TLS validation.
 */
export const remoteEndpointSchema = z
  .object({
    ownership: z.literal("user_self_hosted"),
    origin: z.string().url().max(2_048).refine(isPinnedHttpsOrigin),
    pin: remoteTlsPinSchema,
  })
  .strict();
export type RemoteEndpoint = z.infer<typeof remoteEndpointSchema>;

export const remoteCapabilitySchema = z
  .object({
    operation: environmentOperationSchema,
    availability: z.discriminatedUnion("availability", [
      z.object({ availability: z.literal("available") }).strict(),
      z
        .object({
          availability: z.literal("unavailable"),
          reason: boundedTextSchema,
        })
        .strict(),
    ]),
  })
  .strict();

export const remoteCapabilitySetSchema = z
  .array(remoteCapabilitySchema)
  .length(environmentOperationSchema.options.length)
  .superRefine((capabilities, context) => {
    for (const operation of environmentOperationSchema.options) {
      if (
        capabilities.filter((capability) => capability.operation === operation)
          .length !== 1
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Remote capability negotiation must describe ${operation} exactly once.`,
        });
      }
    }
  });
export type RemoteCapability = z.infer<typeof remoteCapabilitySchema>;

export const remoteNetworkPolicySchema = z.discriminatedUnion("mode", [
  z.object({ mode: z.literal("direct") }).strict(),
  z
    .object({
      mode: z.literal("fixed_proxy"),
      required: z.boolean(),
      /** An administrator-created server-side policy ID, never a path or command. */
      policyId: z.string().uuid(),
    })
    .strict(),
]);
export type RemoteNetworkPolicy = z.infer<typeof remoteNetworkPolicySchema>;

const boundOperationFields = {
  bindingId: z.string().uuid(),
  remoteEnvironmentId: z.string().uuid(),
} as const;

export const remoteOperationBodySchema = z.discriminatedUnion("operation", [
  z
    .object({
      operation: z.literal("create"),
      network: remoteNetworkPolicySchema,
      ttlSeconds: z
        .number()
        .int()
        .min(60)
        .max(REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS),
      costAcknowledged: z.literal(true),
    })
    .strict(),
  z.object({ operation: z.literal("start"), ...boundOperationFields }).strict(),
  z.object({ operation: z.literal("stop"), ...boundOperationFields }).strict(),
  z.object({ operation: z.literal("pause"), ...boundOperationFields }).strict(),
  z
    .object({ operation: z.literal("snapshot"), ...boundOperationFields })
    .strict(),
  z
    .object({
      operation: z.literal("destroy"),
      ...boundOperationFields,
      /** False is a non-mutating request for an already-persisted proof. */
      confirmDestroy: z.boolean(),
    })
    .strict(),
  z
    .object({
      operation: z.literal("configureNetwork"),
      ...boundOperationFields,
      network: remoteNetworkPolicySchema,
    })
    .strict(),
  z
    .object({ operation: z.literal("health"), ...boundOperationFields })
    .strict(),
  z
    .object({
      operation: z.literal("logs"),
      ...boundOperationFields,
      cursor: z.string().uuid().optional(),
      limit: z.number().int().min(1).max(200),
    })
    .strict(),
]);
export type RemoteOperationBody = z.infer<typeof remoteOperationBodySchema>;

export const remotePairingRequestSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    requestId: z.string().uuid(),
    nonce: nonceSchema,
    sentAtUnixMs: unixMillisecondsSchema,
    body: z
      .object({
        operation: z.literal("pair"),
        approvedByUser: z.literal(true),
        pairingTokenId: z.string().uuid(),
        pairingToken: z
          .string()
          .min(32)
          .max(256)
          .regex(/^[A-Za-z0-9_-]+$/u),
        pairingTokenExpiresAtUnixMs: unixMillisecondsSchema,
      })
      .strict(),
  })
  .strict()
  .superRefine((request, context) => {
    const lifetime =
      request.body.pairingTokenExpiresAtUnixMs - request.sentAtUnixMs;
    if (
      lifetime <= 0 ||
      lifetime > REMOTE_ENVIRONMENT_MAX_PAIRING_TOKEN_LIFETIME_MS
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["body", "pairingTokenExpiresAtUnixMs"],
        message: "Pairing tokens must be live for at most five minutes.",
      });
    }
  });
export type RemotePairingRequest = z.infer<typeof remotePairingRequestSchema>;

export const remotePairingResponseSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    responseId: z.string().uuid(),
    inReplyTo: z.string().uuid(),
    nonce: nonceSchema,
    sentAtUnixMs: unixMillisecondsSchema,
    sequence: z.number().int().positive(),
    body: z.discriminatedUnion("status", [
      z
        .object({
          status: z.literal("success"),
          serverId: z.string().uuid(),
          clientCredentialId: z.lazy(() => nonNilUuidSchema),
          node: z.lazy(() => remoteNodeDisclosureSchema),
          /** Wire-only secret; native integration must move it into local secure storage. */
          clientCredential: z
            .string()
            .min(32)
            .max(512)
            .regex(/^[A-Za-z0-9_-]+$/u),
          credentialExpiresAtUnixMs: unixMillisecondsSchema,
          capabilities: remoteCapabilitySetSchema,
        })
        .strict(),
      z
        .object({
          status: z.literal("rejected"),
          code: z.enum([
            "approval_required",
            "token_expired",
            "token_invalid",
            "replay",
            "limit_exceeded",
          ]),
          message: boundedTextSchema,
        })
        .strict(),
    ]),
  })
  .strict();
export type RemotePairingResponse = z.infer<typeof remotePairingResponseSchema>;

export const remoteOperationRequestSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    requestId: z.string().uuid(),
    nonce: nonceSchema,
    sequence: z.number().int().positive().safe(),
    sentAtUnixMs: unixMillisecondsSchema,
    siloId: z.string().uuid(),
    body: remoteOperationBodySchema,
  })
  .strict();
export type RemoteOperationRequest = z.infer<
  typeof remoteOperationRequestSchema
>;

const evidenceCheckStateSchema = z.enum(["verified", "failed", "unavailable"]);

const boundedNetworkValueSchema = z.string().trim().min(1).max(128);

export const remoteGuestEvidenceSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    evidenceId: z.string().uuid(),
    bindingId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    source: z.literal("guest_agent"),
    sequence: z.number().int().positive(),
    observedAtUnixMs: unixMillisecondsSchema,
    proxy: z
      .object({
        state: z.enum(["not_required", "enforced", "failed", "unavailable"]),
        policyId: z.string().uuid().optional(),
      })
      .strict(),
    exit: z
      .object({
        state: evidenceCheckStateSchema,
        publicAddresses: z.array(boundedNetworkValueSchema).max(16),
      })
      .strict(),
    dns: z
      .object({
        state: evidenceCheckStateSchema,
        resolvers: z.array(boundedNetworkValueSchema).max(16),
        leakDetected: z.boolean(),
      })
      .strict(),
    webRtc: z
      .object({
        state: evidenceCheckStateSchema,
        observedCandidates: z.array(boundedNetworkValueSchema).max(32),
        leakDetected: z.boolean(),
      })
      .strict(),
    health: z
      .object({
        state: z.enum(["healthy", "degraded", "unhealthy"]),
        agentVersion: boundedIdSchema,
        checks: z.array(boundedTextSchema).max(32),
      })
      .strict(),
  })
  .strict();
export type RemoteGuestEvidence = z.infer<typeof remoteGuestEvidenceSchema>;

export const remoteLogEntrySchema = z
  .object({
    sequence: z.number().int().positive(),
    observedAtUnixMs: unixMillisecondsSchema,
    level: z.enum(["debug", "info", "warn", "error"]),
    message: boundedTextSchema,
  })
  .strict();

export const remoteOperationResultSchema = z
  .object({
    operation: environmentOperationSchema,
    siloId: z.string().uuid(),
    bindingId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    serverId: z
      .string()
      .uuid()
      .refine(
        (value) => value !== "00000000-0000-0000-0000-000000000000",
        "A nil UUID is invalid.",
      ),
    lastActivityAtUnixMs: unixMillisecondsSchema.positive(),
    state: z.enum([
      "created",
      "started",
      "stopped",
      "paused",
      "snapshot_created",
      "destroyed",
      "network_configured",
      "healthy",
      "logs_returned",
      "blocked",
    ]),
    volume: z.lazy(() => remoteVolumeAttestationSchema).optional(),
    evidence: remoteGuestEvidenceSchema.optional(),
    logs: z.array(remoteLogEntrySchema).max(200).optional(),
    nextCursor: z.string().uuid().optional(),
    deletionProof: z.lazy(() => remoteDeletionProofSchema).optional(),
  })
  .strict()
  .superRefine((result, context) => {
    const allowedStates: Record<
      typeof result.operation,
      (typeof result.state)[]
    > = {
      create: ["created", "blocked"],
      start: ["started", "blocked"],
      stop: ["stopped"],
      pause: ["paused"],
      snapshot: ["snapshot_created"],
      destroy: ["destroyed"],
      configureNetwork: ["network_configured", "blocked"],
      health: ["healthy", "blocked"],
      logs: ["logs_returned"],
    };
    if (!allowedStates[result.operation].includes(result.state)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["state"],
        message: "Result state must match the requested operation.",
      });
    }
    const evidenceRequired = [
      "create",
      "start",
      "configureNetwork",
      "health",
    ].includes(result.operation);
    if (evidenceRequired && result.evidence === undefined) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["evidence"],
        message: "This operation requires guest evidence.",
      });
    }
    if ((result.operation === "create") !== (result.volume !== undefined)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["volume"],
        message: "Only create results must contain a volume attestation.",
      });
    }
    if (
      (result.operation === "destroy") !==
      (result.deletionProof !== undefined)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["deletionProof"],
        message: "Only destroy results must contain a deletion proof.",
      });
    }
    if (
      result.deletionProof !== undefined &&
      (result.deletionProof.siloId !== result.siloId ||
        result.deletionProof.bindingId !== result.bindingId ||
        result.deletionProof.remoteEnvironmentId !==
          result.remoteEnvironmentId ||
        result.deletionProof.deletedAtUnixMs !== result.lastActivityAtUnixMs)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["deletionProof"],
        message: "Deletion proof must match the result binding.",
      });
    }
    if (
      result.operation !== "logs" &&
      (result.logs !== undefined || result.nextCursor !== undefined)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["logs"],
        message: "Only logs results may contain log fields.",
      });
    }
  });
export type RemoteOperationResult = z.infer<typeof remoteOperationResultSchema>;

export const remoteRejectionCodeSchema = z.enum([
  "not_paired",
  "unauthorized",
  "invalid_state",
  "invalid_request",
  "stale_request",
  "replay",
  "limit_exceeded",
  "proxy_unverified",
]);
export type RemoteRejectionCode = z.infer<typeof remoteRejectionCodeSchema>;

export const remoteOperationResponseSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    responseId: z.string().uuid(),
    inReplyTo: z.string().uuid(),
    nonce: nonceSchema,
    sentAtUnixMs: unixMillisecondsSchema,
    sequence: z.number().int().positive(),
    body: z.discriminatedUnion("status", [
      z
        .object({
          status: z.literal("success"),
          result: remoteOperationResultSchema,
        })
        .strict(),
      z
        .object({
          status: z.literal("unavailable"),
          operation: environmentOperationSchema,
          reason: boundedTextSchema,
        })
        .strict(),
      z
        .object({
          status: z.literal("rejected"),
          code: remoteRejectionCodeSchema,
          message: boundedTextSchema,
        })
        .strict(),
    ]),
  })
  .strict();

export const remoteSiloBindingSchema = z
  .object({
    siloId: z.string().uuid(),
    bindingId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    serverId: z.string().uuid(),
    endpoint: remoteEndpointSchema,
    network: remoteNetworkPolicySchema,
    volume: z.lazy(() => remoteVolumeAttestationSchema),
    lastActivityAtUnixMs: unixMillisecondsSchema.positive(),
    humanSession: z.lazy(() => remoteSessionAuthorizationSchema).optional(),
    automationAuthorizations: z
      .array(z.lazy(() => remoteAutomationAuthorizationSchema))
      .max(128)
      .default([]),
    lastScreenChannel: z.lazy(() => remoteScreenChannelSchema).optional(),
    lastInteraction: z
      .lazy(() => remoteAgentInteractionReceiptSchema)
      .optional(),
    lastEvidence: remoteGuestEvidenceSchema.optional(),
  })
  .strict();
export type RemoteSiloBinding = z.infer<typeof remoteSiloBindingSchema>;

export function requiredRemoteProxyHasGuestEvidence(
  binding: Pick<
    RemoteSiloBinding,
    "bindingId" | "remoteEnvironmentId" | "network"
  >,
  evidence: RemoteGuestEvidence | undefined,
): boolean {
  if (binding.network.mode !== "fixed_proxy" || !binding.network.required) {
    return true;
  }
  return (
    evidence !== undefined &&
    evidence.bindingId === binding.bindingId &&
    evidence.remoteEnvironmentId === binding.remoteEnvironmentId &&
    evidence.source === "guest_agent" &&
    evidence.proxy.state === "enforced" &&
    evidence.proxy.policyId === binding.network.policyId &&
    evidence.exit.state === "verified" &&
    evidence.exit.publicAddresses.length > 0 &&
    evidence.dns.state === "verified" &&
    !evidence.dns.leakDetected &&
    evidence.webRtc.state === "verified" &&
    !evidence.webRtc.leakDetected &&
    evidence.health.state === "healthy"
  );
}

/*
 * Transport-independent Remote Agent domain contract.
 *
 * These schemas mirror the typed core in
 * crates/verisilo-remote-backend/src/agent.rs. They deliberately do not add a
 * transport, provider, shell, filesystem path, executable, or arbitrary URL.
 */

const safeUnsignedIntegerSchema = z.number().int().nonnegative().safe();
const positiveSequenceSchema = z.number().int().positive().safe();

function utf8ByteLength(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function boundedAgentTextSchema(minBytes: number, maxBytes: number) {
  return z.string().superRefine((value, context) => {
    const byteLength = utf8ByteLength(value);
    if (
      value.trim() !== value ||
      byteLength < minBytes ||
      byteLength > maxBytes
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: `Text must contain ${minBytes} to ${maxBytes} UTF-8 bytes without surrounding whitespace.`,
      });
    }
  });
}

const nonNilUuidSchema = z
  .string()
  .uuid()
  .refine(
    (value) => value !== "00000000-0000-0000-0000-000000000000",
    "A nil UUID is not an attestation.",
  );

export const remoteNodeOwnershipSchema = z.literal("user_self_hosted");
export type RemoteNodeOwnership = z.infer<typeof remoteNodeOwnershipSchema>;

export const remoteKeyCustodySchema = z.literal("user_controlled");
export type RemoteKeyCustody = z.infer<typeof remoteKeyCustodySchema>;

export const remoteCostDisclosureSchema = z
  .object({
    currency: z.string().regex(/^[A-Z]{3}$/u),
    estimatedMicrosPerHour: safeUnsignedIntegerSchema,
    notice: boundedAgentTextSchema(1, 500),
  })
  .strict();
export type RemoteCostDisclosure = z.infer<typeof remoteCostDisclosureSchema>;

export const remoteNodeDisclosureSchema = z
  .object({
    nodeId: z.string().uuid(),
    ownership: remoteNodeOwnershipSchema,
    operatorLabel: boundedAgentTextSchema(1, 120),
    dataRegion: boundedAgentTextSchema(2, 120),
    keyCustody: remoteKeyCustodySchema,
    cost: remoteCostDisclosureSchema,
  })
  .strict();
export type RemoteNodeDisclosure = z.infer<typeof remoteNodeDisclosureSchema>;

export const remoteEnvironmentStateSchema = z.enum([
  "created",
  "running",
  "stopped",
  "paused",
  "deleted",
]);
export type RemoteEnvironmentState = z.infer<
  typeof remoteEnvironmentStateSchema
>;

/** Only an affirmative, non-nil, user-controlled volume is an attestation. */
export const remoteVolumeAttestationSchema = z
  .object({
    encrypted: z.literal(true),
    keyCustody: remoteKeyCustodySchema,
    volumeId: nonNilUuidSchema,
    keyId: nonNilUuidSchema,
  })
  .strict();
export type RemoteVolumeAttestation = z.infer<
  typeof remoteVolumeAttestationSchema
>;

export const remoteEnvironmentRecordSchema = z
  .object({
    siloId: z.string().uuid(),
    bindingId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    nodeId: z.string().uuid(),
    state: remoteEnvironmentStateSchema,
    network: remoteNetworkPolicySchema,
    volume: remoteVolumeAttestationSchema,
    createdAtUnixMs: safeUnsignedIntegerSchema,
    expiresAtUnixMs: safeUnsignedIntegerSchema,
    lastActivityAtUnixMs: safeUnsignedIntegerSchema,
    deletionProofId: z.string().uuid().nullable(),
  })
  .strict()
  .superRefine((record, context) => {
    const lifetime = record.expiresAtUnixMs - record.createdAtUnixMs;
    if (
      lifetime < 60 * 1_000 ||
      lifetime > REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS * 1_000
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAtUnixMs"],
        message: "Environment lifetime must be one minute to 30 days.",
      });
    }
    if (record.lastActivityAtUnixMs < record.createdAtUnixMs) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["lastActivityAtUnixMs"],
        message: "Last activity cannot predate environment creation.",
      });
    }
    if ((record.state === "deleted") !== (record.deletionProofId !== null)) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["deletionProofId"],
        message:
          "Deleted environments require a deletion proof and live environments cannot claim one.",
      });
    }
  });
export type RemoteEnvironmentRecord = z.infer<
  typeof remoteEnvironmentRecordSchema
>;

export const remoteDeletionReasonSchema = z.enum([
  "user_confirmed",
  "ttl_expired",
  "provider_policy",
]);
export type RemoteDeletionReason = z.infer<typeof remoteDeletionReasonSchema>;

export const remoteDeletionResourceKindSchema = z.enum([
  "compute_instance",
  "persistent_volume",
  "snapshot",
  "ephemeral_key",
]);
export type RemoteDeletionResourceKind = z.infer<
  typeof remoteDeletionResourceKindSchema
>;

export const remoteDeletionResourceStatusSchema = z.enum([
  "deleted",
  "not_applicable",
]);
export type RemoteDeletionResourceStatus = z.infer<
  typeof remoteDeletionResourceStatusSchema
>;

export const remoteResourceDeletionItemSchema = z
  .object({
    kind: remoteDeletionResourceKindSchema,
    resourceId: nonNilUuidSchema.optional(),
    status: remoteDeletionResourceStatusSchema,
  })
  .strict()
  .superRefine((resource, context) => {
    if (
      (resource.status === "deleted") !==
      (resource.resourceId !== undefined)
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["resourceId"],
        message:
          "Deleted resources require an ID and not-applicable resources must omit it.",
      });
    }
    if (resource.kind !== "snapshot" && resource.status !== "deleted") {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["status"],
        message:
          "Compute, persistent volume and ephemeral key must be deleted.",
      });
    }
  });
export type RemoteResourceDeletionItem = z.infer<
  typeof remoteResourceDeletionItemSchema
>;

const requiredDeletionResourceKinds = remoteDeletionResourceKindSchema.options;

const remoteResourceDeletionSetSchema = z
  .array(remoteResourceDeletionItemSchema)
  .length(requiredDeletionResourceKinds.length)
  .superRefine((resources, context) => {
    for (const kind of requiredDeletionResourceKinds) {
      if (resources.filter((resource) => resource.kind === kind).length !== 1) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: `Deletion receipt must contain ${kind} exactly once.`,
        });
      }
    }
  });

function validateBoundDeletionResources(
  value: {
    remoteEnvironmentId: string;
    volumeId: string;
    resourceDeletions: RemoteResourceDeletionItem[];
  },
  context: z.RefinementCtx,
): void {
  const compute = value.resourceDeletions.find(
    (resource) => resource.kind === "compute_instance",
  );
  const volume = value.resourceDeletions.find(
    (resource) => resource.kind === "persistent_volume",
  );
  if (
    compute?.resourceId !== value.remoteEnvironmentId ||
    volume?.resourceId !== value.volumeId
  ) {
    context.addIssue({
      code: z.ZodIssueCode.custom,
      path: ["resourceDeletions"],
      message:
        "Compute and volume deletion IDs must match the bound environment.",
    });
  }
}

export const remoteProviderDeletionReceiptSchema = z
  .object({
    receiptId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    volumeId: nonNilUuidSchema,
    resourceDeletions: remoteResourceDeletionSetSchema,
  })
  .strict()
  .superRefine(validateBoundDeletionResources);
export type RemoteProviderDeletionReceipt = z.infer<
  typeof remoteProviderDeletionReceiptSchema
>;

export const remoteDeletionProofSchema = z
  .object({
    proofId: z.string().uuid(),
    siloId: z.string().uuid(),
    bindingId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    volumeId: nonNilUuidSchema,
    providerReceiptId: z.string().uuid(),
    resourceDeletions: remoteResourceDeletionSetSchema,
    deletedAtUnixMs: safeUnsignedIntegerSchema,
    reason: remoteDeletionReasonSchema,
  })
  .strict()
  .superRefine(validateBoundDeletionResources);
export type RemoteDeletionProof = z.infer<typeof remoteDeletionProofSchema>;

/**
 * Vault-only controller snapshot. `clientCredential` must never be returned to
 * the WebView, logs, reports, or an unencrypted persistence layer.
 */
export const remotePairingSnapshotSchema = z
  .object({
    serverId: z.string().uuid(),
    clientCredentialId: nonNilUuidSchema,
    node: remoteNodeDisclosureSchema,
    clientCredential: z
      .string()
      .min(32)
      .max(512)
      .regex(/^[A-Za-z0-9_-]+$/u),
    credentialExpiresAtUnixMs: safeUnsignedIntegerSchema.positive(),
    capabilities: remoteCapabilitySetSchema,
    lastClientSequence: safeUnsignedIntegerSchema,
    lastServerSequence: positiveSequenceSchema,
  })
  .strict();
export type RemotePairingSnapshot = z.infer<typeof remotePairingSnapshotSchema>;

export const remoteBackendSnapshotSchema = z
  .object({
    pairing: remotePairingSnapshotSchema.optional(),
    usedPairingTokenIds: z.array(z.string().uuid()).max(4_096),
    bindings: z.array(remoteSiloBindingSchema).max(10_000),
  })
  .strict()
  .superRefine((snapshot, context) => {
    if (
      new Set(snapshot.usedPairingTokenIds).size !==
      snapshot.usedPairingTokenIds.length
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["usedPairingTokenIds"],
        message: "Pairing-token replay IDs must be unique.",
      });
    }
    const siloIds = snapshot.bindings.map((binding) => binding.siloId);
    if (new Set(siloIds).size !== siloIds.length) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["bindings"],
        message: "A Silo may have only one remote binding.",
      });
    }
  });
export type RemoteBackendSnapshot = z.infer<typeof remoteBackendSnapshotSchema>;

export const remoteAutomationScopeSchema = z.enum([
  "read_screen",
  "send_input",
]);
export type RemoteAutomationScope = z.infer<typeof remoteAutomationScopeSchema>;

function authorizationLifetimeIssue(
  issuedAtUnixMs: number,
  expiresAtUnixMs: number,
  maxSeconds: number,
): string | undefined {
  const lifetime = expiresAtUnixMs - issuedAtUnixMs;
  if (lifetime < 60 * 1_000 || lifetime > maxSeconds * 1_000) {
    return `Authorization lifetime must be one minute to ${maxSeconds} seconds.`;
  }
  return undefined;
}

export const remoteSessionAuthorizationSchema = z
  .object({
    authorizationId: z.string().uuid(),
    siloId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    issuedAtUnixMs: safeUnsignedIntegerSchema,
    expiresAtUnixMs: safeUnsignedIntegerSchema,
    revoked: z.boolean(),
  })
  .strict()
  .superRefine((authorization, context) => {
    const message = authorizationLifetimeIssue(
      authorization.issuedAtUnixMs,
      authorization.expiresAtUnixMs,
      REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS,
    );
    if (message !== undefined) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAtUnixMs"],
        message,
      });
    }
  });
export type RemoteSessionAuthorization = z.infer<
  typeof remoteSessionAuthorizationSchema
>;

const uniqueAutomationScopesSchema = z
  .array(remoteAutomationScopeSchema)
  .min(1)
  .max(2)
  .superRefine((scopes, context) => {
    if (new Set(scopes).size !== scopes.length) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Automation scopes must be unique.",
      });
    }
  });

export const remoteAutomationAuthorizationSchema = z
  .object({
    authorizationId: z.string().uuid(),
    siloId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    issuedAtUnixMs: safeUnsignedIntegerSchema,
    expiresAtUnixMs: safeUnsignedIntegerSchema,
    scopes: uniqueAutomationScopesSchema,
    approvedByUser: z.literal(true),
    revoked: z.boolean(),
  })
  .strict()
  .superRefine((authorization, context) => {
    const message = authorizationLifetimeIssue(
      authorization.issuedAtUnixMs,
      authorization.expiresAtUnixMs,
      REMOTE_AGENT_MAX_AUTOMATION_SECONDS,
    );
    if (message !== undefined) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["expiresAtUnixMs"],
        message,
      });
    }
  });
export type RemoteAutomationAuthorization = z.infer<
  typeof remoteAutomationAuthorizationSchema
>;

export const remotePrincipalKindSchema = z.enum([
  "control_plane",
  "human_session",
  "automation",
]);
export type RemotePrincipalKind = z.infer<typeof remotePrincipalKindSchema>;

export const remotePrincipalSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("control_plane"),
      credentialId: z.string().uuid(),
      authorizationId: z.null(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("human_session"),
      credentialId: z.string().uuid(),
      authorizationId: z.string().uuid(),
    })
    .strict(),
  z
    .object({
      kind: z.literal("automation"),
      credentialId: z.string().uuid(),
      authorizationId: z.string().uuid(),
    })
    .strict(),
]);
export type RemotePrincipal = z.infer<typeof remotePrincipalSchema>;

export const remotePointerButtonSchema = z.enum([
  "primary",
  "secondary",
  "middle",
]);
export type RemotePointerButton = z.infer<typeof remotePointerButtonSchema>;

const remoteInputTextSchema = boundedAgentTextSchema(1, 512).superRefine(
  (value, context) => {
    for (const character of value) {
      if (
        character !== "\n" &&
        character !== "\t" &&
        /\p{Cc}/u.test(character)
      ) {
        context.addIssue({
          code: z.ZodIssueCode.custom,
          message: "Input text contains an unsupported control character.",
        });
        return;
      }
    }
  },
);

export const remoteInputEventSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("key"),
      code: z.string().regex(/^[A-Za-z0-9_]{1,40}$/u),
      pressed: z.boolean(),
    })
    .strict(),
  z
    .object({
      type: z.literal("pointer_move"),
      x: z.number().int().min(0).max(16_384),
      y: z.number().int().min(0).max(16_384),
    })
    .strict(),
  z
    .object({
      type: z.literal("pointer_button"),
      button: remotePointerButtonSchema,
      pressed: z.boolean(),
    })
    .strict(),
  z
    .object({
      type: z.literal("text"),
      value: remoteInputTextSchema,
    })
    .strict(),
]);
export type RemoteInputEvent = z.infer<typeof remoteInputEventSchema>;

const remoteInputBatchSchema = z
  .array(remoteInputEventSchema)
  .min(1)
  .max(REMOTE_AGENT_MAX_INPUT_EVENTS);

const siloCommandFields = { siloId: z.string().uuid() } as const;

export const remoteAgentCommandSchema = z
  .discriminatedUnion("operation", [
    z
      .object({
        operation: z.literal("create"),
        siloId: z.string().uuid(),
        bindingId: z.string().uuid(),
        remoteEnvironmentId: z.string().uuid(),
        ttlSeconds: z
          .number()
          .int()
          .min(60)
          .max(REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS),
        network: remoteNetworkPolicySchema,
        costAcknowledged: z.literal(true),
      })
      .strict(),
    z.object({ operation: z.literal("start"), ...siloCommandFields }).strict(),
    z.object({ operation: z.literal("stop"), ...siloCommandFields }).strict(),
    z.object({ operation: z.literal("pause"), ...siloCommandFields }).strict(),
    z
      .object({ operation: z.literal("snapshot"), ...siloCommandFields })
      .strict(),
    z
      .object({
        operation: z.literal("destroy"),
        ...siloCommandFields,
        confirmDestroy: z.literal(true),
      })
      .strict(),
    z
      .object({
        operation: z.literal("configureNetwork"),
        ...siloCommandFields,
        network: remoteNetworkPolicySchema,
      })
      .strict(),
    z.object({ operation: z.literal("health"), ...siloCommandFields }).strict(),
    z
      .object({
        operation: z.literal("logs"),
        ...siloCommandFields,
        limit: z.number().int().min(1).max(200),
      })
      .strict(),
    z
      .object({
        operation: z.literal("openHumanSession"),
        ...siloCommandFields,
        lifetimeSeconds: z
          .number()
          .int()
          .min(60)
          .max(REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS),
      })
      .strict(),
    z
      .object({
        operation: z.literal("closeHumanSession"),
        ...siloCommandFields,
      })
      .strict(),
    z
      .object({
        operation: z.literal("grantAutomation"),
        ...siloCommandFields,
        lifetimeSeconds: z
          .number()
          .int()
          .min(60)
          .max(REMOTE_AGENT_MAX_AUTOMATION_SECONDS),
        scopes: uniqueAutomationScopesSchema,
        approvedByUser: z.literal(true),
      })
      .strict(),
    z
      .object({
        operation: z.literal("revokeAutomation"),
        ...siloCommandFields,
        authorizationId: z.string().uuid(),
      })
      .strict(),
    z
      .object({ operation: z.literal("openScreen"), ...siloCommandFields })
      .strict(),
    z
      .object({
        operation: z.literal("sendInput"),
        ...siloCommandFields,
        events: remoteInputBatchSchema,
      })
      .strict(),
  ])
  .superRefine((command, context) => {
    if (
      command.operation === "grantAutomation" &&
      new Set(command.scopes).size !== command.scopes.length
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["scopes"],
        message: "Automation scopes must be unique.",
      });
    }
  });
export type RemoteAgentCommand = z.infer<typeof remoteAgentCommandSchema>;

const controlPlaneOperations = new Set<RemoteAgentCommand["operation"]>([
  "create",
  "start",
  "stop",
  "pause",
  "snapshot",
  "destroy",
  "configureNetwork",
  "health",
  "logs",
  "openHumanSession",
  "grantAutomation",
  "revokeAutomation",
]);

export const remoteAgentRequestSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    requestId: z.string().uuid(),
    nonce: nonceSchema,
    sequence: positiveSequenceSchema,
    sentAtUnixMs: safeUnsignedIntegerSchema,
    principal: remotePrincipalSchema,
    command: remoteAgentCommandSchema,
  })
  .strict()
  .superRefine((request, context) => {
    const { kind } = request.principal;
    const { operation } = request.command;
    const allowed = controlPlaneOperations.has(operation)
      ? kind === "control_plane"
      : operation === "closeHumanSession"
        ? kind === "human_session"
        : kind === "human_session" || kind === "automation";
    if (!allowed) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["principal", "kind"],
        message: "Principal kind is not authorized for this command class.",
      });
    }

    if (
      utf8ByteLength(JSON.stringify(request)) >
      REMOTE_ENVIRONMENT_MAX_MESSAGE_BYTES
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Remote Agent request exceeds 64 KiB.",
      });
    }
  });
export type RemoteAgentRequest = z.infer<typeof remoteAgentRequestSchema>;

export const remoteScreenTransportSchema = z.literal(
  "authenticated_encrypted_stream",
);
export type RemoteScreenTransport = z.infer<typeof remoteScreenTransportSchema>;

export const remoteScreenChannelSchema = z
  .object({
    channelId: z.string().uuid(),
    remoteEnvironmentId: z.string().uuid(),
    authorizationId: z.string().uuid(),
    expiresAtUnixMs: safeUnsignedIntegerSchema,
    transport: remoteScreenTransportSchema,
  })
  .strict();
export type RemoteScreenChannel = z.infer<typeof remoteScreenChannelSchema>;

export const remoteActivityOperationSchema = z.enum([
  "create",
  "start",
  "stop",
  "pause",
  "snapshot",
  "destroy",
  "configure_network",
  "health",
  "logs",
  "open_human_session",
  "close_human_session",
  "grant_automation",
  "revoke_automation",
  "open_screen",
  "send_input",
]);

export const remoteActivityEntrySchema = z
  .object({
    activityId: z.string().uuid(),
    siloId: z.string().uuid(),
    principal: remotePrincipalKindSchema,
    operation: remoteActivityOperationSchema,
    accepted: z.boolean(),
    occurredAtUnixMs: safeUnsignedIntegerSchema,
  })
  .strict();
export type RemoteActivityEntry = z.infer<typeof remoteActivityEntrySchema>;

export const remoteActivityLogSchema = z
  .array(remoteActivityEntrySchema)
  .max(REMOTE_AGENT_MAX_ACTIVITY_ENTRIES);

const remoteAgentLogTextSchema = z.string().superRefine((value, context) => {
  if (utf8ByteLength(value) > 1_024) {
    context.addIssue({
      code: z.ZodIssueCode.custom,
      message: "Agent log entry exceeds 1,024 UTF-8 bytes.",
    });
  }
});

export const remoteAgentResponseSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("environment"),
      record: remoteEnvironmentRecordSchema,
      evidence: remoteGuestEvidenceSchema.nullable(),
    })
    .strict(),
  z
    .object({
      type: z.literal("deleted"),
      proof: remoteDeletionProofSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("human_session"),
      authorization: remoteSessionAuthorizationSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("automation"),
      authorization: remoteAutomationAuthorizationSchema,
    })
    .strict(),
  z
    .object({ type: z.literal("screen"), channel: remoteScreenChannelSchema })
    .strict(),
  z
    .object({
      type: z.literal("input_accepted"),
      eventCount: z.number().int().min(1).max(REMOTE_AGENT_MAX_INPUT_EVENTS),
    })
    .strict(),
  z
    .object({
      type: z.literal("logs"),
      entries: z.array(remoteAgentLogTextSchema).max(200),
      lastActivityAtUnixMs: safeUnsignedIntegerSchema.positive(),
    })
    .strict(),
]);
export type RemoteAgentResponse = z.infer<typeof remoteAgentResponseSchema>;

export const remoteAgentControlOperationSchema = z.enum([
  "open_human_session",
  "close_human_session",
  "grant_automation",
  "revoke_automation",
  "open_screen",
  "send_input",
]);
export type RemoteAgentControlOperation = z.infer<
  typeof remoteAgentControlOperationSchema
>;

export const remoteAgentInteractionReceiptSchema = z
  .object({
    operation: remoteAgentControlOperationSchema,
    observedAtUnixMs: safeUnsignedIntegerSchema,
    response: remoteAgentResponseSchema,
  })
  .strict();
export type RemoteAgentInteractionReceipt = z.infer<
  typeof remoteAgentInteractionReceiptSchema
>;

export const remoteAgentResponseEnvelopeSchema = z
  .object({
    protocolVersion: z.literal(REMOTE_ENVIRONMENT_PROTOCOL_VERSION),
    responseId: z.string().uuid(),
    inReplyTo: z.string().uuid(),
    nonce: nonceSchema,
    sentAtUnixMs: safeUnsignedIntegerSchema,
    sequence: positiveSequenceSchema,
    body: z.discriminatedUnion("status", [
      z
        .object({
          status: z.literal("success"),
          response: remoteAgentResponseSchema,
        })
        .strict(),
      z
        .object({
          status: z.literal("unavailable"),
          reason: boundedTextSchema,
        })
        .strict(),
      z
        .object({
          status: z.literal("rejected"),
          code: remoteRejectionCodeSchema,
          message: boundedTextSchema,
        })
        .strict(),
    ]),
  })
  .strict()
  .superRefine((response, context) => {
    if (
      utf8ByteLength(JSON.stringify(response)) >
      REMOTE_ENVIRONMENT_MAX_MESSAGE_BYTES
    ) {
      context.addIssue({
        code: z.ZodIssueCode.custom,
        message: "Remote Agent response exceeds 64 KiB.",
      });
    }
  });
export type RemoteAgentResponseEnvelope = z.infer<
  typeof remoteAgentResponseEnvelopeSchema
>;
