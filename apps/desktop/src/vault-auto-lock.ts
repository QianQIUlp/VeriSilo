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

/**
 * Fails closed when cached unlocked UI work reaches its authoritative Vault
 * deadline. This synchronous check covers suspended or throttled WebView
 * timers before any operation can consume cached Vault-derived data.
 */
export function vaultAutoLockDeadlinePassed(
  autoLockAt: string | null,
  nowMs = Date.now(),
): boolean {
  if (autoLockAt === null) {
    return true;
  }
  const deadline = Date.parse(autoLockAt);
  return !Number.isFinite(deadline) || nowMs >= deadline;
}
