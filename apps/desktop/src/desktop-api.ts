import { invoke } from "@tauri-apps/api/core";

import type {
  BrowserKind,
  EngineAdapterId,
  EngineDescriptor,
  EngineHealth,
  EngineMaintenanceReceipt,
  EngineNegotiation,
  EnginePackageRequest,
  EnvironmentActionReceipt,
  EnvironmentBackendId,
  EnvironmentBackendStatus,
  EnvironmentOperationRequest,
  MihomoControllerSecretInput,
  NativeNetworkEvidenceCoverage,
  NetworkCheckResult,
  NetworkProfile,
  ProxyCredentialsInput,
  RemoteAgentInteractionReceipt,
  RemoteAutomationAuthorization,
  RemoteAutomationScope,
  RemoteCapability,
  RemoteDeletionProof,
  RemoteEndpoint,
  RemoteGuestEvidence,
  RemoteInputEvent,
  RemoteNetworkPolicy,
  RemoteNodeDisclosure,
  RemoteOperationResult as ContractRemoteOperationResult,
  RemoteScreenChannel,
  RemoteSessionAuthorization,
  RemoteVolumeAttestation,
  RuntimeActivation,
  Silo,
  SiloEngineConfig,
  SiloExecutionTarget,
  VaultState,
} from "@verisilo/contracts";

export interface BrowserCandidate {
  kind: BrowserKind;
  displayName: string;
  executablePath: string;
  version: string | null;
}

export interface CreateSiloInput {
  name: string;
  color: string;
  browserKind: BrowserKind;
  executablePath: string;
  networkProfile: NetworkProfile;
  executionTarget: SiloExecutionTarget;
  engine?: SiloEngineConfig;
  proxyCredentials?: ProxyCredentialsInput;
  mihomoControllerSecret?: MihomoControllerSecretInput;
}

/**
 * The small, user-facing set of identity presets exposed by the managed
 * browser. The native command owns the actual template; the desktop UI only
 * sends the selected preset name.
 */
export type ManagedIdentityPreset =
  "balanced-en-us" | "balanced-zh-cn" | "balanced-de-de" | "match-fixed-proxy";

export type ManagedNetworkProfile =
  | {
      mode: "direct";
      proxyRequired: false;
    }
  | {
      mode: "fixed_proxy";
      proxyRequired: true;
      scheme: "http" | "socks5";
      host: string;
      port: number;
      bypassList: [];
    };

/**
 * JSON boundary for the built-in managed browser creation command. Unlike a
 * Standard Silo, it deliberately has no executable path, browser kind, or
 * execution target: those are selected by the trusted native package.
 */
export interface CreateManagedSiloInput {
  name: string;
  color: string;
  identityPreset: ManagedIdentityPreset;
  networkProfile: ManagedNetworkProfile;
  proxyCredentials?: ProxyCredentialsInput;
}

export interface UpdateSiloInput {
  name: string;
  color: string;
  browserKind: BrowserKind;
  executablePath: string;
}

export interface UpdateSiloNetworkInput {
  networkProfile: NetworkProfile;
  proxyCredentials?: ProxyCredentialsInput;
  mihomoControllerSecret?: MihomoControllerSecretInput;
}

export interface UpdateSiloEngineInput {
  engine: SiloEngineConfig;
}

export interface SiloStorageUsage {
  siloId: string;
  profileDirectory: string;
  bytes: number;
}

export interface VaultBackupReceipt {
  destinationPath: string;
  bytes: number;
}

export interface SiloNetworkEvidence {
  schemaVersion: number;
  protocolVersion: number;
  evidenceId: string;
  requestId: string;
  siloId: string;
  receivedAt: string;
  expiresAt: string;
  coverage: NativeNetworkEvidenceCoverage;
  result: NetworkCheckResult;
}

export interface MihomoControllerInput {
  controllerUrl: string;
  secret: string;
}

export interface MihomoNode {
  name: string;
  proxyType: string | null;
  delayMs: number | null;
  alive: boolean | null;
}

export interface MihomoSelectorGroup {
  name: string;
  selected: string | null;
  nodes: MihomoNode[];
}

export interface MihomoSnapshot {
  checkedAt: string;
  groups: MihomoSelectorGroup[];
  providers: Array<{
    name: string;
    vehicleType: string | null;
    updatedAt: string | null;
    nodeCount: number;
  }>;
}

