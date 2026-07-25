import { invoke } from "@tauri-apps/api/core";

import type {
  BrowserKind,
  NetworkProfile,
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
  listSilos: () => invoke<Silo[]>("list_silos"),
  createSilo: (input: CreateSiloInput) =>
    invoke<Silo>("create_silo", { input }),
  archiveSilo: (siloId: string) => invoke<void>("archive_silo", { siloId }),
  launchSilo: (siloId: string) =>
    invoke<RuntimeActivation>("launch_silo", { siloId }),
};
