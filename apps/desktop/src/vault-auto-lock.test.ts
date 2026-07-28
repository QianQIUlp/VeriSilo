import { describe, expect, it } from "vitest";

import { vaultAutoLockRefreshDelay } from "./vault-auto-lock.js";

describe("vaultAutoLockRefreshDelay", () => {
  it("schedules the UI refresh at the authoritative deadline", () => {
    expect(
      vaultAutoLockRefreshDelay(
        "2026-07-28T12:15:00.000Z",
        Date.parse("2026-07-28T12:00:00Z"),
      ),
    ).toBe(15 * 60 * 1_000);
  });

  it("refreshes immediately after sleep or for invalid timestamps", () => {
    expect(
      vaultAutoLockRefreshDelay(
        "2026-07-28T12:00:00.000Z",
        Date.parse("2026-07-28T12:01:00Z"),
      ),
    ).toBe(0);
    expect(vaultAutoLockRefreshDelay("not-a-date", 0)).toBe(0);
  });

  it("does not schedule a timer for a locked Vault", () => {
    expect(vaultAutoLockRefreshDelay(null, 0)).toBeNull();
  });
});