export interface WslStatus {
  supportedPlatform: boolean;
  available: boolean;
  distributions: string[];
  message: string;
}

export interface LegacyEnvironmentArtifact {
  siloId: string;
  backend: EnvironmentBackendId;
  cleanupAvailable: boolean;
  message: string;
}

export interface DesktopStatus {
  vault: VaultState;
  activation: RuntimeActivation;
}

export interface BrowserVerification {
  state:
    | "verified"
    | "baseline_missing"
    | "version_drift"
    | "missing"
    | "path_changed"
    | "kind_mismatch"
    | "publisher_mismatch"
    | "probe_failed";
  expectedKind: BrowserKind;
  expectedVersion: string | null;
  actualVersion: string | null;
  executablePath: string;
  checkedAt: string;
  message: string;
}

export interface EngineAdapterStatus {
  descriptor: EngineDescriptor;
  negotiation: EngineNegotiation;
  health: EngineHealth;
}

export interface RemoteEnvironmentStatus {
  protocolVersion: number;
  state:
    | "vault_uninitialized"
    | "vault_locked"
    | "not_configured"
    | "not_paired"
    | "paired"
    | "credential_expired"
    | "revoked";
  transportAvailable: boolean;
  durableBindingStoreAvailable: boolean;
  selfHostedAgentAvailable: boolean;
  capabilities: RemoteCapability[];
  message: string;
  endpoint: RemoteEndpoint | null;
  pairing: {
    serverId: string;
    clientCredentialId: string;
    node: RemoteNodeDisclosure;
    credentialExpiresAtUnixMs: number;
    expired: boolean;
  } | null;
  bindings: RemoteSiloBinding[];
  lastResults: DesktopRemoteOperationResult[];
  pairingRevokedAt: string | null;
  orphanReceipts: RemoteOrphanReceipt[];
}

export interface RemoteOrphanReceipt {
  receiptId: string;
  siloId: string;
  bindingId: string;
  remoteEnvironmentId: string;
  serverId: string;
  endpoint: RemoteEndpoint;
  detachedAtUnixMs: number;
  notice: string;
}

export interface RemoteSiloBinding {
  siloId: string;
  bindingId: string;
  remoteEnvironmentId: string;
  serverId: string;
  endpoint: RemoteEndpoint;
  network: RemoteNetworkPolicy;
  volume: RemoteVolumeAttestation;
  lastActivityAtUnixMs: number;
  humanSession?: RemoteSessionAuthorization;
  automationAuthorizations: RemoteAutomationAuthorization[];
  lastScreenChannel?: RemoteScreenChannel;
  lastInteraction?: RemoteAgentInteractionReceipt;
  lastEvidence?: RemoteGuestEvidence;
}

export type RemoteInteractivePrincipal =
  | { kind: "human_session"; authorizationId: string }
  | { kind: "automation"; authorizationId: string };

export type DesktopRemoteOperationResult = ContractRemoteOperationResult & {
  volume?: RemoteVolumeAttestation;
  deletionProof?: RemoteDeletionProof;
};

export interface RemotePairingApproval {
  approvedByUser: boolean;
  pairingTokenId: string;
  pairingToken: string;
  pairingTokenExpiresAtUnixMs: number;
}

