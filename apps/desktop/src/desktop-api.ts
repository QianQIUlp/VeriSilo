import { invoke } from "@tauri-apps/api/core";

import type {
  BrowserKind,
  MihomoControllerSecretInput,
  NetworkProfile,
  ProxyCredentialsInput,
  RuntimeActivation,
  Silo,
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
  proxyCredentials?: ProxyCredentialsInput;
  mihomoControllerSecret?: MihomoControllerSecretInput;
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

export interface DesktopStatus {
  vault: VaultState;
  activation: RuntimeActivation;
}

export const desktopApi = {
  status: () => invoke<DesktopStatus>("desktop_status"),
  initializeVault: (passphrase: string) =>
    invoke<VaultState>("initialize_vault", { passphrase }),
  unlockVault: (passphrase: string) =>
    invoke<VaultState>("unlock_vault", { passphrase }),
  lockVault: () => invoke<VaultState>("lock_vault"),
  discoverBrowsers: () => invoke<BrowserCandidate[]>("discover_browsers"),
  detectWsl: () => invoke<WslStatus>("detect_wsl"),
  inspectMihomoController: (input: MihomoControllerInput) =>
    invoke<MihomoSnapshot>("inspect_mihomo_controller", { input }),
  listSilos: () => invoke<Silo[]>("list_silos"),
  createSilo: (input: CreateSiloInput) =>
    invoke<Silo>("create_silo", { input }),
  archiveSilo: (siloId: string) => invoke<void>("archive_silo", { siloId }),
  launchSilo: (siloId: string) =>
    invoke<RuntimeActivation>("launch_silo", { siloId }),
};
