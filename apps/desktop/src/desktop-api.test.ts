import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { desktopApi } from "./desktop-api.js";

describe("desktop lifecycle command contract", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it("uses camelCase command arguments for Silo lifecycle operations", async () => {
    const update = {
      name: "Work",
      color: "#5b5ce2",
      browserKind: "edge" as const,
      executablePath: "C:\\Program Files\\Edge\\msedge.exe",
    };
    const network = {
      networkProfile: {
        mode: "direct" as const,
        proxyRequired: false as const,
      },
    };

    await desktopApi.updateSilo("silo-1", update);
    await desktopApi.renameSilo("silo-1", "Renamed");
    await desktopApi.updateSiloNetwork("silo-1", network);
    await desktopApi.archiveSilo("silo-1");
    await desktopApi.restoreArchivedSilo("silo-1");
    await desktopApi.deleteSilo("silo-1");
    await desktopApi.siloStorageUsage("silo-1");
    await desktopApi.listNetworkEvidence("silo-1");
    await desktopApi.listNetworkEvidence();
    await desktopApi.clearNetworkEvidence("silo-1");

    expect(invokeMock.mock.calls).toEqual([
      ["update_silo", { siloId: "silo-1", input: update }],
      ["rename_silo", { siloId: "silo-1", name: "Renamed" }],
      ["update_silo_network", { siloId: "silo-1", input: network }],
      ["archive_silo", { siloId: "silo-1" }],
      ["restore_archived_silo", { siloId: "silo-1" }],
      ["delete_silo", { siloId: "silo-1", confirmPermanent: true }],
      ["silo_storage_usage", { siloId: "silo-1" }],
      ["list_network_evidence", { siloId: "silo-1" }],
      ["list_network_evidence", { siloId: null }],
      ["clear_network_evidence", { siloId: "silo-1", confirmClear: true }],
    ]);
  });

  it("uses the bounded managed-browser creation payload", async () => {
    const input = {
      name: "Private work",
      color: "#5b5ce2",
      identityPreset: "balanced-zh-cn" as const,
      followNetworkExit: true,
      screenWidth: 1920,
      screenHeight: 1080,
      hardwareConcurrency: 8,
      networkProfile: {
        mode: "fixed_proxy" as const,
        proxyRequired: true as const,
        scheme: "socks5" as const,
        host: "127.0.0.1",
        port: 7890,
        bypassList: [] as [],
      },
      proxyCredentials: { username: "user", password: "pass" },
    };

    await desktopApi.createManagedSilo(input);

    expect(invokeMock.mock.calls).toEqual([["create_managed_silo", { input }]]);
  });

  it("uses explicit camelCase arguments for Vault mutations", async () => {
    await desktopApi.changeVaultPassphrase("old passphrase", "new passphrase");
    await desktopApi.backupVault("C:\\Backups\\vault.backup");
    await desktopApi.restoreVault(
      "C:\\Backups\\vault.backup",
      "backup passphrase",
      true,
    );

    expect(invokeMock.mock.calls).toEqual([
      [
        "change_vault_passphrase",
        {
          currentPassphrase: "old passphrase",
          newPassphrase: "new passphrase",
        },
      ],
      ["backup_vault", { destinationPath: "C:\\Backups\\vault.backup" }],
      [
        "restore_vault",
        {
          sourcePath: "C:\\Backups\\vault.backup",
          passphrase: "backup passphrase",
          confirmOverwrite: true,
        },
      ],
    ]);
  });

  it("keeps pairing approval and create cost consent explicit", async () => {
    const endpoint = {
      ownership: "user_self_hosted" as const,
      origin: "https://remote.example.test/",
      pin: {
        kind: "spki_sha256" as const,
        sha256: "a".repeat(64),
      },
    };
    const approval = {
      approvedByUser: true,
      pairingTokenId: "748b1e8d-05c6-49df-90e7-850dd30d1a1c",
      pairingToken: "pairing_token_abcdefghijklmnopqrstuvwxyz",
      pairingTokenExpiresAtUnixMs: 1_785_196_900_000,
    };

    await desktopApi.pairRemoteEnvironment(endpoint, approval);
    await desktopApi.createRemoteEnvironment(
      "0f8fad5b-d9cb-469f-a165-70867728950e",
      { mode: "direct" },
      3_600,
      true,
    );
    await desktopApi.logsRemoteEnvironment(
      "0f8fad5b-d9cb-469f-a165-70867728950e",
      null,
      50,
    );
    await desktopApi.revokeRemotePairing();

    expect(invokeMock.mock.calls).toEqual([
      ["pair_remote_environment", { endpoint, approval }],
      [
        "remote_environment_create",
        {
          siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
          network: { mode: "direct" },
          ttlSeconds: 3_600,
          costAcknowledged: true,
        },
      ],
      [
        "remote_environment_logs",
        {
          siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
          cursor: null,
          limit: 50,
        },
      ],
      ["revoke_remote_pairing", { confirmRevoke: true }],
    ]);
  });

  it("keeps pin rotation and force detach behind explicit native confirmations", async () => {
    const endpoint = {
      ownership: "user_self_hosted" as const,
      origin: "https://remote.example.test/",
      pin: {
        kind: "spki_sha256" as const,
        sha256: "b".repeat(64),
      },
    };
    const approval = {
      approvedByUser: true,
      pairingTokenId: "224a3a53-c15b-4c72-a008-b96e3413a1f2",
      pairingToken: "rotation_token_abcdefghijklmnopqrstuvwxyz",
      pairingTokenExpiresAtUnixMs: 1_785_196_900_000,
    };
    const siloId = "0f8fad5b-d9cb-469f-a165-70867728950e";

    await desktopApi.rotateRemoteEnvironmentTlsPin(endpoint, approval);
    await desktopApi.recoverRemoteDeletionProof(siloId);
    await desktopApi.forceDetachRemoteEnvironment(siloId);

    expect(invokeMock.mock.calls).toEqual([
      [
        "rotate_remote_environment_tls_pin",
        { endpoint, approval, confirmRotation: true },
      ],
      ["remote_environment_destroy", { siloId, confirmDestroy: false }],
      [
        "force_detach_remote_environment",
        {
          siloId,
          confirmLocalDetach: true,
          acknowledgeRemoteOrphanRisk: true,
        },
      ],
    ]);
  });

  it("keeps human and automation control typed and explicitly approved", async () => {
    const siloId = "0f8fad5b-d9cb-469f-a165-70867728950e";
    const humanAuthorizationId = "f28d93f1-a71c-4a1e-8e9b-60a8804556eb";
    const automationAuthorizationId = "c005e725-0d8e-49c1-93a0-f2633b37806f";
    const humanPrincipal = {
      kind: "human_session" as const,
      authorizationId: humanAuthorizationId,
    };
    const automationPrincipal = {
      kind: "automation" as const,
      authorizationId: automationAuthorizationId,
    };

    await desktopApi.openRemoteHumanSession(siloId, 900);
    await desktopApi.grantRemoteAutomation(
      siloId,
      300,
      ["read_screen", "send_input"],
      true,
    );
    await desktopApi.openRemoteScreen(siloId, humanPrincipal);
    await desktopApi.sendRemoteInput(siloId, automationPrincipal, [
      { type: "text", value: "typed input" },
    ]);
    await desktopApi.closeRemoteHumanSession(siloId);
    await desktopApi.revokeRemoteAutomation(siloId, automationAuthorizationId);

    expect(invokeMock.mock.calls).toEqual([
      [
        "remote_environment_open_human_session",
        { siloId, lifetimeSeconds: 900 },
      ],
      [
        "remote_environment_grant_automation",
        {
          siloId,
          lifetimeSeconds: 300,
          scopes: ["read_screen", "send_input"],
          approvedByUser: true,
        },
      ],
      ["remote_environment_open_screen", { siloId, principal: humanPrincipal }],
      [
        "remote_environment_send_input",
        {
          siloId,
          principal: automationPrincipal,
          events: [{ type: "text", value: "typed input" }],
        },
      ],
      ["remote_environment_close_human_session", { siloId }],
      [
        "remote_environment_revoke_automation",
        { siloId, authorizationId: automationAuthorizationId },
      ],
    ]);
  });
});
