import type { ComponentProps } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CreateSiloPanel } from "../features/silos/CreateSiloPanel.js";
import { useSiloDraft } from "../features/silos/useSiloDraft.js";

import {
  type BrowserCandidate,
  type CreateManagedSiloInput,
  type CreateSiloInput,
  type DesktopStatus,
  type EngineAdapterStatus,
  type LegacyEnvironmentArtifact,
  type ManagedIdentityPreview,
  type SiloNetworkEvidence,
  type UpdateManagedIdentityInput,
  type UpdateSiloEngineInput,
  type UpdateSiloInput,
  type UpdateSiloNetworkInput,
  desktopApi,
} from "../desktop-api.js";

import {
  type NetworkCheckResult,
  type Silo,
  type SiloExecutionTarget,
  networkProfileSchema,
} from "@verisilo/contracts";

import {
  type Notice,
  errorMessage,
  managedErrorMessage,
} from "../shared/notice.js";

import {
  type WslCreationOption,
  emptyNetwork,
  requiredWslCreationOperations,
} from "../shared/defaults.js";

import {
  type VaultRefreshResult,
  VaultUiSession,
  acceptedRestoredVaultState,
  scrubDesktopStatusForLockedUi,
} from "../vault-ui-session.js";

import { isTauri } from "@tauri-apps/api/core";

import { getCurrentWindow } from "@tauri-apps/api/window";

import {
  vaultAutoLockDeadlinePassed,
  vaultAutoLockRefreshDelay,
} from "../vault-auto-lock.js";

import { UserFacingError } from "../user-errors.js";

import {
  activationNoticeTone,
  browserVerificationMessage,
  legacyEnvironmentLabel,
} from "../shared/presentation.js";

import { describeActivation } from "../formatters.js";

import {
  buildLocalSiloReport,
  renderLocalSiloReportHtml,
  serializeLocalSiloReport,
} from "../reports.js";

import { runDesktopNetworkCheck } from "../network-check-client.js";

import { parseProxyInput } from "../proxy-input.js";

import { readMihomoGroups } from "../features/network/controller.js";

import { clashControllerLabel } from "../proxy-presets.js";

