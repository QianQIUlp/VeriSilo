import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";

import type {
  BrowserKind,
  EngineAdapterId,
  EnvironmentBackendStatus,
  EnvironmentBackendId,
  EnvironmentNetworkProfile,
  EnvironmentOperation,
  EnvironmentOperationRequest,
  NetworkCheckResult,
  NetworkProfile,
  RemoteEndpoint,
  RemoteNetworkPolicy,
  Silo,
  SiloExecutionTarget,
} from "@verisilo/contracts";
import { networkProfileSchema } from "@verisilo/contracts";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  desktopApi,
  type BrowserCandidate,
  type BrowserVerification,
  type CreateManagedSiloInput,
  type CreateSiloInput,
  type DesktopStatus,
  type EngineAdapterStatus,
  type LegacyEnvironmentArtifact,
  type ManagedIdentityPreset,
  type ManagedNetworkProfile,
  type MihomoSnapshot,
  type RemoteEnvironmentStatus,
  type RemoteInteractivePrincipal,
  type SiloNetworkEvidence,
  type UpdateSiloEngineInput,
  type UpdateSiloInput,
  type UpdateSiloNetworkInput,
  type WslStatus,
} from "./desktop-api.js";
import {
  activationStatusLabel,
  describeActivation,
  describeNetwork,
  describeVault,
} from "./formatters.js";
import { runDesktopNetworkCheck } from "./network-check-client.js";
import { parseProxyInput } from "./proxy-input.js";
import { isLoopbackProxyProfile, localMihomoProfile } from "./proxy-presets.js";
import {
  buildLocalSiloReport,
  renderLocalSiloReportHtml,
  serializeLocalSiloReport,
} from "./reports.js";
import {
  vaultAutoLockDeadlinePassed,
  vaultAutoLockRefreshDelay,
} from "./vault-auto-lock.js";
import {
  acceptedRestoredVaultState,
  scrubDesktopStatusForLockedUi,
  VaultUiSession,
  type VaultRefreshResult,
} from "./vault-ui-session.js";
import { UserFacingError, userFacingErrorMessage } from "./user-errors.js";
import {
  canConfigureWslDistribution,
  requiresExplicitWslSelection,
} from "./wsl-selection.js";

const defaultColor = "#5b5ce2";
const defaultMihomoControllerUrl = "http://127.0.0.1:9090/";

type Notice = { tone: "error" | "success" | "info"; message: string } | null;
type View = "overview" | "create" | "edit" | "settings" | "environments";
type CreateMode = "standard" | "managed";
type WslCreationOption = {
  distribution: string;
  ready: boolean;
};

const requiredWslCreationOperations = [
  "configureNetwork",
  "start",
  "stop",
  "health",
] satisfies EnvironmentOperation[];

function emptyNetwork(): NetworkProfile {
  return { mode: "direct", proxyRequired: false };
}

// Sandbox, Hyper-V and remote lifecycle controls are intentionally kept out
// of the product UI until they are bound to Silo.executionTarget and can
// launch the same verified browser identity shown during creation.
function unboundEnvironmentControlsAvailable(): boolean {
  return false;
}

function legacyEnvironmentLabel(backend: EnvironmentBackendId): string {
  switch (backend) {
    case "wsl-chromium":
      return "Linux 环境";
    case "windows-sandbox":
      return "Windows 临时环境";
    case "hyper-v":
      return "虚拟机环境";
  }
}

function errorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  return userFacingErrorMessage(error, fallback);
}

function managedErrorMessage(error: unknown): string {
  return errorMessage(
    error,
    "托管身份浏览器操作没有完成。请检查当前状态后重试。",
  );
}

export function App() {
  const [view, setView] = useState<View>("overview");
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const [uiVaultLocked, setUiVaultLocked] = useState(true);
  const [vaultUiGeneration, setVaultUiGeneration] = useState(0);
  const [vaultTransition, setVaultTransition] = useState<"idle" | "restoring">(
    "idle",
  );
  const [silos, setSilos] = useState<Silo[]>([]);
  const [archivedSilos, setArchivedSilos] = useState<Silo[]>([]);
  const [legacyEnvironmentArtifacts, setLegacyEnvironmentArtifacts] = useState<
    LegacyEnvironmentArtifact[]
  >([]);
  const [storageUsage, setStorageUsage] = useState<
    Record<string, number | null>
  >({});
  const [editingSilo, setEditingSilo] = useState<Silo | null>(null);
  const [networkEvidenceHistory, setNetworkEvidenceHistory] = useState<
    SiloNetworkEvidence[]
  >([]);
  const [browsers, setBrowsers] = useState<BrowserCandidate[]>([]);
  const [engineStatuses, setEngineStatuses] = useState<EngineAdapterStatus[]>(
    [],
  );
  const [managedStatusBusy, setManagedStatusBusy] = useState(false);
  const [managedStatusError, setManagedStatusError] = useState<string | null>(
    null,
  );
  const [passphrase, setPassphrase] = useState("");
  const [notice, setNotice] = useState<Notice>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState(defaultColor);
  const [browserPath, setBrowserPath] = useState("");
  const [browserKind, setBrowserKind] = useState<BrowserKind>("chrome");
  const [executionTarget, setExecutionTarget] = useState<SiloExecutionTarget>({
    kind: "local",
  });
  const [createWslStatus, setCreateWslStatus] = useState<WslStatus | null>(
    null,
  );
  const [createWslOptions, setCreateWslOptions] = useState<WslCreationOption[]>(
    [],
  );
  const [createWslBusy, setCreateWslBusy] = useState(false);
  const [networkProfile, setNetworkProfile] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [mihomoControllerUrl, setMihomoControllerUrl] = useState(
    defaultMihomoControllerUrl,
  );
  const [mihomoControllerSecret, setMihomoControllerSecret] = useState("");
  const [mihomoSnapshot, setMihomoSnapshot] = useState<MihomoSnapshot | null>(
    null,
  );
  const [mihomoBusy, setMihomoBusy] = useState(false);
  const [networkResult, setNetworkResult] = useState<NetworkCheckResult | null>(
    null,
  );
  const [networkBusy, setNetworkBusy] = useState(false);
  const [busy, setBusy] = useState(false);
  const [vaultBusy, setVaultBusy] = useState(false);
  const refreshRequestRef = useRef(0);
  const unlockedOperationRef = useRef(0);
  const vaultOperationRef = useRef(0);
  const networkRequestRef = useRef(0);
  const mihomoRequestRef = useRef(0);
  const createWslRequestRef = useRef(0);
  const browserSelectionExplicitRef = useRef(false);
  const vaultUiSessionRef = useRef(new VaultUiSession());

  useEffect(() => {
    if (!isTauri()) {
      return;
    }

    const appWindow = getCurrentWindow();
    let disposed = false;
    let removeCloseListener: (() => void) | undefined;

    void appWindow
      .onCloseRequested(async (event) => {
        event.preventDefault();
        await appWindow.hide();
      })
      .then((unlisten) => {
        if (disposed) {
          unlisten();
        } else {
          removeCloseListener = unlisten;
        }
      });

    return () => {
      disposed = true;
      removeCloseListener?.();
    };
  }, []);

  const scrubSensitiveUi = useCallback(() => {
    setSilos([]);
    setArchivedSilos([]);
    setLegacyEnvironmentArtifacts([]);
    setStorageUsage({});
    setEditingSilo(null);
    setNetworkEvidenceHistory([]);
    setBrowsers([]);
    setEngineStatuses([]);
    setManagedStatusBusy(false);
    setManagedStatusError(null);
    setPassphrase("");
    setName("");
    setColor(defaultColor);
    setBrowserPath("");
    setBrowserKind("chrome");
    browserSelectionExplicitRef.current = false;
    setExecutionTarget({ kind: "local" });
    setCreateWslStatus(null);
    setCreateWslOptions([]);
    setCreateWslBusy(false);
    createWslRequestRef.current += 1;
    setNetworkProfile(emptyNetwork());
    setProxyImport("");
    setProxyUsername("");
    setProxyPassword("");
    setMihomoControllerUrl(defaultMihomoControllerUrl);
    setMihomoControllerSecret("");
    setMihomoSnapshot(null);
    setMihomoBusy(false);
    setNetworkResult(null);
    setNetworkBusy(false);
    setBusy(false);
    setNotice(null);
    setStatus((currentStatus) => scrubDesktopStatusForLockedUi(currentStatus));
    setView((currentView) =>
      currentView === "create" ||
      currentView === "edit" ||
      currentView === "settings"
        ? "overview"
        : currentView,
    );
  }, []);

  const applyVaultUiLock = useCallback(
    (invalidateSession: boolean) => {
      if (invalidateSession) {
        vaultUiSessionRef.current.invalidate();
      }
      unlockedOperationRef.current += 1;
      networkRequestRef.current += 1;
      mihomoRequestRef.current += 1;
      setUiVaultLocked(true);
      setVaultUiGeneration((generation) => generation + 1);
      scrubSensitiveUi();
    },
    [scrubSensitiveUi],
  );

  const refresh = useCallback(
    async (includeStorageUsage = true): Promise<VaultRefreshResult> => {
      const requestId = ++refreshRequestRef.current;
      let nextStatus: DesktopStatus;
      try {
        nextStatus = await desktopApi.status();
      } catch (error) {
        if (requestId !== refreshRequestRef.current) {
          return "stale";
        }
        throw error;
      }
      if (requestId !== refreshRequestRef.current) {
        return "stale";
      }

      const lockTransition = vaultUiSessionRef.current.observe(
        nextStatus.vault.state,
      );
      setStatus(
        nextStatus.vault.state === "unlocked"
          ? nextStatus
          : scrubDesktopStatusForLockedUi(nextStatus),
      );

      if (nextStatus.vault.state === "unlocked") {
        setUiVaultLocked(false);
        const sessionEpoch = vaultUiSessionRef.current.capture();
        let browserResult:
          | { ok: true; value: BrowserCandidate[] }
          | { ok: false; error: unknown }
          | null = null;
        let active: Silo[];
        let archived: Silo[];
        let evidence: SiloNetworkEvidence[];
        let legacyArtifacts: LegacyEnvironmentArtifact[];
        try {
          const listsPromise = Promise.all([
            desktopApi.listActiveSilos(),
            desktopApi.listArchivedSilos(),
            desktopApi.listNetworkEvidence(),
            desktopApi.listLegacyEnvironmentArtifacts(),
          ]);
          if (includeStorageUsage) {
            const [discovered, lists] = await Promise.all([
              desktopApi.discoverBrowsers().then(
                (value) => ({ ok: true as const, value }),
                (error: unknown) => ({ ok: false as const, error }),
              ),
              listsPromise,
            ]);
            browserResult = discovered;
            [active, archived, evidence, legacyArtifacts] = lists;
          } else {
            [active, archived, evidence, legacyArtifacts] = await listsPromise;
          }
        } catch (error) {
          if (
            requestId !== refreshRequestRef.current ||
            !vaultUiSessionRef.current.accepts(sessionEpoch)
          ) {
            return "stale";
          }
          throw error;
        }
        if (
          requestId !== refreshRequestRef.current ||
          !vaultUiSessionRef.current.accepts(sessionEpoch)
        ) {
          return "stale";
        }
        if (browserResult !== null) {
          setBrowsers(browserResult.ok ? browserResult.value : []);
        }
        setSilos(active);
        setArchivedSilos(archived);
        setNetworkEvidenceHistory(evidence);
        setLegacyEnvironmentArtifacts(legacyArtifacts);

        if (includeStorageUsage) {
          const usageEntries = await Promise.all(
            [...active, ...archived].map(async (silo) => {
              try {
                const usage = await desktopApi.siloStorageUsage(silo.id);
                return [silo.id, usage.bytes] as const;
              } catch {
                return [silo.id, null] as const;
              }
            }),
          );
          if (
            requestId !== refreshRequestRef.current ||
            !vaultUiSessionRef.current.accepts(sessionEpoch)
          ) {
            return "stale";
          }
          setStorageUsage(Object.fromEntries(usageEntries));
        }
        if (browserResult !== null && !browserResult.ok) {
          throw browserResult.error;
        }
      } else if (lockTransition) {
        applyVaultUiLock(false);
      } else {
        setUiVaultLocked(true);
      }
      return nextStatus.vault.state;
    },
    [applyVaultUiLock],
  );

  const refreshManagedBrowserStatus = useCallback(async () => {
    setManagedStatusBusy(true);
    setManagedStatusError(null);
    try {
      setEngineStatuses(await desktopApi.listEngineAdapters());
    } catch {
      setEngineStatuses([]);
      setManagedStatusError(
        "托管身份浏览器的可用状态暂时无法读取，请稍后重试。",
      );
    } finally {
      setManagedStatusBusy(false);
    }
  }, []);

  useEffect(() => {
    if (status?.vault.state !== "unlocked") {
      setEngineStatuses([]);
      setManagedStatusError(null);
      return;
    }
    void refreshManagedBrowserStatus();
  }, [refreshManagedBrowserStatus, status?.vault.state]);

  useEffect(() => {
    const refreshWithNotice = () =>
      void refresh().catch((error: unknown) =>
        setNotice({ tone: "error", message: errorMessage(error) }),
      );
    refreshWithNotice();
    const interval = window.setInterval(
      () =>
        void refresh(false).catch((error: unknown) =>
          setNotice({ tone: "error", message: errorMessage(error) }),
        ),
      30_000,
    );
    return () => window.clearInterval(interval);
  }, [refresh]);

  const pollLocalRuntimeStatus = useCallback(async () => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    if (!vaultUiSessionRef.current.accepts(sessionEpoch)) {
      return;
    }
    const nextStatus = await desktopApi.status();
    if (!vaultUiSessionRef.current.accepts(sessionEpoch)) {
      return;
    }
    const lockTransition = vaultUiSessionRef.current.observe(
      nextStatus.vault.state,
    );
    setStatus(
      nextStatus.vault.state === "unlocked"
        ? nextStatus
        : scrubDesktopStatusForLockedUi(nextStatus),
    );
    if (nextStatus.vault.state === "unlocked") {
      setUiVaultLocked(false);
    } else if (lockTransition) {
      applyVaultUiLock(false);
    } else {
      setUiVaultLocked(true);
    }
  }, [applyVaultUiLock]);

  const localRuntimeActive =
    (status?.activation.activeSiloId ?? null) !== null &&
    silos.some(
      (silo) =>
        silo.id === status?.activation.activeSiloId &&
        silo.executionTarget.kind === "local" &&
        silo.engine.adapter === "stock",
    );

  useEffect(() => {
    if (!localRuntimeActive) {
      return;
    }
    const interval = window.setInterval(
      () =>
        void pollLocalRuntimeStatus().catch((error: unknown) =>
          setNotice({ tone: "error", message: errorMessage(error) }),
        ),
      2_000,
    );
    return () => window.clearInterval(interval);
  }, [localRuntimeActive, pollLocalRuntimeStatus]);

  useEffect(() => {
    if (status?.vault.state !== "unlocked") {
      return;
    }
    const delay = vaultAutoLockRefreshDelay(status.vault.autoLockAt);
    if (delay === null) {
      return;
    }
    const timer = window.setTimeout(() => {
      applyVaultUiLock(true);
      void refresh(false).catch((error: unknown) =>
        setNotice({ tone: "error", message: errorMessage(error) }),
      );
    }, delay);
    return () => window.clearTimeout(timer);
  }, [
    applyVaultUiLock,
    refresh,
    status?.vault.autoLockAt,
    status?.vault.state,
  ]);

  const candidateOptions = useMemo(
    () => browsers.filter((candidate) => candidate.kind === browserKind),
    [browserKind, browsers],
  );

  useEffect(() => {
    if (
      !uiVaultLocked &&
      status?.vault.state === "unlocked" &&
      !browserSelectionExplicitRef.current &&
      browsers.length > 0 &&
      (browserPath === "" ||
        !browsers.some((candidate) => candidate.executablePath === browserPath))
    ) {
      const candidate =
        browsers.find((browser) => browser.kind === browserKind) ?? browsers[0];
      if (candidate !== undefined) {
        setBrowserKind(candidate.kind);
        setBrowserPath(candidate.executablePath);
      }
    }
  }, [browserKind, browserPath, browsers, status?.vault.state, uiVaultLocked]);

  const detectCreateWsl = useCallback(async () => {
    const requestId = ++createWslRequestRef.current;
    setCreateWslBusy(true);
    try {
      const nextStatus = await desktopApi.detectWsl();
      if (requestId !== createWslRequestRef.current) {
        return;
      }
      const nextOptions: WslCreationOption[] = [];
      if (nextStatus.available) {
        for (const distribution of nextStatus.distributions) {
          try {
            const backendStatus =
              await desktopApi.selectWslEnvironmentDistribution(distribution);
            if (requestId !== createWslRequestRef.current) {
              return;
            }
            const unavailableOperations = requiredWslCreationOperations.filter(
              (operation) =>
                !backendStatus.capabilities.some(
                  (capability) =>
                    capability.operation === operation &&
                    capability.availability.availability === "available",
                ),
            );
            nextOptions.push({
              distribution,
              ready:
                backendStatus.backend === "wsl-chromium" &&
                unavailableOperations.length === 0,
            });
          } catch {
            if (requestId !== createWslRequestRef.current) {
              return;
            }
            nextOptions.push({
              distribution,
              ready: false,
            });
          }
        }
      }
      setCreateWslStatus(nextStatus);
      setCreateWslOptions(nextOptions);
      const readyDistributions = new Set(
        nextOptions
          .filter((option) => option.ready)
          .map((option) => option.distribution),
      );
      setExecutionTarget((currentTarget) =>
        currentTarget.kind === "wsl" &&
        !readyDistributions.has(currentTarget.distribution)
          ? { kind: "local" }
          : currentTarget,
      );
    } catch (error) {
      if (requestId !== createWslRequestRef.current) {
        return;
      }
      setCreateWslStatus({
        supportedPlatform: false,
        available: false,
        distributions: [],
        message: errorMessage(error, "暂时无法检查这台电脑上的 Linux 环境。"),
      });
      setCreateWslOptions([]);
      setExecutionTarget((currentTarget) =>
        currentTarget.kind === "wsl" ? { kind: "local" } : currentTarget,
      );
    } finally {
      if (requestId === createWslRequestRef.current) {
        setCreateWslBusy(false);
      }
    }
  }, []);

  const activeSilos = silos;

  const withBusy = async (
    action: (isCurrent: () => boolean) => Promise<void>,
  ) => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    if (!vaultUiSessionRef.current.accepts(sessionEpoch)) {
      return;
    }
    const operationId = ++unlockedOperationRef.current;
    const isCurrent = () =>
      operationId === unlockedOperationRef.current &&
      vaultUiSessionRef.current.accepts(sessionEpoch);
    setBusy(true);
    try {
      await action(isCurrent);
    } catch (error) {
      if (isCurrent()) {
        setNotice({ tone: "error", message: errorMessage(error) });
      }
    } finally {
      if (operationId === unlockedOperationRef.current) {
        setBusy(false);
      }
    }
  };

  const withVaultBusy = async (action: () => Promise<void>) => {
    const operationId = ++vaultOperationRef.current;
    setVaultTransition("idle");
    setVaultBusy(true);
    try {
      await action();
    } catch (error) {
      if (operationId === vaultOperationRef.current) {
        setNotice({ tone: "error", message: errorMessage(error) });
      }
    } finally {
      if (operationId === vaultOperationRef.current) {
        setVaultBusy(false);
      }
    }
  };

  const submitVault = () =>
    withVaultBusy(async () => {
      if (passphrase.length < 12) {
        throw new UserFacingError("请使用至少 12 个字符的保险库口令。");
      }

      if (status?.vault.state === "uninitialized") {
        await desktopApi.initializeVault(passphrase);
        setNotice({
          tone: "success",
          message: "保险库已创建。请妥善保存口令；VeriSilo 不提供找回。",
        });
      } else {
        await desktopApi.unlockVault(passphrase);
        setNotice({ tone: "success", message: "保险库已解锁。" });
      }
      setPassphrase("");
      await refresh();
    });

  const lockVault = () =>
    withVaultBusy(async () => {
      await desktopApi.lockVault();
      applyVaultUiLock(true);
      setView("overview");
      setNotice({ tone: "info", message: "保险库已锁定。" });
      await refresh();
    });

  const loadRestoredVaultState = async (operationId: number) => {
    let result = await refresh();
    if (result === "stale" && operationId === vaultOperationRef.current) {
      result = await refresh();
    }
    const sessionEpoch = vaultUiSessionRef.current.capture();
    const acceptedState = acceptedRestoredVaultState(
      result,
      vaultUiSessionRef.current.accepts(sessionEpoch),
    );
    if (operationId !== vaultOperationRef.current || acceptedState === null) {
      throw new UserFacingError(
        "恢复后的保险库尚未完整载入。为了保护数据，当前操作已暂停；请使用下方按钮重试。",
      );
    }
    return acceptedState;
  };

  const completeVaultRestore = async (invalidatePreviousSession: boolean) => {
    const operationId = ++vaultOperationRef.current;
    setVaultTransition("restoring");
    setVaultBusy(true);
    setNotice(null);
    if (invalidatePreviousSession) {
      applyVaultUiLock(true);
    }
    try {
      const vaultState = await loadRestoredVaultState(operationId);
      if (operationId !== vaultOperationRef.current) {
        return;
      }
      setNotice({
        tone: vaultState === "unlocked" ? "success" : "info",
        message:
          vaultState === "unlocked"
            ? "加密保险库已恢复。请核对 Silo 列表；本机已有浏览器数据不会被备份文件覆盖。"
            : "加密保险库已恢复，但在载入期间已锁定。敏感界面状态仍为空，请重新解锁后核对 Silo 列表。",
      });
      setVaultTransition("idle");
    } catch (error) {
      if (operationId === vaultOperationRef.current) {
        setNotice({ tone: "error", message: errorMessage(error) });
      }
    } finally {
      if (operationId === vaultOperationRef.current) {
        setVaultBusy(false);
      }
    }
  };

  const finishVaultRestore = () => completeVaultRestore(true);
  const retryRestoredVaultState = () => completeVaultRestore(false);

  const chooseBrowser = (candidate: BrowserCandidate) => {
    browserSelectionExplicitRef.current = true;
    setBrowserKind(candidate.kind);
    setBrowserPath(candidate.executablePath);
  };

  const createSilo = () =>
    withBusy(async (isCurrent) => {
      if (executionTarget.kind === "remote") {
        throw new UserFacingError(
          "远程运行位置尚未完成网站可见身份核对，当前不能用于创建 Silo。",
        );
      }
      if (executionTarget.kind === "wsl" && networkProfile.mode !== "direct") {
        throw new UserFacingError(
          "Linux 环境当前仅支持直连。请改为本机运行，或将网络出口恢复为直连。",
        );
      }
      if (executionTarget.kind === "local" && browserPath.trim().length === 0) {
        throw new UserFacingError("请选择这台电脑上要使用的浏览器。");
      }
      if (!networkProfileSchema.safeParse(networkProfile).success) {
        throw new UserFacingError(
          "网络设置尚未填写完整。请检查代理地址、端口或自动代理配置。",
        );
      }
      const hasUsername = proxyUsername.trim() !== "";
      const hasPassword = proxyPassword !== "";
      if (hasUsername !== hasPassword) {
        throw new UserFacingError(
          "代理用户名和密码需要同时填写；无认证代理请都留空。",
        );
      }
      if (
        hasUsername &&
        networkProfile.mode === "fixed_proxy" &&
        !["http", "socks5"].includes(networkProfile.scheme)
      ) {
        throw new UserFacingError(
          "需要登录信息时，请使用 HTTP、SOCKS5，或交给本机 Mihomo / Clash 应用处理。",
        );
      }
      const input: CreateSiloInput & {
        executionTarget: SiloExecutionTarget;
      } = {
        name,
        color,
        browserKind: executionTarget.kind === "wsl" ? "chrome" : browserKind,
        executablePath:
          executionTarget.kind === "wsl" ? "/usr/bin/chromium" : browserPath,
        networkProfile,
        executionTarget,
        ...(networkProfile.mode === "fixed_proxy" && hasUsername
          ? {
              proxyCredentials: {
                username: proxyUsername.trim(),
                password: proxyPassword,
              },
            }
          : {}),
        ...(networkProfile.mode === "fixed_proxy" &&
        networkProfile.externalMihomo !== undefined &&
        mihomoControllerSecret !== ""
          ? {
              mihomoControllerSecret: { secret: mihomoControllerSecret },
            }
          : {}),
      };
      const silo = await desktopApi.createSilo(input);
      if (!isCurrent()) {
        return;
      }
      setName("");
      setExecutionTarget({ kind: "local" });
      setNetworkProfile(emptyNetwork());
      setProxyImport("");
      setProxyUsername("");
      setProxyPassword("");
      setMihomoControllerSecret("");
      setMihomoSnapshot(null);
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」。它不会读取或改写默认浏览器的数据。`,
      });
      setView("overview");
      await refresh();
    });

  const createManagedSilo = async (input: CreateManagedSiloInput) => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    if (!vaultUiSessionRef.current.accepts(sessionEpoch)) {
      throw new UserFacingError("保险库已锁定，请重新解锁后再试。");
    }
    const operationId = ++unlockedOperationRef.current;
    const isCurrent = () =>
      operationId === unlockedOperationRef.current &&
      vaultUiSessionRef.current.accepts(sessionEpoch);
    setBusy(true);
    setNotice(null);
    try {
      const managedEngine = engineStatuses.find(
        (engine) => engine.descriptor.id === "camoufox",
      );
      if (managedEngine?.health.state !== "healthy") {
        throw new UserFacingError(
          "托管身份浏览器当前不可用。请在“运行位置设置”中完成内置组件检查后重试。",
        );
      }
      const silo = await desktopApi.createManagedSilo(input);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」托管身份浏览器。首次启动时会建立独立身份；不会公开内部身份材料。`,
      });
      setView("overview");
      await refresh();
    } catch (error) {
      const message = managedErrorMessage(error);
      if (isCurrent()) {
        setNotice({ tone: "error", message });
      }
      throw new UserFacingError(message);
    } finally {
      if (operationId === unlockedOperationRef.current) {
        setBusy(false);
      }
    }
  };

  const launchSilo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const activation = await desktopApi.launchSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: activationNoticeTone(activation),
        message:
          activation.state === "running" &&
          silo.executionTarget.kind === "local" &&
          silo.engine.adapter === "stock"
            ? `已启动「${silo.name}」。使用完成后请直接关闭这个 Silo 的浏览器窗口；VeriSilo 会核对进程与 Profile 锁后回到空闲，不会强制关闭其他浏览器。`
            : describeActivation(activation),
      });
      await refresh();
    });

  const stopSilo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const activation = await desktopApi.stopSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: activationNoticeTone(activation),
        message: describeActivation(activation),
      });
      await refresh();
    });

  const recheckSiloBrowser = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const verification = await desktopApi.recheckSiloBrowser(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: verification.state === "verified" ? "success" : "error",
        message: browserVerificationMessage(verification),
      });
      await refresh();
    });

  const recheckSiloRuntime = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const activation = await desktopApi.recheckSiloRuntime(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: activation.state === "running" ? "success" : "error",
        message: describeActivation(activation),
      });
      await refresh(false);
    });

  const rebindSiloMihomo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const activation = await desktopApi.rebindSiloMihomo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: activation.state === "running" ? "success" : "error",
        message: describeActivation(activation),
      });
      await refresh(false);
    });

  const archiveSilo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      await desktopApi.archiveSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "info",
        message: `已归档「${silo.name}」。浏览器数据目录仍保留，未被删除。`,
      });
      await refresh();
    });

  const cleanupLegacyEnvironment = async (
    artifact: LegacyEnvironmentArtifact,
  ) => {
    const silo = [...silos, ...archivedSilos].find(
      (candidate) => candidate.id === artifact.siloId,
    );
    if (
      silo === undefined ||
      !window.confirm(
        `清理「${silo.name}」保留的旧${legacyEnvironmentLabel(artifact.backend)}？VeriSilo 会先核对它确实属于这个 Silo，再删除该旧环境；当前浏览器数据和运行位置不会改变。`,
      )
    ) {
      return;
    }
    await withBusy(async (isCurrent) => {
      await desktopApi.cleanupLegacyEnvironmentArtifact(
        artifact.siloId,
        artifact.backend,
      );
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "success",
        message: `已清理「${silo.name}」不再使用的旧${legacyEnvironmentLabel(artifact.backend)}。`,
      });
      await refresh();
    });
  };

  const updateSilo = (
    silo: Silo,
    input: UpdateSiloInput,
    networkInput: UpdateSiloNetworkInput | null,
    engineInput: UpdateSiloEngineInput | null,
  ) =>
    withBusy(async (isCurrent) => {
      const updated = await desktopApi.updateSiloConfiguration(
        silo.id,
        input,
        networkInput,
        engineInput,
      );
      if (!isCurrent()) {
        return;
      }
      setEditingSilo(null);
      setView("overview");
      setNotice({
        tone: "success",
        message: `已保存「${updated.name}」的资料${networkInput === null ? "" : "和网络设置"}${engineInput === null ? "" : "，并改用系统浏览器"}。原有浏览器数据和网站状态没有改变。`,
      });
      await refresh();
    });

  const restoreArchivedSilo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const restored = await desktopApi.restoreArchivedSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "success",
        message: `已恢复「${restored.name}」。原来的浏览器数据目录仍然沿用。`,
      });
      await refresh();
    });

  const deleteSilo = async (silo: Silo) => {
    const confirmed = window.confirm(
      `永久删除「${silo.name}」及其浏览器数据？Cookie、登录状态和站点数据都无法恢复。此操作不会影响默认 Chrome 或 Edge。`,
    );
    if (!confirmed) {
      return;
    }
    await withBusy(async (isCurrent) => {
      await desktopApi.deleteSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "info",
        message: `已永久删除「${silo.name}」的浏览器数据和保险库记录。`,
      });
      await refresh();
    });
  };

  const clearNetworkEvidence = async (silo: Silo) => {
    if (
      !window.confirm(
        `清除「${silo.name}」的网络检查历史？这不会改动浏览器数据或当前检查结果。`,
      )
    ) {
      return;
    }
    await withBusy(async (isCurrent) => {
      const removed = await desktopApi.clearNetworkEvidence(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "info",
        message: `已清除 ${removed} 条「${silo.name}」网络检查记录。`,
      });
      await refresh(false);
    });
  };

  const downloadLocalReport = (silo: Silo, format: "json" | "html") => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    if (status === null || !vaultUiSessionRef.current.accepts(sessionEpoch)) {
      return;
    }
    if (vaultAutoLockDeadlinePassed(status.vault.autoLockAt)) {
      applyVaultUiLock(true);
      void refresh(false).catch((error: unknown) =>
        setNotice({ tone: "error", message: errorMessage(error) }),
      );
      return;
    }
    const report = buildLocalSiloReport({
      generatedAt: new Date().toISOString(),
      silo,
      activation: status.activation,
      vaultEvidence: networkEvidenceHistory,
    });
    const content =
      format === "json"
        ? serializeLocalSiloReport(report)
        : renderLocalSiloReportHtml(report);
    const blob = new Blob([content], {
      type:
        format === "json"
          ? "application/json;charset=utf-8"
          : "text/html;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `verisilo-local-silo-report.${format}`;
    link.hidden = true;
    document.body.append(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
    setNotice({
      tone: "info",
      message: `已向系统发起「${silo.name}」脱敏 ${format.toUpperCase()} 报告下载；请在下载列表确认文件已落盘。`,
    });
  };

  const checkNetwork = async () => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    const requestId = ++networkRequestRef.current;
    const isCurrent = () =>
      requestId === networkRequestRef.current &&
      vaultUiSessionRef.current.accepts(sessionEpoch);
    if (!isCurrent()) {
      return;
    }
    setNetworkBusy(true);
    try {
      const result = await runDesktopNetworkCheck();
      if (!isCurrent()) {
        return;
      }
      setNetworkResult(result);
      const useful = result.ip !== null || result.dns.providers.length > 0;
      setNotice({
        tone: useful ? "success" : "error",
        message: useful
          ? "网络检查完成。结果只代表本次桌面端请求，不会自动判断 IP 是否“纯净”。"
          : "网络检查没有获得有效结果，请检查网络后重试。",
      });
    } catch (error) {
      if (isCurrent()) {
        setNotice({ tone: "error", message: errorMessage(error) });
      }
    } finally {
      if (requestId === networkRequestRef.current) {
        setNetworkBusy(false);
      }
    }
  };

  const importProxy = () => {
    try {
      const parsed = parseProxyInput(proxyImport);
      setNetworkProfile(parsed.profile);
      setProxyUsername(parsed.credentials?.username ?? "");
      setProxyPassword(parsed.credentials?.password ?? "");
      setProxyImport("");
      setMihomoSnapshot(null);
      setNotice({
        tone: "success",
        message: parsed.credentials
          ? "代理地址已解析；认证信息只会进入加密保险库，不会写入浏览器启动参数。"
          : "代理地址已解析。默认开启“必须代理”，端口失效时不会回退真实出口。",
      });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const inspectMihomoController = async () => {
    const sessionEpoch = vaultUiSessionRef.current.capture();
    const requestId = ++mihomoRequestRef.current;
    const isCurrent = () =>
      requestId === mihomoRequestRef.current &&
      vaultUiSessionRef.current.accepts(sessionEpoch);
    if (!isCurrent()) {
      return;
    }
    setMihomoBusy(true);
    try {
      const snapshot = await desktopApi.inspectMihomoController({
        controllerUrl: mihomoControllerUrl,
        secret: mihomoControllerSecret,
      });
      const group = snapshot.groups[0];
      if (group === undefined || group.nodes.length === 0) {
        throw new UserFacingError("本机代理应用没有返回可用的线路分组。");
      }
      const selectedNode =
        group.nodes.find((node) => node.name === group.selected) ??
        group.nodes[0];
      if (selectedNode === undefined) {
        throw new UserFacingError("所选线路分组中没有可用线路。");
      }
      if (networkProfile.mode !== "fixed_proxy") {
        throw new UserFacingError("请先选择 Mihomo / Clash 网络方式。");
      }
      if (!isCurrent()) {
        return;
      }
      setMihomoSnapshot(snapshot);
      setNetworkProfile({
        ...networkProfile,
        proxyRequired: true,
        scheme: "socks5",
        bypassList: [],
        externalMihomo: {
          controllerUrl: mihomoControllerUrl,
          selectorGroup: group.name,
          nodeName: selectedNode.name,
        },
      });
      setNotice({
        tone: "success",
        message: `已读取本机 Mihomo 控制器，并把这个 Silo 预绑定到「${selectedNode.name}」。创建后每次启动都会重新选择并复查。`,
      });
    } catch (error) {
      if (isCurrent()) {
        setMihomoSnapshot(null);
        setNotice({ tone: "error", message: errorMessage(error) });
      }
    } finally {
      if (requestId === mihomoRequestRef.current) {
        setMihomoBusy(false);
      }
    }
  };

  const selectMihomoGroup = (groupName: string) => {
    if (networkProfile.mode !== "fixed_proxy" || mihomoSnapshot === null) {
      return;
    }
    const group = mihomoSnapshot.groups.find((item) => item.name === groupName);
    const node =
      group?.nodes.find((item) => item.name === group.selected) ??
      group?.nodes[0];
    if (group === undefined || node === undefined) {
      return;
    }
    setNetworkProfile({
      ...networkProfile,
      proxyRequired: true,
      scheme: "socks5",
      bypassList: [],
      externalMihomo: {
        controllerUrl: mihomoControllerUrl,
        selectorGroup: group.name,
        nodeName: node.name,
      },
    });
  };

  const selectMihomoNode = (nodeName: string) => {
    if (
      networkProfile.mode !== "fixed_proxy" ||
      networkProfile.externalMihomo === undefined
    ) {
      return;
    }
    setNetworkProfile({
      ...networkProfile,
      externalMihomo: {
        ...networkProfile.externalMihomo,
        nodeName,
      },
    });
  };

  if (status === null) {
    return (
      <main className="shell loading-state">
        <Brand />
        <p>正在读取本机 VeriSilo 状态…</p>
      </main>
    );
  }

  const vaultLocked = uiVaultLocked || status.vault.state !== "unlocked";

  return (
    <main className="shell">
      <header className="topbar">
        <Brand />
        <div className="topbar-status">
          <span className="local-pill">本地控制</span>
          <span className={`vault-pill${vaultLocked ? " locked" : ""}`}>
            {vaultTransition === "restoring"
              ? vaultBusy
                ? "正在载入恢复后的保险库"
                : "恢复状态等待重新载入"
              : vaultLocked
                ? "保险库已锁定"
                : "保险库已解锁"}
          </span>
        </div>
      </header>

      {vaultTransition === "idle" ? (
        <nav className="tabbar" aria-label="VeriSilo 桌面端功能">
          <TabButton
            active={view === "overview" || view === "edit"}
            label="环境概览"
            onClick={() => setView("overview")}
          />
          <TabButton
            active={view === "create"}
            label="创建 Silo"
            onClick={() => setView("create")}
          />
          <TabButton
            active={view === "settings"}
            label="保险库与数据"
            onClick={() => setView("settings")}
          />
          <TabButton
            active={view === "environments"}
            label="运行位置设置"
            onClick={() => setView("environments")}
          />
        </nav>
      ) : null}

      {notice !== null ? (
        <div
          aria-live={notice.tone === "error" ? "assertive" : "polite"}
          className={`notice ${notice.tone}`}
          role={notice.tone === "error" ? "alert" : "status"}
        >
          {notice.message}
        </div>
      ) : null}

      {view === "overview" ? (
        vaultTransition === "restoring" ? (
          <section
            aria-busy="true"
            aria-live="polite"
            className="panel loading-state"
          >
            <p className="eyebrow">保险库恢复</p>
            <h1>{vaultBusy ? "正在恢复安全状态…" : "恢复尚未完成"}</h1>
            <p>
              VeriSilo
              已清除旧会话中的表单和授权信息。完成保险库与运行状态核对前，
              相关操作会保持锁定。
            </p>
            <button
              disabled={vaultBusy}
              onClick={() => void retryRestoredVaultState()}
              type="button"
            >
              {vaultBusy ? "正在重新载入…" : "重新载入"}
            </button>
          </section>
        ) : vaultLocked ? (
          <VaultAccess
            busy={vaultBusy}
            passphrase={passphrase}
            setPassphrase={setPassphrase}
            status={status}
            submitVault={submitVault}
          />
        ) : (
          <>
            <section className="overview-hero panel">
              <div>
                <p className="eyebrow">环境概览</p>
                <h1>
                  {activeSilos.length === 0
                    ? "创建你的第一个 Silo"
                    : `${activeSilos.length} 个 Silo`}
                </h1>
                <p>
                  每个 Silo 的 Cookie、浏览记录和网站权限都分别保存在自己的本机
                  浏览器数据中。若浏览器账号开启了同步，同步内容仍由浏览器服务商管理。
                </p>
              </div>
              <div className="hero-actions">
                <button onClick={() => setView("create")} type="button">
                  新建 Silo
                </button>
                <button
                  className="button-secondary"
                  disabled={busy || vaultBusy}
                  onClick={() => void lockVault()}
                  type="button"
                >
                  立即锁定
                </button>
              </div>
            </section>

            <SiloList
              activation={status.activation.activeSiloId}
              busy={busy}
              onArchive={archiveSilo}
              onCreate={() => setView("create")}
              onEdit={(silo) => {
                setEditingSilo(silo);
                setView("edit");
              }}
              onLaunch={launchSilo}
              onRebindMihomo={rebindSiloMihomo}
              onRecheckBrowser={recheckSiloBrowser}
              onRecheckRuntime={recheckSiloRuntime}
              onStop={stopSilo}
              runtimeActivation={status.activation}
              runtimeState={status.activation.state}
              silos={activeSilos}
              networkEvidence={networkEvidenceHistory}
              managedEngineReady={engineStatuses.some(
                (engine) =>
                  engine.descriptor.id === "camoufox" &&
                  engine.health.state === "healthy",
              )}
              storageUsage={storageUsage}
            />

            <LegacyEnvironmentRecoveryPanel
              artifacts={legacyEnvironmentArtifacts}
              busy={busy}
              onCleanup={cleanupLegacyEnvironment}
              silos={[...activeSilos, ...archivedSilos]}
            />

            <ArchivedSiloList
              busy={busy}
              onDelete={deleteSilo}
              onRestore={restoreArchivedSilo}
              silos={archivedSilos}
              storageUsage={storageUsage}
            />

            <section className="status-grid" aria-label="当前状态">
              <StatusCard
                detail={describeVault(status.vault)}
                eyebrow="本地保险库"
                tone="good"
                value="已解锁"
              />
              <StatusCard
                detail={describeActivation(status.activation)}
                eyebrow="浏览器进程"
                tone={activationStatusTone(status.activation)}
                value={activationStatusLabel(status.activation.state)}
              />
              <StatusCard
                detail={
                  browsers.length === 0
                    ? "可在创建页手动填写绝对路径"
                    : browsers.map((browser) => browser.displayName).join("、")
                }
                eyebrow="已发现浏览器"
                tone={browsers.length > 0 ? "good" : "warn"}
                value={`${browsers.length} 个安装`}
              />
              <StatusCard
                detail={
                  networkResult === null
                    ? "不会自动连接任何检测服务"
                    : `检查于 ${formatDate(networkResult.checkedAt)}`
                }
                eyebrow="当前网络出口"
                tone={
                  networkResult !== null && networkResult.ip !== null
                    ? "good"
                    : "neutral"
                }
                value={
                  networkResult?.ip?.address ??
                  (networkResult === null ? "尚未验证" : "获取失败")
                }
              />
            </section>

            <NetworkCheckCard
              busy={networkBusy}
              onCheck={() => void checkNetwork()}
              onClear={() => setNetworkResult(null)}
              result={networkResult}
            />

            <SiloNetworkEvidenceHistory
              busy={busy}
              evidence={networkEvidenceHistory}
              onClear={clearNetworkEvidence}
              silos={[...activeSilos, ...archivedSilos]}
            />

            <LocalReportExportCard
              busy={busy}
              evidence={networkEvidenceHistory}
              onDownload={downloadLocalReport}
              silos={[...activeSilos, ...archivedSilos]}
            />
          </>
        )
      ) : null}

      {view === "create" ? (
        vaultLocked ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <CreateSiloPanel
            browserKind={browserKind}
            browserPath={browserPath}
            busy={busy}
            candidateOptions={candidateOptions}
            chooseBrowser={chooseBrowser}
            color={color}
            executionTarget={executionTarget}
            createSilo={createSilo}
            createManagedSilo={createManagedSilo}
            managedEngineReady={engineStatuses.some(
              (engine) =>
                engine.descriptor.id === "camoufox" &&
                engine.health.state === "healthy",
            )}
            managedStatusBusy={managedStatusBusy}
            managedStatusError={managedStatusError}
            refreshManagedStatus={refreshManagedBrowserStatus}
            name={name}
            importProxy={importProxy}
            inspectMihomoController={inspectMihomoController}
            mihomoBusy={mihomoBusy}
            mihomoControllerSecret={mihomoControllerSecret}
            mihomoControllerUrl={mihomoControllerUrl}
            mihomoSnapshot={mihomoSnapshot}
            networkProfile={networkProfile}
            proxyImport={proxyImport}
            proxyPassword={proxyPassword}
            proxyUsername={proxyUsername}
            refreshWsl={detectCreateWsl}
            resetMihomoSnapshot={() => setMihomoSnapshot(null)}
            selectMihomoGroup={selectMihomoGroup}
            selectMihomoNode={selectMihomoNode}
            setBrowserKind={(kind) => {
              browserSelectionExplicitRef.current = true;
              setBrowserKind(kind);
              setBrowserPath(
                browsers.find((browser) => browser.kind === kind)
                  ?.executablePath ?? "",
              );
            }}
            setBrowserPath={(path) => {
              browserSelectionExplicitRef.current = true;
              setBrowserPath(path);
            }}
            setColor={setColor}
            setExecutionTarget={(target) => {
              setExecutionTarget(target);
              if (target.kind === "wsl") {
                setNetworkProfile(emptyNetwork());
                setProxyImport("");
                setProxyUsername("");
                setProxyPassword("");
                setMihomoControllerSecret("");
                setMihomoSnapshot(null);
              }
            }}
            setMihomoControllerSecret={setMihomoControllerSecret}
            setMihomoControllerUrl={(value) => {
              setMihomoControllerUrl(value);
              setMihomoSnapshot(null);
              if (
                networkProfile.mode === "fixed_proxy" &&
                networkProfile.externalMihomo !== undefined
              ) {
                const { externalMihomo: _binding, ...withoutBinding } =
                  networkProfile;
                setNetworkProfile(withoutBinding);
              }
            }}
            setName={setName}
            setNetworkProfile={setNetworkProfile}
            setProxyImport={setProxyImport}
            setProxyPassword={setProxyPassword}
            setProxyUsername={setProxyUsername}
            wslBusy={createWslBusy}
            wslOptions={createWslOptions}
            wslStatus={createWslStatus}
          />
        )
      ) : null}

      {view === "edit" ? (
        vaultLocked || editingSilo === null ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <EditSiloPanel
            browsers={browsers}
            busy={busy}
            onCancel={() => {
              setEditingSilo(null);
              setView("overview");
            }}
            onSave={(input, networkInput, engineInput) =>
              updateSilo(editingSilo, input, networkInput, engineInput)
            }
            silo={editingSilo}
          />
        )
      ) : null}

      {view === "settings" ? (
        vaultLocked ? (
          <LockedRoute onUnlock={() => setView("overview")} />
        ) : (
          <VaultAndDataPanel
            busy={busy}
            onNotice={setNotice}
            onRefresh={refresh}
            onVaultRestored={finishVaultRestore}
            runBusy={withBusy}
          />
        )
      ) : null}

      {view === "environments" ? (
        <EnvironmentWorkspace
          key={`${vaultUiGeneration}:${vaultLocked ? "locked" : "unlocked"}`}
          silos={activeSilos}
          vaultLocked={vaultLocked}
        />
      ) : null}

      <footer>
        VeriSilo 帮你把不同用途的浏览活动分开放置。隔离可以减少数据混用，
        但不能替代账号安全和正常的风险判断。关闭窗口后会留在系统托盘；请从托盘菜单退出
        VeriSilo。
      </footer>
    </main>
  );
}

