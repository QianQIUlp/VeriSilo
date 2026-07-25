import { z } from "zod";

import {
  PROTOCOL_VERSION,
  runtimeActivationSchema,
  vaultStateSchema,
} from "./models.js";

const requestBaseSchema = z.object({
  protocolVersion: z.literal(PROTOCOL_VERSION),
  requestId: z.string().uuid(),
});

export const nativeMessageSchema = z.discriminatedUnion("type", [
  requestBaseSchema.extend({ type: z.literal("handshake") }).strict(),
  requestBaseSchema.extend({ type: z.literal("get_runtime_status") }).strict(),
  requestBaseSchema.extend({ type: z.literal("open_desktop") }).strict(),
]);
export type NativeMessage = z.infer<typeof nativeMessageSchema>;

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
      requestId: z.string().uuid(),
      activation: runtimeActivationSchema,
      vault: vaultStateSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("error"),
      requestId: z.string().uuid().optional(),
      code: z.enum(["unauthorized_origin", "invalid_message", "unavailable"]),
      message: z.string().max(200),
    })
    .strict(),
]);
export type NativeResponse = z.infer<typeof nativeResponseSchema>;

const forbiddenKeyPattern =
  /cookie|authorization|password|credential|localstorage|indexeddb/i;

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
  if (hasForbiddenSensitiveKey(value)) {
    throw new Error(
      "Sensitive browser state must never cross the Native Messaging protocol.",
    );
  }

  return nativeMessageSchema.parse(value);
}