export const desktopApi = {
  status: () => invoke<DesktopStatus>("desktop_status"),
  initializeVault: (passphrase: string) =>
    invoke<VaultState>("initialize_vault", { passphrase }),
  unlockVault: (passphrase: string) =>
    invoke<VaultState>("unlock_vault", { passphrase }),
  lockVault: () => invoke<VaultState>("lock_vault"),
  discoverBrowsers: () => invoke<BrowserCandidate[]>("discover_browsers"),
  listEngineAdapters: () =>
    invoke<EngineAdapterStatus[]>("list_engine_adapters"),
  installEnginePackage: (
    adapterId: EngineAdapterId,
    request: EnginePackageRequest,
  ) =>
    invoke<EngineMaintenanceReceipt>("install_engine_package", {
      adapterId,
      request,
    }),
  updateEnginePackage: (
    adapterId: EngineAdapterId,
    request: EnginePackageRequest,
  ) =>
    invoke<EngineMaintenanceReceipt>("update_engine_package", {
      adapterId,
      request,
    }),
  rollbackEnginePackage: (adapterId: EngineAdapterId) =>
    invoke<EngineMaintenanceReceipt>("rollback_engine_package", {
      adapterId,
    }),
  setEngineEmergencyDisabled: (
    adapterId: EngineAdapterId,
    disabled: boolean,
    reason: string | null,
  ) =>
    invoke<EngineAdapterStatus>("set_engine_emergency_disabled", {
      adapterId,
      disabled,
      reason,
    }),
  remoteEnvironmentStatus: () =>
    invoke<RemoteEnvironmentStatus>("remote_environment_status"),
  validateRemoteEnvironmentEndpoint: (endpoint: RemoteEndpoint) =>
    invoke<RemoteEndpoint>("validate_remote_environment_endpoint", {
      endpoint,
    }),
  pairRemoteEnvironment: (
    endpoint: RemoteEndpoint,
    approval: RemotePairingApproval,
  ) =>
    invoke<RemoteEnvironmentStatus>("pair_remote_environment", {
      endpoint,
      approval,
    }),
  rotateRemoteEnvironmentTlsPin: (
    endpoint: RemoteEndpoint,
    approval: RemotePairingApproval,
  ) =>
    invoke<RemoteEnvironmentStatus>("rotate_remote_environment_tls_pin", {
      endpoint,
      approval,
      confirmRotation: true,
    }),
  revokeRemotePairing: () =>
    invoke<RemoteEnvironmentStatus>("revoke_remote_pairing", {
      confirmRevoke: true,
    }),
  forceDetachRemoteEnvironment: (siloId: string) =>
    invoke<RemoteEnvironmentStatus>("force_detach_remote_environment", {
      siloId,
      confirmLocalDetach: true,
      acknowledgeRemoteOrphanRisk: true,
    }),
  createRemoteEnvironment: (
    siloId: string,
    network: RemoteNetworkPolicy,
    ttlSeconds: number,
    costAcknowledged: boolean,
  ) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_create", {
      siloId,
      network,
      ttlSeconds,
      costAcknowledged,
    }),
  startRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_start", {
      siloId,
    }),
  stopRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_stop", { siloId }),
  pauseRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_pause", {
      siloId,
    }),
  snapshotRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_snapshot", {
      siloId,
    }),
  destroyRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_destroy", {
      siloId,
      confirmDestroy: true,
    }),
  recoverRemoteDeletionProof: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_destroy", {
      siloId,
      confirmDestroy: false,
    }),
  configureRemoteEnvironmentNetwork: (
    siloId: string,
    network: RemoteNetworkPolicy,
  ) =>
    invoke<DesktopRemoteOperationResult>(
      "remote_environment_configure_network",
      {
        siloId,
        network,
      },
    ),
  healthRemoteEnvironment: (siloId: string) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_health", {
      siloId,
    }),
  logsRemoteEnvironment: (
    siloId: string,
    cursor: string | null,
    limit: number,
  ) =>
    invoke<DesktopRemoteOperationResult>("remote_environment_logs", {
      siloId,
      cursor,
      limit,
    }),
  openRemoteHumanSession: (siloId: string, lifetimeSeconds: number) =>
    invoke<RemoteAgentInteractionReceipt>(
      "remote_environment_open_human_session",
      { siloId, lifetimeSeconds },
    ),
  closeRemoteHumanSession: (siloId: string) =>
    invoke<RemoteAgentInteractionReceipt>(
      "remote_environment_close_human_session",
      { siloId },
    ),
  grantRemoteAutomation: (
    siloId: string,
    lifetimeSeconds: number,
    scopes: RemoteAutomationScope[],
    approvedByUser: boolean,
  ) =>
    invoke<RemoteAgentInteractionReceipt>(
      "remote_environment_grant_automation",
      { siloId, lifetimeSeconds, scopes, approvedByUser },
    ),
  revokeRemoteAutomation: (siloId: string, authorizationId: string) =>
    invoke<RemoteAgentInteractionReceipt>(
      "remote_environment_revoke_automation",
      { siloId, authorizationId },
    ),
  openRemoteScreen: (siloId: string, principal: RemoteInteractivePrincipal) =>
    invoke<RemoteAgentInteractionReceipt>("remote_environment_open_screen", {
      siloId,
      principal,
    }),
  sendRemoteInput: (
    siloId: string,
    principal: RemoteInteractivePrincipal,
    events: RemoteInputEvent[],
  ) =>
    invoke<RemoteAgentInteractionReceipt>("remote_environment_send_input", {
      siloId,
      principal,
      events,
    }),
  detectWsl: () => invoke<WslStatus>("detect_wsl"),
  environmentBackendStatuses: () =>
    invoke<EnvironmentBackendStatus[]>("environment_backend_statuses"),
  selectWslEnvironmentDistribution: (distribution: string) =>
    invoke<EnvironmentBackendStatus>("select_wsl_environment_distribution", {
      distribution,
    }),
  executeEnvironmentBackend: (request: EnvironmentOperationRequest) =>
    invoke<EnvironmentActionReceipt>("environment_backend_execute", {
      request,
    }),
  listLegacyEnvironmentArtifacts: () =>
    invoke<LegacyEnvironmentArtifact[]>("list_legacy_environment_artifacts"),
  cleanupLegacyEnvironmentArtifact: (
    siloId: string,
    backend: EnvironmentBackendId,
  ) =>
    invoke<EnvironmentActionReceipt>("cleanup_legacy_environment_artifact", {
      siloId,
      backend,
      confirmCleanup: true,
    }),
  inspectMihomoController: (input: MihomoControllerInput) =>
    invoke<MihomoSnapshot>("inspect_mihomo_controller", { input }),
  listSilos: () => invoke<Silo[]>("list_silos"),
  listActiveSilos: () => invoke<Silo[]>("list_active_silos"),
  listArchivedSilos: () => invoke<Silo[]>("list_archived_silos"),
  createSilo: (input: CreateSiloInput) =>
    invoke<Silo>("create_silo", { input }),
  createManagedSilo: (input: CreateManagedSiloInput) =>
    invoke<Silo>("create_managed_silo", { input }),
  updateSilo: (siloId: string, input: UpdateSiloInput) =>
    invoke<Silo>("update_silo", { siloId, input }),
  updateSiloConfiguration: (
    siloId: string,
    input: UpdateSiloInput,
    networkInput: UpdateSiloNetworkInput | null,
    engineInput: UpdateSiloEngineInput | null,
  ) =>
    invoke<Silo>("update_silo_configuration", {
      siloId,
      input,
      networkInput,
      engineInput,
    }),
  renameSilo: (siloId: string, name: string) =>
    invoke<Silo>("rename_silo", { siloId, name }),
  updateSiloNetwork: (siloId: string, input: UpdateSiloNetworkInput) =>
    invoke<Silo>("update_silo_network", { siloId, input }),
  updateSiloEngine: (siloId: string, input: UpdateSiloEngineInput) =>
    invoke<Silo>("update_silo_engine", { siloId, input }),
  archiveSilo: (siloId: string) => invoke<void>("archive_silo", { siloId }),
  restoreArchivedSilo: (siloId: string) =>
    invoke<Silo>("restore_archived_silo", { siloId }),
  deleteSilo: (siloId: string) =>
    invoke<void>("delete_silo", { siloId, confirmPermanent: true }),
  siloStorageUsage: (siloId: string) =>
    invoke<SiloStorageUsage>("silo_storage_usage", { siloId }),
  listNetworkEvidence: (siloId?: string) =>
    invoke<SiloNetworkEvidence[]>("list_network_evidence", {
      siloId: siloId ?? null,
    }),
  clearNetworkEvidence: (siloId: string) =>
    invoke<number>("clear_network_evidence", {
      siloId,
      confirmClear: true,
    }),
  changeVaultPassphrase: (currentPassphrase: string, newPassphrase: string) =>
    invoke<VaultState>("change_vault_passphrase", {
      currentPassphrase,
      newPassphrase,
    }),
  backupVault: (destinationPath: string) =>
    invoke<VaultBackupReceipt>("backup_vault", { destinationPath }),
  restoreVault: (
    sourcePath: string,
    passphrase: string,
    confirmOverwrite: boolean,
  ) =>
    invoke<VaultState>("restore_vault", {
      sourcePath,
      passphrase,
      confirmOverwrite,
    }),
  launchSilo: (siloId: string) =>
    invoke<RuntimeActivation>("launch_silo", { siloId }),
  stopSilo: (siloId: string) =>
    invoke<RuntimeActivation>("stop_silo", { siloId }),
  recheckSiloBrowser: (siloId: string) =>
    invoke<BrowserVerification>("recheck_silo_browser", { siloId }),
  recheckSiloRuntime: (siloId: string) =>
    invoke<RuntimeActivation>("recheck_silo_runtime", { siloId }),
  rebindSiloMihomo: (siloId: string) =>
    invoke<RuntimeActivation>("rebind_silo_mihomo", { siloId }),
};
