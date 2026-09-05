import { desktopApi, type CreateSiloInput } from "../desktop-api.js";
import { previewSilo, previewStatus } from "./fixtures.js";

// Imported only by preview.html. Unsupported operations fail here instead of
// reaching Tauri; all state is synthetic and lasts only until the page reloads.
export function installPreviewApi(scenario: string) {
  const status = previewStatus(
    scenario === "locked"
      ? "locked"
      : scenario === "uninitialized"
        ? "uninitialized"
        : "unlocked",
  );
  let silos = scenario === "empty" ? [] : [structuredClone(previewSilo)];
  if (scenario === "running") {
    status.activation.activeSiloId = previewSilo.id;
    status.activation.state = "running";
  }
  for (const operation of Object.keys(desktopApi)) {
    Object.defineProperty(desktopApi, operation, {
      configurable: true,
      writable: true,
      value: async () => {
        throw new Error("此操作未在 UI 预览中模拟，请使用隔离的桌面测试实例。");
      },
    });
  }
  const unlocked = () => {
    if (status.vault.state !== "unlocked") throw new Error("保险库已锁定。");
  };
  Object.assign(desktopApi, {
    status: async () => structuredClone(status),
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
    listEngineAdapters: async () => [],
    listManagedIdentityPreviews: async () => ({}),
    listLegacyEnvironmentArtifacts: async () => [],
    listNetworkEvidence: async () => [],
    listSilos: async () => {
      unlocked();
      return structuredClone(silos);
    },
    listActiveSilos: async () => {
      unlocked();
      return structuredClone(silos.filter((s) => s.archivedAt === null));
    },
    listArchivedSilos: async () => {
      unlocked();
      return structuredClone(silos.filter((s) => s.archivedAt !== null));
    },
    siloStorageUsage: async (siloId: string) => ({
      siloId,
      profileDirectory: previewSilo.profileDirectory,
      bytes: 24000000,
    }),
    createSilo: async (input: CreateSiloInput) => {
      unlocked();
      const silo = {
        ...structuredClone(previewSilo),
        id: crypto.randomUUID(),
        name: input.name,
        color: input.color,
        networkProfile: input.networkProfile,
      };
      silos.push(silo);
      return structuredClone(silo);
    },
    launchSilo: async (siloId: string) => {
      unlocked();
      if (scenario === "error")
        throw new Error("模拟启动失败：浏览器当前不可用。");
      status.activation.activeSiloId = siloId;
      status.activation.state = "running";
      return structuredClone(status.activation);
    },
    stopSilo: async () => {
      unlocked();
      status.activation.activeSiloId = null;
      status.activation.state = "stopped";
      return structuredClone(status.activation);
    },
    archiveSilo: async (id: string) => {
      unlocked();
      silos = silos.map((s) =>
        s.id === id ? { ...s, archivedAt: new Date().toISOString() } : s,
      );
    },
    recheckSiloRuntime: async () => structuredClone(status.activation),
  } satisfies Partial<typeof desktopApi>);
}
