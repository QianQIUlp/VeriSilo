import { z } from "zod";

import { labsStopConditionCodeSchema } from "./labs.js";
import { observationReportSchema } from "./models.js";

export const extensionPageMessageSchema = z.discriminatedUnion("type", [
  z.object({ type: z.literal("scan_current_tab") }).strict(),
  z.object({ type: z.literal("request_current_site_access") }).strict(),
  z.object({ type: z.literal("get_current_report") }).strict(),
  z.object({ type: z.literal("get_saved_report_history") }).strict(),
  z.object({ type: z.literal("clear_saved_report_history") }).strict(),
  z.object({ type: z.literal("get_network_check") }).strict(),
  z.object({ type: z.literal("run_network_check") }).strict(),
  z.object({ type: z.literal("clear_network_check") }).strict(),
  z.object({ type: z.literal("get_lightweight_isolation_status") }).strict(),
  z.object({ type: z.literal("open_private_workspace") }).strict(),
  z.object({ type: z.literal("request_optional_privacy_permission") }).strict(),
  z.object({ type: z.literal("apply_webrtc_leak_reduction") }).strict(),
  z.object({ type: z.literal("restore_webrtc_leak_reduction") }).strict(),
  z.object({ type: z.literal("apply_network_prediction_reduction") }).strict(),
  z
    .object({ type: z.literal("restore_network_prediction_reduction") })
    .strict(),
  z.object({ type: z.literal("open_desktop") }).strict(),
  z.object({ type: z.literal("get_labs_status") }).strict(),
  z.object({ type: z.literal("enable_dedicated_worker_experiment") }).strict(),
  z.object({ type: z.literal("stop_dedicated_worker_experiment") }).strict(),
  z.object({ type: z.literal("get_labs_receipts") }).strict(),
  z.object({ type: z.literal("clear_labs_receipts") }).strict(),
]);
export type ExtensionPageMessage = z.infer<typeof extensionPageMessageSchema>;

export const contentMessageSchema = z.discriminatedUnion("type", [
  z
    .object({
      type: z.literal("verisilo_observation"),
      report: observationReportSchema,
    })
    .strict(),
  z
    .object({
      type: z.literal("verisilo_labs_stop"),
      runId: z.string().uuid(),
      stopCode: labsStopConditionCodeSchema.extract([
        "page_error",
        "worker_error",
        "worker_canary_leak",
        "timeout",
        "scope_violation",
      ]),
    })
    .strict(),
]);
export type ContentMessage = z.infer<typeof contentMessageSchema>;