function Brand() {
  return (
    <div className="brand">
      <img
        alt=""
        aria-hidden="true"
        className="brand-mark"
        src="/verisilo-mark.svg"
      />
      <div>
        <strong>VeriSilo</strong>
        <span>让不同用途的浏览数据各自分开</span>
      </div>
    </div>
  );
}

function TabButton({
  active,
  label,
  onClick,
}: {
  active: boolean;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-pressed={active}
      className="tab"
      onClick={onClick}
      type="button"
    >
      {label}
    </button>
  );
}

function VaultAccess({
  busy,
  passphrase,
  setPassphrase,
  status,
  submitVault,
}: {
  busy: boolean;
  passphrase: string;
  setPassphrase: (value: string) => void;
  status: DesktopStatus;
  submitVault: () => Promise<void>;
}) {
  const initialize = status.vault.state === "uninitialized";
  return (
    <section className="vault-layout">
      <article className="panel vault-intro">
        <p className="eyebrow">本地保险库</p>
        <h1>{initialize ? "先保护你的 Silo 配置" : "欢迎回来"}</h1>
        <p>
          保险库会加密保存 Silo 配置、身份绑定和可选网络设置。每个 Silo
          的浏览器数据保存在这台电脑上的独立 Profile 中，不会被复制进保险库。
        </p>
        <ul className="plain-list">
          <li>默认 15 分钟自动锁定</li>
          <li>口令只在本机使用</li>
          <li>没有云端找回机制</li>
        </ul>
      </article>
      <article className="panel vault-form">
        <h2>{initialize ? "创建保险库" : "解锁保险库"}</h2>
        <p>请输入至少 12 个字符。遗忘口令后无法恢复。</p>
        <label>
          保险库口令
          <input
            aria-label="保险库口令"
            autoComplete={initialize ? "new-password" : "current-password"}
            disabled={busy}
            minLength={12}
            onChange={(event) => setPassphrase(event.target.value)}
            type="password"
            value={passphrase}
          />
        </label>
        <button
          disabled={busy || passphrase.length < 12}
          onClick={() => void submitVault()}
          type="button"
        >
          {initialize ? "创建本地保险库" : "解锁"}
        </button>
      </article>
    </section>
  );
}

function LockedRoute({ onUnlock }: { onUnlock: () => void }) {
  return (
    <section className="panel locked-route">
      <h1>先解锁保险库</h1>
      <p>解锁后才能读取并管理你的 Silo 配置。</p>
      <button onClick={onUnlock} type="button">
        返回解锁
      </button>
    </section>
  );
}

function StatusCard({
  detail,
  eyebrow,
  tone,
  value,
}: {
  detail: string;
  eyebrow: string;
  tone: "good" | "warn" | "neutral";
  value: string;
}) {
  return (
    <article className="status-card">
      <span className="status-label">{eyebrow}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
      <span className={`status-dot ${tone}`} aria-hidden="true" />
    </article>
  );
}

function NetworkCheckCard({
  busy,
  onCheck,
  onClear,
  result,
}: {
  busy: boolean;
  onCheck: () => void;
  onClear: () => void;
  result: NetworkCheckResult | null;
}) {
  return (
    <section className="panel network-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">用户主动验证</p>
          <h2>检查桌面端实际看到的网络出口</h2>
          <p>
            这项检查从 VeriSilo 桌面界面发出，用于确认公网
            IP、地区、ASN、出口时区和公共 DNS
            结果；它不代表某个已启动浏览器标签页一定走相同路径。
          </p>
        </div>
        <div className="panel-actions">
          <button disabled={busy} onClick={onCheck} type="button">
            {busy ? "正在检查…" : result === null ? "同意并检查" : "重新检查"}
          </button>
          {result !== null ? (
            <button
              className="button-secondary"
              onClick={onClear}
              type="button"
            >
              清除
            </button>
          ) : null}
        </div>
      </div>

      {result === null ? (
        <div className="empty-inline">
          <strong>尚未向三方检测端点发送请求</strong>
          <span>
            点击后会连接 ipwho.is、Cloudflare 1.1.1.1 和 Google Public
            DNS；三方会看到请求 IP，结果只保存在本次界面内存中。
          </span>
        </div>
      ) : (
        <div className="network-result">
          <div className="network-primary">
            <span>公网 IP</span>
            <strong>{result.ip?.address ?? "获取失败"}</strong>
            <small>{networkLocation(result)}</small>
          </div>
          <div className="network-details">
            <ResultItem label="网络归属" value={networkOwner(result)} />
            <ResultItem
              label="出口时区"
              value={result.ip?.timezone ?? "未知"}
            />
            <ResultItem label="公共 DNS" value={dnsStateLabel(result)} />
            <ResultItem
              label="DNSSEC"
              value={
                result.dns.dnssec === "validated"
                  ? "两家均返回已验证"
                  : "未完整验证"
              }
            />
          </div>
          <div className="network-chips">
            <span className={result.ip === null ? "warn" : "good"}>
              {result.ip === null ? "IP 未确认" : "IP 已确认"}
            </span>
            <span
              className={result.dns.state === "consistent" ? "good" : "warn"}
            >
              {dnsStateLabel(result)}
            </span>
          </div>
          <p className="scope-copy">
            DNS 结果只反映本次检查。网络设置发生变化后，请重新检查。
          </p>
          {result.errors.length > 0 ? (
            <details className="error-details">
              <summary>查看部分检查错误</summary>
              <ul>
                {result.errors.map((error) => (
                  <li key={error}>{error}</li>
                ))}
              </ul>
            </details>
          ) : null}
        </div>
      )}
    </section>
  );
}

