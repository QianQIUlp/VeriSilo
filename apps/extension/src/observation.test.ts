import { describe, expect, it } from "vitest";

import {
  extensionPageMessageSchema,
  observationReportSchema,
} from "@verisilo/contracts";

describe("extension observation contract", () => {
  it("does not allow reports to omit coverage declarations", () => {
    expect(() =>
      observationReportSchema.parse({
        schemaVersion: 1,
        reportId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
        origin: "https://example.test",
        collectedAt: new Date().toISOString(),
        signals: [],
      }),
    ).toThrow();
  });

  it("accepts the explicit current-site access request", () => {
    expect(
      extensionPageMessageSchema.parse({
        type: "request_current_site_access",
      }),
    ).toEqual({ type: "request_current_site_access" });
    expect(
      extensionPageMessageSchema.parse({
        type: "revoke_current_site_access",
      }),
    ).toEqual({ type: "revoke_current_site_access" });
  });

  it("accepts lightweight isolation controls and rejects unknown fields", () => {
    expect(
      extensionPageMessageSchema.parse({ type: "open_private_workspace" }),
    ).toEqual({ type: "open_private_workspace" });
    expect(
      extensionPageMessageSchema.parse({
        type: "apply_network_prediction_reduction",
      }),
    ).toEqual({ type: "apply_network_prediction_reduction" });
    expect(() =>
      extensionPageMessageSchema.parse({
        type: "open_private_workspace",
        accountCookieJar: "pretend-container",
      }),
    ).toThrow();
  });

  it("accepts only fixed network-check operations", () => {
    expect(
      extensionPageMessageSchema.parse({ type: "run_network_check" }),
    ).toEqual({ type: "run_network_check" });
    expect(() =>
      extensionPageMessageSchema.parse({
        type: "run_network_check",
        endpoint: "https://attacker.invalid/collect",
      }),
    ).toThrow();
  });
});
