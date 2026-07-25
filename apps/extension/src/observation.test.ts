import { describe, expect, it } from "vitest";

import { observationReportSchema } from "@verisilo/contracts";

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
});