function ResultItem({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function SiloNetworkEvidenceHistory({
  busy,
  evidence,
  onClear,
  silos,
}: {
  busy: boolean;
  evidence: SiloNetworkEvidence[];
  onClear: (silo: Silo) => Promise<void>;
  silos: Silo[];
}) {
  if (evidence.length === 0) {
    return (
      <section className="panel evidence-history-panel empty-evidence-history">
        <div>
          <p className="eyebrow">网络检查记录</p>
          <h2>还没有 Silo 的检查结果</h2>
          <p>
            启动一个 Silo 后，从浏览器侧边栏运行网络检查。结果会加密保存在本机。
          </p>
        </div>
      </section>
    );
  }

  const siloById = new Map(silos.map((silo) => [silo.id, silo]));
  const visible = evidence.slice(0, 12);
  const clearableSilos = [
    ...new Map(
      visible
        .map((entry) => siloById.get(entry.siloId))
        .filter((silo): silo is Silo => silo !== undefined)
        .map((silo) => [silo.id, silo]),
    ).values(),
  ];

  return (
    <section className="panel evidence-history-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">网络检查记录</p>
          <h2>最近的 Silo 网络检查</h2>
          <p>
            这些结果来自你在 Silo 内主动运行的检查，并加密保存在这台电脑上。
          </p>
        </div>
        <span className="provider-badge">最近 {visible.length} 条</span>
      </div>
      <div className="evidence-history-list">
        {visible.map((entry) => {
          const silo = siloById.get(entry.siloId);
          const ip = entry.result.ip;
          return (
            <article className="evidence-history-row" key={entry.evidenceId}>
              <div className="evidence-history-heading">
                <span
                  className="silo-mark small-mark"
                  style={{ backgroundColor: silo?.color ?? "#667085" }}
                >
                  {(silo?.name ?? "?").slice(0, 1).toUpperCase()}
                </span>
                <div>
                  <strong>{silo?.name ?? "已删除的 Silo"}</strong>
                  <span>{formatDate(entry.result.checkedAt)}</span>
                </div>
              </div>
              <dl className="evidence-history-facts">
                <div>
                  <dt>当次请求出口</dt>
                  <dd>{ip?.address ?? "本次未取得"}</dd>
                </div>
                <div>
                  <dt>地区与网络</dt>
                  <dd>
                    {ip === null
                      ? "无可用观察"
                      : [
                          ip.countryCode ?? ip.country,
                          ip.region,
                          ip.city,
                          ip.asn,
                        ]
                          .filter((value): value is string => value !== null)
                          .join(" · ") || "未返回"}
                  </dd>
                </div>
                <div>
                  <dt>公共 DNS</dt>
                  <dd>{dnsStateLabel(entry.result)}</dd>
                </div>
              </dl>
            </article>
          );
        })}
      </div>
      <div className="evidence-history-actions">
        {clearableSilos.map((silo) => (
          <button
            className="button-secondary"
            disabled={busy}
            key={silo.id}
            onClick={() => void onClear(silo)}
            type="button"
          >
            清除「{silo.name}」记录
          </button>
        ))}
      </div>
    </section>
  );
}

function LocalReportExportCard({
  busy,
  evidence,
  onDownload,
  silos,
}: {
  busy: boolean;
  evidence: SiloNetworkEvidence[];
  onDownload: (silo: Silo, format: "json" | "html") => void;
  silos: Silo[];
}) {
  const [selectedSiloId, setSelectedSiloId] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const selectedSilo = silos.find((silo) => silo.id === selectedSiloId);
  const selectedEvidenceCount = evidence.filter(
    (entry) => entry.siloId === selectedSiloId,
  ).length;
  const canDownload = selectedSilo !== undefined && confirmed && !busy;

  return (
    <section className="panel report-export-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">隐私检查报告</p>
          <h2>导出一个 Silo 的检查结果</h2>
          <p>报告只在这台电脑上生成，不会上传，也不会自动保存。</p>
        </div>
        <span className="provider-badge">默认脱敏</span>
      </div>

      <div className="report-export-controls">
        <label>
          要导出的 Silo
          <select
            aria-label="要导出的 Silo"
            disabled={busy}
            onChange={(event) => {
              setSelectedSiloId(event.target.value);
              setConfirmed(false);
            }}
            value={selectedSiloId}
          >
            <option value="">请选择一个 Silo</option>
            {silos.map((silo) => (
              <option key={silo.id} value={silo.id}>
                {silo.name}
                {silo.archivedAt === null ? "" : "（已归档）"}
              </option>
            ))}
          </select>
        </label>
        <div className="report-selection-summary" aria-live="polite">
          {selectedSilo === undefined
            ? "先选择 Silo，报告不会默认包含任何 Silo。"
            : `报告将包含 ${selectedEvidenceCount} 条该 Silo 的网络检查记录和当前设置。`}
        </div>
        <label className="report-confirmation">
          <input
            checked={confirmed}
            disabled={selectedSilo === undefined || busy}
            onChange={(event) => setConfirmed(event.target.checked)}
            type="checkbox"
          />
          <span>
            我确认只导出「{selectedSilo?.name ?? "所选 Silo"}」的脱敏本地报告。
          </span>
        </label>
        <div className="report-export-actions">
          <button
            disabled={!canDownload}
            onClick={() =>
              selectedSilo === undefined
                ? undefined
                : onDownload(selectedSilo, "html")
            }
            type="button"
          >
            下载报告
          </button>
          <button
            className="button-secondary"
            disabled={!canDownload}
            onClick={() =>
              selectedSilo === undefined
                ? undefined
                : onDownload(selectedSilo, "json")
            }
            type="button"
          >
            下载数据文件
          </button>
        </div>
      </div>

      <div className="report-boundary">
        <strong>报告说明</strong>
        <p>
          报告包含浏览器类型、版本和网络检查结果。DNS 信息只反映检查当时的结果。
        </p>
      </div>
      <details className="report-developer-details">
        <summary>报告中不包含的内容</summary>
        <p>
          报告不会包含浏览器数据位置、代理地址、完整 IP、城市、访问密钥、凭据
          或其他可以直接识别本机配置的信息。
        </p>
      </details>
    </section>
  );
}

function SiloList({
  activation,
  busy,
  managedEngineReady,
  networkEvidence,
  onArchive,
  onCreate,
  onEdit,
  onLaunch,
  onRebindMihomo,
  onRecheckBrowser,
  onRecheckRuntime,
  onStop,
  runtimeActivation,
  runtimeState,
  silos,
  storageUsage,
}: {
  activation: string | null;
  busy: boolean;
  managedEngineReady: boolean;
  networkEvidence: SiloNetworkEvidence[];
  onArchive: (silo: Silo) => Promise<void>;
  onCreate: () => void;
  onEdit: (silo: Silo) => void;
  onLaunch: (silo: Silo) => Promise<void>;
  onRebindMihomo: (silo: Silo) => Promise<void>;
  onRecheckBrowser: (silo: Silo) => Promise<void>;
  onRecheckRuntime: (silo: Silo) => Promise<void>;
  onStop: (silo: Silo) => Promise<void>;
  runtimeActivation: DesktopStatus["activation"];
  runtimeState: DesktopStatus["activation"]["state"];
  silos: Silo[];
  storageUsage: Record<string, number | null>;
}) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">我的 Silo</p>
          <h2>选择一个浏览器空间</h2>
          <p>
            切换 Silo 就是在切换一整套浏览器数据，而不是只替换 Cookie。
            一次只运行一个 Silo。
          </p>
        </div>
        <button className="button-secondary" onClick={onCreate} type="button">
          新建
        </button>
      </div>
      {silos.length === 0 ? (
        <div className="empty-silos">
          <strong>还没有 Silo</strong>
          <p>创建一个工作、个人或临时用途的独立浏览器环境。</p>
          <button onClick={onCreate} type="button">
            创建第一个 Silo
          </button>
        </div>
      ) : (
        <div className="silo-grid">
          {silos.map((silo) => {
            const managedCamoufox = silo.engine.adapter === "camoufox";
            const canStop =
              activation === silo.id &&
              runtimeState === "running" &&
              (silo.executionTarget.kind === "wsl" || managedCamoufox);
            return (
              <article className="silo-card" key={silo.id}>
                <div className="silo-heading">
                  <span
                    className="silo-mark"
                    style={{ backgroundColor: silo.color }}
                  >
                    {silo.name.slice(0, 1).toUpperCase()}
                  </span>
                  <div>
                    <h3>{silo.name}</h3>
                    <p>{siloBrowserLabel(silo)}</p>
                  </div>
                  {activation === silo.id ? (
                    <span className="running-badge">
                      {runtimeState === "verification_failed"
                        ? "网络已阻断"
                        : runtimeState === "recovery_required"
                          ? "待恢复核对"
                          : "运行中"}
                    </span>
                  ) : null}
                </div>
                <dl className="silo-facts">
                  <div>
                    <dt>网站数据</dt>
                    <dd>
                      与其他 Silo 分开保存
                      {formatStorageSuffix(storageUsage[silo.id])}
                    </dd>
                  </div>
                  <div>
                    <dt>运行位置</dt>
                    <dd>{siloExecutionTargetLabel(silo)}</dd>
                  </div>
                  <div>
                    <dt>网站可见身份</dt>
                    <dd>{siloWebsiteIdentityBoundary(silo)}</dd>
                  </div>
                  {silo.engine.adapter === "stock" ? (
                    <>
                      <div>
                        <dt>Profile 隔离</dt>
                        <dd>
                          <CapabilityState state="native" />
                          每个 Silo 使用独立浏览器数据目录
                        </dd>
                      </div>
                      <div>
                        <dt>设备与指纹</dt>
                        <dd>
                          <CapabilityState state="inherit" />
                          跟随这台电脑和系统浏览器
                        </dd>
                      </div>
                      <div>
                        <dt>受控指纹</dt>
                        <dd>
                          <CapabilityState state="unavailable" />
                          Standard Silo 不提供受控指纹能力
                        </dd>
                      </div>
                    </>
                  ) : null}
                  <div>
                    <dt>网络</dt>
                    <dd>{describeNetwork(silo.networkProfile)}</dd>
                  </div>
                  <div>
                    <dt>身份状态</dt>
                    <dd>
                      <span
                        className={`identity-lock-state${
                          silo.identityLockedAt === null
                            ? " pending"
                            : " locked"
                        }`}
                      >
                        {silo.identityLockedAt === null
                          ? "首次成功启动时锁定浏览器与运行位置"
                          : "浏览器与运行位置已锁定"}
                      </span>
                    </dd>
                  </div>
                  {silo.networkProfile.mode === "fixed_proxy" &&
                  silo.networkProfile.credentialRef !== undefined ? (
                    <div>
                      <dt>代理认证</dt>
                      <dd>凭据已加密，启动时走本机中继</dd>
                    </div>
                  ) : null}
                  <div>
                    <dt>网络检查</dt>
                    <dd>可从浏览器侧边栏中按需运行</dd>
                  </div>
                </dl>
                {silo.engine.adapter !== "stock" ? (
                  <ManagedStatusGroups
                    activation={runtimeActivation}
                    evidence={networkEvidence}
                    engineHealthy={managedEngineReady}
                    runtimeState={
                      activation === silo.id ? runtimeState : "idle"
                    }
                    silo={silo}
                  />
                ) : null}
                {activation === silo.id &&
                silo.executionTarget.kind === "local" &&
                silo.engine.adapter === "stock" ? (
                  <p className="local-runtime-guidance">
                    使用完成后，直接关闭这个 Silo 的浏览器窗口即可停止。VeriSilo
                    会核对进程与 Profile 锁；不会强制关闭其他 Chrome 或 Edge
                    窗口。
                  </p>
                ) : null}
                <div className="card-actions">
                  <button
                    disabled={busy || (activation !== null && !canStop)}
                    onClick={() =>
                      void (canStop ? onStop(silo) : onLaunch(silo))
                    }
                    type="button"
                  >
                    {canStop
                      ? managedCamoufox
                        ? "停止托管浏览器"
                        : "停止 Silo"
                      : activation === silo.id &&
                          silo.executionTarget.kind === "local"
                        ? managedCamoufox
                          ? "停止托管浏览器"
                          : "关闭浏览器窗口以停止"
                        : "启动 Silo"}
                  </button>
                  <button
                    className="button-secondary"
                    disabled={busy || activation === silo.id}
                    onClick={() => onEdit(silo)}
                    type="button"
                  >
                    编辑
                  </button>
                  <button
                    className="button-secondary"
                    disabled={busy || activation === silo.id}
                    onClick={() => void onArchive(silo)}
                    type="button"
                  >
                    归档
                  </button>
                  {activation === silo.id ? (
                    <button
                      className="button-secondary"
                      disabled={busy}
                      onClick={() => void onRecheckRuntime(silo)}
                      type="button"
                    >
                      {runtimeState === "recovery_required"
                        ? "核对恢复状态"
                        : runtimeState === "verification_failed"
                          ? "检查网络阻断"
                          : "检查运行状态"}
                    </button>
                  ) : (
                    <button
                      className="button-secondary"
                      disabled={busy || runtimeState === "verification_failed"}
                      onClick={() => void onRecheckBrowser(silo)}
                      type="button"
                    >
                      检查浏览器
                    </button>
                  )}
                  {activation === silo.id &&
                  silo.networkProfile.mode === "fixed_proxy" &&
                  silo.networkProfile.externalMihomo !== undefined ? (
                    <button
                      className="button-secondary"
                      disabled={busy}
                      onClick={() => void onRebindMihomo(silo)}
                      type="button"
                    >
                      {runtimeState === "verification_failed"
                        ? "关闭浏览器后重新启动"
                        : "重新连接代理节点"}
                    </button>
                  ) : null}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}

type ManagedUiState =
  | "configured"
  | "reachable"
  | "applied"
  | "observed"
  | "verified"
  | "unavailable"
  | "not_requested";

function managedUiStateLabel(state: ManagedUiState): string {
  const labels: Record<ManagedUiState, string> = {
    configured: "已配置",
    reachable: "可达",
    applied: "已应用",
    observed: "已观察",
    verified: "已验证",
    unavailable: "不可用",
    not_requested: "未请求",
  };
  return `${state} · ${labels[state]}`;
}

function ManagedStatusGroups({
  activation,
  evidence,
  engineHealthy,
  runtimeState,
  silo,
}: {
  activation: DesktopStatus["activation"];
  evidence: SiloNetworkEvidence[];
  engineHealthy: boolean;
  runtimeState: DesktopStatus["activation"]["state"];
  silo: Silo;
}) {
  const runtimeApplied = ["preflight", "launching", "running"].includes(
    runtimeState,
  );
  const latestEvidence = evidence.some((entry) => entry.siloId === silo.id);
  const activeEvidence = activation.activeSiloId === silo.id;
  const engineEvidence = activeEvidence ? activation.engineEvidence : null;
  const networkEvidence = activeEvidence ? activation.networkEvidence : null;
  const artifactConfigured =
    silo.engine.adapter === "camoufox" &&
    silo.engine.artifactBinding !== undefined;
  const hostBindingVerified =
    engineEvidence?.verifiedAdapter === "camoufox" &&
    engineEvidence.hostLaunch === "verified";
  const packageVerified =
    engineEvidence?.packageVerification === "verified" &&
    engineEvidence.packageVerificationDetails !== null;
  const networkState: ManagedUiState =
    networkEvidence === null
      ? "configured"
      : networkEvidence.exit === "observed"
        ? "observed"
        : networkEvidence.browserRouting === "applied"
          ? "applied"
          : networkEvidence.endpoint === "reachable"
            ? "reachable"
            : [
                  networkEvidence.configuration,
                  networkEvidence.endpoint,
                  networkEvidence.browserRouting,
                ].some((state) => state === "failed" || state === "unavailable")
              ? "unavailable"
              : "configured";
  const packageDetails = engineEvidence?.packageVerificationDetails;
  const states: Array<[string, ManagedUiState, string]> = [
    [
      "Profile",
      hostBindingVerified ? "applied" : "configured",
      "独立 Profile 已准备；运行时才会应用。",
    ],
    [
      "Artifact",
      artifactConfigured
        ? hostBindingVerified
          ? "applied"
          : "configured"
        : "unavailable",
      artifactConfigured
        ? "已绑定到托管身份配置。"
        : "当前没有可展示的受信身份绑定。",
    ],
    [
      "Engine",
      packageVerified
        ? "verified"
        : engineHealthy
          ? runtimeApplied
            ? "applied"
            : "configured"
          : "unavailable",
      packageVerified
        ? `CMS、digest、signer 与 package tree 已验证${packageDetails?.engineRevision === null || packageDetails?.engineRevision === undefined ? "。" : ` · ${packageDetails.engineRevision}`}`
        : engineHealthy
          ? "内置组件健康；未把健康检查当作运行时验证。"
          : "内置组件当前未通过健康检查。",
    ],
    [
      "Network",
      networkState,
      silo.networkProfile.proxyRequired
        ? "必须代理；连接失败时不会回退直连。"
        : "当前为 Direct 直连。",
    ],
    [
      "Evidence",
      hostBindingVerified && packageVerified
        ? "verified"
        : latestEvidence
          ? "observed"
          : "not_requested",
      hostBindingVerified && packageVerified
        ? "当前 Host、Artifact、Profile、Engine 与 Network binding 已匹配。"
        : latestEvidence
          ? "已有用户主动检查记录。"
          : "尚未请求网络观察。",
    ],
  ];
  return (
    <section className="managed-status-groups" aria-label="托管身份状态">
      <div className="managed-status-heading">
        <strong>托管身份状态</strong>
        <span>状态只表示当前边界，不替代运行时证据。</span>
      </div>
      <div className="managed-status-grid">
        {states.map(([name, state, detail]) => (
          <div key={name}>
            <span>{name}</span>
            <strong className={`managed-state ${state}`}>
              {managedUiStateLabel(state)}
            </strong>
            <small>{detail}</small>
          </div>
        ))}
      </div>
    </section>
  );
}

function LegacyEnvironmentRecoveryPanel({
  artifacts,
  busy,
  onCleanup,
  silos,
}: {
  artifacts: LegacyEnvironmentArtifact[];
  busy: boolean;
  onCleanup: (artifact: LegacyEnvironmentArtifact) => Promise<void>;
  silos: Silo[];
}) {
  if (artifacts.length === 0) {
    return null;
  }

  return (
    <section className="panel legacy-environment-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">需要处理一次</p>
          <h2>清理不再使用的旧运行环境</h2>
          <p>
            这些环境来自较早的设置，不属于 Silo
            当前选择的运行位置。清理后才能正常归档、删除或恢复保险库。
          </p>
        </div>
      </div>
      <div className="legacy-environment-list">
        {artifacts.map((artifact) => {
          const silo = silos.find(
            (candidate) => candidate.id === artifact.siloId,
          );
          return (
            <div
              className={`legacy-environment-row${
                artifact.cleanupAvailable ? "" : " blocked"
              }`}
              key={`${artifact.siloId}-${artifact.backend}`}
            >
              <div>
                <strong>{silo?.name ?? "未知 Silo"}</strong>
                <span>{legacyEnvironmentLabel(artifact.backend)}</span>
                <p>
                  {artifact.cleanupAvailable
                    ? "归属信息已核对，可以安全清理，不会改变当前运行位置。"
                    : "归属信息不完整或与当前运行位置不一致，VeriSilo 已阻止自动删除。"}
                </p>
              </div>
              {artifact.cleanupAvailable ? (
                <button
                  className="button-danger"
                  disabled={busy}
                  onClick={() => void onCleanup(artifact)}
                  type="button"
                >
                  核对并清理
                </button>
              ) : (
                <span className="provider-badge warning">需要人工核对</span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}

function ArchivedSiloList({
  busy,
  onDelete,
  onRestore,
  silos,
  storageUsage,
}: {
  busy: boolean;
  onDelete: (silo: Silo) => Promise<void>;
  onRestore: (silo: Silo) => Promise<void>;
  silos: Silo[];
  storageUsage: Record<string, number | null>;
}) {
  if (silos.length === 0) {
    return null;
  }

  return (
    <section className="panel archived-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">已归档</p>
          <h2>数据仍在，只是不再出现在启动列表</h2>
          <p>恢复不会复制数据；永久删除才会移除这个 Silo 的浏览器数据。</p>
        </div>
      </div>
      <div className="archived-list">
        {silos.map((silo) => (
          <article className="archived-row" key={silo.id}>
            <span className="silo-mark" style={{ backgroundColor: silo.color }}>
              {silo.name.slice(0, 1).toUpperCase()}
            </span>
            <div className="archived-copy">
              <strong>{silo.name}</strong>
              <span>
                {silo.archivedAt === null
                  ? "归档时间未知"
                  : `归档于 ${formatDate(silo.archivedAt)}`}
                {formatStorageSuffix(storageUsage[silo.id])}
              </span>
            </div>
            <div className="card-actions compact-actions">
              <button
                className="button-secondary"
                disabled={busy}
                onClick={() => void onRestore(silo)}
                type="button"
              >
                恢复
              </button>
              <button
                className="button-danger"
                disabled={busy}
                onClick={() => void onDelete(silo)}
                type="button"
              >
                永久删除
              </button>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function EditSiloPanel({
  browsers,
  busy,
  onCancel,
  onSave,
  silo,
}: {
  browsers: BrowserCandidate[];
  busy: boolean;
  onCancel: () => void;
  onSave: (
    input: UpdateSiloInput,
    networkInput: UpdateSiloNetworkInput | null,
    engineInput: UpdateSiloEngineInput | null,
  ) => Promise<void>;
  silo: Silo;
}) {
  const [name, setName] = useState(silo.name);
  const [color, setColor] = useState(silo.color);
  const [browserKind, setBrowserKind] = useState<BrowserKind>(
    silo.browser?.kind ?? "chrome",
  );
  const [executablePath, setExecutablePath] = useState(
    silo.browser?.executablePath ?? "",
  );
  const [replaceNetwork, setReplaceNetwork] = useState(false);
  const [useSystemBrowser, setUseSystemBrowser] = useState(false);
  const [replacementNetwork, setReplacementNetwork] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyImportError, setProxyImportError] = useState<string | null>(null);
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const localExecution = silo.executionTarget.kind === "local";
  const identityLocked = silo.identityLockedAt !== null;
  const managedIdentity =
    silo.engine.adapter !== "stock" || silo.browser === null;
  const candidates = browsers.filter(
    (candidate) => candidate.kind === browserKind,
  );
  const networkValid =
    networkProfileSchema.safeParse(replacementNetwork).success;
  const credentialsValid =
    (proxyUsername.trim() === "" && proxyPassword === "") ||
    (proxyUsername.trim() !== "" && proxyPassword !== "");
  const credentialsSupported =
    proxyUsername.trim() === "" ||
    (replacementNetwork.mode === "fixed_proxy" &&
      ["http", "socks5"].includes(replacementNetwork.scheme));
  const managedProxyRequired =
    silo.engine.adapter === "controlled-chromium"
      ? silo.engine.identityTemplate.network.proxyRequired
      : null;
  const managedBrowserNetworkMismatch =
    replaceNetwork &&
    silo.engine.adapter !== "stock" &&
    !useSystemBrowser &&
    managedProxyRequired !== null &&
    replacementNetwork.proxyRequired !== managedProxyRequired;
  const applyProxyImport = () => {
    try {
      const parsed = parseProxyInput(proxyImport);
      setReplacementNetwork(parsed.profile);
      setProxyUsername(parsed.credentials?.username ?? "");
      setProxyPassword(parsed.credentials?.password ?? "");
      setProxyImport("");
      setProxyImportError(null);
    } catch (error) {
      setProxyImportError(errorMessage(error));
    }
  };

  return (
    <section className="panel settings-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">编辑 Silo</p>
          <h1>调整资料，必要时明确替换网络</h1>
          <p>保存后会继续使用原有浏览器数据。运行中的 Silo 不能编辑。</p>
        </div>
      </div>
      <div className="form-grid identity-grid">
        <label>
          名称
          <input
            disabled={busy}
            maxLength={64}
            onChange={(event) => setName(event.target.value)}
            value={name}
          />
        </label>
        <label>
          颜色
          <input
            disabled={busy}
            onChange={(event) => setColor(event.target.value)}
            type="color"
            value={color}
          />
        </label>
      </div>

      {!localExecution ? (
        <div className="identity-readonly-card">
          <div>
            <span className="readonly-kicker">浏览器与运行位置</span>
            <strong>
              {silo.executionTarget.kind === "wsl"
                ? "Linux Chromium"
                : "远程浏览器"}
            </strong>
            <p>{siloExecutionTargetLabel(silo)}</p>
          </div>
          <p>
            {silo.executionTarget.kind === "wsl"
              ? "这个 Silo 使用所选 Linux 环境内的 Chromium，不读取 Windows 浏览器路径。运行位置和浏览器身份不能在这里更换。"
              : "这个 Silo 使用已连接位置中的浏览器，不读取本机浏览器路径。运行位置和浏览器身份不能在这里更换。"}
          </p>
        </div>
      ) : identityLocked || managedIdentity ? (
        <div className="identity-readonly-card">
          <div>
            <span className="readonly-kicker">已锁定的身份配置</span>
            <strong>{siloBrowserLabel(silo)}</strong>
            <p>{siloExecutionTargetLabel(silo)}</p>
          </div>
          <p>{siloWebsiteIdentityBoundary(silo)}</p>
        </div>
      ) : (
        <div className="form-grid">
          <label>
            浏览器
            <select
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              onChange={(event) => {
                const nextKind = event.target.value as BrowserKind;
                setBrowserKind(nextKind);
                const next = browsers.find(
                  (candidate) => candidate.kind === nextKind,
                );
                if (next !== undefined) {
                  setExecutablePath(next.executablePath);
                }
              }}
              value={browserKind}
            >
              <option value="chrome">Google Chrome</option>
              <option value="edge">Microsoft Edge</option>
            </select>
          </label>
          <label>
            浏览器可执行文件
            <input
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              onChange={(event) => setExecutablePath(event.target.value)}
              value={executablePath}
            />
          </label>
        </div>
      )}
      {localExecution &&
      !identityLocked &&
      !managedIdentity &&
      candidates.length > 0 ? (
        <div className="candidate-row">
          {candidates.map((candidate) => (
            <button
              className="button-secondary"
              disabled={
                busy || (silo.engine.adapter !== "stock" && !useSystemBrowser)
              }
              key={candidate.executablePath}
              onClick={() => setExecutablePath(candidate.executablePath)}
              type="button"
            >
              使用已发现的 {candidate.displayName}
              {candidate.version === null ? "" : ` ${candidate.version}`}
            </button>
          ))}
        </div>
      ) : null}
      <div className="boundary-note">
        <strong>当前浏览器方式</strong>
        <span>
          {siloBrowserLabel(silo)} · {siloExecutionTargetLabel(silo)}
        </span>
      </div>
      {identityLocked || managedIdentity ? (
        <div className="identity-clone-note">
          <strong>浏览器身份配置与运行位置已锁定</strong>
          <p>
            要更换浏览器家族、受控身份或运行位置，请创建新的 Silo。名称、颜色和
            停止运行后的网络设置仍可在这里调整。
          </p>
        </div>
      ) : null}
      {localExecution &&
      !identityLocked &&
      silo.engine.adapter !== "stock" &&
      silo.browser !== null ? (
        <label className="check-field network-replace-toggle">
          <input
            checked={useSystemBrowser}
            disabled={busy}
            onChange={(event) => {
              setUseSystemBrowser(event.target.checked);
              if (!event.target.checked) {
                setBrowserKind(silo.browser?.kind ?? "chrome");
                setExecutablePath(silo.browser?.executablePath ?? "");
              }
            }}
            type="checkbox"
          />
          改用这台电脑上的{" "}
          {silo.browser?.kind === "chrome" ? "Google Chrome" : "Microsoft Edge"}
        </label>
      ) : null}
      <div className="boundary-note">
        <strong>当前网络设置</strong>
        <span>{describeNetwork(silo.networkProfile)}</span>
      </div>
      <label className="check-field network-replace-toggle">
        <input
          checked={replaceNetwork}
          disabled={busy}
          onChange={(event) => {
            setReplaceNetwork(event.target.checked);
            if (event.target.checked) {
              setReplacementNetwork(emptyNetwork());
              setProxyImport("");
              setProxyUsername("");
              setProxyPassword("");
              setProxyImportError(null);
            }
          }}
          type="checkbox"
        />
        替换网络设置，并清除原有代理登录信息和本机代理访问密钥
      </label>
      {replaceNetwork ? (
        <div className="network-replace-card">
          <p className="field-warning">
            替换会清除原有代理登录信息和本机代理访问密钥。需要认证时请在下方重新输入；普通改名请不要勾选此项。
          </p>
          <label>
            新网络方式
            <select
              disabled={busy}
              onChange={(event) => {
                const mode = event.target.value;
                setProxyUsername("");
                setProxyPassword("");
                if (mode === "direct") {
                  setReplacementNetwork(emptyNetwork());
                } else if (mode === "fixed_proxy") {
                  setReplacementNetwork({
                    mode: "fixed_proxy",
                    proxyRequired: true,
                    scheme: "socks5",
                    host: "127.0.0.1",
                    port: 7890,
                    bypassList: [],
                  });
                } else {
                  setReplacementNetwork({
                    mode: "pac",
                    proxyRequired: false,
                    pacUrl: "",
                  });
                }
              }}
              value={replacementNetwork.mode}
            >
              <option value="direct">直连</option>
              {localExecution ? (
                <>
                  <option value="fixed_proxy">固定 HTTP / SOCKS5 代理</option>
                  <option value="pac">自动代理规则（PAC）</option>
                </>
              ) : null}
            </select>
          </label>
          {!localExecution ? (
            <p className="field-warning">
              Linux 运行位置当前仅支持直连，暂无其他网络方式可选。
            </p>
          ) : null}
          {replacementNetwork.mode === "fixed_proxy" ? (
            <>
              <div className="proxy-import-row edit-proxy-import">
                <label>
                  一行代理
                  <input
                    aria-describedby={
                      proxyImportError === null
                        ? undefined
                        : "edit-proxy-import-error"
                    }
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setProxyImport(event.target.value)}
                    placeholder="socks5://host:port:user:password"
                    spellCheck={false}
                    type="password"
                    value={proxyImport}
                  />
                </label>
                <button
                  className="button-secondary"
                  disabled={busy || proxyImport.trim() === ""}
                  onClick={applyProxyImport}
                  type="button"
                >
                  解析
                </button>
              </div>
              {proxyImportError !== null ? (
                <p
                  className="field-error"
                  id="edit-proxy-import-error"
                  role="alert"
                >
                  {proxyImportError}
                </p>
              ) : null}
              <div className="form-grid auth-grid">
                <label>
                  用户名（可空）
                  <input
                    autoComplete="off"
                    disabled={busy}
                    onChange={(event) => setProxyUsername(event.target.value)}
                    value={proxyUsername}
                  />
                </label>
                <label>
                  密码（可空）
                  <input
                    autoComplete="new-password"
                    disabled={busy}
                    onChange={(event) => setProxyPassword(event.target.value)}
                    type="password"
                    value={proxyPassword}
                  />
                </label>
              </div>
              {!credentialsSupported ? (
                <p className="field-error" role="alert">
                  需要登录信息时，请使用 HTTP、SOCKS5，或交给本机 Mihomo / Clash
                  应用处理。
                </p>
              ) : null}
              <div className="boundary-note compact-boundary">
                <strong>准备应用</strong>
                <span>{describeNetwork(replacementNetwork)}</span>
              </div>
            </>
          ) : null}
          {replacementNetwork.mode === "pac" ? (
            <div className="form-grid pac-grid">
              <label>
                自动代理配置地址（PAC）
                <input
                  disabled={busy}
                  onChange={(event) =>
                    setReplacementNetwork({
                      ...replacementNetwork,
                      pacUrl: event.target.value,
                    })
                  }
                  value={replacementNetwork.pacUrl}
                />
              </label>
              <label className="check-field">
                <input
                  checked={replacementNetwork.proxyRequired}
                  disabled={busy}
                  onChange={(event) =>
                    setReplacementNetwork({
                      ...replacementNetwork,
                      proxyRequired: event.target.checked,
                    })
                  }
                  type="checkbox"
                />
                必须代理（如果规则可能直连，启动前阻止）
              </label>
            </div>
          ) : null}
        </div>
      ) : null}
      {managedBrowserNetworkMismatch ? (
        <p className="field-error" role="alert">
          当前独立浏览器的网络保护方式与新设置不一致。请同时改用这台电脑上的系统浏览器，或保持原网络设置。
        </p>
      ) : null}
      <div className="submit-row">
        <div>
          <strong>浏览器数据保持不变</strong>
          <span>现有 Cookie、登录状态和网站设置会继续保留。</span>
        </div>
        <div className="card-actions">
          <button
            className="button-secondary"
            disabled={busy}
            onClick={onCancel}
            type="button"
          >
            取消
          </button>
          <button
            disabled={
              busy ||
              name.trim().length === 0 ||
              (!managedIdentity && executablePath.trim().length === 0) ||
              (replaceNetwork &&
                (!networkValid ||
                  !credentialsValid ||
                  !credentialsSupported ||
                  managedBrowserNetworkMismatch))
            }
            onClick={() => {
              if (
                replaceNetwork &&
                managedIdentity &&
                !window.confirm(
                  "确认重新绑定托管身份浏览器的网络？当前网络证据会失效，必须代理不会回退直连。",
                )
              ) {
                return;
              }
              void onSave(
                {
                  name: name.trim(),
                  color,
                  browserKind,
                  executablePath: executablePath.trim(),
                },
                replaceNetwork
                  ? {
                      networkProfile: replacementNetwork,
                      ...(proxyUsername.trim() !== "" && proxyPassword !== ""
                        ? {
                            proxyCredentials: {
                              username: proxyUsername.trim(),
                              password: proxyPassword,
                            },
                          }
                        : {}),
                    }
                  : null,
                useSystemBrowser ? { engine: { adapter: "stock" } } : null,
              );
            }}
            type="button"
          >
            保存更改
          </button>
        </div>
      </div>
    </section>
  );
}

function VaultAndDataPanel({
  busy,
  onNotice,
  onRefresh,
  onVaultRestored,
  runBusy,
}: {
  busy: boolean;
  onNotice: (notice: Notice) => void;
  onRefresh: () => Promise<unknown>;
  onVaultRestored: () => Promise<void>;
  runBusy: (
    action: (isCurrent: () => boolean) => Promise<void>,
  ) => Promise<void>;
}) {
  const [currentPassphrase, setCurrentPassphrase] = useState("");
  const [newPassphrase, setNewPassphrase] = useState("");
  const [confirmPassphrase, setConfirmPassphrase] = useState("");
  const [backupPath, setBackupPath] = useState("");
  const [restorePath, setRestorePath] = useState("");
  const [restorePassphrase, setRestorePassphrase] = useState("");
  const [confirmOverwrite, setConfirmOverwrite] = useState(false);

  const changePassphrase = () =>
    runBusy(async (isCurrent) => {
      if (newPassphrase.length < 12) {
        throw new UserFacingError("新口令至少需要 12 个字符。");
      }
      if (newPassphrase !== confirmPassphrase) {
        throw new UserFacingError("两次输入的新口令不一致。");
      }
      await desktopApi.changeVaultPassphrase(currentPassphrase, newPassphrase);
      if (!isCurrent()) {
        return;
      }
      setCurrentPassphrase("");
      setNewPassphrase("");
      setConfirmPassphrase("");
      onNotice({
        tone: "success",
        message:
          "保险库口令和加密密钥已更新。浏览器数据不在保险库中，因此不会被改动。",
      });
      await onRefresh();
    });

  const backupVault = () =>
    runBusy(async (isCurrent) => {
      const receipt = await desktopApi.backupVault(backupPath.trim());
      if (!isCurrent()) {
        return;
      }
      onNotice({
        tone: "success",
        message: `已备份加密保险库：${receipt.destinationPath}（${formatBytes(receipt.bytes)}）。浏览器数据未包含在内。`,
      });
    });

  const restoreVault = () =>
    runBusy(async (isCurrent) => {
      if (!confirmOverwrite) {
        throw new UserFacingError("请先确认覆盖当前保险库记录。");
      }
      await desktopApi.restoreVault(
        restorePath.trim(),
        restorePassphrase,
        true,
      );
      if (!isCurrent()) {
        return;
      }
      await onVaultRestored();
    });

  return (
    <div className="settings-stack">
      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">保险库口令</p>
            <h1>更换保护本地配置的口令</h1>
            <p>
              VeriSilo
              没有找回机制。更换口令会轮换保险库的数据加密密钥；旧备份仍是独立历史快照，应按敏感数据妥善保管或销毁。
            </p>
          </div>
        </div>
        <div className="form-grid three-columns">
          <label>
            当前口令
            <input
              autoComplete="current-password"
              disabled={busy}
              onChange={(event) => setCurrentPassphrase(event.target.value)}
              type="password"
              value={currentPassphrase}
            />
          </label>
          <label>
            新口令
            <input
              autoComplete="new-password"
              disabled={busy}
              minLength={12}
              onChange={(event) => setNewPassphrase(event.target.value)}
              type="password"
              value={newPassphrase}
            />
          </label>
          <label>
            再输一次
            <input
              autoComplete="new-password"
              disabled={busy}
              minLength={12}
              onChange={(event) => setConfirmPassphrase(event.target.value)}
              type="password"
              value={confirmPassphrase}
            />
          </label>
        </div>
        <button
          disabled={
            busy ||
            currentPassphrase.length === 0 ||
            newPassphrase.length < 12 ||
            newPassphrase !== confirmPassphrase
          }
          onClick={() => void changePassphrase()}
          type="button"
        >
          更新保险库口令
        </button>
      </section>

      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">加密备份</p>
            <h2>备份 Silo 配置，不冒充浏览器数据备份</h2>
            <p>
              备份包含加密的元数据、稳定种子和网络秘密；不包含
              Cookie、浏览记录或整个浏览器数据目录。
            </p>
          </div>
        </div>
        <label>
          目标文件绝对路径
          <input
            autoComplete="off"
            disabled={busy}
            onChange={(event) => setBackupPath(event.target.value)}
            placeholder="C:\\Users\\你\\Documents\\verisilo-vault.backup"
            spellCheck={false}
            value={backupPath}
          />
        </label>
        <button
          disabled={busy || backupPath.trim().length === 0}
          onClick={() => void backupVault()}
          type="button"
        >
          创建加密备份
        </button>
      </section>

      <section className="panel settings-panel danger-zone">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">恢复保险库</p>
            <h2>覆盖前先验证备份口令和格式</h2>
            <p>
              这会替换当前保险库记录，但不会自动删除、复制或覆盖任何浏览器
              浏览器数据。
            </p>
          </div>
        </div>
        <div className="form-grid">
          <label>
            备份文件绝对路径
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => {
                setRestorePath(event.target.value);
                setConfirmOverwrite(false);
              }}
              spellCheck={false}
              value={restorePath}
            />
          </label>
          <label>
            该备份的口令
            <input
              autoComplete="off"
              disabled={busy}
              onChange={(event) => {
                setRestorePassphrase(event.target.value);
                setConfirmOverwrite(false);
              }}
              type="password"
              value={restorePassphrase}
            />
          </label>
        </div>
        <label className="check-field">
          <input
            checked={confirmOverwrite}
            disabled={busy}
            onChange={(event) => setConfirmOverwrite(event.target.checked)}
            type="checkbox"
          />
          我确认覆盖当前保险库记录，并理解浏览器数据不在此备份中
        </label>
        <button
          className="button-danger"
          disabled={
            busy ||
            !confirmOverwrite ||
            restorePath.trim().length === 0 ||
            restorePassphrase.length === 0
          }
          onClick={() => void restoreVault()}
          type="button"
        >
          验证并恢复保险库
        </button>
      </section>
    </div>
  );
}

function CreateSiloPanel({
  browserKind,
  browserPath,
  busy,
  candidateOptions,
  chooseBrowser,
  color,
  createSilo,
  createManagedSilo,
  executionTarget,
  importProxy,
  inspectMihomoController,
  mihomoBusy,
  mihomoControllerSecret,
  mihomoControllerUrl,
  mihomoSnapshot,
  managedEngineReady,
  managedStatusBusy,
  managedStatusError,
  name,
  networkProfile,
  proxyImport,
  proxyPassword,
  proxyUsername,
  refreshManagedStatus,
  refreshWsl,
  resetMihomoSnapshot,
  selectMihomoGroup,
  selectMihomoNode,
  setBrowserKind,
  setBrowserPath,
  setColor,
  setExecutionTarget,
  setMihomoControllerSecret,
  setMihomoControllerUrl,
  setName,
  setNetworkProfile,
  setProxyImport,
  setProxyPassword,
  setProxyUsername,
  wslBusy,
  wslOptions,
  wslStatus,
}: {
  browserKind: BrowserKind;
  browserPath: string;
  busy: boolean;
  candidateOptions: BrowserCandidate[];
  chooseBrowser: (candidate: BrowserCandidate) => void;
  color: string;
  createSilo: () => Promise<void>;
  createManagedSilo: (input: CreateManagedSiloInput) => Promise<void>;
  executionTarget: SiloExecutionTarget;
  importProxy: () => void;
  inspectMihomoController: () => Promise<void>;
  mihomoBusy: boolean;
  mihomoControllerSecret: string;
  mihomoControllerUrl: string;
  mihomoSnapshot: MihomoSnapshot | null;
  managedEngineReady: boolean;
  managedStatusBusy: boolean;
  managedStatusError: string | null;
  name: string;
  networkProfile: NetworkProfile;
  proxyImport: string;
  proxyPassword: string;
  proxyUsername: string;
  refreshManagedStatus: () => Promise<void>;
  refreshWsl: () => Promise<void>;
  resetMihomoSnapshot: () => void;
  selectMihomoGroup: (groupName: string) => void;
  selectMihomoNode: (nodeName: string) => void;
  setBrowserKind: (kind: BrowserKind) => void;
  setBrowserPath: (path: string) => void;
  setColor: (color: string) => void;
  setExecutionTarget: (target: SiloExecutionTarget) => void;
  setMihomoControllerSecret: (secret: string) => void;
  setMihomoControllerUrl: (url: string) => void;
  setName: (name: string) => void;
  setNetworkProfile: (profile: NetworkProfile) => void;
  setProxyImport: (value: string) => void;
  setProxyPassword: (value: string) => void;
  setProxyUsername: (value: string) => void;
  wslBusy: boolean;
  wslOptions: WslCreationOption[];
  wslStatus: WslStatus | null;
}) {
  const [websiteBoundaryConfirmed, setWebsiteBoundaryConfirmed] =
    useState(false);
  const [creationMode, setCreationMode] = useState<CreateMode>("standard");
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const localProxySelected = isLoopbackProxyProfile(networkProfile);
  const mihomoBinding =
    networkProfile.mode === "fixed_proxy"
      ? networkProfile.externalMihomo
      : undefined;
  const selectedMihomoGroup = mihomoSnapshot?.groups.find(
    (group) => group.name === mihomoBinding?.selectorGroup,
  );
  useEffect(() => {
    setWebsiteBoundaryConfirmed(false);
  }, [browserKind, browserPath, executionTarget, networkProfile]);

  const localExecution = executionTarget.kind === "local";
  const readyWslOptions = wslOptions.filter((option) => option.ready);
  const unavailableWslCount = wslOptions.length - readyWslOptions.length;
  const selectedBrowserCandidate = candidateOptions.find(
    (candidate) => candidate.executablePath === browserPath,
  );
  const standardDefaultsSelected =
    localExecution &&
    networkProfile.mode === "direct" &&
    selectedBrowserCandidate?.kind === browserKind;
  return (
    <>
      <section className="create-hero panel">
        <p className="eyebrow">新环境</p>
        <h1>创建一个独立的浏览器空间</h1>
        <p>
          VeriSilo 会为每个 Silo 保存独立的网站数据，并关闭浏览器同步；
          不会读取或修改默认浏览器的数据。
        </p>
        <div className="assurance-row">
          <span>Cookie 与站点数据独立</span>
          <span>浏览器同步已关闭</span>
          <span>硬件特征跟随本机</span>
        </div>
      </section>

      <section
        aria-busy={managedStatusBusy}
        aria-label="选择浏览器方式"
        className="panel browser-mode-panel"
      >
        <div className="panel-heading">
          <div>
            <p className="eyebrow">浏览器方式</p>
            <h2>选择要创建的浏览器</h2>
            <p>系统浏览器保留本机浏览器特征；托管身份浏览器由内置组件提供。</p>
          </div>
        </div>
        <div
          className="browser-mode-grid"
          role="radiogroup"
          aria-label="浏览器方式"
        >
          <button
            aria-checked={creationMode === "standard"}
            className={`browser-mode-card${creationMode === "standard" ? " selected" : ""}`}
            onClick={() => setCreationMode("standard")}
            role="radio"
            type="button"
          >
            <strong>系统浏览器</strong>
            <span>使用这台电脑上已安装的 Chrome 或 Edge。</span>
            <small>Profile 独立保存，设备和浏览器特征跟随本机。</small>
          </button>
          <button
            aria-checked={creationMode === "managed"}
            aria-describedby="managed-browser-availability"
            className={`browser-mode-card${creationMode === "managed" ? " selected" : ""}`}
            disabled={!managedEngineReady || managedStatusBusy}
            onClick={() => setCreationMode("managed")}
            role="radio"
            type="button"
          >
            <strong>托管身份浏览器</strong>
            <span>使用 VeriSilo 内置的受控浏览器组件。</span>
            <small>
              {managedStatusBusy
                ? "正在检查内置组件…"
                : managedEngineReady
                  ? "内置组件已通过健康检查。"
                  : "内置组件未就绪，暂不能创建。"}
            </small>
          </button>
        </div>
        <div id="managed-browser-availability">
          {managedStatusError !== null ? (
            <div className="managed-status-error" role="alert">
              <span>{managedStatusError}</span>
              <button
                className="button-secondary"
                disabled={managedStatusBusy}
                onClick={() => void refreshManagedStatus()}
                type="button"
              >
                {managedStatusBusy ? "检查中…" : "重新检查"}
              </button>
            </div>
          ) : !managedEngineReady && !managedStatusBusy ? (
            <p className="field-warning" role="status">
              托管身份浏览器只有在内置组件健康检查通过后才可创建；系统浏览器仍可正常使用。
            </p>
          ) : null}
        </div>
      </section>

      {creationMode === "managed" ? (
        <ManagedSiloForm
          busy={busy}
          initialColor={color}
          onSubmit={createManagedSilo}
        />
      ) : (
        <section className="panel form-panel">
          <div className="step-heading">
            <span>1</span>
            <div>
              <h2>给这个 Silo 一个名字</h2>
              <p>名称和颜色只用于本机识别，不会发送给网站。</p>
            </div>
          </div>
          <div className="form-grid identity-grid">
            <label>
              Silo 名称
              <input
                disabled={busy}
                maxLength={64}
                onChange={(event) => setName(event.target.value)}
                placeholder="例如：工作账号"
                value={name}
              />
            </label>
            <label className="color-field">
              标识颜色
              <span className="color-control">
                <input
                  disabled={busy}
                  onChange={(event) => setColor(event.target.value)}
                  type="color"
                  value={color}
                />
                <span>{color.toUpperCase()}</span>
              </span>
            </label>
          </div>

          <section
            aria-label="Standard Silo 当前设置"
            className="standard-default-summary"
          >
            <div>
              <p className="eyebrow">
                {standardDefaultsSelected ? "推荐设置已就绪" : "当前设置"}
              </p>
              <h2>
                {localExecution
                  ? browserPath === ""
                    ? "尚未发现可用的 Chrome 或 Edge"
                    : `${browserKind === "chrome" ? "Google Chrome" : "Microsoft Edge"} · Windows 本机`
                  : executionTarget.kind === "wsl"
                    ? `Chromium · ${executionTarget.distribution}`
                    : "远程运行"}
              </h2>
              <p>
                {standardDefaultsSelected
                  ? "已自动选择本机浏览器并使用 Direct 直连。只需命名、核对边界并确认创建。"
                  : "你调整了高级设置；创建前请核对下面显示的运行位置、网络与网站可见边界。"}
              </p>
            </div>
            <div className="standard-default-facts">
              <span>
                <strong>运行位置</strong>
                {localExecution ? "Windows 本机" : "Linux 环境"}
              </span>
              <span>
                <strong>网络</strong>
                {localExecution
                  ? describeNetwork(networkProfile)
                  : "Direct 直连"}
              </span>
              <span>
                <strong>网站数据</strong>独立 Profile
              </span>
            </div>
          </section>

          <button
            aria-expanded={advancedOpen}
            className="create-advanced-toggle"
            disabled={busy}
            onClick={() => {
              const nextOpen = !advancedOpen;
              setAdvancedOpen(nextOpen);
              if (nextOpen && wslStatus === null && !wslBusy) {
                void refreshWsl();
              }
            }}
            type="button"
          >
            <span>
              <strong>高级设置</strong>
              <small>切换浏览器、手工路径、Linux 运行位置或网络方式</small>
            </span>
            <span className="create-advanced-state">
              {advancedOpen
                ? "收起"
                : standardDefaultsSelected
                  ? "使用推荐默认值"
                  : "已调整"}
            </span>
          </button>
          <div
            className="step-heading advanced-step-heading"
            hidden={!advancedOpen}
          >
            <span>A</span>
            <div>
              <h2>浏览器身份与运行位置</h2>
              <p>
                先决定浏览器在哪里运行。网站可见的系统和硬件边界会随运行位置变化。
              </p>
            </div>
          </div>
          <div
            className="execution-target-grid"
            aria-label="运行位置"
            hidden={!advancedOpen}
          >
            <button
              aria-pressed={executionTarget.kind === "local"}
              className={`execution-card${
                executionTarget.kind === "local" ? " selected" : ""
              }`}
              disabled={busy}
              onClick={() => setExecutionTarget({ kind: "local" })}
              type="button"
            >
              <span className="execution-card-kicker">这台电脑</span>
              <strong>Windows 本机</strong>
              <small>使用已安装的 Chrome 或 Edge，网站数据单独保存。</small>
              <span className="execution-card-state">可用</span>
            </button>
            {readyWslOptions.map(({ distribution }) => {
              const selected =
                executionTarget.kind === "wsl" &&
                executionTarget.distribution === distribution;
              return (
                <button
                  aria-pressed={selected}
                  className={`execution-card${selected ? " selected" : ""}`}
                  disabled={busy || wslBusy}
                  key={distribution}
                  onClick={() =>
                    setExecutionTarget({ kind: "wsl", distribution })
                  }
                  type="button"
                >
                  <span className="execution-card-kicker">Linux 环境</span>
                  <strong>{distribution}</strong>
                  <small>使用环境内的 Chromium；当前只允许直连网络。</small>
                  <span className="execution-card-state">已就绪</span>
                </button>
              );
            })}
          </div>
          <div
            className="wsl-discovery-row"
            hidden={!advancedOpen}
            role="status"
          >
            <span>
              {wslBusy
                ? "正在检查这台电脑上的 Linux 环境…"
                : wslStatus === null
                  ? "尚未检查 Linux 环境。"
                  : readyWslOptions.length > 0
                    ? `已确认 ${readyWslOptions.length} 个可创建的 Linux 环境。`
                    : wslStatus.distributions.length > 0
                      ? "已发现 Linux 环境，但尚未通过完整运行检查。"
                      : wslStatus.message || "没有发现已就绪的 Linux 环境。"}
            </span>
            <button
              className="button-secondary button-quiet"
              disabled={busy || wslBusy}
              onClick={() => void refreshWsl()}
              type="button"
            >
              {wslBusy ? "检查中…" : "重新检查"}
            </button>
          </div>
          {unavailableWslCount > 0 ? (
            <p className="wsl-unavailable-note" hidden={!advancedOpen}>
              另有 {unavailableWslCount} 个 Linux
              环境未通过启动、停止、网络和状态检查。请在“运行位置设置”中修复后重新检查。
            </p>
          ) : null}

          {localExecution ? (
            <div className="local-browser-choice" hidden={!advancedOpen}>
              <div className="subsection-heading">
                <strong>选择本机浏览器</strong>
                <span>支持 Windows 版 Google Chrome 和 Microsoft Edge。</span>
              </div>
              <div
                className="browser-switch"
                role="group"
                aria-label="浏览器类型"
              >
                {(["chrome", "edge"] as const).map((kind) => (
                  <button
                    aria-pressed={browserKind === kind}
                    className={browserKind === kind ? "selected" : ""}
                    disabled={busy}
                    key={kind}
                    onClick={() => setBrowserKind(kind)}
                    type="button"
                  >
                    <strong>
                      {kind === "chrome" ? "Google Chrome" : "Microsoft Edge"}
                    </strong>
                    <span>
                      {kind === "chrome" ? "Chrome Stable" : "Edge Stable"}
                    </span>
                  </button>
                ))}
              </div>
              {candidateOptions.length > 0 ? (
                <div className="candidate-list" aria-label="已检测到的浏览器">
                  {candidateOptions.map((candidate) => (
                    <button
                      aria-pressed={browserPath === candidate.executablePath}
                      className={
                        browserPath === candidate.executablePath
                          ? "selected"
                          : ""
                      }
                      key={candidate.executablePath}
                      onClick={() => chooseBrowser(candidate)}
                      type="button"
                    >
                      <span>
                        <strong>{candidate.displayName}</strong>
                        <small>{candidate.version ?? "版本未知"}</small>
                      </span>
                    </button>
                  ))}
                </div>
              ) : (
                <p className="form-hint">
                  尚未在常见 Windows
                  安装位置找到该浏览器，请填写可执行文件绝对路径。
                </p>
              )}
              <label>
                浏览器可执行文件路径
                <input
                  disabled={busy}
                  onChange={(event) => setBrowserPath(event.target.value)}
                  placeholder="C:\\Program Files\\...\\browser.exe"
                  value={browserPath}
                />
              </label>
              {browserKind === "edge" ? (
                <p className="form-hint">
                  Edge 仍可能显示 Windows
                  已登录的微软账户，但不会复用默认浏览器的
                  Cookie。微软或企业网站仍可能通过 Windows 单点登录识别该账户。
                </p>
              ) : null}
            </div>
          ) : executionTarget.kind === "wsl" ? (
            <div className="wsl-browser-summary" hidden={!advancedOpen}>
              <div>
                <strong>
                  使用 {executionTarget.distribution} 中的 Chromium
                </strong>
                <p>
                  不需要选择 Windows 浏览器路径。网站会看到 Linux 浏览器环境；
                  CPU、内存和图形特征仍跟随该环境与本机硬件。
                </p>
              </div>
            </div>
          ) : null}

          <div
            className="step-heading advanced-step-heading"
            hidden={!advancedOpen}
          >
            <span>B</span>
            <div>
              <h2>选择网络方式</h2>
              <p>
                “已配置”与“实际出口已验证”是两件事，创建后可在概览页主动检查。
              </p>
            </div>
          </div>
          {localExecution ? (
            <fieldset disabled={busy || mihomoBusy} hidden={!advancedOpen}>
              <legend className="sr-only">网络配置</legend>
              <div className="proxy-import-card">
                <div>
                  <strong>有现成代理？粘贴一行即可</strong>
                  <span>
                    支持 host:port、host:port:user:password 或带协议的代理 URL。
                  </span>
                </div>
                <div className="proxy-import-row">
                  <input
                    aria-label="一行代理配置"
                    autoComplete="off"
                    onChange={(event) => setProxyImport(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.preventDefault();
                        importProxy();
                      }
                    }}
                    placeholder="socks5://user:password@host:port"
                    spellCheck={false}
                    type="password"
                    value={proxyImport}
                  />
                  <button
                    className="button-secondary"
                    disabled={proxyImport.trim() === ""}
                    onClick={importProxy}
                    type="button"
                  >
                    解析并填入
                  </button>
                </div>
              </div>
              <div className="network-options">
                <NetworkOption
                  checked={networkProfile.mode === "direct"}
                  description="不使用系统代理"
                  label="直连"
                  onChange={() => {
                    setNetworkProfile(emptyNetwork());
                    setProxyUsername("");
                    setProxyPassword("");
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={localProxySelected}
                  description="连接本机 SOCKS/Mixed 端口"
                  label="Mihomo / Clash"
                  onChange={() => {
                    setNetworkProfile(localMihomoProfile());
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={
                    networkProfile.mode === "fixed_proxy" && !localProxySelected
                  }
                  description="HTTP、HTTPS 或 SOCKS"
                  label="远程固定代理"
                  onChange={() => {
                    setNetworkProfile({
                      mode: "fixed_proxy",
                      proxyRequired: true,
                      scheme: "socks5",
                      host: "",
                      port: 1080,
                      bypassList: [],
                    });
                    resetMihomoSnapshot();
                  }}
                />
                <NetworkOption
                  checked={networkProfile.mode === "pac"}
                  description="按照自动代理配置决定路由"
                  label="自动代理规则"
                  onChange={() => {
                    setNetworkProfile({
                      mode: "pac",
                      proxyRequired: false,
                      pacUrl: "",
                    });
                    setProxyUsername("");
                    setProxyPassword("");
                    resetMihomoSnapshot();
                  }}
                />
              </div>
              {networkProfile.mode === "fixed_proxy" ? (
                <>
                  {localProxySelected ? (
                    <div className="mihomo-stack">
                      <div className="proxy-bridge-note">
                        <span className="bridge-badge">使用本机代理应用</span>
                        <div>
                          <strong>
                            订阅仍由你自己的 Mihomo / Clash 客户端管理
                          </strong>
                          <p>
                            只填监听端口时，VeriSilo
                            固定的是端口；连接本机代理应用后， 还会为这个 Silo
                            记住选择组和节点，并在每次启动前重新确认。
                          </p>
                        </div>
                      </div>
                      <div className="controller-card">
                        <div className="controller-heading">
                          <div>
                            <strong>连接本机代理应用（推荐）</strong>
                            <span>
                              只允许这台电脑上的管理地址，不接受远程管理地址。
                            </span>
                          </div>
                          <button
                            className="button-secondary"
                            disabled={
                              mihomoBusy || mihomoControllerUrl.trim() === ""
                            }
                            onClick={() => void inspectMihomoController()}
                            type="button"
                          >
                            {mihomoBusy ? "正在读取…" : "连接并读取节点"}
                          </button>
                        </div>
                        <div className="form-grid controller-grid">
                          <label>
                            本机管理地址
                            <input
                              autoComplete="off"
                              onChange={(event) =>
                                setMihomoControllerUrl(event.target.value)
                              }
                              placeholder="http://127.0.0.1:9090/"
                              spellCheck={false}
                              value={mihomoControllerUrl}
                            />
                          </label>
                          <label>
                            访问密钥（可空）
                            <input
                              autoComplete="off"
                              onChange={(event) =>
                                setMihomoControllerSecret(event.target.value)
                              }
                              placeholder="只进入加密保险库"
                              type="password"
                              value={mihomoControllerSecret}
                            />
                          </label>
                        </div>
                        {mihomoSnapshot !== null ? (
                          <div className="form-grid controller-selection">
                            <label>
                              选择组
                              <select
                                onChange={(event) =>
                                  selectMihomoGroup(event.target.value)
                                }
                                value={
                                  mihomoBinding?.selectorGroup ??
                                  mihomoSnapshot.groups[0]?.name ??
                                  ""
                                }
                              >
                                {mihomoSnapshot.groups.map((group) => (
                                  <option key={group.name} value={group.name}>
                                    {group.name}
                                  </option>
                                ))}
                              </select>
                            </label>
                            <label>
                              固定节点
                              <select
                                onChange={(event) =>
                                  selectMihomoNode(event.target.value)
                                }
                                value={mihomoBinding?.nodeName ?? ""}
                              >
                                {(selectedMihomoGroup?.nodes ?? []).map(
                                  (node) => (
                                    <option key={node.name} value={node.name}>
                                      {node.name}
                                      {node.delayMs === null
                                        ? ""
                                        : ` · ${node.delayMs} ms`}
                                    </option>
                                  ),
                                )}
                              </select>
                            </label>
                            <p className="controller-proof">
                              已于 {formatDate(mihomoSnapshot.checkedAt)}
                              读取控制器。延迟是 Mihomo
                              返回的最近记录，不等于实际出口已经验证。
                              {mihomoSnapshot.providers.length > 0
                                ? ` 订阅：${mihomoSnapshot.providers
                                    .map(
                                      (provider) =>
                                        `${provider.name}（${provider.nodeCount} 节点${
                                          provider.updatedAt === null
                                            ? ""
                                            : `，更新于 ${formatDate(provider.updatedAt)}`
                                        }）`,
                                    )
                                    .join("、")}。`
                                : " 当前代理应用未提供可读的订阅更新时间。"}
                            </p>
                          </div>
                        ) : (
                          <p className="controller-proof">
                            尚未绑定节点。仍可按固定端口使用，但外部客户端切换节点时，Silo
                            的出口也会随之变化。
                          </p>
                        )}
                      </div>
                    </div>
                  ) : null}
                  <div className="form-grid proxy-grid">
                    <label>
                      协议
                      <select
                        disabled={mihomoBinding !== undefined}
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            scheme: event.target.value as
                              "http" | "https" | "socks4" | "socks5",
                          })
                        }
                        value={networkProfile.scheme}
                      >
                        <option value="http">HTTP</option>
                        <option value="https">HTTPS</option>
                        <option value="socks4">SOCKS4</option>
                        <option value="socks5">SOCKS5</option>
                      </select>
                    </label>
                    <label>
                      {localProxySelected ? "本机监听地址" : "代理主机"}
                      <input
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            host: event.target.value,
                          })
                        }
                        placeholder="127.0.0.1"
                        value={networkProfile.host}
                      />
                    </label>
                    <label>
                      {localProxySelected ? "本机端口" : "代理端口"}
                      <input
                        max="65535"
                        min="1"
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            port: Number(event.target.value),
                          })
                        }
                        type="number"
                        value={networkProfile.port}
                      />
                    </label>
                    <label className="check-field">
                      <input
                        disabled={mihomoBinding !== undefined}
                        checked={networkProfile.proxyRequired}
                        onChange={(event) =>
                          setNetworkProfile({
                            ...networkProfile,
                            proxyRequired: event.target.checked,
                          })
                        }
                        type="checkbox"
                      />
                      {localProxySelected
                        ? "本机代理端口不可连接就拒绝启动，不回退真实出口"
                        : "代理不可连接就拒绝启动，不回退真实出口"}
                    </label>
                  </div>
                  <div className="proxy-auth-card">
                    <div>
                      <strong>代理认证（可空）</strong>
                      <span>
                        支持 HTTP Basic 与 SOCKS5
                        用户名/密码。凭据只进入加密保险库，浏览器只看到随机本机
                        SOCKS5
                        端口；这不是按浏览器进程授权，同一用户下其他本机进程若发现端口仍可能借用。
                      </span>
                    </div>
                    <div className="form-grid auth-grid">
                      <label>
                        用户名
                        <input
                          autoComplete="off"
                          onChange={(event) =>
                            setProxyUsername(event.target.value)
                          }
                          value={proxyUsername}
                        />
                      </label>
                      <label>
                        密码
                        <input
                          autoComplete="new-password"
                          onChange={(event) =>
                            setProxyPassword(event.target.value)
                          }
                          type="password"
                          value={proxyPassword}
                        />
                      </label>
                    </div>
                    {proxyUsername !== "" &&
                    !["http", "socks5"].includes(networkProfile.scheme) ? (
                      <p className="field-warning">
                        当前方式无法安全保存这类代理的登录信息；请改用
                        HTTP/SOCKS5， 或让 Mihomo / Clash 应用自行处理。
                      </p>
                    ) : null}
                  </div>
                </>
              ) : null}
              {networkProfile.mode === "pac" ? (
                <div className="form-grid pac-grid">
                  <label>
                    自动代理配置地址（PAC）
                    <input
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          pacUrl: event.target.value,
                        })
                      }
                      placeholder="https://example.test/proxy.pac"
                      value={networkProfile.pacUrl}
                    />
                  </label>
                  <label className="check-field">
                    <input
                      checked={networkProfile.proxyRequired}
                      onChange={(event) =>
                        setNetworkProfile({
                          ...networkProfile,
                          proxyRequired: event.target.checked,
                        })
                      }
                      type="checkbox"
                    />
                    必须代理（如果规则可能直连，启动前阻止）
                  </label>
                </div>
              ) : null}
            </fieldset>
          ) : executionTarget.kind === "wsl" ? (
            <div className="wsl-network-boundary" hidden={!advancedOpen}>
              <div>
                <strong>Linux 环境当前仅支持直连</strong>
                <p>
                  浏览器通过该 Linux
                  环境的默认网络访问网站。代理、自动代理规则和 本机 Mihomo
                  绑定暂不能用于这个运行位置。
                </p>
              </div>
              <span className="boundary-status">直连</span>
            </div>
          ) : null}

          <div className="step-heading">
            <span>2</span>
            <div>
              <h2>确认网站将看到什么</h2>
              <p>
                这些是当前能够诚实说明的身份边界，不会把未控制的硬件特征标为已隐藏。
              </p>
            </div>
          </div>
          <section
            className="website-visibility-card"
            aria-label="网站可见信息确认"
          >
            <div className="visibility-heading">
              <div>
                <strong>网站可见身份摘要</strong>
                <p>
                  首次成功启动时，浏览器身份配置与运行位置会固定到这个 Silo。
                </p>
              </div>
            </div>
            <dl className="visibility-facts">
              <div>
                <dt>运行位置</dt>
                <dd>
                  {localExecution
                    ? "这台 Windows 电脑"
                    : executionTarget.kind === "wsl"
                      ? `WSL · ${executionTarget.distribution}`
                      : "远程运行"}
                </dd>
              </div>
              <div>
                <dt>浏览器身份</dt>
                <dd>
                  {localExecution
                    ? `${browserKind === "chrome" ? "Google Chrome" : "Microsoft Edge"} · Windows`
                    : "Chromium · Linux"}
                </dd>
              </div>
              <div>
                <dt>Profile 隔离</dt>
                <dd>
                  <CapabilityState state="native" />
                  每个 Silo 独立保存 Cookie、登录状态和网站数据
                </dd>
              </div>
              <div>
                <dt>设备与浏览器指纹</dt>
                <dd>
                  <CapabilityState state="inherit" />
                  {localExecution
                    ? "CPU、内存、Canvas、WebGL 与字体跟随本机，当前未做统一控制"
                    : "CPU、内存与图形特征跟随 WSL 和本机，当前未做统一控制"}
                </dd>
              </div>
              <div>
                <dt>受控指纹</dt>
                <dd>
                  <CapabilityState state="unavailable" />
                  Standard Silo 不提供受控指纹，不会把 Profile 隔离标成指纹验证
                </dd>
              </div>
              <div>
                <dt>网络出口</dt>
                <dd>
                  {localExecution
                    ? describeNetwork(networkProfile)
                    : "通过 Linux 环境直连；实际出口可在创建后主动检查"}
                </dd>
              </div>
              <div>
                <dt>语言与时区</dt>
                <dd>
                  {localExecution
                    ? "跟随所选浏览器与 Windows，当前未固定"
                    : "跟随 Linux 环境，当前未固定"}
                </dd>
              </div>
              <div>
                <dt>屏幕</dt>
                <dd>跟随当前显示设备，创建时不伪装为固定值</dd>
              </div>
              <div>
                <dt>WebRTC 与 DNS</dt>
                <dd>创建时尚未测量；启动后从浏览器侧边栏主动检查</dd>
              </div>
              {localExecution && browserKind === "edge" ? (
                <div>
                  <dt>系统账户</dt>
                  <dd>微软或企业网站仍可能通过 Windows 单点登录识别设备账户</dd>
                </div>
              ) : null}
            </dl>
            <label className="visibility-confirmation">
              <input
                checked={websiteBoundaryConfirmed}
                disabled={busy || mihomoBusy}
                onChange={(event) =>
                  setWebsiteBoundaryConfirmed(event.target.checked)
                }
                type="checkbox"
              />
              <span>
                <strong>我已核对以上边界</strong>
                <small>
                  按这些设置创建，并在首次启动时锁定浏览器身份配置与运行位置。
                </small>
              </span>
            </label>
          </section>
          <div className="submit-row">
            <div>
              <strong>准备创建新的 Silo</strong>
              <span>不会导入、复制或改写已有浏览器数据。</span>
            </div>
            <button
              disabled={
                busy ||
                mihomoBusy ||
                !websiteBoundaryConfirmed ||
                name.trim().length === 0 ||
                executionTarget.kind === "remote" ||
                (localExecution && browserPath.trim().length === 0)
              }
              onClick={() => void createSilo()}
              type="button"
            >
              创建 Silo
            </button>
          </div>
        </section>
      )}
    </>
  );
}

function ManagedSiloForm({
  busy,
  initialColor,
  onSubmit,
}: {
  busy: boolean;
  initialColor: string;
  onSubmit: (input: CreateManagedSiloInput) => Promise<void>;
}) {
  const [name, setName] = useState("");
  const [color, setColor] = useState(initialColor);
  const [identityPreset, setIdentityPreset] =
    useState<ManagedIdentityPreset>("balanced-en-us");
  const [networkMode, setNetworkMode] = useState<"direct" | "proxy">("direct");
  const [proxyScheme, setProxyScheme] = useState<"http" | "socks5">("http");
  const [proxyHost, setProxyHost] = useState("");
  const [proxyPort, setProxyPort] = useState("8080");
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const formRef = useRef<HTMLFormElement>(null);
  const errorRef = useRef<HTMLDivElement>(null);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError(null);
    setSuccess(null);
    if (name.trim() === "") {
      setError("请填写 Silo 名称。");
      return;
    }
    const selectedIdentityPreset: ManagedIdentityPreset =
      networkMode === "proxy" ? "match-fixed-proxy" : identityPreset;
    let networkProfile: ManagedNetworkProfile;
    let proxyCredentials: CreateManagedSiloInput["proxyCredentials"];
    if (networkMode === "direct") {
      networkProfile = { mode: "direct", proxyRequired: false };
    } else {
      const port = Number(proxyPort);
      if (
        proxyHost.trim() === "" ||
        !Number.isInteger(port) ||
        port < 1 ||
        port > 65_535
      ) {
        setError("请填写有效的代理主机和 1–65535 之间的端口。");
        return;
      }
      const hasUsername = proxyUsername.trim() !== "";
      const hasPassword = proxyPassword !== "";
      if (hasUsername !== hasPassword) {
        setError("代理用户名和密码需要同时填写；无认证代理请都留空。");
        return;
      }
      networkProfile = {
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: proxyScheme,
        host: proxyHost.trim(),
        port,
        bypassList: [],
      };
      if (hasUsername) {
        proxyCredentials = {
          username: proxyUsername.trim(),
          password: proxyPassword,
        };
      }
    }
    try {
      await onSubmit({
        name: name.trim(),
        color,
        identityPreset: selectedIdentityPreset,
        networkProfile,
        ...(proxyCredentials === undefined ? {} : { proxyCredentials }),
      });
      setSuccess("托管身份浏览器已创建。");
    } catch (submitError) {
      setError(managedErrorMessage(submitError));
      window.setTimeout(() => errorRef.current?.focus(), 0);
    }
  };

  return (
    <section className="panel managed-create-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">托管身份浏览器</p>
          <h1>创建托管身份浏览器</h1>
          <p>
            内置浏览器组件会管理网站可见的身份预设。你只需要选择预设和网络方式；
            内部身份材料不会显示在这里。
          </p>
        </div>
        <span className="provider-health healthy">内置组件已就绪</span>
      </div>
      <form
        aria-busy={busy}
        className="managed-create-form"
        noValidate
        onSubmit={(event) => void submit(event)}
        ref={formRef}
      >
        {error !== null ? (
          <div
            aria-labelledby="managed-create-error-title"
            className="managed-form-message error"
            ref={errorRef}
            role="alert"
            tabIndex={-1}
          >
            <strong id="managed-create-error-title">创建没有完成</strong>
            <span>{error}</span>
            <button
              className="button-secondary"
              disabled={busy}
              onClick={() => formRef.current?.requestSubmit()}
              type="button"
            >
              重试
            </button>
          </div>
        ) : null}
        {success !== null ? (
          <p className="managed-form-message success" role="status">
            {success}
          </p>
        ) : null}
        <div className="form-grid identity-grid">
          <label htmlFor="managed-silo-name">
            Silo 名称
            <input
              disabled={busy}
              id="managed-silo-name"
              maxLength={64}
              onChange={(event) => setName(event.target.value)}
              placeholder="例如：隔离工作账号"
              value={name}
            />
          </label>
          <label htmlFor="managed-silo-color">
            标识颜色
            <span className="color-control">
              <input
                disabled={busy}
                id="managed-silo-color"
                onChange={(event) => setColor(event.target.value)}
                type="color"
                value={color}
              />
              <span>{color.toUpperCase()}</span>
            </span>
          </label>
        </div>
        <label htmlFor="managed-identity-preset">
          身份预设
          {networkMode === "proxy" ? (
            <select
              disabled
              id="managed-identity-preset"
              value="match-fixed-proxy"
            >
              <option value="match-fixed-proxy">跟随固定代理</option>
            </select>
          ) : (
            <select
              disabled={busy}
              id="managed-identity-preset"
              onChange={(event) =>
                setIdentityPreset(event.target.value as ManagedIdentityPreset)
              }
              value={identityPreset}
            >
              <option value="balanced-en-us">均衡 · English (US)</option>
              <option value="balanced-zh-cn">均衡 · 中文（简体）</option>
              <option value="balanced-de-de">均衡 · Deutsch (DE)</option>
            </select>
          )}
          <span className="form-hint">
            预设由内置组件解析；页面不展示原始身份配置。
          </span>
        </label>
        <fieldset className="managed-network-fieldset">
          <legend>网络方式</legend>
          <div
            className="network-options"
            role="radiogroup"
            aria-label="托管浏览器网络方式"
          >
            <label
              className={
                networkMode === "direct"
                  ? "network-option selected"
                  : "network-option"
              }
            >
              <input
                checked={networkMode === "direct"}
                disabled={busy}
                name="managed-network"
                onChange={() => {
                  setNetworkMode("direct");
                  if (identityPreset === "match-fixed-proxy") {
                    setIdentityPreset("balanced-en-us");
                  }
                }}
                type="radio"
              />
              <span>
                <strong>Direct 直连</strong>
                <small>不使用代理。</small>
              </span>
            </label>
            <label
              className={
                networkMode === "proxy"
                  ? "network-option selected"
                  : "network-option"
              }
            >
              <input
                checked={networkMode === "proxy"}
                disabled={busy}
                name="managed-network"
                onChange={() => {
                  setNetworkMode("proxy");
                  setIdentityPreset("match-fixed-proxy");
                }}
                type="radio"
              />
              <span>
                <strong>必须代理</strong>
                <small>HTTP 或 SOCKS5；代理不可用就拒绝启动。</small>
              </span>
            </label>
          </div>
          {networkMode === "proxy" ? (
            <div className="managed-proxy-fields">
              <div className="form-grid proxy-grid">
                <label htmlFor="managed-proxy-scheme">
                  代理协议
                  <select
                    disabled={busy}
                    id="managed-proxy-scheme"
                    onChange={(event) =>
                      setProxyScheme(event.target.value as "http" | "socks5")
                    }
                    value={proxyScheme}
                  >
                    <option value="http">HTTP</option>
                    <option value="socks5">SOCKS5</option>
                  </select>
                </label>
                <label htmlFor="managed-proxy-host">
                  代理主机
                  <input
                    disabled={busy}
                    id="managed-proxy-host"
                    onChange={(event) => setProxyHost(event.target.value)}
                    placeholder="proxy.example.test"
                    value={proxyHost}
                  />
                </label>
                <label htmlFor="managed-proxy-port">
                  代理端口
                  <input
                    disabled={busy}
                    id="managed-proxy-port"
                    inputMode="numeric"
                    max="65535"
                    min="1"
                    onChange={(event) => setProxyPort(event.target.value)}
                    type="number"
                    value={proxyPort}
                  />
                </label>
              </div>
              <div className="form-grid auth-grid">
                <label htmlFor="managed-proxy-username">
                  代理用户名（可空）
                  <input
                    autoComplete="username"
                    disabled={busy}
                    id="managed-proxy-username"
                    onChange={(event) => setProxyUsername(event.target.value)}
                    value={proxyUsername}
                  />
                </label>
                <label htmlFor="managed-proxy-password">
                  代理密码（可空）
                  <input
                    autoComplete="current-password"
                    disabled={busy}
                    id="managed-proxy-password"
                    onChange={(event) => setProxyPassword(event.target.value)}
                    type="password"
                    value={proxyPassword}
                  />
                </label>
              </div>
              <p className="form-hint">
                代理凭据只会进入加密保险库；托管浏览器不会看到原始凭据。
              </p>
            </div>
          ) : null}
        </fieldset>
        <div className="submit-row">
          <div>
            <strong>只创建独立的托管浏览器 Profile</strong>
            <span>不会导入或改写系统浏览器数据。</span>
          </div>
          <button disabled={busy || name.trim() === ""} type="submit">
            {busy ? "正在创建…" : "创建托管身份浏览器"}
          </button>
        </div>
      </form>
    </section>
  );
}

function CapabilityState({
  state,
}: {
  state: "native" | "inherit" | "unavailable";
}) {
  const labels = {
    native: "本机原生",
    inherit: "跟随本机",
    unavailable: "当前不可用",
  } as const;
  return (
    <span className={`capability-state ${state}`}>
      <code>{state}</code> · {labels[state]}
    </span>
  );
}

function NetworkOption({
  checked,
  description,
  label,
  onChange,
}: {
  checked: boolean;
  description: string;
  label: string;
  onChange: () => void;
}) {
  return (
    <label className={checked ? "network-option selected" : "network-option"}>
      <input
        checked={checked}
        name="network"
        onChange={onChange}
        type="radio"
      />
      <span>
        <strong>{label}</strong>
        <small>{description}</small>
      </span>
    </label>
  );
}

type EnvironmentSection = "browser" | "local" | "remote";

function EnvironmentWorkspace({
  silos,
  vaultLocked,
}: {
  silos: Silo[];
  vaultLocked: boolean;
}) {
  const [environmentSection, setEnvironmentSection] =
    useState<EnvironmentSection>("browser");
  const [wslStatus, setWslStatus] = useState<WslStatus | null>(null);
  const [wslBusy, setWslBusy] = useState(false);
  const [selectedWslDistribution, setSelectedWslDistribution] = useState("");
  const [engineStatuses, setEngineStatuses] = useState<EngineAdapterStatus[]>(
    [],
  );
  const [environmentStatuses, setEnvironmentStatuses] = useState<
    EnvironmentBackendStatus[]
  >([]);
  const [technologyError, setTechnologyError] = useState<string | null>(null);
  const [remoteStatus, setRemoteStatus] =
    useState<RemoteEnvironmentStatus | null>(null);
  const [remoteOrigin, setRemoteOrigin] = useState("");
  const [remotePinKind, setRemotePinKind] =
    useState<RemoteEndpoint["pin"]["kind"]>("spki_sha256");
  const [remotePinSha256, setRemotePinSha256] = useState("");
  const [remoteValidation, setRemoteValidation] = useState<{
    tone: "success" | "error";
    text: string;
  } | null>(null);
  const [remotePairingTokenId, setRemotePairingTokenId] = useState("");
  const [remotePairingToken, setRemotePairingToken] = useState("");
  const [remotePairingExpiresAt, setRemotePairingExpiresAt] = useState("");
  const [remotePairingApproved, setRemotePairingApproved] = useState(false);
  const [remoteRotationPinKind, setRemoteRotationPinKind] =
    useState<RemoteEndpoint["pin"]["kind"]>("spki_sha256");
  const [remoteRotationPinSha256, setRemoteRotationPinSha256] = useState("");
  const [remoteRotationTokenId, setRemoteRotationTokenId] = useState("");
  const remoteRotationTokenRef = useRef<HTMLInputElement>(null);
  const [remoteRotationTokenReady, setRemoteRotationTokenReady] =
    useState(false);
  const [remoteRotationExpiresAt, setRemoteRotationExpiresAt] = useState("");
  const [remoteRotationApproved, setRemoteRotationApproved] = useState(false);
  const [selectedRemoteSilo, setSelectedRemoteSilo] = useState("");
  const [remoteNetworkMode, setRemoteNetworkMode] =
    useState<RemoteNetworkPolicy["mode"]>("direct");
  const [remoteProxyPolicyId, setRemoteProxyPolicyId] = useState("");
  const [remoteProxyRequired, setRemoteProxyRequired] = useState(true);
  const [remoteTtlSeconds, setRemoteTtlSeconds] = useState("");
  const [remoteCostAcknowledged, setRemoteCostAcknowledged] = useState(false);
  const [remoteHumanLifetime, setRemoteHumanLifetime] = useState("1800");
  const [remoteAutomationLifetime, setRemoteAutomationLifetime] =
    useState("300");
  const [remoteAutomationReadScreen, setRemoteAutomationReadScreen] =
    useState(true);
  const [remoteAutomationSendInput, setRemoteAutomationSendInput] =
    useState(false);
  const [remoteAutomationApproved, setRemoteAutomationApproved] =
    useState(false);
  const [remoteInputText, setRemoteInputText] = useState("");
  const [remoteInteractionClock, setRemoteInteractionClock] = useState(() =>
    Date.now(),
  );
  const [remoteInteractionMessage, setRemoteInteractionMessage] = useState<{
    tone: "success" | "error";
    text: string;
  } | null>(null);
  const [remoteBusy, setRemoteBusy] = useState(false);
  const [remoteActionMessage, setRemoteActionMessage] = useState<{
    tone: "success" | "error";
    text: string;
  } | null>(null);
  const [selectedEnvironmentBackend, setSelectedEnvironmentBackend] =
    useState<EnvironmentBackendStatus["backend"]>("wsl-chromium");
  const [selectedEnvironmentSilo, setSelectedEnvironmentSilo] = useState("");
  const [environmentActionBusy, setEnvironmentActionBusy] = useState(false);
  const [environmentActionMessage, setEnvironmentActionMessage] = useState<{
    tone: "success" | "error";
    text: string;
  } | null>(null);
  const visibleEngineStatuses = engineStatuses;

  useEffect(() => {
    if (vaultLocked) {
      setEngineStatuses([]);
      setEnvironmentStatuses([]);
      setRemoteStatus(null);
      setTechnologyError(null);
      return;
    }
    void Promise.all([
      desktopApi.listEngineAdapters(),
      desktopApi.environmentBackendStatuses(),
      desktopApi.remoteEnvironmentStatus(),
    ])
      .then(([engines, environments, remote]) => {
        setEngineStatuses(engines);
        setEnvironmentStatuses(environments);
        setRemoteStatus(remote);
        if (remote.endpoint !== null) {
          setRemoteOrigin(remote.endpoint.origin);
          setRemotePinKind(remote.endpoint.pin.kind);
          setRemotePinSha256(remote.endpoint.pin.sha256);
        }
        setTechnologyError(null);
      })
      .catch((error: unknown) => setTechnologyError(errorMessage(error)));
  }, [vaultLocked]);

  useEffect(() => {
    const interval = window.setInterval(
      () => setRemoteInteractionClock(Date.now()),
      30_000,
    );
    return () => window.clearInterval(interval);
  }, []);

  useEffect(() => {
    setRemoteInputText("");
    setRemoteInteractionMessage(null);
    setRemoteAutomationApproved(false);
  }, [selectedRemoteSilo]);

  useEffect(() => {
    if (
      selectedEnvironmentSilo === "" &&
      silos[0] !== undefined &&
      !vaultLocked
    ) {
      setSelectedEnvironmentSilo(silos[0].id);
    }
  }, [selectedEnvironmentSilo, silos, vaultLocked]);

  useEffect(() => {
    if (vaultLocked) {
      return;
    }
    const firstBinding = remoteStatus?.bindings[0];
    const selectionHasBinding = remoteStatus?.bindings.some(
      (binding) => binding.siloId === selectedRemoteSilo,
    );
    if (firstBinding !== undefined && !selectionHasBinding) {
      setSelectedRemoteSilo(firstBinding.siloId);
    } else if (selectedRemoteSilo === "" && silos[0] !== undefined) {
      setSelectedRemoteSilo(firstBinding?.siloId ?? silos[0].id);
    }
  }, [remoteStatus, selectedRemoteSilo, silos, vaultLocked]);

  useEffect(() => {
    if (
      environmentSection === "remote" &&
      (remoteStatus?.bindings.length ?? 0) === 0
    ) {
      setEnvironmentSection("browser");
    }
  }, [environmentSection, remoteStatus?.bindings.length]);

  const runEnvironmentOperation = async (operation: EnvironmentOperation) => {
    const silo = silos.find(
      (candidate) => candidate.id === selectedEnvironmentSilo,
    );
    const status = environmentStatuses.find(
      (candidate) => candidate.backend === selectedEnvironmentBackend,
    );
    const capability = status?.capabilities.find(
      (candidate) => candidate.operation === operation,
    );
    if (vaultLocked || silo === undefined) {
      setEnvironmentActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个 Silo。",
      });
      return;
    }
    if (
      selectedEnvironmentBackend === "wsl-chromium" &&
      wslStatus !== null &&
      requiresExplicitWslSelection(
        wslStatus.distributions,
        selectedWslDistribution,
      )
    ) {
      setEnvironmentActionMessage({
        tone: "error",
        text: "发现了多个 Linux 环境。请先在下方选择要使用的一个。",
      });
      return;
    }
    if (
      capability === undefined ||
      capability.availability.availability !== "available"
    ) {
      setEnvironmentActionMessage({
        tone: "error",
        text: "当前运行位置暂不支持此操作。",
      });
      return;
    }

    const network = environmentNetworkForSilo(silo);
    if (
      (operation === "create" || operation === "configureNetwork") &&
      network === null
    ) {
      setEnvironmentActionMessage({
        tone: "error",
        text: "当前网络设置无法安全用于这个运行位置。请先改为直连，或使用环境可以访问的 HTTP、HTTPS 或 SOCKS5 代理。",
      });
      return;
    }
    if (
      operation === "destroy" &&
      !window.confirm(
        `确认删除「${silo.name}」在 ${environmentBackendLabel(selectedEnvironmentBackend)} 中的环境？`,
      )
    ) {
      return;
    }

    let request: EnvironmentOperationRequest;
    const base = {
      backend: selectedEnvironmentBackend,
      environmentId: silo.id,
    };
    switch (operation) {
      case "create":
        request = { ...base, operation, network: network! };
        break;
      case "configureNetwork":
        request = { ...base, operation, network: network! };
        break;
      case "destroy":
        request = { ...base, operation, confirmDestroy: true };
        break;
      case "start":
      case "stop":
      case "pause":
      case "snapshot":
      case "health":
      case "logs":
        request = { ...base, operation };
        break;
    }

    setEnvironmentActionBusy(true);
    setEnvironmentActionMessage(null);
    try {
      await desktopApi.executeEnvironmentBackend(request);
      setEnvironmentActionMessage({
        tone: "success",
        text: `${environmentOperationLabel(operation)}已完成。`,
      });
      setEnvironmentStatuses(await desktopApi.environmentBackendStatuses());
    } catch (error) {
      setEnvironmentActionMessage({
        tone: "error",
        text: "操作没有完成。请检查上方使用条件后重试。",
      });
    } finally {
      setEnvironmentActionBusy(false);
    }
  };

  const checkWsl = async () => {
    setWslBusy(true);
    try {
      const detected = await desktopApi.detectWsl();
      setWslStatus(detected);
      setSelectedWslDistribution("");
      setEnvironmentStatuses(await desktopApi.environmentBackendStatuses());
    } catch (error) {
      setWslStatus({
        supportedPlatform: false,
        available: false,
        distributions: [],
        message: errorMessage(error),
      });
    } finally {
      setWslBusy(false);
    }
  };

  const configureWslDistribution = async () => {
    if (
      wslStatus === null ||
      !canConfigureWslDistribution(
        wslStatus.distributions,
        selectedWslDistribution,
      )
    ) {
      return;
    }
    setWslBusy(true);
    try {
      await desktopApi.selectWslEnvironmentDistribution(
        selectedWslDistribution,
      );
      setEnvironmentStatuses(await desktopApi.environmentBackendStatuses());
    } catch (error) {
      setTechnologyError(errorMessage(error));
    } finally {
      setWslBusy(false);
    }
  };

  const validateRemoteEndpoint = async () => {
    setRemoteValidation(null);
    try {
      await desktopApi.validateRemoteEnvironmentEndpoint({
        ownership: "user_self_hosted",
        origin: remoteOrigin.trim(),
        pin: {
          kind: remotePinKind,
          sha256: remotePinSha256.trim().toLowerCase(),
        },
      });
      setRemoteValidation({
        tone: "success",
        text: "填写内容格式正确。本次检查没有联网，也没有保存任何信息。",
      });
    } catch (error) {
      setRemoteValidation({
        tone: "error",
        text: "填写内容有误，请检查服务地址和安全指纹。",
      });
    }
  };

  const remoteEndpointInput = (): RemoteEndpoint => ({
    ownership: "user_self_hosted",
    origin: remoteOrigin.trim(),
    pin: {
      kind: remotePinKind,
      sha256: remotePinSha256.trim().toLowerCase(),
    },
  });

  const refreshRemoteStatus = async () => {
    const next = await desktopApi.remoteEnvironmentStatus();
    setRemoteStatus(next);
    return next;
  };

  const pairRemoteEndpoint = async () => {
    const expiresAt = Date.parse(remotePairingExpiresAt);
    if (vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库。连接信息只会加密保存在本机。",
      });
      return;
    }
    if (!remotePairingApproved) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先确认本次连接。连接确认不会同时接受后续创建费用。",
      });
      return;
    }
    if (!Number.isFinite(expiresAt)) {
      setRemoteActionMessage({
        tone: "error",
        text: "请填写一次性配对码的到期时间。",
      });
      return;
    }
    const token = remotePairingToken;
    const tokenId = remotePairingTokenId.trim();
    setRemotePairingToken("");
    setRemotePairingTokenId("");
    setRemotePairingExpiresAt("");
    setRemotePairingApproved(false);
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const next = await desktopApi.pairRemoteEnvironment(
        remoteEndpointInput(),
        {
          approvedByUser: true,
          pairingTokenId: tokenId,
          pairingToken: token,
          pairingTokenExpiresAtUnixMs: expiresAt,
        },
      );
      setRemoteStatus(next);
      setRemoteValidation(null);
      setRemoteActionMessage({
        tone: "success",
        text: "远程服务已连接，访问凭据已加密保存在本机。一次性配对码已清空。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: "连接没有完成。请检查服务地址、安全指纹和配对码后重试。",
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const revokeRemotePairing = async () => {
    if (
      !window.confirm(
        "确认断开这台电脑与远程服务的连接？已创建的远程环境不会因此自动删除。",
      )
    ) {
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      setRemoteStatus(await desktopApi.revokeRemotePairing());
      setRemoteActionMessage({
        tone: "success",
        text: "已断开连接并清除这台电脑保存的访问凭据。",
      });
    } catch (error) {
      setRemoteActionMessage({
        tone: "error",
        text: "暂时无法断开连接，请稍后重试。",
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const checkRemoteDeletionStatus = async () => {
    if (selectedRemoteBinding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的 Silo。",
      });
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      await desktopApi.recoverRemoteDeletionProof(selectedRemoteBinding.siloId);
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: "已确认远程环境完成删除，并移除了这台电脑上的连接记录。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(
          error,
          "暂时无法确认远程环境已删除。请先恢复连接，或向远程服务运营者核实。",
        ),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const removeLocalRemoteConnection = async () => {
    if (selectedRemoteBinding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的 Silo。",
      });
      return;
    }
    const selectedSilo = silos.find(
      (silo) => silo.id === selectedRemoteBinding.siloId,
    );
    if (
      !window.confirm(
        `Force Detach「${selectedSilo?.name ?? "所选 Silo"}」？这只会移除本机连接记录，不会删除远程环境；它可能仍在运行并继续产生费用。请确认你已阅读此风险。`,
      )
    ) {
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      setRemoteStatus(
        await desktopApi.forceDetachRemoteEnvironment(
          selectedRemoteBinding.siloId,
        ),
      );
      setRemoteActionMessage({
        tone: "success",
        text: "已移除这台电脑上的连接记录。远程环境没有被删除，请按需联系运营者完成清理。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(error, "暂时无法移除本地连接记录，请稍后重试。"),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const runRemoteInteraction = async (
    action:
      | "open_human"
      | "close_human"
      | "grant_automation"
      | "revoke_automation"
      | "check_screen"
      | "send_input",
    authorizationId?: string,
  ) => {
    const silo = silos.find((candidate) => candidate.id === selectedRemoteSilo);
    const binding = remoteStatus?.bindings.find(
      (candidate) => candidate.siloId === selectedRemoteSilo,
    );
    if (
      vaultLocked ||
      silo === undefined ||
      binding === undefined ||
      remoteStatus?.state !== "paired"
    ) {
      setRemoteInteractionMessage({
        tone: "error",
        text: "请先连接远程服务，并选择一个已经创建远程环境的 Silo。",
      });
      return;
    }

    const now = Date.now();
    const activeHuman =
      binding.humanSession !== undefined &&
      !binding.humanSession.revoked &&
      binding.humanSession.expiresAtUnixMs > now
        ? binding.humanSession
        : undefined;
    const activeAutomations = binding.automationAuthorizations.filter(
      (authorization) =>
        !authorization.revoked && authorization.expiresAtUnixMs > now,
    );
    const screenPrincipal: RemoteInteractivePrincipal | null =
      activeHuman !== undefined
        ? {
            kind: "human_session",
            authorizationId: activeHuman.authorizationId,
          }
        : (() => {
            const authorization = activeAutomations.find((candidate) =>
              candidate.scopes.includes("read_screen"),
            );
            return authorization === undefined
              ? null
              : {
                  kind: "automation" as const,
                  authorizationId: authorization.authorizationId,
                };
          })();
    const inputPrincipal: RemoteInteractivePrincipal | null =
      activeHuman !== undefined
        ? {
            kind: "human_session",
            authorizationId: activeHuman.authorizationId,
          }
        : (() => {
            const authorization = activeAutomations.find((candidate) =>
              candidate.scopes.includes("send_input"),
            );
            return authorization === undefined
              ? null
              : {
                  kind: "automation" as const,
                  authorizationId: authorization.authorizationId,
                };
          })();

    if (action === "revoke_automation" && authorizationId === undefined) {
      return;
    }
    if (
      action === "revoke_automation" &&
      !window.confirm("确认取消这项自动操作权限？")
    ) {
      return;
    }
    if (action === "check_screen" && screenPrincipal === null) {
      setRemoteInteractionMessage({
        tone: "error",
        text: "请先开始临时控制，或允许自动操作读取远程画面状态。",
      });
      return;
    }
    if (action === "send_input" && inputPrincipal === null) {
      setRemoteInteractionMessage({
        tone: "error",
        text: "请先开始临时控制，或允许自动操作向远程环境发送输入。",
      });
      return;
    }

    setRemoteBusy(true);
    setRemoteInteractionMessage(null);
    try {
      switch (action) {
        case "open_human": {
          const lifetimeSeconds = Number(remoteHumanLifetime);
          if (![900, 1800, 3600, 14_400, 28_800].includes(lifetimeSeconds)) {
            throw new UserFacingError("请选择临时控制时长。");
          }
          await desktopApi.openRemoteHumanSession(silo.id, lifetimeSeconds);
          break;
        }
        case "close_human":
          await desktopApi.closeRemoteHumanSession(silo.id);
          break;
        case "grant_automation": {
          const lifetimeSeconds = Number(remoteAutomationLifetime);
          const scopes: Array<"read_screen" | "send_input"> = [];
          if (remoteAutomationReadScreen) {
            scopes.push("read_screen");
          }
          if (remoteAutomationSendInput) {
            scopes.push("send_input");
          }
          if (
            !remoteAutomationApproved ||
            scopes.length === 0 ||
            ![300, 900, 1800, 3600].includes(lifetimeSeconds)
          ) {
            throw new UserFacingError("请确认自动操作的时长和允许范围。");
          }
          await desktopApi.grantRemoteAutomation(
            silo.id,
            lifetimeSeconds,
            scopes,
            true,
          );
          break;
        }
        case "revoke_automation":
          await desktopApi.revokeRemoteAutomation(silo.id, authorizationId!);
          break;
        case "check_screen":
          await desktopApi.openRemoteScreen(silo.id, screenPrincipal!);
          break;
        case "send_input": {
          const bytes = new TextEncoder().encode(remoteInputText).byteLength;
          if (
            remoteInputText.trim() !== remoteInputText ||
            bytes < 1 ||
            bytes > 512
          ) {
            throw new UserFacingError("输入内容不能为空、过长或包含首尾空格。");
          }
          await desktopApi.sendRemoteInput(silo.id, inputPrincipal!, [
            { type: "text", value: remoteInputText },
          ]);
          setRemoteInputText("");
          break;
        }
      }
      await refreshRemoteStatus();
      const successText: Record<typeof action, string> = {
        open_human: "临时控制已开启。",
        close_human: "临时控制已结束。",
        grant_automation: "已允许本次自动操作。",
        revoke_automation: "已取消这项自动操作权限。",
        check_screen: "远程画面连接检查通过；当前窗口暂不显示远程画面。",
        send_input: "文本已发送到远程环境。",
      };
      setRemoteInteractionMessage({
        tone: "success",
        text: successText[action],
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteInteractionMessage({
        tone: "error",
        text: errorMessage(error, "远程操作没有完成，请检查连接后重试。"),
      });
    } finally {
      if (action === "grant_automation") {
        setRemoteAutomationApproved(false);
      }
      setRemoteBusy(false);
    }
  };

  const rotateRemoteTlsPin = async () => {
    const currentEndpoint = remoteStatus?.endpoint;
    const expiresAt = Date.parse(remoteRotationExpiresAt);
    const tokenInput = remoteRotationTokenRef.current;
    const token = tokenInput?.value ?? "";
    if (
      vaultLocked ||
      currentEndpoint === null ||
      currentEndpoint === undefined ||
      remoteStatus?.state !== "paired"
    ) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库，并确认当前连接仍然有效。",
      });
      return;
    }
    if (!remoteRotationApproved) {
      setRemoteActionMessage({
        tone: "error",
        text: "请确认本次安全指纹更换。",
      });
      return;
    }
    if (!Number.isFinite(expiresAt) || token.length < 32) {
      setRemoteActionMessage({
        tone: "error",
        text: "请填写远程服务生成的新一次性配对码和到期时间。",
      });
      return;
    }

    const endpoint: RemoteEndpoint = {
      ...currentEndpoint,
      pin: {
        kind: remoteRotationPinKind,
        sha256: remoteRotationPinSha256.trim().toLowerCase(),
      },
    };
    const tokenId = remoteRotationTokenId.trim();
    // The secret is read from an uncontrolled password input, never copied
    // into React state, and cleared before the native network call begins.
    if (tokenInput !== null) {
      tokenInput.value = "";
    }
    setRemoteRotationTokenReady(false);
    setRemoteRotationTokenId("");
    setRemoteRotationExpiresAt("");
    setRemoteRotationApproved(false);
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const next = await desktopApi.rotateRemoteEnvironmentTlsPin(endpoint, {
        approvedByUser: true,
        pairingTokenId: tokenId,
        pairingToken: token,
        pairingTokenExpiresAtUnixMs: expiresAt,
      });
      setRemoteStatus(next);
      setRemoteRotationPinSha256("");
      setRemoteValidation(null);
      setRemoteActionMessage({
        tone: "success",
        text: "安全指纹已更新，新的连接信息已保存。一次性配对码已清空。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: "安全指纹未能更新。原连接信息保持不变，请检查后重试。",
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const remoteNetworkPolicy = (): RemoteNetworkPolicy | null => {
    if (remoteNetworkMode === "direct") {
      return { mode: "direct" };
    }
    if (
      !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(
        remoteProxyPolicyId.trim(),
      )
    ) {
      return null;
    }
    return {
      mode: "fixed_proxy",
      required: remoteProxyRequired,
      policyId: remoteProxyPolicyId.trim(),
    };
  };

  const runRemoteOperation = async (operation: EnvironmentOperation) => {
    const silo = silos.find((candidate) => candidate.id === selectedRemoteSilo);
    const capability = remoteStatus?.capabilities.find(
      (candidate) => candidate.operation === operation,
    );
    if (vaultLocked || silo === undefined) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个现有 Silo。",
      });
      return;
    }
    if (
      capability === undefined ||
      capability.availability.availability !== "available"
    ) {
      setRemoteActionMessage({
        tone: "error",
        text:
          capability?.availability.availability === "unavailable"
            ? "当前远程服务暂不支持此操作。"
            : "当前远程服务暂不支持此操作。",
      });
      return;
    }
    const network = remoteNetworkPolicy();
    if (
      (operation === "create" || operation === "configureNetwork") &&
      network === null
    ) {
      setRemoteActionMessage({
        tone: "error",
        text: "使用远程代理时，请填写服务运营者提供的代理策略编号。",
      });
      return;
    }
    const ttlSeconds = Number(remoteTtlSeconds);
    if (
      operation === "create" &&
      (!Number.isInteger(ttlSeconds) ||
        ttlSeconds < 60 ||
        ttlSeconds > 2_592_000)
    ) {
      setRemoteActionMessage({
        tone: "error",
        text: "最长保留时间必须在 60 秒到 30 天之间。",
      });
      return;
    }
    if (operation === "create" && !remoteCostAcknowledged) {
      setRemoteActionMessage({
        tone: "error",
        text: "创建费用确认与配对批准是两个独立动作；请先阅读并勾选本次创建确认。",
      });
      return;
    }
    if (
      operation === "destroy" &&
      !window.confirm(
        `确认永久删除「${silo.name}」的远程环境？删除完成后，这台电脑上的连接记录也会移除。`,
      )
    ) {
      return;
    }

    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const result = await (async () => {
        switch (operation) {
          case "create":
            return desktopApi.createRemoteEnvironment(
              silo.id,
              network!,
              ttlSeconds,
              true,
            );
          case "start":
            return desktopApi.startRemoteEnvironment(silo.id);
          case "stop":
            return desktopApi.stopRemoteEnvironment(silo.id);
          case "pause":
            return desktopApi.pauseRemoteEnvironment(silo.id);
          case "snapshot":
            return desktopApi.snapshotRemoteEnvironment(silo.id);
          case "destroy":
            return desktopApi.destroyRemoteEnvironment(silo.id);
          case "configureNetwork":
            return desktopApi.configureRemoteEnvironmentNetwork(
              silo.id,
              network!,
            );
          case "health":
            return desktopApi.healthRemoteEnvironment(silo.id);
          case "logs": {
            return desktopApi.logsRemoteEnvironment(silo.id, null, 50);
          }
        }
      })();
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: `${environmentOperationLabel(operation)}完成：${remoteResultStateLabel(result.state)}。`,
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      if (operation === "create") {
        setRemoteCostAcknowledged(false);
      }
      setRemoteBusy(false);
    }
  };

  const runRemoteCleanupOperation = async (
    operation: "stop" | "health" | "logs" | "destroy",
  ) => {
    const binding = remoteStatus?.bindings.find(
      (candidate) => candidate.siloId === selectedRemoteSilo,
    );
    if (binding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "请先解锁保险库并选择一个仍保留连接记录的远程环境。",
      });
      return;
    }
    const silo = silos.find((candidate) => candidate.id === binding.siloId);
    if (silo === undefined) {
      setRemoteActionMessage({
        tone: "error",
        text: "找不到这个远程环境对应的本地 Silo，不能安全执行清理。",
      });
      return;
    }
    if (
      operation === "destroy" &&
      !window.confirm(
        `确认删除「${silo.name}」的远程环境？这会联系远程服务；成功后本机连接记录也会移除。`,
      )
    ) {
      return;
    }

    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const result =
        operation === "stop"
          ? await desktopApi.stopRemoteEnvironment(silo.id)
          : operation === "health"
            ? await desktopApi.healthRemoteEnvironment(silo.id)
            : operation === "logs"
              ? await desktopApi.logsRemoteEnvironment(silo.id, null, 50)
              : await desktopApi.destroyRemoteEnvironment(silo.id);
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: `${environmentOperationLabel(operation)}完成：${remoteResultStateLabel(result.state)}。`,
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({
        tone: "error",
        text: errorMessage(error, "远程清理操作没有完成，请检查连接后重试。"),
      });
    } finally {
      setRemoteBusy(false);
    }
  };

  const selectedBackendStatus = environmentStatuses.find(
    (environment) => environment.backend === selectedEnvironmentBackend,
  );
  const selectedRemoteSiloRecord = silos.find(
    (silo) => silo.id === selectedRemoteSilo,
  );
  const selectedRemoteBinding = remoteStatus?.bindings.find(
    (binding) => binding.siloId === selectedRemoteSilo,
  );
  const selectedRemoteResult = remoteStatus?.lastResults.find(
    (result) => result.siloId === selectedRemoteSilo,
  );
  const activeRemoteHumanSession =
    selectedRemoteBinding?.humanSession !== undefined &&
    !selectedRemoteBinding.humanSession.revoked &&
    selectedRemoteBinding.humanSession.expiresAtUnixMs > remoteInteractionClock
      ? selectedRemoteBinding.humanSession
      : undefined;
  const activeRemoteAutomations =
    selectedRemoteBinding?.automationAuthorizations.filter(
      (authorization) =>
        !authorization.revoked &&
        authorization.expiresAtUnixMs > remoteInteractionClock,
    ) ?? [];
  const remoteInteractionReady =
    !vaultLocked &&
    !remoteBusy &&
    remoteStatus?.state === "paired" &&
    selectedRemoteBinding !== undefined;
  const canCheckRemoteScreen =
    activeRemoteHumanSession !== undefined ||
    activeRemoteAutomations.some((authorization) =>
      authorization.scopes.includes("read_screen"),
    );
  const canSendRemoteInput =
    activeRemoteHumanSession !== undefined ||
    activeRemoteAutomations.some((authorization) =>
      authorization.scopes.includes("send_input"),
    );

  const remotePairingExpiryMs = Date.parse(remotePairingExpiresAt);
  const remotePairingLifetimeMs = remotePairingExpiryMs - Date.now();
  const remotePairingExpiryValid =
    Number.isFinite(remotePairingExpiryMs) &&
    remotePairingLifetimeMs > 0 &&
    remotePairingLifetimeMs <= 5 * 60 * 1_000;
  const remotePairingFieldsValid =
    remotePairingToken.length >= 32 &&
    remotePairingExpiryValid &&
    /^[0-9a-f-]{36}$/iu.test(remotePairingTokenId.trim()) &&
    remoteOrigin.trim() !== "" &&
    /^[a-f0-9]{64}$/u.test(remotePinSha256.trim().toLowerCase());
  const remoteRotationExpiryMs = Date.parse(remoteRotationExpiresAt);
  const remoteRotationLifetimeMs = remoteRotationExpiryMs - Date.now();
  const remoteRotationExpiryValid =
    Number.isFinite(remoteRotationExpiryMs) &&
    remoteRotationLifetimeMs > 0 &&
    remoteRotationLifetimeMs <= 5 * 60 * 1_000;
  const remoteRotationPinValid = /^[a-f0-9]{64}$/u.test(
    remoteRotationPinSha256.trim().toLowerCase(),
  );
  const remoteRotationPinChanged =
    remoteStatus?.endpoint !== null &&
    remoteStatus?.endpoint !== undefined &&
    (remoteStatus.endpoint.pin.kind !== remoteRotationPinKind ||
      remoteStatus.endpoint.pin.sha256 !==
        remoteRotationPinSha256.trim().toLowerCase());
  const remoteRotationFieldsValid =
    remoteRotationTokenReady &&
    remoteRotationExpiryValid &&
    remoteRotationPinValid &&
    remoteRotationPinChanged &&
    /^[0-9a-f-]{36}$/iu.test(remoteRotationTokenId.trim());

  return (
    <>
      <section className="workspace-intro">
        <div>
          <p className="eyebrow">运行位置设置</p>
          <h1>准备或修复可选运行位置</h1>
          <p>
            创建 Silo 时直接选择运行位置。这里只用于准备已安装浏览器，
            以及检查可用于新 Silo 的 Linux 环境。
          </p>
        </div>
        <nav className="environment-switcher" aria-label="运行位置设置类别">
          <button
            aria-pressed={environmentSection === "browser"}
            className="environment-switch"
            onClick={() => setEnvironmentSection("browser")}
            type="button"
          >
            浏览器准备
          </button>
          <button
            aria-pressed={environmentSection === "local"}
            className="environment-switch"
            onClick={() => setEnvironmentSection("local")}
            type="button"
          >
            Linux 环境
          </button>
          {(remoteStatus?.bindings.length ?? 0) > 0 ? (
            <button
              aria-pressed={environmentSection === "remote"}
              className="environment-switch"
              onClick={() => setEnvironmentSection("remote")}
              type="button"
            >
              旧远程环境
            </button>
          ) : null}
        </nav>
      </section>

      {environmentSection === "browser" ? (
        <section className="panel provider-catalog">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">浏览器</p>
              <h2>管理 Silo 可以使用的浏览器</h2>
              <p>
                已安装的 Chrome 和 Edge 可以直接使用。已有 Silo
                使用独立浏览器时，也可以在这里查看并维护对应组件。
              </p>
            </div>
          </div>
          <div className="provider-status-grid">
            {visibleEngineStatuses.map((engine) => (
              <article
                className="provider-status-card"
                key={engine.descriptor.id}
              >
                <div>
                  <strong>{engineAdapterLabel(engine.descriptor.id)}</strong>
                  <span className={`provider-health ${engine.health.state}`}>
                    {engineHealthLabel(engine.health.state)}
                  </span>
                </div>
                <p>{engineHealthDescription(engine.health.state)}</p>
                <small>
                  {engine.descriptor.externallyPackaged
                    ? "通过完整性检查后才会用于新的浏览会话。"
                    : "由浏览器供应商更新；VeriSilo 使用这台电脑上已安装的版本。"}
                </small>
              </article>
            ))}
            {visibleEngineStatuses.length === 0 ? (
              <p className="empty-provider-copy">尚未发现可用浏览器。</p>
            ) : null}
          </div>
        </section>
      ) : null}

      {unboundEnvironmentControlsAvailable() &&
      environmentSection === "local" ? (
        <section className="panel provider-catalog">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">本机位置</p>
              <h2>检查可选运行位置所需的 Windows 功能</h2>
              <p>
                这里保留已有位置的检查和修复入口。新的 Silo 请回到创建页选择
                Windows 本机或已发现的 Linux 环境。
              </p>
            </div>
          </div>
          <div className="provider-status-grid">
            {environmentStatuses.map((environment) => {
              const available = environment.capabilities.filter(
                (capability) =>
                  capability.availability.availability === "available" &&
                  isUserEnvironmentOperation(capability.operation),
              ).length;
              const missing = environment.prerequisites.filter(
                (prerequisite) => prerequisite.state !== "verified",
              );
              return (
                <article
                  className="provider-status-card"
                  key={environment.backend}
                >
                  <div>
                    <strong>
                      {environmentBackendLabel(environment.backend)}
                    </strong>
                    <span
                      className={`provider-health ${available > 0 ? "degraded" : "unavailable"}`}
                    >
                      {available > 0 ? "可以使用" : "需要设置"}
                    </span>
                  </div>
                  <p>
                    {available > 0
                      ? "可以为现有 Silo 创建并管理此类环境。"
                      : "这台电脑尚未满足使用条件。"}
                  </p>
                  <small>
                    {missing.length === 0
                      ? "本机检查已完成。"
                      : "完成所需的 Windows 设置后即可重试。"}
                  </small>
                </article>
              );
            })}
          </div>
          {technologyError !== null ? (
            <p className="field-error" role="alert">
              部分运行环境的状态暂时无法读取，请稍后重试。
            </p>
          ) : null}
          <div className="environment-console">
            <div className="environment-console-heading">
              <div>
                <strong>管理现有 Silo</strong>
                <span>选择运行位置和 Silo 后，可用操作会自动启用。</span>
              </div>
            </div>
            <div className="form-grid environment-console-selects">
              <label>
                运行位置
                <select
                  disabled={environmentActionBusy}
                  onChange={(event) => {
                    setSelectedEnvironmentBackend(
                      event.target.value as EnvironmentBackendStatus["backend"],
                    );
                    setEnvironmentActionMessage(null);
                  }}
                  value={selectedEnvironmentBackend}
                >
                  {environmentStatuses.map((environment) => (
                    <option
                      key={environment.backend}
                      value={environment.backend}
                    >
                      {environmentBackendLabel(environment.backend)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                选择 Silo
                <select
                  disabled={environmentActionBusy || vaultLocked}
                  onChange={(event) => {
                    setSelectedEnvironmentSilo(event.target.value);
                    setEnvironmentActionMessage(null);
                  }}
                  value={selectedEnvironmentSilo}
                >
                  <option value="">
                    {vaultLocked ? "先解锁保险库" : "请选择 Silo"}
                  </option>
                  {silos.map((silo) => (
                    <option key={silo.id} value={silo.id}>
                      {silo.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            <div className="environment-operation-grid">
              {selectedBackendStatus?.capabilities
                .filter(
                  (capability) =>
                    capability.availability.availability === "available" &&
                    isUserEnvironmentOperation(capability.operation),
                )
                .map((capability) => {
                  return (
                    <button
                      className={
                        capability.operation === "destroy"
                          ? "button-danger"
                          : "button-secondary"
                      }
                      disabled={
                        environmentActionBusy ||
                        vaultLocked ||
                        selectedEnvironmentSilo === ""
                      }
                      key={capability.operation}
                      onClick={() =>
                        void runEnvironmentOperation(capability.operation)
                      }
                      type="button"
                    >
                      {environmentOperationLabel(capability.operation)}
                    </button>
                  );
                })}
              {selectedBackendStatus !== undefined &&
              selectedBackendStatus.capabilities.every(
                (capability) =>
                  capability.availability.availability !== "available" ||
                  !isUserEnvironmentOperation(capability.operation),
              ) ? (
                <p className="empty-provider-copy">
                  完成下方使用条件后，这里会显示可用操作。
                </p>
              ) : null}
            </div>
            {selectedBackendStatus !== undefined ? (
              <div className="environment-evidence-boundary">
                <strong>使用条件</strong>
                <p>
                  VeriSilo
                  会在执行前再次检查这些条件。尚未就绪的环境不会显示操作按钮。
                </p>
                <ul>
                  {selectedBackendStatus.prerequisites.map((prerequisite) => (
                    <li key={prerequisite.id}>
                      <span
                        className={`environment-state ${prerequisite.state}`}
                      >
                        {environmentPrerequisiteStateLabel(prerequisite.state)}
                      </span>
                      <span>
                        {environmentPrerequisiteLabel(prerequisite.id)}
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ) : null}
            {environmentActionMessage !== null ? (
              <p
                className={`environment-action-message ${environmentActionMessage.tone}`}
                role={
                  environmentActionMessage.tone === "error" ? "alert" : "status"
                }
              >
                {environmentActionMessage.text}
              </p>
            ) : null}
          </div>
        </section>
      ) : null}

      {unboundEnvironmentControlsAvailable() &&
      environmentSection === "remote" &&
      (remoteStatus?.bindings.length ?? 0) === 0 ? (
        <section className="panel provider-catalog remote-provider-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">远程环境</p>
              <h2>连接你自己的远程服务</h2>
              <p>
                VeriSilo 不会自动连接公共云。填写你信任的服务地址和安全指纹，
                再使用一次性配对码连接。仅在你确认操作时才会联网。
              </p>
            </div>
            <span
              className={`provider-health ${remoteStatus?.state === "paired" ? "healthy" : "unavailable"}`}
            >
              {remoteStatus === null
                ? "状态未知"
                : remoteStateLabel(remoteStatus.state)}
            </span>
          </div>
          {remoteStatus !== null ? (
            <>
              {remoteStatus.endpoint !== null ? (
                <div className="remote-endpoint-proof">
                  <div>
                    <span>已保存的服务地址</span>
                    <strong>{remoteStatus.endpoint.origin}</strong>
                  </div>
                  <div>
                    <span>安全指纹</span>
                    <code>{remoteStatus.endpoint.pin.sha256}</code>
                  </div>
                  {remoteStatus.pairing !== null ? (
                    <div>
                      <span>连接有效期</span>
                      <strong>
                        {new Date(
                          remoteStatus.pairing.credentialExpiresAtUnixMs,
                        ).toLocaleString("zh-CN")}
                        {remoteStatus.pairing.expired ? "（已过期）" : ""}
                      </strong>
                    </div>
                  ) : null}
                  {remoteStatus.pairing !== null ? (
                    <div>
                      <span>远程服务</span>
                      <strong>
                        {remoteStatus.pairing.node.operatorLabel} ·{" "}
                        {remoteStatus.pairing.node.dataRegion}
                      </strong>
                    </div>
                  ) : null}
                </div>
              ) : null}
            </>
          ) : null}
          {remoteStatus === null || remoteStatus.pairing === null ? (
            <>
              <div className="form-grid remote-endpoint-form">
                <label>
                  远程服务地址
                  <input
                    autoComplete="off"
                    disabled={remoteBusy || vaultLocked}
                    onChange={(event) => {
                      setRemoteOrigin(event.target.value);
                      setRemoteValidation(null);
                      setRemotePairingApproved(false);
                    }}
                    placeholder="https://browser.example.com/"
                    spellCheck={false}
                    value={remoteOrigin}
                  />
                </label>
                <label>
                  验证方式
                  <select
                    disabled={remoteBusy || vaultLocked}
                    onChange={(event) => {
                      setRemotePinKind(
                        event.target.value as RemoteEndpoint["pin"]["kind"],
                      );
                      setRemoteValidation(null);
                      setRemotePairingApproved(false);
                    }}
                    value={remotePinKind}
                  >
                    <option value="spki_sha256">服务公钥指纹（推荐）</option>
                    <option value="certificate_sha256">服务证书指纹</option>
                  </select>
                </label>
                <label>
                  安全指纹（64 位小写十六进制）
                  <input
                    autoComplete="off"
                    disabled={remoteBusy || vaultLocked}
                    maxLength={64}
                    onChange={(event) => {
                      setRemotePinSha256(event.target.value);
                      setRemoteValidation(null);
                      setRemotePairingApproved(false);
                    }}
                    spellCheck={false}
                    value={remotePinSha256}
                  />
                </label>
              </div>
              <div className="card-actions">
                <button
                  className="button-secondary"
                  disabled={
                    vaultLocked ||
                    remoteOrigin.trim() === "" ||
                    remoteBusy ||
                    !/^[a-f0-9]{64}$/u.test(
                      remotePinSha256.trim().toLowerCase(),
                    )
                  }
                  onClick={() => void validateRemoteEndpoint()}
                  type="button"
                >
                  检查填写内容
                </button>
              </div>
              {remoteValidation !== null ? (
                <p
                  className={`environment-action-message ${remoteValidation.tone}`}
                  role={remoteValidation.tone === "error" ? "alert" : "status"}
                >
                  {remoteValidation.text}
                </p>
              ) : null}

              <div className="remote-pairing-panel">
                <div className="environment-console-heading">
                  <div>
                    <strong>使用一次性配对码连接</strong>
                    <span>
                      配对码最长有效五分钟。尝试连接后会立即清空，不能再次使用。
                    </span>
                  </div>
                </div>
                <div className="form-grid remote-pairing-form">
                  <label>
                    配对编号
                    <input
                      autoComplete="off"
                      disabled={
                        remoteBusy ||
                        vaultLocked ||
                        remoteStatus?.pairing !== null
                      }
                      onChange={(event) => {
                        setRemotePairingTokenId(event.target.value);
                        setRemotePairingApproved(false);
                      }}
                      spellCheck={false}
                      value={remotePairingTokenId}
                    />
                  </label>
                  <label>
                    一次性配对码
                    <input
                      autoComplete="off"
                      disabled={
                        remoteBusy ||
                        vaultLocked ||
                        remoteStatus?.pairing !== null
                      }
                      onChange={(event) => {
                        setRemotePairingToken(event.target.value);
                        setRemotePairingApproved(false);
                      }}
                      spellCheck={false}
                      type="password"
                      value={remotePairingToken}
                    />
                  </label>
                  <label>
                    配对码到期时间
                    <input
                      disabled={
                        remoteBusy ||
                        vaultLocked ||
                        remoteStatus?.pairing !== null
                      }
                      onChange={(event) => {
                        setRemotePairingExpiresAt(event.target.value);
                        setRemotePairingApproved(false);
                      }}
                      type="datetime-local"
                      value={remotePairingExpiresAt}
                    />
                  </label>
                </div>
                {remotePairingExpiresAt !== "" ? (
                  <p
                    className={`remote-token-expiry ${remotePairingExpiryValid ? "valid" : "invalid"}`}
                    role={remotePairingExpiryValid ? "status" : "alert"}
                  >
                    配对码到期：
                    {Number.isFinite(remotePairingExpiryMs)
                      ? new Date(remotePairingExpiryMs).toLocaleString("zh-CN")
                      : "格式无效"}
                    。
                    {remotePairingExpiryValid
                      ? "当前仍可使用。"
                      : "配对码必须尚未过期，且最多有效五分钟。"}
                  </p>
                ) : null}
                <label className="remote-confirmation">
                  <input
                    checked={remotePairingApproved}
                    disabled={
                      remoteBusy ||
                      vaultLocked ||
                      remoteStatus?.pairing !== null ||
                      !remotePairingFieldsValid
                    }
                    onChange={(event) =>
                      setRemotePairingApproved(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>
                    我确认只将这枚配对码发送到上方服务地址，并核对安全指纹。
                    此确认不代表接受后续创建费用。
                  </span>
                </label>
                <div className="card-actions">
                  <button
                    className="button-primary"
                    disabled={
                      remoteBusy ||
                      vaultLocked ||
                      remoteStatus?.pairing !== null ||
                      !remotePairingApproved ||
                      !remotePairingFieldsValid
                    }
                    onClick={() => void pairRemoteEndpoint()}
                    type="button"
                  >
                    {remoteBusy ? "正在连接…" : "确认并连接"}
                  </button>
                </div>
              </div>
            </>
          ) : (
            <div className="card-actions remote-connection-actions">
              <button
                className="button-danger"
                disabled={remoteBusy || vaultLocked}
                onClick={() => void revokeRemotePairing()}
                type="button"
              >
                断开此远程服务
              </button>
            </div>
          )}

          {remoteStatus?.pairing !== null &&
          remoteStatus?.pairing !== undefined ? (
            <details className="remote-advanced">
              <summary>更换安全指纹</summary>
              <div className="remote-rotation-panel">
                <div className="environment-console-heading">
                  <div>
                    <strong>更新当前服务的安全指纹</strong>
                    <span>
                      远程服务必须同时确认旧连接和新指纹。更新失败时会继续使用原连接信息。
                    </span>
                  </div>
                </div>
                <p className="remote-rotation-boundary">
                  更新需要当前连接仍然可用，并使用远程服务生成的新一次性配对码。
                  无论成功与否，这枚配对码都不能再次使用。
                </p>
                <div className="form-grid remote-rotation-form">
                  <label>
                    当前服务地址（不可更改）
                    <input
                      disabled
                      value={
                        remoteStatus?.endpoint?.origin ?? "尚未连接远程服务"
                      }
                    />
                  </label>
                  <label>
                    新的验证方式
                    <select
                      disabled={remoteBusy || remoteStatus?.state !== "paired"}
                      onChange={(event) => {
                        setRemoteRotationPinKind(
                          event.target.value as RemoteEndpoint["pin"]["kind"],
                        );
                        setRemoteRotationApproved(false);
                      }}
                      value={remoteRotationPinKind}
                    >
                      <option value="spki_sha256">服务公钥指纹（推荐）</option>
                      <option value="certificate_sha256">服务证书指纹</option>
                    </select>
                  </label>
                  <label>
                    新的安全指纹
                    <input
                      autoComplete="off"
                      disabled={remoteBusy || remoteStatus?.state !== "paired"}
                      maxLength={64}
                      onChange={(event) => {
                        setRemoteRotationPinSha256(event.target.value);
                        setRemoteRotationApproved(false);
                      }}
                      spellCheck={false}
                      value={remoteRotationPinSha256}
                    />
                  </label>
                  <label>
                    新配对编号
                    <input
                      autoComplete="off"
                      disabled={remoteBusy || remoteStatus?.state !== "paired"}
                      onChange={(event) => {
                        setRemoteRotationTokenId(event.target.value);
                        setRemoteRotationApproved(false);
                      }}
                      spellCheck={false}
                      value={remoteRotationTokenId}
                    />
                  </label>
                  <label>
                    新一次性配对码
                    <input
                      autoComplete="off"
                      disabled={remoteBusy || remoteStatus?.state !== "paired"}
                      onInput={(event) => {
                        setRemoteRotationTokenReady(
                          event.currentTarget.value.length >= 32,
                        );
                        setRemoteRotationApproved(false);
                      }}
                      ref={remoteRotationTokenRef}
                      spellCheck={false}
                      type="password"
                    />
                  </label>
                  <label>
                    新配对码到期时间
                    <input
                      disabled={remoteBusy || remoteStatus?.state !== "paired"}
                      onChange={(event) => {
                        setRemoteRotationExpiresAt(event.target.value);
                        setRemoteRotationApproved(false);
                      }}
                      type="datetime-local"
                      value={remoteRotationExpiresAt}
                    />
                  </label>
                </div>
                {remoteRotationExpiresAt !== "" ? (
                  <p
                    className={`remote-token-expiry ${remoteRotationExpiryValid ? "valid" : "invalid"}`}
                    role={remoteRotationExpiryValid ? "status" : "alert"}
                  >
                    新配对码到期：
                    {Number.isFinite(remoteRotationExpiryMs)
                      ? new Date(remoteRotationExpiryMs).toLocaleString("zh-CN")
                      : "格式无效"}
                    。
                    {remoteRotationExpiryValid
                      ? "当前仍可使用。"
                      : "新配对码必须尚未过期，且最多有效五分钟。"}
                  </p>
                ) : null}
                <label className="remote-confirmation remote-rotation-confirmation">
                  <input
                    checked={remoteRotationApproved}
                    disabled={
                      remoteBusy ||
                      remoteStatus?.state !== "paired" ||
                      !remoteRotationFieldsValid
                    }
                    onChange={(event) =>
                      setRemoteRotationApproved(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>
                    我确认只将新配对码发送到当前服务地址，并使用上方新指纹核对服务。
                    我知道这枚配对码尝试后不能重用。
                  </span>
                </label>
                <div className="card-actions">
                  <button
                    className="button-primary"
                    disabled={
                      remoteBusy ||
                      vaultLocked ||
                      remoteStatus?.state !== "paired" ||
                      !remoteRotationApproved ||
                      !remoteRotationFieldsValid
                    }
                    onClick={() => void rotateRemoteTlsPin()}
                    type="button"
                  >
                    {remoteBusy ? "正在更新…" : "确认更新安全指纹"}
                  </button>
                </div>
              </div>
            </details>
          ) : null}

          {remoteStatus?.pairing !== null &&
          remoteStatus?.pairing !== undefined ? (
            <div className="remote-lifecycle-console">
              <div className="environment-console-heading">
                <div>
                  <strong>管理 Silo 的远程环境</strong>
                  <span>选择一个 Silo 后，这里只显示当前可以执行的操作。</span>
                </div>
              </div>
              <div className="form-grid remote-operation-form">
                <label>
                  Silo
                  <select
                    disabled={remoteBusy || vaultLocked}
                    onChange={(event) => {
                      setSelectedRemoteSilo(event.target.value);
                      setRemoteActionMessage(null);
                    }}
                    value={selectedRemoteSilo}
                  >
                    <option value="">
                      {vaultLocked ? "先解锁保险库" : "请选择 Silo"}
                    </option>
                    {silos.map((silo) => (
                      <option key={silo.id} value={silo.id}>
                        {silo.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  网络方式
                  <select
                    disabled={remoteBusy}
                    onChange={(event) =>
                      setRemoteNetworkMode(
                        event.target.value as RemoteNetworkPolicy["mode"],
                      )
                    }
                    value={remoteNetworkMode}
                  >
                    <option value="direct">直连</option>
                    <option value="fixed_proxy">使用远程服务的代理</option>
                  </select>
                </label>
                {remoteNetworkMode === "fixed_proxy" ? (
                  <label>
                    代理配置编号
                    <input
                      disabled={remoteBusy}
                      onChange={(event) =>
                        setRemoteProxyPolicyId(event.target.value)
                      }
                      spellCheck={false}
                      value={remoteProxyPolicyId}
                    />
                  </label>
                ) : null}
                <label>
                  最长保留时间（秒）
                  <input
                    disabled={remoteBusy}
                    inputMode="numeric"
                    max={2_592_000}
                    min={60}
                    onChange={(event) =>
                      setRemoteTtlSeconds(event.target.value)
                    }
                    placeholder="60–2592000"
                    type="number"
                    value={remoteTtlSeconds}
                  />
                </label>
              </div>
              {remoteNetworkMode === "fixed_proxy" ? (
                <label className="remote-confirmation compact">
                  <input
                    checked={remoteProxyRequired}
                    disabled={remoteBusy}
                    onChange={(event) =>
                      setRemoteProxyRequired(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>始终使用此代理；代理不可用时阻止环境直接联网。</span>
                </label>
              ) : null}
              <div className="remote-cost-disclosure">
                <strong>创建前确认费用</strong>
                {remoteStatus?.pairing !== null &&
                remoteStatus?.pairing !== undefined ? (
                  <>
                    <dl className="remote-cost-facts">
                      <div>
                        <dt>运营者</dt>
                        <dd>{remoteStatus.pairing.node.operatorLabel}</dd>
                      </div>
                      <div>
                        <dt>数据区域</dt>
                        <dd>{remoteStatus.pairing.node.dataRegion}</dd>
                      </div>
                      <div>
                        <dt>密钥保管</dt>
                        <dd>由你控制</dd>
                      </div>
                      <div>
                        <dt>估算每小时费用</dt>
                        <dd>
                          {formatMicrosCurrency(
                            remoteStatus.pairing.node.cost
                              .estimatedMicrosPerHour,
                            remoteStatus.pairing.node.cost.currency,
                          )}
                        </dd>
                      </div>
                    </dl>
                    <p>费用由远程服务运营者计算，实际金额以对方账单为准。</p>
                  </>
                ) : (
                  <p>
                    连接远程服务后，这里会显示运营者、区域、密钥保管方式和预计费用。
                  </p>
                )}
                <label className="remote-confirmation compact">
                  <input
                    checked={remoteCostAcknowledged}
                    disabled={remoteBusy || remoteStatus?.pairing === null}
                    onChange={(event) =>
                      setRemoteCostAcknowledged(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>
                    我已查看上方费用，并确认下一次创建操作。连接确认不会自动接受费用。
                  </span>
                </label>
              </div>
              <div className="environment-operation-grid remote-operation-grid">
                {remoteStatus?.capabilities
                  .filter(
                    (capability) =>
                      capability.availability.availability === "available" &&
                      isUserEnvironmentOperation(capability.operation),
                  )
                  .filter((capability) =>
                    selectedRemoteBinding === undefined
                      ? capability.operation === "create"
                      : capability.operation !== "create",
                  )
                  .map((capability) => {
                    return (
                      <button
                        className={
                          capability.operation === "destroy"
                            ? "button-danger"
                            : "button-secondary"
                        }
                        disabled={
                          remoteBusy || vaultLocked || selectedRemoteSilo === ""
                        }
                        key={capability.operation}
                        onClick={() =>
                          void runRemoteOperation(capability.operation)
                        }
                        type="button"
                      >
                        {environmentOperationLabel(capability.operation)}
                      </button>
                    );
                  })}
              </div>
              {selectedRemoteSiloRecord !== undefined ? (
                <div className="remote-selected-state">
                  <div className="remote-selected-heading">
                    <div>
                      <span>当前 Silo</span>
                      <strong>{selectedRemoteSiloRecord.name}</strong>
                    </div>
                    <span
                      className={`provider-health ${selectedRemoteBinding === undefined ? "unavailable" : "healthy"}`}
                    >
                      {selectedRemoteBinding === undefined
                        ? selectedRemoteResult?.state === "destroyed"
                          ? "已删除"
                          : "尚未创建"
                        : "已连接"}
                    </span>
                  </div>
                  {selectedRemoteBinding !== undefined ? (
                    <>
                      <dl className="remote-binding-facts">
                        <div>
                          <dt>服务地址</dt>
                          <dd>{selectedRemoteBinding.endpoint.origin}</dd>
                        </div>
                        <div>
                          <dt>网络</dt>
                          <dd>
                            {selectedRemoteBinding.network.mode === "direct"
                              ? "直连"
                              : "使用远程代理"}
                          </dd>
                        </div>
                        <div>
                          <dt>存储</dt>
                          <dd>
                            {selectedRemoteBinding.volume.encrypted
                              ? "已加密"
                              : "未加密"}
                          </dd>
                        </div>
                        <div>
                          <dt>最近活动</dt>
                          <dd>
                            {new Date(
                              selectedRemoteBinding.lastActivityAtUnixMs,
                            ).toLocaleString("zh-CN")}
                          </dd>
                        </div>
                      </dl>
                      <div className="remote-interaction-section">
                        <div className="environment-console-heading">
                          <div>
                            <strong>远程操作</strong>
                            <span>
                              临时控制和自动操作都会到期，可随时在这里结束。
                            </span>
                          </div>
                        </div>
                        <div className="remote-inline-actions">
                          {activeRemoteHumanSession === undefined ? (
                            <>
                              <label>
                                临时控制时长
                                <select
                                  disabled={!remoteInteractionReady}
                                  onChange={(event) =>
                                    setRemoteHumanLifetime(event.target.value)
                                  }
                                  value={remoteHumanLifetime}
                                >
                                  <option value="900">15 分钟</option>
                                  <option value="1800">30 分钟</option>
                                  <option value="3600">1 小时</option>
                                  <option value="14400">4 小时</option>
                                  <option value="28800">8 小时</option>
                                </select>
                              </label>
                              <button
                                className="button-secondary"
                                disabled={!remoteInteractionReady}
                                onClick={() =>
                                  void runRemoteInteraction("open_human")
                                }
                                type="button"
                              >
                                开始临时控制
                              </button>
                            </>
                          ) : (
                            <>
                              <p className="remote-session-status">
                                临时控制已开启，至
                                {new Date(
                                  activeRemoteHumanSession.expiresAtUnixMs,
                                ).toLocaleTimeString("zh-CN", {
                                  hour: "2-digit",
                                  minute: "2-digit",
                                })}
                              </p>
                              <button
                                className="button-danger"
                                disabled={!remoteInteractionReady}
                                onClick={() =>
                                  void runRemoteInteraction("close_human")
                                }
                                type="button"
                              >
                                结束控制
                              </button>
                            </>
                          )}
                          <button
                            className="button-secondary"
                            disabled={
                              !remoteInteractionReady || !canCheckRemoteScreen
                            }
                            onClick={() =>
                              void runRemoteInteraction("check_screen")
                            }
                            type="button"
                          >
                            检查画面连接
                          </button>
                        </div>
                        <div className="remote-input-row">
                          <label>
                            文本输入
                            <input
                              autoComplete="off"
                              disabled={
                                !remoteInteractionReady || !canSendRemoteInput
                              }
                              maxLength={512}
                              onChange={(event) =>
                                setRemoteInputText(event.target.value)
                              }
                              value={remoteInputText}
                            />
                          </label>
                          <button
                            className="button-secondary"
                            disabled={
                              !remoteInteractionReady ||
                              !canSendRemoteInput ||
                              remoteInputText.length === 0
                            }
                            onClick={() =>
                              void runRemoteInteraction("send_input")
                            }
                            type="button"
                          >
                            发送到远程环境
                          </button>
                        </div>
                        <p className="remote-interaction-note">
                          文本会输入到远程环境当前焦点；本窗口无法预览目标位置或远程画面。
                        </p>
                        <details className="remote-advanced remote-automation-panel">
                          <summary>允许自动操作</summary>
                          <div className="form-grid remote-interaction-form automation">
                            <label>
                              允许时长
                              <select
                                disabled={!remoteInteractionReady}
                                onChange={(event) => {
                                  setRemoteAutomationLifetime(
                                    event.target.value,
                                  );
                                  setRemoteAutomationApproved(false);
                                }}
                                value={remoteAutomationLifetime}
                              >
                                <option value="300">5 分钟</option>
                                <option value="900">15 分钟</option>
                                <option value="1800">30 分钟</option>
                                <option value="3600">1 小时</option>
                              </select>
                            </label>
                            <label className="remote-confirmation compact">
                              <input
                                checked={remoteAutomationReadScreen}
                                disabled={!remoteInteractionReady}
                                onChange={(event) => {
                                  setRemoteAutomationReadScreen(
                                    event.target.checked,
                                  );
                                  setRemoteAutomationApproved(false);
                                }}
                                type="checkbox"
                              />
                              <span>读取远程画面状态</span>
                            </label>
                            <label className="remote-confirmation compact">
                              <input
                                checked={remoteAutomationSendInput}
                                disabled={!remoteInteractionReady}
                                onChange={(event) => {
                                  setRemoteAutomationSendInput(
                                    event.target.checked,
                                  );
                                  setRemoteAutomationApproved(false);
                                }}
                                type="checkbox"
                              />
                              <span>向远程环境发送输入</span>
                            </label>
                          </div>
                          <label className="remote-confirmation">
                            <input
                              checked={remoteAutomationApproved}
                              disabled={
                                !remoteInteractionReady ||
                                (!remoteAutomationReadScreen &&
                                  !remoteAutomationSendInput)
                              }
                              onChange={(event) =>
                                setRemoteAutomationApproved(
                                  event.target.checked,
                                )
                              }
                              type="checkbox"
                            />
                            <span>
                              我确认在上方时长内允许所选自动操作；到期后需要重新确认。
                            </span>
                          </label>
                          <div className="card-actions">
                            <button
                              className="button-secondary"
                              disabled={
                                !remoteInteractionReady ||
                                !remoteAutomationApproved ||
                                (!remoteAutomationReadScreen &&
                                  !remoteAutomationSendInput)
                              }
                              onClick={() =>
                                void runRemoteInteraction("grant_automation")
                              }
                              type="button"
                            >
                              允许自动操作
                            </button>
                          </div>
                          {activeRemoteAutomations.length > 0 ? (
                            <ul className="remote-authorization-list">
                              {activeRemoteAutomations.map((authorization) => (
                                <li key={authorization.authorizationId}>
                                  <span>
                                    {authorization.scopes
                                      .map((scope) =>
                                        scope === "read_screen"
                                          ? "可检查画面连接"
                                          : "可发送输入",
                                      )
                                      .join("、")}
                                  </span>
                                  <strong>
                                    至
                                    {new Date(
                                      authorization.expiresAtUnixMs,
                                    ).toLocaleTimeString("zh-CN", {
                                      hour: "2-digit",
                                      minute: "2-digit",
                                    })}
                                  </strong>
                                  <button
                                    className="button-danger"
                                    disabled={!remoteInteractionReady}
                                    onClick={() =>
                                      void runRemoteInteraction(
                                        "revoke_automation",
                                        authorization.authorizationId,
                                      )
                                    }
                                    type="button"
                                  >
                                    取消
                                  </button>
                                </li>
                              ))}
                            </ul>
                          ) : null}
                        </details>
                        {remoteInteractionMessage !== null ? (
                          <p
                            className={`environment-action-message ${remoteInteractionMessage.tone}`}
                            role={
                              remoteInteractionMessage.tone === "error"
                                ? "alert"
                                : "status"
                            }
                          >
                            {remoteInteractionMessage.text}
                          </p>
                        ) : null}
                      </div>
                    </>
                  ) : (
                    <p className="remote-interaction-note">
                      选择“创建”后，这里会显示远程环境状态。
                    </p>
                  )}
                </div>
              ) : null}
              {remoteActionMessage !== null ? (
                <p
                  className={`environment-action-message ${remoteActionMessage.tone}`}
                  role={
                    remoteActionMessage.tone === "error" ? "alert" : "status"
                  }
                >
                  {remoteActionMessage.text}
                </p>
              ) : null}
            </div>
          ) : (
            <div className="remote-selected-state remote-connection-empty">
              <strong>连接后即可管理远程环境</strong>
              <p>
                完成上方配对后，你可以为现有 Silo
                创建、启动、停止或删除远程环境。
              </p>
            </div>
          )}
        </section>
      ) : null}

      {environmentSection === "remote" &&
      (remoteStatus?.bindings.length ?? 0) > 0 ? (
        <section className="panel provider-catalog remote-provider-panel">
          <div className="panel-heading">
            <div>
              <p className="eyebrow">旧远程环境</p>
              <h2>只处理已经存在的远程环境</h2>
              <p>
                这台电脑仍保留远程环境连接记录。此处只提供停止、检查、日志和删除，
                不会创建新环境，也不会打开远程浏览器控制。
              </p>
            </div>
            <span
              className={`provider-health ${remoteStatus?.state === "paired" ? "healthy" : "unavailable"}`}
            >
              {remoteStatus === null
                ? "状态未知"
                : remoteStateLabel(remoteStatus.state)}
            </span>
          </div>
          <p className="remote-recovery-warning">
            连接状态不会改变这个界面的权限。配对有效、过期或已取消时，都只能清理旧环境；
            不能在这里重新配对、启动或交互。
          </p>
          <label>
            Silo
            <select
              disabled={remoteBusy || vaultLocked}
              onChange={(event) => {
                setSelectedRemoteSilo(event.target.value);
                setRemoteActionMessage(null);
              }}
              value={selectedRemoteSilo}
            >
              {remoteStatus?.bindings.map((binding) => {
                const silo = silos.find(
                  (candidate) => candidate.id === binding.siloId,
                );
                return (
                  <option key={binding.siloId} value={binding.siloId}>
                    {silo?.name ?? "已移除的本地 Silo"}
                  </option>
                );
              })}
            </select>
          </label>
          {selectedRemoteBinding !== undefined ? (
            <div className="remote-selected-state">
              <dl className="remote-binding-facts">
                <div>
                  <dt>服务地址</dt>
                  <dd>{selectedRemoteBinding.endpoint.origin}</dd>
                </div>
                <div>
                  <dt>网络</dt>
                  <dd>
                    {selectedRemoteBinding.network.mode === "direct"
                      ? "直连"
                      : "使用远程代理"}
                  </dd>
                </div>
                <div>
                  <dt>存储</dt>
                  <dd>已加密</dd>
                </div>
                <div>
                  <dt>最近活动</dt>
                  <dd>
                    {new Date(
                      selectedRemoteBinding.lastActivityAtUnixMs,
                    ).toLocaleString("zh-CN")}
                  </dd>
                </div>
              </dl>
              <div className="environment-operation-grid remote-operation-grid">
                <button
                  className="button-secondary"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void runRemoteCleanupOperation("stop")}
                  type="button"
                >
                  停止远程环境
                </button>
                <button
                  className="button-secondary"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void runRemoteCleanupOperation("health")}
                  type="button"
                >
                  检查状态
                </button>
                <button
                  className="button-secondary"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void runRemoteCleanupOperation("logs")}
                  type="button"
                >
                  查看日志
                </button>
                <button
                  className="button-danger"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void runRemoteCleanupOperation("destroy")}
                  type="button"
                >
                  删除远程环境
                </button>
              </div>
              {selectedRemoteResult !== undefined ? (
                <div className="remote-result-card">
                  <div>
                    <span>最近一次清理结果</span>
                    <strong>
                      {environmentOperationLabel(
                        selectedRemoteResult.operation,
                      )}
                      ：{remoteResultStateLabel(selectedRemoteResult.state)}
                    </strong>
                  </div>
                  {selectedRemoteResult.logs !== undefined ? (
                    <ul className="remote-log-list">
                      {selectedRemoteResult.logs.map((log) => (
                        <li key={log.sequence}>
                          <span>{log.level}</span>
                          <code>{log.message}</code>
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              ) : null}
              <div className="remote-proof-recovery">
                <strong>远程服务已确认删除时</strong>
                <p>
                  只验证远程服务提供的删除证明，并移除本机连接记录；不会重新创建或启动环境。
                </p>
                <button
                  className="button-secondary"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void checkRemoteDeletionStatus()}
                  type="button"
                >
                  验证远程删除证明
                </button>
              </div>
              <div className="remote-force-detach">
                <strong>无法连接时的最后手段</strong>
                <p>
                  Force Detach 只移除这台电脑上的连接记录，不会删除远程环境。
                  远程环境可能继续运行并产生费用，请先联系远程服务运营者。
                </p>
                <button
                  className="button-danger"
                  disabled={remoteBusy || vaultLocked}
                  onClick={() => void removeLocalRemoteConnection()}
                  type="button"
                >
                  Force Detach：仅移除本机记录
                </button>
              </div>
            </div>
          ) : null}
          {remoteActionMessage !== null ? (
            <p
              className={`environment-action-message ${remoteActionMessage.tone}`}
              role={remoteActionMessage.tone === "error" ? "alert" : "status"}
            >
              {remoteActionMessage.text}
            </p>
          ) : null}
        </section>
      ) : null}

      {environmentSection === "local" ? (
        <section className="panel provider-readiness">
          <div>
            <p className="eyebrow">WSL 设置</p>
            <h2>选择要用于 Silo 的 Linux 环境</h2>
            <p>检查这台电脑已安装的 WSL 发行版，然后选择一个供本次使用。</p>
            {wslStatus !== null ? (
              <div className="provider-result">
                <strong>{wslStatus.available ? "发现 WSL" : "尚不可用"}</strong>
                <span>
                  {wslStatus.available
                    ? `发现 ${wslStatus.distributions.length} 个可选发行版。`
                    : "请先在 Windows 中安装并启用 WSL。"}
                </span>
                {wslStatus.distributions.length > 0 ? (
                  <label>
                    Linux 发行版
                    <select
                      disabled={wslBusy}
                      onChange={(event) =>
                        setSelectedWslDistribution(event.target.value)
                      }
                      value={selectedWslDistribution}
                    >
                      <option value="">请选择</option>
                      {wslStatus.distributions.map((distribution) => (
                        <option key={distribution} value={distribution}>
                          {distribution}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                <button
                  className="button-secondary"
                  disabled={
                    wslBusy ||
                    !canConfigureWslDistribution(
                      wslStatus.distributions,
                      selectedWslDistribution,
                    )
                  }
                  onClick={() => void configureWslDistribution()}
                  type="button"
                >
                  使用此发行版
                </button>
              </div>
            ) : null}
          </div>
          <button
            className="button-secondary"
            disabled={wslBusy}
            onClick={() => void checkWsl()}
            type="button"
          >
            {wslBusy ? "正在检查…" : "检查本机 WSL"}
          </button>
        </section>
      ) : null}
    </>
  );
}

function engineAdapterLabel(
  adapter: EngineAdapterStatus["descriptor"]["id"],
): string {
  switch (adapter) {
    case "stock-chrome":
      return "Google Chrome";
    case "stock-edge":
      return "Microsoft Edge";
    case "controlled-chromium":
      return "独立 Chromium";
    case "camoufox":
      return "Camoufox";
  }
}

function browserVerificationMessage(verification: BrowserVerification): string {
  const messages: Record<BrowserVerification["state"], string> = {
    verified: "浏览器文件检查通过。",
    baseline_missing: "还没有可供对照的浏览器记录，请重新选择浏览器。",
    version_drift: "浏览器已经更新，请确认后继续使用。",
    missing: "找不到已选择的浏览器，请重新选择。",
    path_changed: "浏览器位置发生变化，请重新选择。",
    kind_mismatch: "所选文件不是当前 Silo 使用的浏览器。",
    publisher_mismatch: "浏览器来源与上次记录不一致，已阻止启动。",
    probe_failed: "暂时无法检查浏览器文件，请稍后重试。",
  };
  return messages[verification.state];
}

function activationNoticeTone(
  activation: DesktopStatus["activation"],
): "error" | "success" | "info" {
  if (activation.state === "running") {
    return "success";
  }
  return ["failed", "verification_failed"].includes(activation.state)
    ? "error"
    : "info";
}

function activationStatusTone(
  activation: DesktopStatus["activation"],
): "good" | "warn" | "neutral" {
  if (activation.state === "running") {
    return "good";
  }
  return ["failed", "verification_failed", "recovery_required"].includes(
    activation.state,
  )
    ? "warn"
    : "neutral";
}

function siloBrowserLabel(silo: Silo): string {
  switch (silo.engine.adapter) {
    case "stock":
      return silo.browser?.kind === "chrome"
        ? "Google Chrome"
        : "Microsoft Edge";
    case "controlled-chromium":
      return "独立 Chromium";
    case "camoufox":
      return "Camoufox（Firefox 兼容）";
  }
}

function siloExecutionTargetLabel(silo: Silo): string {
  switch (silo.executionTarget.kind) {
    case "local":
      return "这台 Windows 电脑";
    case "wsl":
      return `WSL · ${silo.executionTarget.distribution}`;
    case "remote":
      try {
        return `远程 · ${new URL(silo.executionTarget.endpointOrigin).host}`;
      } catch {
        return "远程运行";
      }
  }
}

function siloWebsiteIdentityBoundary(silo: Silo): string {
  if (silo.engine.adapter === "controlled-chromium") {
    const template = silo.engine.identityTemplate;
    const browserFamily =
      template.browser.family === "chromium" ? "Chromium" : "Firefox";
    const renderBoundary =
      template.render.canvas === "native" ? "原生渲染" : "模板渲染";
    return [
      `Windows ${template.os.version}`,
      `${browserFamily} ${template.browser.majorVersion}`,
      template.languages.primary,
      template.timezone,
      `${template.screen.width}×${template.screen.height}`,
      renderBoundary,
    ].join(" · ");
  }
  if (silo.engine.adapter === "camoufox") {
    return "由已保存的 Identity Artifact 固定；切换网络时重新协调身份网络字段";
  }

  switch (silo.executionTarget.kind) {
    case "local":
      return "Windows 浏览器；CPU、内存、Canvas、WebGL 与字体跟随本机";
    case "wsl":
      return "Linux Chromium；CPU、内存与图形特征跟随 WSL 和本机";
    case "remote":
      return "远程浏览器；网站可见身份尚未取得完整核对结果";
  }
}

function engineHealthLabel(
  state: EngineAdapterStatus["health"]["state"],
): string {
  switch (state) {
    case "healthy":
      return "可用";
    case "degraded":
      return "需要检查";
    case "unavailable":
      return "不可用";
    case "emergency_disabled":
      return "已停用";
  }
}

function engineHealthDescription(
  state: EngineAdapterStatus["health"]["state"],
): string {
  switch (state) {
    case "healthy":
      return "已完成安全检查，可以用于 Silo。";
    case "degraded":
      return "部分检查尚未完成，使用前请确认本机设置。";
    case "unavailable":
      return "当前无法在这台电脑上使用。";
    case "emergency_disabled":
      return "已由你手动停用。";
  }
}

function environmentBackendLabel(
  backend: EnvironmentBackendStatus["backend"],
): string {
  switch (backend) {
    case "wsl-chromium":
      return "WSL";
    case "windows-sandbox":
      return "Windows Sandbox";
    case "hyper-v":
      return "Hyper-V";
  }
}

function environmentPrerequisiteStateLabel(
  state: EnvironmentBackendStatus["prerequisites"][number]["state"],
): string {
  switch (state) {
    case "configured":
      return "已设置";
    case "guest_observed":
      return "已检查";
    case "verified":
      return "已就绪";
    case "missing":
      return "需要设置";
    case "unavailable":
      return "需要设置";
    case "unknown":
      return "待检查";
  }
}

function environmentPrerequisiteLabel(id: string): string {
  return (
    (
      {
        "selected-distribution": "已选择 Linux 发行版",
        "windows-host": "Windows 系统",
        wsl: "WSL 功能",
        "discovered-distribution": "Linux 发行版",
        "guest-agent": "环境服务",
        "guest-network-evidence": "网络连接",
        "linux-gui": "图形界面",
        "windows-sandbox-feature": "Windows Sandbox",
        "default-deny-descriptor": "隔离策略",
        "guest-return-channel": "环境状态反馈",
        "windows-sku": "Windows 版本",
        administrator: "管理员权限",
        virtualization: "虚拟化功能",
        reboot: "Windows 重启状态",
        "signed-host-probe": "系统检查组件",
        "signed-provider-scripts": "系统文件完整性",
        "base-image": "系统映像",
        "guest-agent-receipt": "环境服务",
        "concurrent-multi-silo": "同时运行多个 Silo",
        "bundled-mihomo-tun": "专用网络路由",
      } satisfies Record<string, string>
    )[id] ?? "运行条件"
  );
}

function environmentOperationLabel(operation: EnvironmentOperation): string {
  switch (operation) {
    case "create":
      return "创建";
    case "start":
      return "启动";
    case "stop":
      return "停止";
    case "pause":
      return "暂停";
    case "snapshot":
      return "创建快照";
    case "destroy":
      return "删除环境";
    case "configureNetwork":
      return "设置网络";
    case "health":
      return "检查状态";
    case "logs":
      return "查看日志";
  }
}

function isUserEnvironmentOperation(operation: EnvironmentOperation): boolean {
  return operation !== "logs";
}

function environmentNetworkForSilo(
  silo: Silo,
): EnvironmentNetworkProfile | null {
  const profile = silo.networkProfile;
  if (profile.mode === "direct") {
    return { mode: "direct" };
  }
  if (
    profile.mode === "pac" ||
    profile.scheme === "socks4" ||
    profile.credentialRef !== undefined ||
    profile.externalMihomo !== undefined
  ) {
    return null;
  }
  return {
    mode: "fixed_proxy",
    proxyRequired: profile.proxyRequired,
    scheme: profile.scheme,
    host: profile.host,
    port: profile.port,
  };
}

function remoteStateLabel(state: RemoteEnvironmentStatus["state"]): string {
  switch (state) {
    case "vault_uninitialized":
      return "保险库未初始化";
    case "vault_locked":
      return "保险库已锁定";
    case "not_configured":
      return "尚未配置";
    case "not_paired":
      return "配对未完成";
    case "paired":
      return "已连接";
    case "credential_expired":
      return "连接已过期";
    case "revoked":
      return "连接已取消";
  }
}

function remoteResultStateLabel(
  state: RemoteEnvironmentStatus["lastResults"][number]["state"],
): string {
  return (
    {
      created: "已创建",
      started: "已启动",
      stopped: "已停止",
      paused: "已暂停",
      snapshot_created: "快照已创建",
      destroyed: "已删除",
      network_configured: "网络设置已更新",
      healthy: "健康检查完成",
      logs_returned: "记录已获取",
      blocked: "已阻止",
    } satisfies Record<
      RemoteEnvironmentStatus["lastResults"][number]["state"],
      string
    >
  )[state];
}

function networkLocation(result: NetworkCheckResult): string {
  if (result.ip === null) {
    return result.errors[0] ?? "第三方服务没有返回有效 IP 数据";
  }
  return (
    [
      result.ip.countryCode ?? result.ip.country,
      result.ip.region,
      result.ip.city,
    ]
      .filter((part): part is string => part !== null)
      .join(" · ") || "位置未知"
  );
}

function networkOwner(result: NetworkCheckResult): string {
  if (result.ip === null) {
    return "未知";
  }
  return (
    [result.ip.asn, result.ip.organization ?? result.ip.isp]
      .filter((part): part is string => part !== null)
      .join(" · ") || "未知"
  );
}

function dnsStateLabel(result: NetworkCheckResult): string {
  const labels: Record<NetworkCheckResult["dns"]["state"], string> = {
    consistent: "两家公共 DNS 结果一致",
    different: "两家公共 DNS 结果有差异",
    resolver_error: "公共 DNS 返回错误",
    partial: "仅一家公共 DNS 可用",
    failed: "公共 DNS 检查失败",
  };
  return labels[result.dns.state];
}

function formatDate(isoDate: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "numeric",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(isoDate));
  } catch {
    return isoDate;
  }
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    return "大小未知";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toLocaleString("zh-CN", {
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
  })} ${units[unitIndex]}`;
}

function formatMicrosCurrency(micros: number, currency: string): string {
  const amount = micros / 1_000_000;
  try {
    return new Intl.NumberFormat("zh-CN", {
      style: "currency",
      currency,
      maximumFractionDigits: 6,
    }).format(amount);
  } catch {
    return `${amount.toFixed(6)} ${currency}`;
  }
}

function formatStorageSuffix(bytes: number | null | undefined): string {
  return bytes === undefined || bytes === null
    ? " · 大小暂不可用"
    : ` · ${formatBytes(bytes)}`;
}
