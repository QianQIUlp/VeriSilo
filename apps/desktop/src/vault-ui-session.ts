import type { VaultState } from "@verisilo/contracts";

import type { DesktopStatus } from "./desktop-api.js";

export type VaultRefreshResult = VaultState["state"] | "stale";

export function acceptedRestoredVaultState(
  result: VaultRefreshResult,
  sessionAcceptsUnlockedWork: boolean,
): VaultState["state"] | null {
  if (result === "stale") {
    return null;
  }
  const resultIsUnlocked = result === "unlocked";
  return resultIsUnlocked === sessionAcceptsUnlockedWork ? result : null;
}

/** Removes Vault-derived runtime detail while retaining the authoritative
 * Vault state needed to render the locked access screen. */
export function scrubDesktopStatusForLockedUi(
  status: DesktopStatus | null,
  updatedAt = new Date().toISOString(),
): DesktopStatus | null {
  if (status === null) {
    return null;
  }
  return {
    vault: status.vault,
    activation: {
      activeSiloId: null,
      state: "idle",
      updatedAt,
      message: null,
      engineEvidence: null,
      networkEvidence: null,
    },
    websiteIdentity: null,
  };
}

/**
 * Tracks the lifetime of data returned while the Vault is unlocked.
 *
 * JavaScript strings cannot be zeroized, but invalidating the epoch lets the
 * UI drop late async results instead of making them reachable again after a
 * lock transition.
 */
export class VaultUiSession {
  private epoch = 0;
  private unlocked = false;

  observe(state: VaultState["state"]): boolean {
    const nextUnlocked = state === "unlocked";
    const invalidated = this.unlocked && !nextUnlocked;
    if (invalidated) {
      this.epoch += 1;
    }
    this.unlocked = nextUnlocked;
    return invalidated;
  }

  invalidate(): void {
    this.epoch += 1;
    this.unlocked = false;
  }

  capture(): number {
    return this.epoch;
  }

  accepts(capturedEpoch: number): boolean {
    return this.unlocked && capturedEpoch === this.epoch;
  }
}
