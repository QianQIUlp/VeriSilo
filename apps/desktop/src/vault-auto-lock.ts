const MAX_TIMER_DELAY_MS = 2_147_000_000;

/**
 * Returns the delay until the desktop should refresh an Argon2 Vault state.
 * The Rust Vault remains authoritative and rejects expired sensitive calls;
 * this timer only prevents the UI from showing a stale unlocked badge.
 */
export function vaultAutoLockRefreshDelay(
  autoLockAt: string | null,
  nowMs = Date.now(),
): number | null {
  if (autoLockAt === null) {
    return null;
  }
  const deadline = Date.parse(autoLockAt);
  if (!Number.isFinite(deadline)) {
    return 0;
  }
  return Math.min(Math.max(deadline - nowMs, 0), MAX_TIMER_DELAY_MS);
}
