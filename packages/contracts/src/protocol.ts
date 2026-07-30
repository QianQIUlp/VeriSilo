import { z } from "zod";

import {
  PROTOCOL_VERSION,
  runtimeNetworkEvidenceSchema,
  runtimeStateSchema,
  vaultStateSchema,
} from "./models.js";
import { networkCheckResultSchema } from "./network-check.js";

export const NATIVE_MESSAGE_MAX_BYTES = 16 * 1024;

export const NETWORK_EVIDENCE_COVERAGE = {
  trigger: "user_initiated",
  transport: "companion_extension_fetch",
  ip: "third_party_https_observation",
  publicDns: "public_doh_answer_comparison",
  actualDnsPath: "not_observed",
  webRtc: "not_observed",
  quic: "not_observed",
} as const;

export const nativeNetworkEvidenceCoverageSchema = z
  .object({
    trigger: z.literal(NETWORK_EVIDENCE_COVERAGE.trigger),
    transport: z.literal(NETWORK_EVIDENCE_COVERAGE.transport),
    ip: z.literal(NETWORK_EVIDENCE_COVERAGE.ip),
    publicDns: z.literal(NETWORK_EVIDENCE_COVERAGE.publicDns),
    actualDnsPath: z.literal(NETWORK_EVIDENCE_COVERAGE.actualDnsPath),
    webRtc: z.literal(NETWORK_EVIDENCE_COVERAGE.webRtc),
    quic: z.literal(NETWORK_EVIDENCE_COVERAGE.quic),
  })
  .strict();
export type NativeNetworkEvidenceCoverage = z.infer<
  typeof nativeNetworkEvidenceCoverageSchema
>;

const requestBaseSchema = z.object({
  protocolVersion: z.literal(PROTOCOL_VERSION),
  requestId: z.string().uuid(),
});

export const nativeMessageSchema = z.discriminatedUnion("type", [
  requestBaseSchema.extend({ type: z.literal("handshake") }).strict(),
  requestBaseSchema.extend({ type: z.literal("get_runtime_status") }).strict(),
  requestBaseSchema.extend({ type: z.literal("open_desktop") }).strict(),
  requestBaseSchema
    .extend({
      type: z.literal("submit_network_evidence"),
      siloId: z.string().uuid(),
      runtimeId: z.string().uuid(),
      networkCheck: networkCheckResultSchema,
      coverage: nativeNetworkEvidenceCoverageSchema,
    })
    .strict(),
]);
export type NativeMessage = z.infer<typeof nativeMessageSchema>;

export const nativeRuntimeSnapshotActivationSchema = z
  .object({
    activeSiloId: z.string().uuid().nullable(),
    state: runtimeStateSchema,
    updatedAt: z.string().datetime(),
    networkEvidence: runtimeNetworkEvidenceSchema.nullable(),
  })
  .strict();
export type NativeRuntimeSnapshotActivation = z.infer<
  typeof nativeRuntimeSnapshotActivationSchema
>;

export const nativeResponseSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("handshake_ack"),
      protocolVersion: z.literal(PROTOCOL_VERSION),
      requestId: z.string().uuid(),
      product: z.literal("VeriSilo"),
    })
    .strict(),
  z
    .object({
      type: z.literal("runtime_status"),
      protocolVersion: z.literal(PROTOCOL_VERSION),
      requestId: z.string().uuid(),
      snapshotWrittenAt: z.string().datetime(),
      activation: nativeRuntimeSnapshotActivationSchema,
      vault: vaultStateSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("desktop_opened"),
      protocolVersion: z.literal(PROTOCOL_VERSION),
      requestId: z.string().uuid(),
    })
    .strict(),
  z
    .object({
      type: z.literal("evidence_accepted"),
      protocolVersion: z.literal(PROTOCOL_VERSION),
      requestId: z.string().uuid(),
      evidenceId: z.string().uuid(),
      acceptedAt: z.string().datetime(),
      expiresAt: z.string().datetime(),
    })
    .strict(),
  z
    .object({
      type: z.literal("error"),
      protocolVersion: z.literal(PROTOCOL_VERSION),
      requestId: z.string().uuid().optional(),
      code: z.enum([
        "unauthorized_origin",
        "invalid_message",
        "unsupported_protocol",
        "unavailable",
        "desktop_unavailable",
        "evidence_rejected",
        "evidence_inbox_full",
      ]),
      message: z.string().max(200),
    })
    .strict(),
]);
export type NativeResponse = z.infer<typeof nativeResponseSchema>;

const forbiddenKeyPattern =
  /authorization|browserdata|cachestorage|cookie|credential|indexeddb|localstorage|password|passphrase|profiledata|secret|seed|sessionstorage|token|vaultdata/i;

export function hasForbiddenSensitiveKey(value: unknown): boolean {
  if (Array.isArray(value)) {
    return value.some(hasForbiddenSensitiveKey);
  }

  if (value !== null && typeof value === "object") {
    return Object.entries(value as Record<string, unknown>).some(
      ([key, nested]) =>
        forbiddenKeyPattern.test(key) || hasForbiddenSensitiveKey(nested),
    );
  }

  return false;
}

export function parseNativeMessage(value: unknown): NativeMessage {
  let serialized: string | undefined;
  try {
    serialized = JSON.stringify(value);
  } catch {
    throw new Error("Native Messaging input must be valid JSON.");
  }
  if (
    serialized === undefined ||
    new TextEncoder().encode(serialized).byteLength > NATIVE_MESSAGE_MAX_BYTES
  ) {
    throw new Error("Native Messaging input exceeds the 16 KiB limit.");
  }

  if (hasForbiddenSensitiveKey(value)) {
    throw new Error(
      "Sensitive browser state must never cross the Native Messaging protocol.",
    );
  }

  return nativeMessageSchema.parse(value);
}
