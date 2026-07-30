import { describe, expect, it } from "vitest";

import {
  MAX_SAVED_REPORTS,
  planSavedReportPrune,
  SAVED_REPORT_KEY_PREFIX,
  SAVED_REPORT_TTL_MS,
  shouldPersistSavedReport,
} from "./saved-report-history.js";

const now = Date.parse("2026-07-28T12:00:00.000Z");

describe("saved observation report retention", () => {
  it("persists regular-tab reports but not private-tab reports", () => {
    expect(shouldPersistSavedReport(false)).toBe(true);
    expect(shouldPersistSavedReport(true)).toBe(false);
    expect(shouldPersistSavedReport(undefined)).toBe(false);
  });

  it("keeps the newest bounded history while reserving the incoming slot", () => {
    const values = Object.fromEntries(
      Array.from({ length: MAX_SAVED_REPORTS + 5 }, (_, index) => [
        `${SAVED_REPORT_KEY_PREFIX}${String(index).padStart(2, "0")}`,
        { savedAt: new Date(now - index * 1_000).toISOString() },
      ]),
    );
    const plan = planSavedReportPrune(values, now, 1);
    expect(plan.keptKeys).toHaveLength(MAX_SAVED_REPORTS - 1);
    expect(plan.keptKeys[0]).toBe(`${SAVED_REPORT_KEY_PREFIX}00`);
    expect(plan.removeKeys).toHaveLength(6);
  });

  it("removes expired, malformed, and implausibly future records only", () => {
    const plan = planSavedReportPrune(
      {
        unrelated: { savedAt: "not relevant" },
        [`${SAVED_REPORT_KEY_PREFIX}fresh`]: {
          savedAt: new Date(now - SAVED_REPORT_TTL_MS + 1).toISOString(),
        },
        [`${SAVED_REPORT_KEY_PREFIX}expired`]: {
          savedAt: new Date(now - SAVED_REPORT_TTL_MS).toISOString(),
        },
        [`${SAVED_REPORT_KEY_PREFIX}invalid`]: { savedAt: "invalid" },
        [`${SAVED_REPORT_KEY_PREFIX}future`]: {
          savedAt: new Date(now + 10 * 60 * 1_000).toISOString(),
        },
      },
      now,
    );
    expect(plan.keptKeys).toEqual([`${SAVED_REPORT_KEY_PREFIX}fresh`]);
    expect(plan.removeKeys).toEqual([
      `${SAVED_REPORT_KEY_PREFIX}expired`,
      `${SAVED_REPORT_KEY_PREFIX}future`,
      `${SAVED_REPORT_KEY_PREFIX}invalid`,
    ]);
  });

  it("rejects invalid retention arithmetic", () => {
    expect(() => planSavedReportPrune({}, Number.NaN)).toThrow();
    expect(() =>
      planSavedReportPrune({}, now, MAX_SAVED_REPORTS + 1),
    ).toThrow();
  });
});
