import { describe, expect, it } from "vitest";

import {
  vaultAutoLockDeadlinePassed,
  vaultAutoLockRefreshDelay,
} from "./vault-auto-lock.js";

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

  it("synchronously rejects cached work at or after the deadline", () => {
    const deadline = "2026-07-28T12:15:00.000Z";
    expect(
      vaultAutoLockDeadlinePassed(
        deadline,
        Date.parse("2026-07-28T12:14:59.999Z"),
      ),
    ).toBe(false);
    expect(vaultAutoLockDeadlinePassed(deadline, Date.parse(deadline))).toBe(
      true,
    );
    expect(
      vaultAutoLockDeadlinePassed(
        deadline,
        Date.parse("2026-07-28T12:15:00.001Z"),
      ),
    ).toBe(true);
  });

  it("fails closed for missing or invalid unlocked deadlines", () => {
    expect(vaultAutoLockDeadlinePassed(null, 0)).toBe(true);
    expect(vaultAutoLockDeadlinePassed("not-a-date", 0)).toBe(true);
  });
});
