import { describe, expect, it } from "vitest";

import {
  acceptedRestoredVaultState,
  scrubDesktopStatusForLockedUi,
  VaultUiSession,
} from "./vault-ui-session.js";

describe("VaultUiSession", () => {
  it("rejects unlocked work after an authoritative lock transition", () => {
    const session = new VaultUiSession();
    expect(session.observe("unlocked")).toBe(false);
    const unlockedEpoch = session.capture();

    expect(session.accepts(unlockedEpoch)).toBe(true);

    expect(session.observe("locked")).toBe(true);

    expect(session.accepts(unlockedEpoch)).toBe(false);
  });

  it("uses a new epoch after the Vault is unlocked again", () => {
    const session = new VaultUiSession();
    expect(session.observe("unlocked")).toBe(false);
    const firstEpoch = session.capture();
    expect(session.observe("locked")).toBe(true);
    expect(session.observe("unlocked")).toBe(false);
    const secondEpoch = session.capture();

    expect(secondEpoch).not.toBe(firstEpoch);
    expect(session.accepts(firstEpoch)).toBe(false);
    expect(session.accepts(secondEpoch)).toBe(true);
  });

  it("can invalidate work at the server-provided auto-lock deadline", () => {
    const session = new VaultUiSession();
    session.observe("unlocked");
    const epoch = session.capture();

    session.invalidate();

    expect(session.accepts(epoch)).toBe(false);
  });

  it("does not create repeated lock transitions while already locked", () => {
    const session = new VaultUiSession();
    session.observe("unlocked");
    expect(session.observe("locked")).toBe(true);
    const lockedEpoch = session.capture();

    expect(session.observe("locked")).toBe(false);
    expect(session.capture()).toBe(lockedEpoch);
  });
});

describe("acceptedRestoredVaultState", () => {
  it("accepts an unlocked state only for an active unlocked epoch", () => {
    expect(acceptedRestoredVaultState("unlocked", true)).toBe("unlocked");
    expect(acceptedRestoredVaultState("unlocked", false)).toBeNull();
  });

  it("accepts locked states only after unlocked work is rejected", () => {
    expect(acceptedRestoredVaultState("locked", false)).toBe("locked");
    expect(acceptedRestoredVaultState("uninitialized", false)).toBe(
      "uninitialized",
    );
    expect(acceptedRestoredVaultState("locked", true)).toBeNull();
  });

  it("never accepts a stale refresh", () => {
    expect(acceptedRestoredVaultState("stale", false)).toBeNull();
    expect(acceptedRestoredVaultState("stale", true)).toBeNull();
  });
});

describe("scrubDesktopStatusForLockedUi", () => {
  it("retains Vault state but drops every runtime identity and evidence field", () => {
    const scrubbed = scrubDesktopStatusForLockedUi(
      {
        vault: {
          state: "locked",
          autoLockAt: null,
        },
        activation: {
          activeSiloId: "5b611c8e-1640-4d80-99da-dd2bb9003302",
          state: "running",
          updatedAt: "2026-07-28T12:00:00.000Z",
          message: "sensitive runtime detail",
          browserVerification: {
            state: "verified",
            expectedKind: "chrome",
            expectedVersion: "1",
            actualVersion: "1",
            executablePath: "C:\\private\\browser.exe",
            checkedAt: "2026-07-28T12:00:00.000Z",
            message: "verified",
          },
          engineEvidence: null,
          networkEvidence: null,
        },
      },
      "2026-07-28T12:15:00.000Z",
    );

    expect(scrubbed).toEqual({
      vault: { state: "locked", autoLockAt: null },
      activation: {
        activeSiloId: null,
        state: "idle",
        updatedAt: "2026-07-28T12:15:00.000Z",
        message: null,
        engineEvidence: null,
        networkEvidence: null,
      },
      websiteIdentity: null,
    });
  });
});