export function useDesktopWorkspace() {
  const [view, setView] = useState<View>("overview");
  const [status, setStatus] = useState<DesktopStatus | null>(null);
  const [uiVaultLocked, setUiVaultLocked] = useState(true);
  const [vaultUiGeneration, setVaultUiGeneration] = useState(0);
  const [vaultTransition, setVaultTransition] = useState<"idle" | "restoring">(
    "idle",
  );
  const [silos, setSilos] = useState<Silo[]>([]);
  const [identityPreviews, setIdentityPreviews] = useState<
    Record<string, ManagedIdentityPreview>
  >({});
  const [inspectIdentity, setInspectIdentity] = useState(() => {
    try {
      return window.localStorage.getItem("verisilo.inspectIdentity") === "1";
    } catch {
      return false;
    }
  });
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
  const {
    name,
    setName,
    color,
    setColor,
    browserPath,
    setBrowserPath,
    browserKind,
    setBrowserKind,
    executionTarget,
    setExecutionTarget,
    createWslStatus,
    setCreateWslStatus,
    createWslOptions,
    setCreateWslOptions,
    createWslBusy,
    setCreateWslBusy,
    networkProfile,
    setNetworkProfile,
    proxyImport,
    setProxyImport,
    proxyUsername,
    setProxyUsername,
    proxyPassword,
    setProxyPassword,
    mihomoControllerUrl,
    setMihomoControllerUrl,
    mihomoControllerSecret,
    setMihomoControllerSecret,
    mihomoSnapshot,
    setMihomoSnapshot,
    mihomoBusy,
    setMihomoBusy,
    mihomoRequestRef,
    createWslRequestRef,
    browserSelectionExplicitRef,
    resetSiloDraft,
  } = useSiloDraft();
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
    setIdentityPreviews({});
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
    resetSiloDraft();
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
  }, [resetSiloDraft]);

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
        let active: Silo[];
        let archived: Silo[];
        let evidence: SiloNetworkEvidence[];
        let legacyArtifacts: LegacyEnvironmentArtifact[];
        try {
          [active, archived, evidence, legacyArtifacts] = await Promise.all([
            desktopApi.listActiveSilos(),
            desktopApi.listArchivedSilos(),
            desktopApi.listNetworkEvidence(),
            desktopApi.listLegacyEnvironmentArtifacts(),
          ]);
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
        setSilos(active);
        setArchivedSilos(archived);
        setNetworkEvidenceHistory(evidence);
        setLegacyEnvironmentArtifacts(legacyArtifacts);

        if (includeStorageUsage) {
          const silosForUsage = [...active, ...archived];
          void Promise.all([
            desktopApi.discoverBrowsers().then(
              (value) => ({ ok: true as const, value }),
              (error: unknown) => ({ ok: false as const, error }),
            ),
            Promise.all(
              silosForUsage.map(async (silo) => {
                try {
                  const usage = await desktopApi.siloStorageUsage(silo.id);
                  return [silo.id, usage.bytes] as const;
                } catch {
                  return [silo.id, null] as const;
                }
              }),
            ),
            desktopApi.listManagedIdentityPreviews().then(
              (value) => ({ ok: true as const, value }),
              () => ({ ok: false as const }),
            ),
          ]).then(([discovered, usageEntries, previews]) => {
            if (
              requestId !== refreshRequestRef.current ||
              !vaultUiSessionRef.current.accepts(sessionEpoch)
            ) {
              return;
            }
            if (discovered.ok) {
              setBrowsers(discovered.value);
            }
            setStorageUsage(Object.fromEntries(usageEntries));
            if (previews.ok) {
              setIdentityPreviews(previews.value);
            }
          });
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
          "需要登录信息时，请使用 HTTP、SOCKS5，或交给本机 Clash 处理。",
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
          "独立浏览器现在还不能用。请到“运行位置”看一下后再试。",
        );
      }
      const silo = await desktopApi.createManagedSilo(input);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」托管身份浏览器。首次启动前可以查看和微调指纹；启动后身份锁定。`,
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

  const updateManagedIdentity = (
    silo: Silo,
    input: UpdateManagedIdentityInput,
  ) =>
    withBusy(async (isCurrent) => {
      await desktopApi.updateManagedIdentity(silo.id, input);
      if (!isCurrent()) {
        return;
      }
      setNotice({
        tone: "success",
        message: input.rotateSeed
          ? `已为「${silo.name}」换了一套指纹。首次启动后会锁定。`
          : `已更新「${silo.name}」的身份设置。首次启动后会锁定。`,
      });
      await refresh();
    });

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
            ? `已打开「${silo.name}」。用完后直接关掉那个浏览器窗口即可。`
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
      const inspected = await readMihomoGroups(
        mihomoControllerUrl,
        mihomoControllerSecret,
      );
      const snapshot = inspected.snapshot;
      if (isCurrent()) {
        setMihomoControllerUrl(inspected.controllerUrl);
      }
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
        throw new UserFacingError("请先选择本机 Clash。");
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
          controllerUrl: inspected.controllerUrl,
          selectorGroup: group.name,
          nodeName: selectedNode.name,
        },
      });
      setNotice({
        tone: "success",
        message: `已读取 ${clashControllerLabel(inspected.controllerUrl)}，并把这个 Silo 预绑定到「${selectedNode.name}」。创建后每次启动都会重新选择并复查。`,
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

  const creation: ComponentProps<typeof CreateSiloPanel> = {
    browserKind: browserKind,
    browserPath: browserPath,
    busy: busy,
    candidateOptions: candidateOptions,
    chooseBrowser: chooseBrowser,
    color: color,
    executionTarget: executionTarget,
    createSilo: createSilo,
    createManagedSilo: createManagedSilo,
    managedEngineReady: engineStatuses.some(
      (engine) =>
        engine.descriptor.id === "camoufox" &&
        engine.health.state === "healthy",
    ),
    managedStatusBusy: managedStatusBusy,
    managedStatusError: managedStatusError,
    refreshManagedStatus: refreshManagedBrowserStatus,
    name: name,
    importProxy: importProxy,
    inspectMihomoController: inspectMihomoController,
    mihomoBusy: mihomoBusy,
    mihomoControllerSecret: mihomoControllerSecret,
    mihomoControllerUrl: mihomoControllerUrl,
    mihomoSnapshot: mihomoSnapshot,
    networkProfile: networkProfile,
    proxyImport: proxyImport,
    proxyPassword: proxyPassword,
    proxyUsername: proxyUsername,
    refreshWsl: detectCreateWsl,
    resetMihomoSnapshot: () => setMihomoSnapshot(null),
    selectMihomoGroup: selectMihomoGroup,
    selectMihomoNode: selectMihomoNode,
    setBrowserKind: (kind) => {
      browserSelectionExplicitRef.current = true;
      setBrowserKind(kind);
      setBrowserPath(
        browsers.find((browser) => browser.kind === kind)?.executablePath ?? "",
      );
    },
    setBrowserPath: (path) => {
      browserSelectionExplicitRef.current = true;
      setBrowserPath(path);
    },
    setColor: setColor,
    setExecutionTarget: (target) => {
      setExecutionTarget(target);
      if (target.kind === "wsl") {
        setNetworkProfile(emptyNetwork());
        setProxyImport("");
        setProxyUsername("");
        setProxyPassword("");
        setMihomoControllerSecret("");
        setMihomoSnapshot(null);
      }
    },
    setMihomoControllerSecret: setMihomoControllerSecret,
    setMihomoControllerUrl: (value) => {
      setMihomoControllerUrl(value);
      setMihomoSnapshot(null);
      if (
        networkProfile.mode === "fixed_proxy" &&
        networkProfile.externalMihomo !== undefined
      ) {
        const { externalMihomo: _binding, ...withoutBinding } = networkProfile;
        setNetworkProfile(withoutBinding);
      }
    },
    setName: setName,
    setNetworkProfile: setNetworkProfile,
    setProxyImport: setProxyImport,
    setProxyPassword: setProxyPassword,
    setProxyUsername: setProxyUsername,
    wslBusy: createWslBusy,
    wslOptions: createWslOptions,
    wslStatus: createWslStatus,
  };

  return {
    creation,
    status,
    uiVaultLocked,
    vaultTransition,
    vaultBusy,
    inspectIdentity,
    setInspectIdentity,
    view,
    setView,
    notice,
    retryRestoredVaultState,
    passphrase,
    setPassphrase,
    submitVault,
    activeSilos,
    busy,
    lockVault,
    identityPreviews,
    archiveSilo,
    setEditingSilo,
    launchSilo,
    rebindSiloMihomo,
    recheckSiloBrowser,
    recheckSiloRuntime,
    stopSilo,
    networkEvidenceHistory,
    engineStatuses,
    storageUsage,
    legacyEnvironmentArtifacts,
    cleanupLegacyEnvironment,
    archivedSilos,
    deleteSilo,
    restoreArchivedSilo,
    browsers,
    networkResult,
    networkBusy,
    checkNetwork,
    setNetworkResult,
    clearNetworkEvidence,
    downloadLocalReport,
    editingSilo,
    updateSilo,
    updateManagedIdentity,
    setNotice,
    refresh,
    finishVaultRestore,
    withBusy,
    vaultUiGeneration,
  };
}

type View =
  "overview" | "create" | "edit" | "settings" | "cli" | "environments";
