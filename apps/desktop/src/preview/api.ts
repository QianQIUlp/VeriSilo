import type { Silo } from "@verisilo/contracts";
import {
  desktopApi,
  type CreateManagedSiloInput,
  type CreateSiloInput,
  type DesktopStatus,
} from "../desktop-api.js";
import {
  previewEngineStatuses,
  previewIdentityEvidence,
  previewManagedIdentity,
  previewManagedSilo,
  previewSilo,
  previewStatus,
} from "./fixtures.js";

// Imported only by UI preview entries, including the public demo. Unsupported operations fail here instead of
// reaching Tauri; all state is synthetic and lasts only until the page reloads.
export function installPreviewApi(scenario: string) {
  const status = previewStatus(
    scenario === "locked"
      ? "locked"
      : scenario === "uninitialized"
        ? "uninitialized"
        : "unlocked",
  );
  let silos =
    scenario === "empty"
      ? []
      : [structuredClone(previewSilo), structuredClone(previewManagedSilo)];
  if (scenario === "long-name") {
    silos[1]!.name =
      "北美业务 · 长期协作与研究专用身份空间 / Research & Operations";
  }
  if (
    ["matched", "mismatched", "unavailable", "stale", "long-name"].includes(
      scenario,
    )
  ) {
    status.activation.activeSiloId = previewManagedSilo.id;
    status.activation.state = "running";
    status.activation.identityEvidence = previewIdentityEvidence(
      previewManagedSilo,
      scenario === "mismatched" ||
        scenario === "unavailable" ||
        scenario === "stale"
        ? scenario
        : "matched",
    );
  }
  if (scenario === "running") {
    status.activation.activeSiloId = previewSilo.id;
    status.activation.state = "running";
  }
  for (const operation of Object.keys(desktopApi)) {
    Object.defineProperty(desktopApi, operation, {
      configurable: true,
      writable: true,
      value: async () => {
        throw new Error("这项操作尚未在演示中模拟。请在安装后的桌面应用中使用。");
      },
    });
  }
  const unlocked = () => {
    if (status.vault.state !== "unlocked") throw new Error("保险库已锁定。");
  };
  Object.assign(desktopApi, {
    status: async () =>
      scenario === "loading"
        ? new Promise<DesktopStatus>(() => {})
        : structuredClone(status),
    initializeVault: async () => {
      status.vault.state = "unlocked";
      return structuredClone(status.vault);
    },
    unlockVault: async () => {
      status.vault.state = "unlocked";
      return structuredClone(status.vault);
    },
    lockVault: async () => {
      status.vault.state = "locked";
      return structuredClone(status.vault);
    },
    discoverBrowsers: async () => [
      {
        kind: "edge",
        displayName: "Microsoft Edge（示例）",
        executablePath: previewSilo.browser!.executablePath,
        version: "preview",
      },
    ],
    listEngineAdapters: async () => structuredClone(previewEngineStatuses),
    localApiInfo: async () => {
      if (scenario === "error") throw new Error("模拟：命令文件暂不可用。");
      return {
        url: "http://127.0.0.1:0",
        pid: 0,
        discoveryPath: "C:\\Preview\\discovery.json",
        cliPath: "C:\\Preview\\verisilo-cli.exe",
        vaultName: "preview-only",
      };
    },
    environmentBackendStatuses: async () => {
      if (scenario === "error") throw new Error("模拟：部分位置状态不可用。");
      return [];
    },
    remoteEnvironmentStatus: async () => ({
      protocolVersion: 1,
      state: "not_configured",
      transportAvailable: false,
      durableBindingStoreAvailable: false,
      selfHostedAgentAvailable: false,
      capabilities: [],
      message: "UI Preview：未配置远程服务。",
      endpoint: null,
      pairing: null,
      bindings: [],
      lastResults: [],
      pairingRevokedAt: null,
      orphanReceipts: [],
    }),
    detectWsl: async () => ({
      supportedPlatform: true,
      available: true,
      distributions: ["Ubuntu-Preview", "Debian-Preview"],
      message: "UI Preview 模拟发行版，不读取本机 WSL。",
    }),
    backupVault: async (destinationPath: string) => {
      unlocked();
      if (scenario === "error") throw new Error("模拟：备份无法写入。");
      return { destinationPath, bytes: 4096 };
    },
    listManagedIdentityPreviews: async () =>
      Object.fromEntries(
        silos
          .filter((s) => s.engine.adapter === "camoufox")
          .map((s) => [s.id, structuredClone(previewManagedIdentity)]),
      ),
    listLegacyEnvironmentArtifacts: async () => [],
    listNetworkEvidence: async () => [],
    listActiveSilos: async () => {
      unlocked();
      return structuredClone(silos.filter((s) => s.archivedAt === null));
    },
    listArchivedSilos: async () => {
      unlocked();
      return structuredClone(silos.filter((s) => s.archivedAt !== null));
    },
    siloStorageUsages: async () =>
      silos.map((s) => ({ siloId: s.id, bytes: 24000000 })),
    createSilo: async (input: CreateSiloInput) => {
      unlocked();
      const silo: Silo = {
        ...structuredClone(previewSilo),
        id: crypto.randomUUID(),
        name: input.name,
        color: input.color,
        networkProfile: input.networkProfile,
      };
      silo.profileDirectory = `C:\\Preview\\profiles\\${silo.id}`;
      silos.push(silo);
      return structuredClone(silo);
    },
    createManagedSilo: async (input: CreateManagedSiloInput) => {
      unlocked();
      // Mirrors the real create path: a fresh Silo with a new id, new
      // profile, new Artifact binding, and no inherited runtime state.
      const silo: Silo = {
        ...structuredClone(previewManagedSilo),
        id: crypto.randomUUID(),
        name: input.name,
        color: input.color,
        networkProfile: input.networkProfile,
        seedReference: crypto.randomUUID(),
        identityLockedAt: null,
        engine: {
          adapter: "camoufox",
          artifactBinding: {
            artifactId: `identity-${crypto.randomUUID().slice(0, 8)}`,
            artifactFileSha256: "c".repeat(64),
            schema: "verisilo-camoufox-resolved-identity/v6",
          },
        },
      };
      silo.profileDirectory = `C:\\Preview\\profiles\\${silo.id}`;
      silos.push(silo);
      return structuredClone(silo);
    },
    launchSilo: async (siloId: string) => {
      unlocked();
      if (scenario === "error")
        throw new Error("模拟启动失败：浏览器当前不可用。");
      status.activation.activeSiloId = siloId;
      status.activation.state = "running";
      const silo = silos.find((s) => s.id === siloId)!;
      status.activation.identityEvidence =
        silo.engine.adapter === "camoufox"
          ? previewIdentityEvidence(silo)
          : null;
      return structuredClone(status.activation);
    },
    restoreArchivedSilo: async (id: string) => {
      unlocked();
      silos = silos.map((s) => (s.id === id ? { ...s, archivedAt: null } : s));
      return structuredClone(silos.find((s) => s.id === id)!);
    },
    deleteSilo: async (id: string) => {
      unlocked();
      silos = silos.filter((s) => s.id !== id);
    },
    stopSilo: async () => {
      unlocked();
      status.activation.activeSiloId = null;
      status.activation.state = "stopped";
      status.activation.identityEvidence = null;
      return structuredClone(status.activation);
    },
    archiveSilo: async (id: string) => {
      unlocked();
      silos = silos.map((s) =>
        s.id === id ? { ...s, archivedAt: new Date().toISOString() } : s,
      );
    },
    recheckSiloRuntime: async () => {
      if (status.activation.identityEvidence) {
        status.activation.identityEvidence.observedAt =
          new Date().toISOString();
      }
      return structuredClone(status.activation);
    },
  } satisfies Partial<typeof desktopApi>);
}
