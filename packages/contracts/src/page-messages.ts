import { z } from "zod";

import { observationReportSchema } from "./models.js";

export const extensionPageMessageSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("scan_current_tab") }).strict(),
  z.object({ type: z.literal("get_current_report") }).strict(),
  z.object({ type: z.literal("request_optional_privacy_permission") }).strict(),
  z.object({ type: z.literal("apply_webrtc_leak_reduction") }).strict(),
  z.object({ type: z.literal("restore_webrtc_leak_reduction") }).strict(),
  z.object({ type: z.literal("open_desktop") }).strict(),
]);
export type ExtensionPageMessage = z.infer<typeof extensionPageMessageSchema>;

export const contentMessageSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("verisilo_observation"),
      report: observationReportSchema,
    })
    .strict(),
]);
export type ContentMessage = z.infer<typeof contentMessageSchema>;
