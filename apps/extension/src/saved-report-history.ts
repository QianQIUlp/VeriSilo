export const SAVED_REPORT_KEY_PREFIX = "saved-report:";
export const MAX_SAVED_REPORTS = 20;
export const SAVED_REPORT_TTL_MS = 30 * 24 * 60 * 60 * 1_000;

const MAX_FUTURE_CLOCK_SKEW_MS = 5 * 60 * 1_000;

export interface SavedReportPrunePlan {
  keptKeys: string[];
  removeKeys: string[];
}

export function shouldPersistSavedReport(
  tabIncognito: boolean | undefined,
): boolean {
  return tabIncognito === false;
}

export function planSavedReportPrune(
  values: Record<string, unknown>,
  nowUnixMs: number,
  reserveSlots = 0,
): SavedReportPrunePlan {
  if (
    !Number.isSafeInteger(nowUnixMs) ||
    nowUnixMs < 0 ||
    !Number.isSafeInteger(reserveSlots) ||
    reserveSlots < 0 ||
    reserveSlots > MAX_SAVED_REPORTS
  ) {
    throw new Error("Saved-report retention inputs are invalid.");
  }

  const valid: Array<{ key: string; savedAtUnixMs: number }> = [];
  const removeKeys: string[] = [];
  for (const [key, value] of Object.entries(values)) {
    if (!key.startsWith(SAVED_REPORT_KEY_PREFIX)) {
      continue;
    }
    const savedAt =
      isRecord(value) && typeof value.savedAt === "string"
        ? Date.parse(value.savedAt)
        : Number.NaN;
    if (
      !Number.isFinite(savedAt) ||
      savedAt > nowUnixMs + MAX_FUTURE_CLOCK_SKEW_MS ||
      nowUnixMs - savedAt >= SAVED_REPORT_TTL_MS
    ) {
      removeKeys.push(key);
      continue;
    }
    valid.push({ key, savedAtUnixMs: savedAt });
  }

  valid.sort(
    (left, right) =>
      right.savedAtUnixMs - left.savedAtUnixMs ||
      left.key.localeCompare(right.key),
  );
  const keepLimit = MAX_SAVED_REPORTS - reserveSlots;
  const kept = valid.slice(0, keepLimit);
  removeKeys.push(...valid.slice(keepLimit).map((entry) => entry.key));
  return {
    keptKeys: kept.map((entry) => entry.key),
    removeKeys: [...new Set(removeKeys)].sort(),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
