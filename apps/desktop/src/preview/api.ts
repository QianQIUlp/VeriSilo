import type { RecentRunRecord, Silo } from "@verisilo/contracts";
import {
  desktopApi,
  type CreateManagedSiloInput,
  type CreateSiloInput,
  type DesktopStatus,
  type ManagedIdentityPreview,
} from "../desktop-api.js";
import { defaultTimezoneForPreset } from "../timezone-presets.js";
import {
  previewEngineStatuses,
  previewIdentityEvidence,
  previewManagedIdentity,
  previewManagedSilo,
  previewRuntimeActivation,
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
    ["matched", "mismatched", "unavailable", "stale", "long-name", "recheck-failed", "complex-mismatch", "network-expired", "binding-mismatch", "history-save-failed"].includes(
      scenario,
    )
  ) {
    status.activation = previewRuntimeActivation(previewManagedSilo);
    status.activation.identityEvidence = previewIdentityEvidence(
      previewManagedSilo,
      scenario === "mismatched" ||
        scenario === "unavailable" ||
        scenario === "stale"
        ? scenario
        : "matched",
    );
  }
  if (scenario === "complex-mismatch") {
    status.activation.identityEvidence!.state = "mismatched";
    status.activation.identityEvidence!.signals.push({
      signal: "voices", state: "mismatched",
      expected: [{ name: "Voice A", lang: "en-US", default: true }],
      observed: [{ name: "Voice B", lang: "en-US", default: true }],
    });
  }
  if (scenario === "network-expired") {
    status.activation.networkEvidence!.observedAt = new Date(Date.now() - 600000).toISOString();
    status.activation.networkEvidence!.expiresAt = new Date(Date.now() - 60000).toISOString();
  }
  if (scenario === "history-save-failed") status.recentRunsWarning = "save_failed";
  if (scenario === "binding-mismatch") {
    status.activation.networkEvidence!.runtimeId = "33333333-3333-4333-8333-333333333333";
  }
  if (scenario === "running") {
    status.activation.activeSiloId = previewSilo.id;
    status.activation.state = "running";
  }
  const previews: Record<string, ManagedIdentityPreview> = {
    [previewManagedSilo.id]: structuredClone(previewManagedIdentity),
  };
  const recentRuns: Record<string, RecentRunRecord> = {};
  const saveRecent = (siloId: string, freshRun = false) => {
    const silo = silos.find((entry) => entry.id === siloId)!;
    const activation = status.activation;
    if (activation.identityEvidence !== null && activation.networkEvidence !== null &&
        activation.identityEvidence.runtimeId !== activation.networkEvidence.runtimeId) return;
    const previous = freshRun ? undefined : recentRuns[siloId];
    recentRuns[siloId] = {
      siloId, runId: previous?.runId ?? crypto.randomUUID(),
      runtimeId: activation.identityEvidence?.runtimeId ?? activation.networkEvidence?.runtimeId ?? null,
      startedAt: previous?.startedAt ?? activation.updatedAt,
      updatedAt: activation.updatedAt,
      endedAt: ["stopped", "failed", "verification_failed"].includes(activation.state) ? activation.updatedAt : null,
      profileSiloId: siloId,
      artifactBinding: silo.engine.adapter === "camoufox" ? silo.engine.artifactBinding ?? null : null,
      engineAdapter: activation.engineEvidence?.configuredAdapter ?? (silo.engine.adapter === "camoufox" ? "camoufox" : "stock-edge"),
      networkPolicy: previous?.networkPolicy ?? {
        mode: silo.networkProfile.mode, proxyRequired: silo.networkProfile.proxyRequired,
        endpointLabel: activation.networkEvidence?.endpointLabel ?? null,
        externalMihomo: silo.networkProfile.mode === "fixed_proxy" && silo.networkProfile.externalMihomo !== undefined,
      },
      state: activation.state, reason: activation.message,
      identityEvidence: structuredClone(activation.identityEvidence),
      engineEvidence: structuredClone(activation.engineEvidence),
      networkEvidence: structuredClone(activation.networkEvidence),
    };
  };
  if (status.activation.activeSiloId !== null) saveRecent(status.activation.activeSiloId);
  if (scenario === "recent-run" || scenario === "history-error") {
    for (const silo of silos) {
      status.activation = previewRuntimeActivation(silo);
      status.activation.state = "stopped";
      saveRecent(silo.id);
    }
    status.activation = previewStatus().activation;
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
    listManagedIdentityPreviews: async () => structuredClone(previews),
    listLegacyEnvironmentArtifacts: async () => [],
    listNetworkEvidence: async () => [],
    listRecentRuns: async () => {
      unlocked();
      if (scenario === "history-error") throw new Error("模拟：最近运行记录读取失败。");
      return structuredClone(Object.values(recentRuns));
    },
    getRecentRun: async (siloId: string) => {
      unlocked();
      return structuredClone(recentRuns[siloId] ?? null);
    },
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
      const followsExit = input.networkProfile.mode !== "direct" && input.followNetworkExit !== false;
      const language = input.identityPreset.replace("balanced-", "").split("-");
      previews[silo.id] = {
        ...structuredClone(previewManagedIdentity),
        language: followsExit ? previewManagedIdentity.language : `${language[0]}-${language[1]?.toUpperCase()}`,
        timezone: followsExit ? previewManagedIdentity.timezone : input.timezone ?? defaultTimezoneForPreset(input.identityPreset),
        screenWidth: input.screenWidth ?? previewManagedIdentity.screenWidth,
        screenHeight: input.screenHeight ?? previewManagedIdentity.screenHeight,
        networkBound: input.networkProfile.mode !== "direct",
        publicAddress: input.networkProfile.mode === "direct" ? null : previewManagedIdentity.publicAddress,
        countryCode: input.networkProfile.mode === "direct" ? null : previewManagedIdentity.countryCode,
      };
      silos.push(silo);
      return structuredClone(silo);
    },
    launchSilo: async (siloId: string) => {
      unlocked();
      if (scenario === "error")
        throw new Error("模拟启动失败：浏览器当前不可用。");
      if (status.activation.activeSiloId !== null) throw new Error("已有一个 Silo 正在运行。");
      const silo = silos.find((s) => s.id === siloId)!;
      status.activation = previewRuntimeActivation(silo);
      silo.identityLockedAt ??= new Date().toISOString();
      saveRecent(siloId, true);
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
      delete recentRuns[id];
      delete previews[id];
    },
    stopSilo: async () => {
      unlocked();
      const stoppedSiloId = status.activation.activeSiloId;
      status.activation.state = "stopped";
      status.activation.updatedAt = new Date().toISOString();
      if (stoppedSiloId !== null) saveRecent(stoppedSiloId);
      status.activation.activeSiloId = null;
      return structuredClone(status.activation);
    },
    archiveSilo: async (id: string) => {
      unlocked();
      silos = silos.map((s) =>
        s.id === id ? { ...s, archivedAt: new Date().toISOString() } : s,
      );
    },
    recheckSiloRuntime: async (siloId: string) => {
      unlocked();
      if (siloId !== status.activation.activeSiloId) throw new Error("该 Silo 没有可重新检查的活动会话。");
      if (status.activation.identityEvidence) {
        if (scenario === "recheck-failed") {
          status.activation.identityEvidence.state = "unavailable";
          status.activation.identityEvidence.reason = "模拟：本次身份复核未能重新观察网站身份。";
        } else {
          status.activation.identityEvidence.observedAt = new Date().toISOString();
        }
      }
      status.activation.updatedAt = new Date().toISOString();
      saveRecent(siloId);
      return structuredClone(status.activation);
    },
  } satisfies Partial<typeof desktopApi>);
}
