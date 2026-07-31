import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type {
  BrowserKind,
  EngineAdapterId,
  EnvironmentBackendStatus,
  EnvironmentNetworkProfile,
  EnvironmentOperation,
  EnvironmentOperationRequest,
  NetworkCheckResult,
  NetworkProfile,
  RemoteAgentControlOperation,
  RemoteAgentResponse,
  RemoteEndpoint,
  RemoteNetworkPolicy,
  RuntimeNetworkEvidence,
  Silo,
  SiloEngineConfig,
  LabsExperiment,
} from "@verisilo/contracts";
import {
  createDefaultLabsExperiments,
  LABS_EXPERIMENT_DEFINITIONS,
  networkProfileSchema,
  siloEngineConfigSchema,
} from "@verisilo/contracts";

import {
  ENVIRONMENT_LAYERS,
  PRODUCT_CAPABILITIES,
  type CapabilityTone,
} from "./capabilities.js";
import {
  desktopApi,
  type BrowserCandidate,
  type CreateSiloInput,
  type DesktopStatus,
  type EngineAdapterStatus,
  type MihomoSnapshot,
  type RemoteEnvironmentStatus,
  type RemoteInteractivePrincipal,
  type SiloNetworkEvidence,
  type UpdateSiloInput,
  type UpdateSiloEngineInput,
  type UpdateSiloNetworkInput,
  type WslStatus,
} from "./desktop-api.js";
import {
  describeActivation,
  describeEngineCapabilityOperation,
  describeEnginePhaseReceipt,
  describeNetwork,
  describeRuntimeEngineReceipts,
  describeSiteFallbackReceipt,
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
import {
  canConfigureWslDistribution,
  requiresExplicitWslSelection,
} from "./wsl-selection.js";

const defaultColor = "#5b5ce2";
const defaultMihomoControllerUrl = "http://127.0.0.1:9090/";

type Notice = { tone: "error" | "success" | "info"; message: string } | null;
type View =
  "overview" | "create" | "edit" | "settings" | "labs" | "capabilities";

function emptyNetwork(): NetworkProfile {
  return { mode: "direct", proxyRequired: false };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
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
  const [storageUsage, setStorageUsage] = useState<
    Record<string, number | null>
  >({});
  const [editingSilo, setEditingSilo] = useState<Silo | null>(null);
  const [networkEvidenceHistory, setNetworkEvidenceHistory] = useState<
    SiloNetworkEvidence[]
  >([]);
  const [browsers, setBrowsers] = useState<BrowserCandidate[]>([]);
  const [passphrase, setPassphrase] = useState("");
  const [notice, setNotice] = useState<Notice>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState(defaultColor);
  const [browserPath, setBrowserPath] = useState("");
  const [browserKind, setBrowserKind] = useState<BrowserKind>("chrome");
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
  const vaultUiSessionRef = useRef(new VaultUiSession());

  const scrubSensitiveUi = useCallback(() => {
    setSilos([]);
    setArchivedSilos([]);
    setStorageUsage({});
    setEditingSilo(null);
    setNetworkEvidenceHistory([]);
    setBrowsers([]);
    setPassphrase("");
    setName("");
    setColor(defaultColor);
    setBrowserPath("");
    setBrowserKind("chrome");
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
      const browsersPromise = desktopApi.discoverBrowsers().then(
        (value) => ({ ok: true as const, value }),
        (error: unknown) => ({ ok: false as const, error }),
      );
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
          | { ok: false; error: unknown };
        let active: Silo[];
        let archived: Silo[];
        let evidence: SiloNetworkEvidence[];
        try {
          [browserResult, [active, archived, evidence]] = await Promise.all([
            browsersPromise,
            Promise.all([
              desktopApi.listActiveSilos(),
              desktopApi.listArchivedSilos(),
              desktopApi.listNetworkEvidence(),
            ]),
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
        setBrowsers(browserResult.ok ? browserResult.value : []);
        setSilos(active);
        setArchivedSilos(archived);
        setNetworkEvidenceHistory(evidence);

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
        if (!browserResult.ok) {
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
      browserPath === "" &&
      candidateOptions[0] !== undefined
    ) {
      setBrowserPath(candidateOptions[0].executablePath);
    }
  }, [browserPath, candidateOptions, status?.vault.state, uiVaultLocked]);

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
        throw new Error("请使用至少 12 个字符的保险库口令。");
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
      throw new Error(
        "恢复后的权威状态尚未完整载入；操作仍被隔离，请使用下方按钮重试。",
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
            ? "加密保险库已恢复并完成格式校验。请核对 Silo 列表；本机已有 Profile 不会被备份文件自动覆盖。"
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
    setBrowserKind(candidate.kind);
    setBrowserPath(candidate.executablePath);
  };

  const createSilo = () =>
    withBusy(async (isCurrent) => {
      if (!networkProfileSchema.safeParse(networkProfile).success) {
        throw new Error(
          "网络配置尚未填写完整。请检查协议、主机、端口、PAC URL 或 Mihomo 绑定。",
        );
      }
      const hasUsername = proxyUsername.trim() !== "";
      const hasPassword = proxyPassword !== "";
      if (hasUsername !== hasPassword) {
        throw new Error("代理用户名和密码需要同时填写；无认证代理请都留空。");
      }
      if (
        hasUsername &&
        networkProfile.mode === "fixed_proxy" &&
        !["http", "socks5"].includes(networkProfile.scheme)
      ) {
        throw new Error(
          "自动代理认证目前支持 HTTP 和 SOCKS5；HTTPS/SOCKS4 请交给外部 Mihomo 端口。",
        );
      }
      const input: CreateSiloInput = {
        name,
        color,
        browserKind,
        executablePath: browserPath,
        networkProfile,
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
      setNetworkProfile(emptyNetwork());
      setProxyImport("");
      setProxyUsername("");
      setProxyPassword("");
      setMihomoControllerSecret("");
      setMihomoSnapshot(null);
      setNotice({
        tone: "success",
        message: `已创建「${silo.name}」。它不会读取或改写你的默认浏览器 Profile。`,
      });
      setView("overview");
      await refresh();
    });

  const launchSilo = (silo: Silo) =>
    withBusy(async (isCurrent) => {
      const activation = await desktopApi.launchSilo(silo.id);
      if (!isCurrent()) {
        return;
      }
      setNotice({ tone: "success", message: describeActivation(activation) });
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
        message: verification.message,
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
        message: `已保存「${updated.name}」的资料${networkInput === null ? "" : "、网络"}${engineInput === null ? "" : "、引擎选择"}。Profile 根目录、稳定种子和网站数据没有改变。`,
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
      `永久删除「${silo.name}」及其浏览器数据？Cookie、登录状态和站点数据都无法恢复。此操作不会触碰默认 Chrome/Edge Profile。`,
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
        message: `已永久删除「${silo.name}」的受管目录和保险库记录。`,
      });
      await refresh();
    });
  };

  const clearNetworkEvidence = async (silo: Silo) => {
    if (
      !window.confirm(
        `清除「${silo.name}」在桌面保险库中的网络检查历史？这不会改动浏览器 Profile 或扩展里的当前结果。`,
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
        message: `已清除 ${removed} 条「${silo.name}」网络证据记录。`,
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
        throw new Error("控制器可连接，但没有返回可绑定的选择组和节点。");
      }
      const selectedNode =
        group.nodes.find((node) => node.name === group.selected) ??
        group.nodes[0];
      if (selectedNode === undefined) {
        throw new Error("控制器选择组没有返回可用节点。");
      }
      if (networkProfile.mode !== "fixed_proxy") {
        throw new Error("请先选择 Mihomo / Clash 网络方式。");
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
          <span className="local-pill">仅本机</span>
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
            active={view === "labs"}
            label="实验室"
            onClick={() => setView("labs")}
          />
          <TabButton
            active={view === "capabilities"}
            label="能力路线"
            onClick={() => setView("capabilities")}
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
            <h1>
              {vaultBusy
                ? "正在载入恢复后的权威状态…"
                : "恢复后的状态仍处于隔离"}
            </h1>
            <p>
              VeriSilo
              已清除旧会话中的表单、远程状态和批准位；完成新保险库、运行时与环境归属核对前不会开放操作。
            </p>
            <button
              disabled={vaultBusy}
              onClick={() => void retryRestoredVaultState()}
              type="button"
            >
              {vaultBusy ? "正在重新载入…" : "重新载入权威状态"}
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
                    : `${activeSilos.length} 个 Silo 配置已保存`}
                </h1>
                <p>
                  VeriSilo 以独立 user-data-dir 启动每个 Silo，避免复用默认
                  Chrome 或 Edge
                  Profile；Cookie、站点数据、历史和权限的实际隔离仍需在当前
                  Windows 与浏览器组合上完成本机验收。
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
                tone={
                  [
                    "failed",
                    "verification_failed",
                    "recovery_required",
                  ].includes(status.activation.state)
                    ? "warn"
                    : "good"
                }
                value={
                  status.activation.state === "verification_failed"
                    ? "网络已阻断"
                    : status.activation.state === "recovery_required"
                      ? "需要恢复核对"
                      : status.activation.activeSiloId === null
                        ? "空闲"
                        : "运行中"
                }
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
                eyebrow="桌面控制器出口"
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

            {status.activation.engineEvidence !== null ? (
              <section className="panel evidence-panel">
                <div className="panel-heading">
                  <div>
                    <p className="eyebrow">本次引擎证据</p>
                    <h2>配置、启动、ACK 与逐能力运行收据分开记录</h2>
                    <p>
                      {`配置：${engineAdapterLabel(status.activation.engineEvidence.configuredAdapter)}；启动：${status.activation.engineEvidence.launchedAdapter === null ? "无" : engineAdapterLabel(status.activation.engineEvidence.launchedAdapter)}；逐能力核验：${status.activation.engineEvidence.verifiedAdapter === null ? "无" : engineAdapterLabel(status.activation.engineEvidence.verifiedAdapter)}。`}
                      Bootstrap ACK 只证明受信包进程接收了本次
                      bootstrap；只有按序、绑定且覆盖完整能力的 Observe → Apply
                      → Verify 收据才会上调能力状态。收据只证明受信引擎对具体
                      evidence 的声明，不独立证明 Canvas、TLS 或 QUIC
                      的真实行为。
                    </p>
                    <p>
                      {describeRuntimeEngineReceipts(
                        status.activation.engineEvidence,
                      )}
                    </p>
                  </div>
                </div>
                {status.activation.engineEvidence.capabilities.length > 0 ? (
                  <div className="desktop-capability-table">
                    {status.activation.engineEvidence.capabilities.map(
                      (capability) => (
                        <article className="capability-row" key={capability.id}>
                          <div className="capability-name">
                            <strong>{capability.id}</strong>
                            <span>
                              {describeEngineCapabilityOperation(
                                capability.operation,
                              )}
                            </span>
                          </div>
                          <p>{capability.reason}</p>
                          <div className="evidence-rule">
                            <span>最近证据</span>
                            <p>
                              {capability.evidence.length === 0
                                ? "无"
                                : capability.evidence.join("；")}
                            </p>
                          </div>
                        </article>
                      ),
                    )}
                  </div>
                ) : null}
                {status.activation.engineEvidence.phaseReceipts.length > 0 ? (
                  <div className="evidence-rule">
                    <span>已接受的阶段收据</span>
                    <ol>
                      {status.activation.engineEvidence.phaseReceipts.map(
                        (receipt) => (
                          <li key={`${receipt.phase}-${receipt.recordedAt}`}>
                            {describeEnginePhaseReceipt(receipt)}
                          </li>
                        ),
                      )}
                    </ol>
                  </div>
                ) : null}
                {status.activation.engineEvidence.fallbackReceipts.length >
                0 ? (
                  <div className="evidence-rule">
                    <span>已接受的站点回退收据</span>
                    <p>
                      {status.activation.engineEvidence.fallbackReceipts
                        .map(describeSiteFallbackReceipt)
                        .join("；")}
                    </p>
                  </div>
                ) : null}
              </section>
            ) : null}

            {status.activation.networkEvidence !== null ? (
              <RuntimeNetworkEvidenceCard
                evidence={status.activation.networkEvidence}
              />
            ) : null}

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
              runtimeState={status.activation.state}
              silos={activeSilos}
              storageUsage={storageUsage}
            />

            <ArchivedSiloList
              busy={busy}
              onDelete={deleteSilo}
              onRestore={restoreArchivedSilo}
              silos={archivedSilos}
              storageUsage={storageUsage}
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
            createSilo={createSilo}
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
            resetMihomoSnapshot={() => setMihomoSnapshot(null)}
            selectMihomoGroup={selectMihomoGroup}
            selectMihomoNode={selectMihomoNode}
            setBrowserKind={(kind) => {
              setBrowserKind(kind);
              setBrowserPath("");
            }}
            setBrowserPath={setBrowserPath}
            setColor={setColor}
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

      {view === "capabilities" ? (
        <CapabilityRoadmap
          key={`${vaultUiGeneration}:${vaultLocked ? "locked" : "unlocked"}`}
          silos={activeSilos}
          vaultLocked={vaultLocked}
        />
      ) : null}

      {view === "labs" ? (
        <LabsPanel silos={activeSilos} vaultLocked={vaultLocked} />
      ) : null}

      <footer>
        VeriSilo
        的目标是把不同强度的环境隔离做成可选层级，而不是承诺“不可检测”。
        当前可用能力、未来版本和验证证据会始终分开显示。
      </footer>
    </main>
  );
}

function Brand() {
  return (
    <div className="brand">
      <div className="brand-mark" aria-hidden="true">
        VS
      </div>
      <div>
        <strong>VeriSilo</strong>
        <span>看懂并隔离你的浏览器身份</span>
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
          保险库加密 Silo 元数据、稳定种子和可选网络配置。浏览器自身的 Profile
          文件仍由 Chrome/Edge 管理，不会被复制进保险库。
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
      <div className="locked-icon" aria-hidden="true">
        ◇
      </div>
      <h1>先解锁保险库</h1>
      <p>创建 Silo 需要读取加密的本地配置。能力路线不需要解锁即可查看。</p>
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
            <span>IP 信誉/黑名单未评分</span>
          </div>
          <p className="scope-copy">
            两家公共 DoH
            结果一致，只说明这两次固定域名查询一致；不能证明系统、路由器或运营商
            DNS 一定没有污染或劫持。机房线路线索也不等于 IP 一定有风险。
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

function RuntimeNetworkEvidenceCard({
  evidence,
}: {
  evidence: RuntimeNetworkEvidence;
}) {
  const stages = [
    ["网络配置", evidence.configuration],
    ["Mihomo 节点绑定", evidence.controllerBinding],
    ["代理端点", evidence.endpoint],
    ["代理认证", evidence.authentication],
    ["浏览器路由", evidence.browserRouting],
    ["Silo 当次请求出口声明", evidence.exit],
    ["DNS 路径证据", evidence.dns],
    ["WebRTC 路径", evidence.webRtc],
  ] as const;
  return (
    <section className="panel evidence-panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">本次启动证据</p>
          <h2>配置成功，不等于出口已经验证</h2>
          <p>
            桌面端只确认它实际完成的步骤；公网出口、DNS 与 WebRTC 必须在已启动的
            Silo 内用 Companion 主动检查。
          </p>
        </div>
        <span className="provider-badge">
          {networkProviderLabel(evidence.provider)}
        </span>
      </div>
      {evidence.endpointLabel !== undefined ? (
        <p className="endpoint-chain">路径：{evidence.endpointLabel}</p>
      ) : null}
      <p className="form-hint">
        证据时间：{formatDate(evidence.observedAt)}；来源：
        {evidence.provenance === "extension_asserted"
          ? "Companion 扩展观测声明（Native inbox 未做本机进程级认证）"
          : evidence.provenance === "relay_observed"
            ? "受管 relay 本机观测"
            : "桌面控制面配置/可达性检查"}
        。
      </p>
      {evidence.authentication !== "not_applicable" ? (
        <p className="form-hint">
          代理认证来源：
          {evidence.authenticationProvenance === "relay_observed"
            ? "同一 runtime 的受管 relay 观测；verified 还要求同一检查窗口内的 extension_asserted 公网出口声明。该联合结果不是独立可信的浏览器进程证明"
            : "桌面控制面的配置或协议预检（HTTP 无 relay 收据时不会因 Companion 成功而升级）"}
          。
        </p>
      ) : null}
      <div className="evidence-grid">
        {stages
          .filter(([, state]) => state !== "not_applicable")
          .map(([label, state]) => (
            <div className="evidence-stage" key={label}>
              <span>{label}</span>
              <strong className={evidenceTone(state)}>
                {evidenceStateLabel(state)}
              </strong>
            </div>
          ))}
      </div>
      {evidence.safeguards.length > 0 ? (
        <div className="safeguard-list">
          {evidence.safeguards.map((safeguard) => (
            <span key={safeguard}>{safeguardLabel(safeguard)}</span>
          ))}
        </div>
      ) : null}
    </section>
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
          <p className="eyebrow">Silo 内证据</p>
          <h2>还没有从 Companion 收到检查结果</h2>
          <p>
            启动 Silo 后，在其中安装 Companion
            并主动运行出口检查。桌面控制器自己的请求不会写进这里。
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
          <p className="eyebrow">Silo 内证据</p>
          <h2>Companion 声明的活动 Silo 观测</h2>
          <p>
            这些用户主动检查经 Native Host 接收后保存在加密 Vault。IP 是
            Companion 声明的当次请求观察，证据级别保持
            extension_asserted；Native inbox 未独立认证浏览器进程。公共 DoH
            只比较答案，不能证明实际 DNS 路径。
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
                  <dd>{dnsStateLabel(entry.result)}；实际解析路径未观测</dd>
                </div>
                <div>
                  <dt>WebRTC / QUIC</dt>
                  <dd>本次未观测，不能标记已验证</dd>
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
          <p className="eyebrow">本地脱敏报告</p>
          <h2>只导出你明确选中的一个 Silo</h2>
          <p>
            导出文件只在这台设备由浏览器 Blob 下载生成；不会上传、不会自动保存，
            也不会把桌面控制器自己的网络检查混入 Silo 证据。
          </p>
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
            ? "先选择 Silo，报告不会默认指向任何身份。"
            : `将包含 ${selectedEvidenceCount} 条来自该 Silo Companion 的已加密 Vault 证据；没有记录时仍会导出证据边界和当前配置状态。`}
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
                : onDownload(selectedSilo, "json")
            }
            type="button"
          >
            下载 JSON
          </button>
          <button
            className="button-secondary"
            disabled={!canDownload}
            onClick={() =>
              selectedSilo === undefined
                ? undefined
                : onDownload(selectedSilo, "html")
            }
            type="button"
          >
            下载 HTML
          </button>
        </div>
      </div>

      <div className="report-boundary">
        <strong>证据边界</strong>
        <p>
          报告保留浏览器类型/版本、网络能力状态和公共 DoH 覆盖声明；
          “已配置”“已应用”“已验证”会分开记录。公共 DoH 只比较固定查询的答案，
          不证明实际 DNS 路径；WebRTC 和 QUIC 未被观察时不会标成已验证。
        </p>
      </div>
      <details className="report-developer-details">
        <summary>开发者：查看默认排除项与格式约束</summary>
        <p>
          JSON 与 HTML 都排除本地 Profile/浏览器路径、请求 ID、代理主机与端口、
          完整 IP、城市与区域、原始错误，以及秘密、凭据、种子和所有引用标识。
          IPv4 仅保留 /24 前缀，IPv6 仅保留 /48 前缀。HTML
          是完全转义的静态文件， 不含脚本或远程资源。
        </p>
      </details>
    </section>
  );
}

function SiloList({
  activation,
  busy,
  onArchive,
  onCreate,
  onEdit,
  onLaunch,
  onRebindMihomo,
  onRecheckBrowser,
  onRecheckRuntime,
  runtimeState,
  silos,
  storageUsage,
}: {
  activation: string | null;
  busy: boolean;
  onArchive: (silo: Silo) => Promise<void>;
  onCreate: () => void;
  onEdit: (silo: Silo) => void;
  onLaunch: (silo: Silo) => Promise<void>;
  onRebindMihomo: (silo: Silo) => Promise<void>;
  onRecheckBrowser: (silo: Silo) => Promise<void>;
  onRecheckRuntime: (silo: Silo) => Promise<void>;
  runtimeState: DesktopStatus["activation"]["state"];
  silos: Silo[];
  storageUsage: Record<string, number | null>;
}) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="eyebrow">长期身份</p>
          <h2>你的 Silos</h2>
          <p>切换 Silo 就是在切换一整套浏览器数据，而不是只替换 Cookie。</p>
        </div>
        <button className="button-secondary" onClick={onCreate} type="button">
          新建
        </button>
      </div>
      {silos.length === 0 ? (
        <div className="empty-silos">
          <span aria-hidden="true">◎</span>
          <strong>还没有 Silo</strong>
          <p>创建一个工作、个人或临时用途的独立浏览器环境。</p>
          <button onClick={onCreate} type="button">
            创建第一个 Silo
          </button>
        </div>
      ) : (
        <div className="silo-grid">
          {silos.map((silo) => (
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
                  <p>
                    {silo.browser.kind === "chrome"
                      ? "Google Chrome"
                      : "Microsoft Edge"}
                  </p>
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
                    独立 Profile 路径机制（待本机站点隔离验收）
                    {formatStorageSuffix(storageUsage[silo.id])}
                  </dd>
                </div>
                <div>
                  <dt>网络</dt>
                  <dd>{describeNetwork(silo.networkProfile)}</dd>
                </div>
                {silo.networkProfile.mode === "fixed_proxy" &&
                silo.networkProfile.credentialRef !== undefined ? (
                  <div>
                    <dt>代理认证</dt>
                    <dd>凭据已加密，启动时走本机中继</dd>
                  </div>
                ) : null}
                <div>
                  <dt>Companion 当次请求出口</dt>
                  <dd>
                    启动后由该 Silo 内 Companion 以 extension_asserted 声明
                  </dd>
                </div>
                <div>
                  <dt>伴侣扩展</dt>
                  <dd>在此 Silo 内由你确认安装</dd>
                </div>
              </dl>
              <div className="card-actions">
                <button
                  disabled={busy || activation !== null}
                  onClick={() => void onLaunch(silo)}
                  type="button"
                >
                  启动 Silo
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
                        ? "复查阻断状态（不重开端口）"
                        : "复查运行与网络"}
                  </button>
                ) : (
                  <button
                    className="button-secondary"
                    disabled={busy || runtimeState === "verification_failed"}
                    onClick={() => void onRecheckBrowser(silo)}
                    type="button"
                  >
                    重新核验浏览器
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
                      : "明确重绑 Mihomo 节点"}
                  </button>
                ) : null}
              </div>
            </article>
          ))}
        </div>
      )}
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
          <p>恢复不会复制 Profile；永久删除才会移除这个受管浏览器目录。</p>
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
    silo.browser.kind,
  );
  const [executablePath, setExecutablePath] = useState(
    silo.browser.executablePath,
  );
  const [replaceNetwork, setReplaceNetwork] = useState(false);
  const [replaceEngine, setReplaceEngine] = useState(false);
  const [engineConfigJson, setEngineConfigJson] = useState(() =>
    JSON.stringify(silo.engine, null, 2),
  );
  const [replacementNetwork, setReplacementNetwork] =
    useState<NetworkProfile>(emptyNetwork);
  const [proxyImport, setProxyImport] = useState("");
  const [proxyImportError, setProxyImportError] = useState<string | null>(null);
  const [proxyUsername, setProxyUsername] = useState("");
  const [proxyPassword, setProxyPassword] = useState("");
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
  let replacementEngine: SiloEngineConfig | null = null;
  if (replaceEngine) {
    try {
      const parsed = siloEngineConfigSchema.safeParse(
        JSON.parse(engineConfigJson) as unknown,
      );
      replacementEngine = parsed.success ? parsed.data : null;
    } catch {
      replacementEngine = null;
    }
  }
  const engineConfigValid = !replaceEngine || replacementEngine !== null;

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
          <p>
            保存后继续使用同一个受管 Profile 和稳定种子。运行中的 Silo
            不能编辑。
          </p>
        </div>
      </div>
      <div className="form-grid">
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
        <label>
          浏览器
          <select
            disabled={busy}
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
            disabled={busy}
            onChange={(event) => setExecutablePath(event.target.value)}
            value={executablePath}
          />
        </label>
      </div>
      {candidates.length > 0 ? (
        <div className="candidate-row">
          {candidates.map((candidate) => (
            <button
              className="button-secondary"
              disabled={busy}
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
        <strong>当前网络身份</strong>
        <span>{describeNetwork(silo.networkProfile)}</span>
      </div>
      <div className="boundary-note">
        <strong>当前引擎</strong>
        <span>{siloEngineLabel(silo.engine)}</span>
      </div>
      <label className="check-field network-replace-toggle">
        <input
          checked={replaceEngine}
          disabled={busy}
          onChange={(event) => setReplaceEngine(event.target.checked)}
          type="checkbox"
        />
        替换每 Silo 引擎配置（外部引擎失败时不会回退 Stock）
      </label>
      {replaceEngine ? (
        <div className="network-replace-card">
          <div className="card-actions">
            <button
              className="button-secondary"
              disabled={busy}
              onClick={() =>
                setEngineConfigJson(
                  JSON.stringify({ adapter: "stock" }, null, 2),
                )
              }
              type="button"
            >
              恢复 Stock
            </button>
          </div>
          <label>
            严格引擎配置 JSON
            <textarea
              disabled={busy}
              onChange={(event) => setEngineConfigJson(event.target.value)}
              rows={14}
              spellCheck={false}
              value={engineConfigJson}
            />
          </label>
          <p className={engineConfigValid ? "form-hint" : "field-error"}>
            {engineConfigValid
              ? "只接受 stock、controlled-chromium 或 camoufox 的固定结构；受控模式必须携带完整 IdentityTemplate 与 host-only fallbackRules，不接受启动参数或 URL。"
              : "配置不是受支持的严格引擎结构，或 IdentityTemplate 内部信号矛盾。"}
          </p>
        </div>
      ) : null}
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
        完整替换网络配置，并清除旧代理凭据和旧 Mihomo Controller Secret
      </label>
      {replaceNetwork ? (
        <div className="network-replace-card">
          <p className="field-warning">
            替换会清除旧代理凭据和旧 Mihomo Controller
            Secret。需要认证时请在下方重新输入；普通改名请不要勾选此项。
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
              <option value="fixed_proxy">固定 HTTP / SOCKS5 代理</option>
              <option value="pac">PAC</option>
            </select>
          </label>
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
                  自动代理认证只支持 HTTP 和 SOCKS5。HTTPS/SOCKS4 请交给外部
                  Mihomo 端口。
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
                PAC URL
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
                必须代理（当前无法证明 PAC 无 DIRECT，会在启动前拒绝）
              </label>
            </div>
          ) : null}
        </div>
      ) : null}
      <div className="submit-row">
        <div>
          <strong>Profile 路径保持不变</strong>
          <span>{silo.profileDirectory}</span>
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
              executablePath.trim().length === 0 ||
              (replaceNetwork &&
                (!networkValid ||
                  !credentialsValid ||
                  !credentialsSupported)) ||
              !engineConfigValid
            }
            onClick={() =>
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
                replaceEngine && replacementEngine !== null
                  ? { engine: replacementEngine }
                  : null,
              )
            }
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
        throw new Error("新口令至少需要 12 个字符。");
      }
      if (newPassphrase !== confirmPassphrase) {
        throw new Error("两次输入的新口令不一致。");
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
          "保险库口令和数据加密密钥已轮换。浏览器 Profile 不属于保险库加密范围，没有被重新加密。",
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
        message: `已备份加密保险库：${receipt.destinationPath}（${formatBytes(receipt.bytes)}）。浏览器 Profile 未包含在内。`,
      });
    });

  const restoreVault = () =>
    runBusy(async (isCurrent) => {
      if (!confirmOverwrite) {
        throw new Error("请先确认覆盖当前保险库记录。");
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
              Cookie、历史或整个 Profile 目录。
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
              Profile。
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
          我确认覆盖当前保险库记录，并理解 Profile 文件不在此备份中
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
  importProxy,
  inspectMihomoController,
  mihomoBusy,
  mihomoControllerSecret,
  mihomoControllerUrl,
  mihomoSnapshot,
  name,
  networkProfile,
  proxyImport,
  proxyPassword,
  proxyUsername,
  resetMihomoSnapshot,
  selectMihomoGroup,
  selectMihomoNode,
  setBrowserKind,
  setBrowserPath,
  setColor,
  setMihomoControllerSecret,
  setMihomoControllerUrl,
  setName,
  setNetworkProfile,
  setProxyImport,
  setProxyPassword,
  setProxyUsername,
}: {
  browserKind: BrowserKind;
  browserPath: string;
  busy: boolean;
  candidateOptions: BrowserCandidate[];
  chooseBrowser: (candidate: BrowserCandidate) => void;
  color: string;
  createSilo: () => Promise<void>;
  importProxy: () => void;
  inspectMihomoController: () => Promise<void>;
  mihomoBusy: boolean;
  mihomoControllerSecret: string;
  mihomoControllerUrl: string;
  mihomoSnapshot: MihomoSnapshot | null;
  name: string;
  networkProfile: NetworkProfile;
  proxyImport: string;
  proxyPassword: string;
  proxyUsername: string;
  resetMihomoSnapshot: () => void;
  selectMihomoGroup: (groupName: string) => void;
  selectMihomoNode: (nodeName: string) => void;
  setBrowserKind: (kind: BrowserKind) => void;
  setBrowserPath: (path: string) => void;
  setColor: (color: string) => void;
  setMihomoControllerSecret: (secret: string) => void;
  setMihomoControllerUrl: (url: string) => void;
  setName: (name: string) => void;
  setNetworkProfile: (profile: NetworkProfile) => void;
  setProxyImport: (value: string) => void;
  setProxyPassword: (value: string) => void;
  setProxyUsername: (value: string) => void;
}) {
  const localProxySelected = isLoopbackProxyProfile(networkProfile);
  const mihomoBinding =
    networkProfile.mode === "fixed_proxy"
      ? networkProfile.externalMihomo
      : undefined;
  const selectedMihomoGroup = mihomoSnapshot?.groups.find(
    (group) => group.name === mihomoBinding?.selectorGroup,
  );
  return (
    <>
      <section className="create-hero panel">
        <p className="eyebrow">新环境</p>
        <h1>创建一个完整、长期的浏览器身份</h1>
        <p>
          VeriSilo
          会新建数据目录并用参数数组启动浏览器。不会复制现有账号，也不会修改默认
          Profile。
        </p>
        <div className="assurance-row">
          <span>✓ Cookie 与站点数据独立</span>
          <span>✓ 关闭后可继续使用</span>
          <span>✓ 单 Silo 安全启动</span>
        </div>
      </section>

      <section className="panel form-panel">
        <div className="step-heading">
          <span>1</span>
          <div>
            <h2>给这个身份一个名字</h2>
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

        <div className="step-heading">
          <span>2</span>
          <div>
            <h2>选择浏览器</h2>
            <p>首发支持 Windows 上的 Chrome 与 Edge Stable。</p>
          </div>
        </div>
        <div className="browser-switch" role="group" aria-label="浏览器类型">
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
              <span>{kind === "chrome" ? "Chrome Stable" : "Edge Stable"}</span>
            </button>
          ))}
        </div>
        {candidateOptions.length > 0 ? (
          <div className="candidate-list" aria-label="已检测到的浏览器">
            {candidateOptions.map((candidate) => (
              <button
                aria-pressed={browserPath === candidate.executablePath}
                className={
                  browserPath === candidate.executablePath ? "selected" : ""
                }
                key={candidate.executablePath}
                onClick={() => chooseBrowser(candidate)}
                type="button"
              >
                <span className="candidate-check" aria-hidden="true">
                  {browserPath === candidate.executablePath ? "✓" : ""}
                </span>
                <span>
                  <strong>{candidate.displayName}</strong>
                  <small>{candidate.version ?? "版本未知"}</small>
                </span>
              </button>
            ))}
          </div>
        ) : (
          <p className="form-hint">
            尚未在常见 Windows 安装位置找到该浏览器，请填写可执行文件绝对路径。
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

        <div className="step-heading">
          <span>3</span>
          <div>
            <h2>选择网络方式</h2>
            <p>
              “已配置”与“实际出口已验证”是两件事，创建后可在概览页主动检查。
            </p>
          </div>
        </div>
        <fieldset disabled={busy || mihomoBusy}>
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
              description="由 PAC URL 决定路由"
              label="PAC"
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
                    <span className="bridge-badge">外部内核桥接</span>
                    <div>
                      <strong>
                        订阅仍由你自己的 Mihomo / Clash 客户端管理
                      </strong>
                      <p>
                        只填监听端口时，VeriSilo 固定的是端口；连接本机
                        Controller 后，才会把选择组和节点也写进这个
                        Silo，并在每次启动前重新选择、读取确认。
                      </p>
                    </div>
                  </div>
                  <div className="controller-card">
                    <div className="controller-heading">
                      <div>
                        <strong>绑定本机 Controller（推荐）</strong>
                        <span>
                          只允许 127.0.0.1 / ::1 的 HTTP
                          Controller，拒绝远程管理地址。
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
                        Controller 地址
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
                        Controller Secret（可空）
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
                            {(selectedMihomoGroup?.nodes ?? []).map((node) => (
                              <option key={node.name} value={node.name}>
                                {node.name}
                                {node.delayMs === null
                                  ? ""
                                  : ` · ${node.delayMs} ms`}
                              </option>
                            ))}
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
                            : " 当前 Controller 未提供可读的订阅更新时间。"}
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
                      onChange={(event) => setProxyUsername(event.target.value)}
                      value={proxyUsername}
                    />
                  </label>
                  <label>
                    密码
                    <input
                      autoComplete="new-password"
                      onChange={(event) => setProxyPassword(event.target.value)}
                      type="password"
                      value={proxyPassword}
                    />
                  </label>
                </div>
                {proxyUsername !== "" &&
                !["http", "socks5"].includes(networkProfile.scheme) ? (
                  <p className="field-warning">
                    此协议不能由当前认证中继安全处理；请改用 HTTP/SOCKS5，或让
                    Mihomo 处理上游认证。
                  </p>
                ) : null}
              </div>
            </>
          ) : null}
          {networkProfile.mode === "pac" ? (
            <div className="form-grid pac-grid">
              <label>
                PAC URL
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
                必须代理（当前 PAC 尚无出口预检，会拒绝启动）
              </label>
            </div>
          ) : null}
        </fieldset>
        <div className="submit-row">
          <div>
            <strong>准备创建新的浏览器数据目录</strong>
            <span>不会导入、复制或改写已有 Profile。</span>
          </div>
          <button
            disabled={
              busy ||
              mihomoBusy ||
              name.trim().length === 0 ||
              browserPath.trim().length === 0
            }
            onClick={() => void createSilo()}
            type="button"
          >
            创建隔离 Silo
          </button>
        </div>
      </section>
    </>
  );
}

function LabsPanel({
  silos,
  vaultLocked,
}: {
  silos: Silo[];
  vaultLocked: boolean;
}) {
  const experiments = createDefaultLabsExperiments();
  return (
    <>
      <section className="panel labs-desktop-hero">
        <div>
          <p className="eyebrow">VeriSilo Labs · V0.5 · 默认关闭</p>
          <h1>可能破坏网站；泄漏即停</h1>
          <p>
            Labs 只允许在 Companion 中为“当前运行的
            Silo＋当前站点”逐次明确授权。每次按 Observe → Apply → Verify →
            Restore 执行；检测到跨标签页、iframe、Worker、Service Worker
            URL、页面可见 Cookie canary
            泄漏，或页面异常、超时、权限被接管时，会恢复并停用该站点。
          </p>
        </div>
        <span className="capability-badge best_effort">高风险实验</span>
      </section>

      <section className="panel labs-silo-scope">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">逐 Silo / 逐站点门禁</p>
            <h2>桌面端不会替扩展推断授权或验证结果</h2>
            <p>
              只有 Companion 能看到当前网页并执行窄实验。桌面端仅列出可绑定的
              Silo；授权站点、临时 canary、运行状态和脱敏收据都由 Companion
              当场显示。无桌面连接时只能运行有到期时间的本机临时实验，不能归入任何
              Silo。
            </p>
          </div>
        </div>
        {vaultLocked ? (
          <p className="labs-empty-scope">
            保险库已锁定，无法列出可绑定 Silo。
          </p>
        ) : silos.length === 0 ? (
          <p className="labs-empty-scope">
            当前没有 Silo；Companion 只能清楚标记为“本机临时实验”。
          </p>
        ) : (
          <div className="labs-silo-list">
            {silos.map((silo) => (
              <article key={silo.id}>
                <span
                  className="silo-dot"
                  style={{ backgroundColor: silo.color }}
                />
                <div>
                  <strong>{silo.name}</strong>
                  <small>默认无站点授权；须从该 Silo 当前网页逐站开启</small>
                </div>
                <span className="capability-badge boundary">disabled</span>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">实验目录与五态证据</p>
            <h2>可选、尽力与不支持分开显示</h2>
            <p>
              “配置”“已应用”“自检通过”“已恢复”“不支持”不是同一件事。当前用户触发的
              MAIN-world 注入无法证明早于页面脚本，因此 Worker 自检成功也只能是
              best-effort，不能写成 verified。
            </p>
          </div>
        </div>
        <div className="desktop-capability-table labs-catalog">
          {experiments.map((experiment) => {
            const definition = LABS_EXPERIMENT_DEFINITIONS.find(
              (candidate) => candidate.id === experiment.id,
            )!;
            return (
              <article className="capability-row" key={experiment.id}>
                <div className="capability-name">
                  <strong>{definition.title}</strong>
                  <span
                    className={`capability-badge ${definition.tier === "unsupported" ? "boundary" : "best_effort"}`}
                  >
                    {labsDesktopStateLabel(experiment)}
                  </span>
                </div>
                <div>
                  <p>{definition.summary}</p>
                  <ul className="labs-limitations">
                    {definition.limitations.map((limitation) => (
                      <li key={limitation}>{limitation}</li>
                    ))}
                  </ul>
                </div>
                <div className="evidence-rule">
                  <span>
                    {definition.tier === "unsupported"
                      ? "当前替代"
                      : "证据 / 恢复"}
                  </span>
                  <p>
                    {definition.alternative ??
                      "Companion 记录 Observe / Apply / Verify / Restore 阶段、窄覆盖范围、停止代码和恢复结果；收据不保存 canary、Cookie 或 token。"}
                  </p>
                </div>
              </article>
            );
          })}
        </div>
      </section>

      <section className="panel labs-stop-conditions">
        <p className="eyebrow">Machine-readable stop conditions</p>
        <h2>任一命中都执行 restore_and_disable_site</h2>
        <p>
          cross_tab_canary_leak · iframe_canary_leak · worker_canary_leak ·
          service_worker_canary_leak · cookie_canary_leak · page_error ·
          worker_error · timeout · permission_taken_over · site_navigation ·
          scope_violation。
        </p>
      </section>
    </>
  );
}

function labsDesktopStateLabel(experiment: LabsExperiment): string {
  return experiment.state === "unsupported"
    ? "unsupported · 不可选"
    : "disabled · 默认关闭";
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

function CapabilityRoadmap({
  silos,
  vaultLocked,
}: {
  silos: Silo[];
  vaultLocked: boolean;
}) {
  const [wslStatus, setWslStatus] = useState<WslStatus | null>(null);
  const [wslBusy, setWslBusy] = useState(false);
  const [selectedWslDistribution, setSelectedWslDistribution] = useState("");
  const [engineStatuses, setEngineStatuses] = useState<EngineAdapterStatus[]>(
    [],
  );
  const [enginePackageRoot, setEnginePackageRoot] = useState("");
  const [enginePackageVersion, setEnginePackageVersion] = useState("");
  const [engineActionBusy, setEngineActionBusy] = useState(false);
  const [engineActionMessage, setEngineActionMessage] = useState<{
    tone: "success" | "error";
    text: string;
  } | null>(null);
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
  const [remoteDetachLocalConfirmed, setRemoteDetachLocalConfirmed] =
    useState(false);
  const [remoteDetachRiskAcknowledged, setRemoteDetachRiskAcknowledged] =
    useState(false);
  const [selectedRemoteSilo, setSelectedRemoteSilo] = useState("");
  const [remoteNetworkMode, setRemoteNetworkMode] =
    useState<RemoteNetworkPolicy["mode"]>("direct");
  const [remoteProxyPolicyId, setRemoteProxyPolicyId] = useState("");
  const [remoteProxyRequired, setRemoteProxyRequired] = useState(true);
  const [remoteTtlSeconds, setRemoteTtlSeconds] = useState("");
  const [remoteCostAcknowledged, setRemoteCostAcknowledged] = useState(false);
  const [remoteLogsLimit, setRemoteLogsLimit] = useState("50");
  const [remoteHumanLifetime, setRemoteHumanLifetime] = useState("1800");
  const [remoteAutomationLifetime, setRemoteAutomationLifetime] =
    useState("300");
  const [remoteAutomationReadScreen, setRemoteAutomationReadScreen] =
    useState(true);
  const [remoteAutomationSendInput, setRemoteAutomationSendInput] =
    useState(false);
  const [remoteAutomationApproved, setRemoteAutomationApproved] =
    useState(false);
  const [remotePrincipalSelection, setRemotePrincipalSelection] = useState("");
  const [remoteAutomationToRevoke, setRemoteAutomationToRevoke] = useState("");
  const [remoteInputText, setRemoteInputText] = useState("");
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

  useEffect(() => {
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
  }, []);

  const refreshEngineStatuses = async () => {
    setEngineStatuses(await desktopApi.listEngineAdapters());
  };

  const runEngineAction = async (
    adapterId: EngineAdapterId,
    action: "install" | "update" | "rollback" | "disable" | "enable",
  ) => {
    if (!["controlled-chromium", "camoufox"].includes(adapterId)) {
      setEngineActionMessage({
        tone: "error",
        text: "Stock 浏览器由其供应商管理，VeriSilo 不安装或回滚它。",
      });
      return;
    }
    if (
      (action === "install" || action === "update") &&
      (enginePackageRoot.trim() === "" || enginePackageVersion.trim() === "")
    ) {
      setEngineActionMessage({
        tone: "error",
        text: "安装或更新必须填写本机 package 根目录和固定语义版本。",
      });
      return;
    }
    if (
      (action === "rollback" || action === "disable") &&
      !window.confirm(
        action === "rollback"
          ? `确认回滚 ${engineAdapterLabel(adapterId)} 到上一个已验证包？`
          : `确认紧急停用 ${engineAdapterLabel(adapterId)}？已配置它的 Silo 将 fail closed。`,
      )
    ) {
      return;
    }
    setEngineActionBusy(true);
    setEngineActionMessage(null);
    try {
      if (action === "install") {
        await desktopApi.installEnginePackage(adapterId, {
          packageRoot: enginePackageRoot.trim(),
          expectedVersion: enginePackageVersion.trim(),
        });
      } else if (action === "update") {
        await desktopApi.updateEnginePackage(adapterId, {
          packageRoot: enginePackageRoot.trim(),
          expectedVersion: enginePackageVersion.trim(),
        });
      } else if (action === "rollback") {
        await desktopApi.rollbackEnginePackage(adapterId);
      } else {
        await desktopApi.setEngineEmergencyDisabled(
          adapterId,
          action === "disable",
          action === "disable" ? "User emergency disable from desktop" : null,
        );
      }
      await refreshEngineStatuses();
      setEngineActionMessage({
        tone: "success",
        text: `${engineAdapterLabel(adapterId)} 操作已持久化；下次启动仍会重新验证包。`,
      });
    } catch (error) {
      setEngineActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      setEngineActionBusy(false);
    }
  };

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
    if (selectedRemoteSilo === "" && silos[0] !== undefined && !vaultLocked) {
      setSelectedRemoteSilo(silos[0].id);
    }
  }, [selectedRemoteSilo, silos, vaultLocked]);

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
        text: "先解锁保险库并选择一个 Silo；环境 UUID 只允许来自现有 Silo。",
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
        text: "检测到多个 WSL 发行版；必须在下方显式选择并配置一个，不能使用发现顺序。",
      });
      return;
    }
    if (
      capability === undefined ||
      capability.availability.availability !== "available"
    ) {
      setEnvironmentActionMessage({
        tone: "error",
        text:
          capability?.availability.availability === "unavailable"
            ? capability.availability.reason
            : "该后端没有声明此操作。",
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
        text: "该 Silo 使用 PAC、SOCKS4、认证代理或宿主 loopback Mihomo，不能安全直接搬到来宾；请先配置来宾可访问的无认证 HTTP/HTTPS/SOCKS5 端点。",
      });
      return;
    }
    if (
      operation === "destroy" &&
      !window.confirm(
        `确认销毁「${silo.name}」在 ${environmentBackendLabel(selectedEnvironmentBackend)} 中的环境？后端只允许删除这个 Silo UUID 派生的资源。`,
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
      const receipt = await desktopApi.executeEnvironmentBackend(request);
      setEnvironmentActionMessage({
        tone: "success",
        text: receipt.message,
      });
      setEnvironmentStatuses(await desktopApi.environmentBackendStatuses());
    } catch (error) {
      setEnvironmentActionMessage({
        tone: "error",
        text: errorMessage(error),
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
        text: "端点格式与 pin 通过本地校验；本次没有联网、配对或保存。只有下方明确批准的配对与生命周期按钮会使用生产 pinned HTTPS 传输。",
      });
    } catch (error) {
      setRemoteValidation({ tone: "error", text: errorMessage(error) });
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
        text: "先解锁保险库；端点、凭据和绑定只存入加密保险库。",
      });
      return;
    }
    if (!remotePairingApproved) {
      setRemoteActionMessage({
        tone: "error",
        text: "配对必须单独勾选用户批准；它不会同时代表创建环境或接受费用。",
      });
      return;
    }
    if (!Number.isFinite(expiresAt)) {
      setRemoteActionMessage({
        tone: "error",
        text: "填写 Agent 签发的配对令牌到期时间。",
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
        text: "已通过普通 PKI 与所填 pin 完成一次用户批准的配对；应用凭据已加密保存，令牌输入已清空。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      setRemoteBusy(false);
    }
  };

  const revokeRemotePairing = async () => {
    if (
      !window.confirm(
        "确认从本机加密保险库中擦除远程应用凭据，以及本地保存的人类/自动化授权和屏幕通道元数据？稳定绑定与审计回执会保留，以免误建或偷换端点；此动作不会向 Agent 发送销毁请求。",
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
        text: "本地远程凭据、交互授权与屏幕通道元数据已撤销并擦除；稳定绑定和审计回执保留，后续请求会在联网前被拒绝。",
      });
    } catch (error) {
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
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
        text: "安全轮换要求保险库已解锁，并且旧配对凭据仍存在且未过期。",
      });
      return;
    }
    if (!remoteRotationApproved) {
      setRemoteActionMessage({
        tone: "error",
        text: "必须单独确认本次 pin 轮换；普通配对批准不会自动沿用。",
      });
      return;
    }
    if (!Number.isFinite(expiresAt) || token.length < 32) {
      setRemoteActionMessage({
        tone: "error",
        text: "填写新 pin 下 Agent 签发的有效一次性令牌与到期时间。",
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
        text: "TLS pin 已在同一 Origin、同一 Agent 身份下安全轮换；新凭据、端点和全部绑定已一次性保存，旧交互授权已清空。一次性令牌输入已清除。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      setRemoteBusy(false);
    }
  };

  const forceDetachRemoteBinding = async () => {
    if (selectedRemoteBinding === undefined) {
      setRemoteActionMessage({
        tone: "error",
        text: "所选 Silo 没有可强制分离的本地远程绑定。",
      });
      return;
    }
    if (!remoteDetachLocalConfirmed || !remoteDetachRiskAcknowledged) {
      setRemoteActionMessage({
        tone: "error",
        text: "强制分离必须完成两项独立确认：删除本地绑定，以及接受远端可能继续运行和计费的风险。",
      });
      return;
    }

    const siloId = selectedRemoteBinding.siloId;
    setRemoteDetachLocalConfirmed(false);
    setRemoteDetachRiskAcknowledged(false);
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      setRemoteStatus(await desktopApi.forceDetachRemoteEnvironment(siloId));
      setRemoteActionMessage({
        tone: "success",
        text: "仅本地绑定已强制分离，永久孤儿回执已加密保存。这不是远端删除：远端环境可能仍在运行并继续计费。现在可永久删除本地 Silo。",
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
    } finally {
      setRemoteBusy(false);
    }
  };

  const recoverRemoteDeletionProof = async () => {
    if (selectedRemoteBinding === undefined || vaultLocked) {
      setRemoteActionMessage({
        tone: "error",
        text: "先解锁保险库并选择仍有本地稳定绑定的 Silo。",
      });
      return;
    }
    setRemoteBusy(true);
    setRemoteActionMessage(null);
    try {
      const result = await desktopApi.recoverRemoteDeletionProof(
        selectedRemoteBinding.siloId,
      );
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: `已取回 Agent 先前持久化的 Provider 删除回执（${result.deletionProof?.reason ?? "unknown"}），并在环境/卷/密钥绑定核对后解除本地绑定。该动作本身没有发起新删除。`,
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteActionMessage({ tone: "error", text: errorMessage(error) });
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
        text: "先解锁保险库并选择一个现有 Silo；远程身份不能手工输入。",
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
            ? capability.availability.reason
            : "Agent 没有声明此操作。",
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
        text: "固定代理模式需要填写 Agent 管理员提供的策略 UUID；桌面不会发送代理 URL、凭据、命令或路径。",
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
        text: "创建前明确填写 60–2,592,000 秒的环境 TTL。",
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
        `确认销毁「${silo.name}」绑定的远程环境？只有 Agent 返回经过认证的 destroyed 结果后，本地稳定绑定才会移除。`,
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
            const limit = Number(remoteLogsLimit);
            if (!Number.isInteger(limit) || limit < 1 || limit > 200) {
              throw new Error("日志上限必须是 1–200。 ");
            }
            return desktopApi.logsRemoteEnvironment(silo.id, null, limit);
          }
        }
      })();
      await refreshRemoteStatus();
      setRemoteActionMessage({
        tone: "success",
        text: `${environmentOperationLabel(operation)}完成：${remoteResultStateLabel(result.state)}。绑定 ${result.bindingId}，远程环境 ${result.remoteEnvironmentId}。`,
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

  const runRemoteInteraction = async (
    action:
      | "open_human"
      | "close_human"
      | "grant_automation"
      | "revoke_automation"
      | "open_screen"
      | "send_input",
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
        text: "先解锁保险库、完成配对，并选择一个已有稳定远程绑定的 Silo。",
      });
      return;
    }

    const principal = (() => {
      const separator = remotePrincipalSelection.indexOf(":");
      if (separator < 0) {
        return null;
      }
      const kind = remotePrincipalSelection.slice(0, separator);
      const authorizationId = remotePrincipalSelection.slice(separator + 1);
      if (kind !== "human_session" && kind !== "automation") {
        return null;
      }
      return { kind, authorizationId } as RemoteInteractivePrincipal;
    })();

    if (
      (action === "open_screen" || action === "send_input") &&
      principal === null
    ) {
      setRemoteInteractionMessage({
        tone: "error",
        text: "选择一个尚未撤销且未过期的人类会话或自动化授权。",
      });
      return;
    }
    if (
      action === "send_input" &&
      principal?.kind === "automation" &&
      binding.humanSession !== undefined &&
      !binding.humanSession.revoked &&
      binding.humanSession.expiresAtUnixMs > Date.now()
    ) {
      setRemoteInteractionMessage({
        tone: "error",
        text: "人类会话仍在活动；自动化输入会被本地控制面暂停，直到人类会话关闭或过期。",
      });
      return;
    }
    if (
      action === "revoke_automation" &&
      (remoteAutomationToRevoke === "" ||
        !window.confirm(
          "确认撤销选中的自动化授权？撤销后的授权不能继续看屏或发送输入。",
        ))
    ) {
      return;
    }

    setRemoteBusy(true);
    setRemoteInteractionMessage(null);
    try {
      const receipt = await (async () => {
        switch (action) {
          case "open_human": {
            const lifetimeSeconds = Number(remoteHumanLifetime);
            if (
              !Number.isInteger(lifetimeSeconds) ||
              lifetimeSeconds < 60 ||
              lifetimeSeconds > 28_800
            ) {
              throw new Error("人类会话有效期必须是 60–28,800 秒。");
            }
            return desktopApi.openRemoteHumanSession(silo.id, lifetimeSeconds);
          }
          case "close_human":
            return desktopApi.closeRemoteHumanSession(silo.id);
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
              !Number.isInteger(lifetimeSeconds) ||
              lifetimeSeconds < 60 ||
              lifetimeSeconds > 3_600
            ) {
              throw new Error(
                "自动化授权需要单独批准、至少一个作用域，以及 60–3,600 秒有效期。",
              );
            }
            return desktopApi.grantRemoteAutomation(
              silo.id,
              lifetimeSeconds,
              scopes,
              true,
            );
          }
          case "revoke_automation":
            return desktopApi.revokeRemoteAutomation(
              silo.id,
              remoteAutomationToRevoke,
            );
          case "open_screen":
            return desktopApi.openRemoteScreen(silo.id, principal!);
          case "send_input": {
            const bytes = new TextEncoder().encode(remoteInputText).byteLength;
            if (
              remoteInputText.trim() !== remoteInputText ||
              bytes < 1 ||
              bytes > 512
            ) {
              throw new Error(
                "文本输入必须是 1–512 UTF-8 字节，且不能带首尾空白。",
              );
            }
            return desktopApi.sendRemoteInput(silo.id, principal!, [
              { type: "text", value: remoteInputText },
            ]);
          }
        }
      })();
      await refreshRemoteStatus();
      if (action === "send_input") {
        setRemoteInputText("");
      }
      setRemoteInteractionMessage({
        tone: "success",
        text: `Agent 已接受 ${remoteInteractionOperationLabel(receipt.operation)}；认证回执类型为 ${remoteAgentResponseLabel(receipt.response.type)}，本地已加密持久化。`,
      });
    } catch (error) {
      await refreshRemoteStatus().catch(() => undefined);
      setRemoteInteractionMessage({
        tone: "error",
        text: errorMessage(error),
      });
    } finally {
      if (action === "grant_automation") {
        setRemoteAutomationApproved(false);
      }
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
  const remoteAuthorizationNow = Date.now();
  const activeRemoteHumanSession =
    selectedRemoteBinding?.humanSession !== undefined &&
    !selectedRemoteBinding.humanSession.revoked &&
    selectedRemoteBinding.humanSession.expiresAtUnixMs > remoteAuthorizationNow
      ? selectedRemoteBinding.humanSession
      : undefined;
  const activeRemoteAutomations =
    selectedRemoteBinding?.automationAuthorizations.filter(
      (authorization) =>
        !authorization.revoked &&
        authorization.expiresAtUnixMs > remoteAuthorizationNow,
    ) ?? [];
  const remotePrincipalOptions = [
    ...(activeRemoteHumanSession === undefined
      ? []
      : [
          {
            value: `human_session:${activeRemoteHumanSession.authorizationId}`,
            label: `人类会话 · ${activeRemoteHumanSession.authorizationId}`,
          },
        ]),
    ...activeRemoteAutomations.map((authorization) => ({
      value: `automation:${authorization.authorizationId}`,
      label: `自动化 · ${authorization.scopes.join(" + ")} · ${authorization.authorizationId}`,
    })),
  ];
  const selectedRemotePrincipalAuthorization =
    remotePrincipalSelection.startsWith("automation:")
      ? activeRemoteAutomations.find(
          (authorization) =>
            authorization.authorizationId ===
            remotePrincipalSelection.slice("automation:".length),
        )
      : undefined;
  const revocableRemoteAutomations =
    selectedRemoteBinding?.automationAuthorizations.filter(
      (authorization) => !authorization.revoked,
    ) ?? [];
  const remoteInteractionReady =
    !vaultLocked &&
    !remoteBusy &&
    remoteStatus?.state === "paired" &&
    selectedRemoteBinding !== undefined;
  const selectedPrincipalIsHuman =
    activeRemoteHumanSession !== undefined &&
    remotePrincipalSelection ===
      `human_session:${activeRemoteHumanSession.authorizationId}`;
  const selectedPrincipalCanReadScreen =
    selectedPrincipalIsHuman ||
    selectedRemotePrincipalAuthorization?.scopes.includes("read_screen") ===
      true;
  const selectedPrincipalCanSendInput =
    selectedPrincipalIsHuman ||
    (selectedRemotePrincipalAuthorization?.scopes.includes("send_input") ===
      true &&
      activeRemoteHumanSession === undefined);

  useEffect(() => {
    setRemoteDetachLocalConfirmed(false);
    setRemoteDetachRiskAcknowledged(false);
  }, [
    selectedRemoteBinding?.bindingId,
    selectedRemoteBinding?.endpoint.pin.kind,
    selectedRemoteBinding?.endpoint.pin.sha256,
  ]);

  useEffect(() => {
    if (
      !remotePrincipalOptions.some(
        (option) => option.value === remotePrincipalSelection,
      )
    ) {
      setRemotePrincipalSelection(remotePrincipalOptions[0]?.value ?? "");
    }
  }, [remotePrincipalOptions, remotePrincipalSelection]);

  useEffect(() => {
    if (
      !revocableRemoteAutomations.some(
        (authorization) =>
          authorization.authorizationId === remoteAutomationToRevoke,
      )
    ) {
      setRemoteAutomationToRevoke(
        revocableRemoteAutomations[0]?.authorizationId ?? "",
      );
    }
  }, [remoteAutomationToRevoke, revocableRemoteAutomations]);

  const remotePairingExpiryMs = Date.parse(remotePairingExpiresAt);
  const remotePairingLifetimeMs = remotePairingExpiryMs - Date.now();
  const remotePairingExpiryValid =
    Number.isFinite(remotePairingExpiryMs) &&
    remotePairingLifetimeMs > 0 &&
    remotePairingLifetimeMs <= 5 * 60 * 1_000;
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

  return (
    <>
      <section className="roadmap-hero panel">
        <div>
          <p className="eyebrow">产品路线</p>
          <h1>同一个 VeriSilo，按需要选择隔离强度</h1>
          <p>
            独立 Profile 是已经可用的基础层。V0.7–V0.9
            的适配器和后端会在下方显示真实运行状态； 缺签名制品、Windows
            功能或自托管节点时只会显示不可用。
          </p>
        </div>
        <span className="roadmap-principle">能力必须可验证</span>
      </section>

      <section className="layer-grid" aria-label="环境实现层级">
        {ENVIRONMENT_LAYERS.map((layer, index) => (
          <article
            className={`layer-card${layer.status === "available" ? " available" : layer.status === "implemented" ? " implemented" : " external-gate"}`}
            key={layer.id}
          >
            <div className="layer-topline">
              <span className="layer-index">0{index + 1}</span>
              <span className="layer-version">{layer.version}</span>
            </div>
            <h2>{layer.name}</h2>
            <p>{layer.summary}</p>
            <ul>
              {layer.delivers.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
            <span className="layer-status">
              {layer.status === "available"
                ? "当前可用"
                : layer.status === "implemented"
                  ? "代码已接入 · 以本机状态为准"
                  : "外部门槛未满足"}
            </span>
          </article>
        ))}
      </section>

      <section className="panel provider-catalog">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">V0.7 EngineAdapter</p>
            <h2>浏览器引擎不会因为“有接口”就冒充可用</h2>
            <p>
              Stock Chrome/Edge
              已经过适配器启动；外部受控引擎必须先通过版本、哈希和签名校验。
            </p>
          </div>
        </div>
        <div className="form-grid">
          <label>
            外部 engine package 根目录
            <input
              disabled={engineActionBusy}
              onChange={(event) => setEnginePackageRoot(event.target.value)}
              placeholder="C:\\VeriSilo\\engine-packages\\controlled-150"
              value={enginePackageRoot}
            />
          </label>
          <label>
            固定 engine 版本
            <input
              disabled={engineActionBusy}
              onChange={(event) => setEnginePackageVersion(event.target.value)}
              placeholder="150.0.0"
              value={enginePackageVersion}
            />
          </label>
        </div>
        {engineActionMessage !== null ? (
          <p className={`inline-message ${engineActionMessage.tone}`}>
            {engineActionMessage.text}
          </p>
        ) : null}
        <div className="provider-status-grid">
          {engineStatuses.map((engine) => (
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
              <p>{engine.health.message}</p>
              <small>
                已接受 {engine.negotiation.accepted.length} /{" "}
                {engine.negotiation.capabilities.length}{" "}
                项声明能力；只有带直接证据的能力才可进入已验证。
              </small>
              {engine.descriptor.externallyPackaged ? (
                <div className="inline-actions">
                  <button
                    className="button-secondary"
                    disabled={engineActionBusy}
                    onClick={() =>
                      void runEngineAction(engine.descriptor.id, "install")
                    }
                    type="button"
                  >
                    安装
                  </button>
                  <button
                    className="button-secondary"
                    disabled={engineActionBusy}
                    onClick={() =>
                      void runEngineAction(engine.descriptor.id, "update")
                    }
                    type="button"
                  >
                    更新
                  </button>
                  <button
                    className="button-secondary"
                    disabled={engineActionBusy}
                    onClick={() =>
                      void runEngineAction(engine.descriptor.id, "rollback")
                    }
                    type="button"
                  >
                    回滚
                  </button>
                  <button
                    className="button-secondary"
                    disabled={engineActionBusy}
                    onClick={() =>
                      void runEngineAction(
                        engine.descriptor.id,
                        engine.descriptor.emergencyDisabled
                          ? "enable"
                          : "disable",
                      )
                    }
                    type="button"
                  >
                    {engine.descriptor.emergencyDisabled
                      ? "重新启用"
                      : "紧急停用"}
                  </button>
                </div>
              ) : null}
            </article>
          ))}
          {engineStatuses.length === 0 ? (
            <p className="empty-provider-copy">尚未取得引擎状态。</p>
          ) : null}
        </div>
      </section>

      <section className="panel provider-catalog">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">V0.8 EnvironmentBackend</p>
            <h2>固定九项生命周期，逐个说明能不能执行</h2>
            <p>
              WSL、Sandbox 与 Hyper-V
              不共享假能力；暂停、快照、销毁和来宾网络证据均按后端单独协商。
            </p>
          </div>
        </div>
        <div className="provider-status-grid">
          {environmentStatuses.map((environment) => {
            const available = environment.capabilities.filter(
              (capability) =>
                capability.availability.availability === "available",
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
                    {available}/9 项可执行
                  </span>
                </div>
                <p>
                  {missing.length === 0
                    ? "本次前置条件均已报告满足；仍需执行后端操作取得运行证据。"
                    : missing
                        .slice(0, 2)
                        .map((item) => item.detail)
                        .join(" ")}
                </p>
                <small>不可用操作会返回错误，不会静默成功。</small>
              </article>
            );
          })}
        </div>
        {technologyError !== null ? (
          <p className="field-error" role="alert">
            无法读取部分适配器状态：{technologyError}
          </p>
        ) : null}
        <div className="environment-console">
          <div className="environment-console-heading">
            <div>
              <strong>操作一个现有 Silo 的环境后端</strong>
              <span>
                这里只能发送固定生命周期操作；不会接收命令文本、脚本路径或任意参数。
              </span>
            </div>
          </div>
          <div className="form-grid environment-console-selects">
            <label>
              环境后端
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
                  <option key={environment.backend} value={environment.backend}>
                    {environmentBackendLabel(environment.backend)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              使用哪个 Silo 身份
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
            {selectedBackendStatus?.capabilities.map((capability) => {
              const available =
                capability.availability.availability === "available";
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
                    selectedEnvironmentSilo === "" ||
                    !available
                  }
                  key={capability.operation}
                  onClick={() =>
                    void runEnvironmentOperation(capability.operation)
                  }
                  title={
                    available
                      ? undefined
                      : capability.availability.availability === "unavailable"
                        ? capability.availability.reason
                        : undefined
                  }
                  type="button"
                >
                  {environmentOperationLabel(capability.operation)}
                </button>
              );
            })}
          </div>
          {selectedBackendStatus !== undefined ? (
            <div className="environment-evidence-boundary">
              <strong>能力与证据边界</strong>
              <p>
                已配置仅表示控制器写入策略；来宾观测是来宾回执；已验证要求当前绑定与证据条件全部满足。不可用不会被折算成通过。
              </p>
              <ul>
                {selectedBackendStatus.prerequisites.map((prerequisite) => (
                  <li key={prerequisite.id}>
                    <span className={`environment-state ${prerequisite.state}`}>
                      {environmentPrerequisiteStateLabel(prerequisite.state)}
                    </span>
                    <span>{prerequisite.detail}</span>
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

      <section className="panel provider-catalog remote-provider-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">V0.9 自托管远程环境</p>
            <h2>固定 pin、加密凭据与稳定 Silo 绑定的控制面</h2>
            <p>
              没有默认端点或公共云。状态读取与本地校验绝不联网；只有你点击配对、九项固定生命周期操作或六项授权交互时，
              桌面才会连接所填的自托管 HTTPS Origin，并同时要求普通 PKI
              与证书/SPKI pin。
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
            <div className="remote-status-facts">
              <span>协议 v{remoteStatus.protocolVersion}</span>
              <span>
                pin 传输：
                {remoteStatus.transportAvailable ? "已编译" : "不可用"}
              </span>
              <span>
                加密持久绑定：
                {remoteStatus.durableBindingStoreAvailable ? "可用" : "不可用"}
              </span>
              <span>
                可用凭据：
                {remoteStatus.selfHostedAgentAvailable ? "有" : "无"}
              </span>
              <span>
                协商操作：
                {
                  remoteStatus.capabilities.filter(
                    (capability) =>
                      capability.availability.availability === "available",
                  ).length
                }
                /9
              </span>
            </div>
            <p className="remote-status-message">{remoteStatus.message}</p>
            {remoteStatus.endpoint !== null ? (
              <div className="remote-endpoint-proof">
                <div>
                  <span>加密保存的自托管 Origin</span>
                  <strong>{remoteStatus.endpoint.origin}</strong>
                </div>
                <div>
                  <span>
                    {remoteStatus.endpoint.pin.kind === "spki_sha256"
                      ? "SPKI SHA-256 pin"
                      : "Certificate SHA-256 pin"}
                  </span>
                  <code>{remoteStatus.endpoint.pin.sha256}</code>
                </div>
                {remoteStatus.pairing !== null ? (
                  <div>
                    <span>应用凭据到期</span>
                    <strong>
                      {new Date(
                        remoteStatus.pairing.credentialExpiresAtUnixMs,
                      ).toLocaleString("zh-CN")}
                      {remoteStatus.pairing.expired ? "（已过期）" : ""}
                    </strong>
                    <code>{remoteStatus.pairing.clientCredentialId}</code>
                  </div>
                ) : null}
                {remoteStatus.pairing !== null ? (
                  <div>
                    <span>Agent 节点披露</span>
                    <strong>
                      {remoteStatus.pairing.node.operatorLabel} ·{" "}
                      {remoteStatus.pairing.node.dataRegion}
                    </strong>
                    <code>{remoteStatus.pairing.node.nodeId}</code>
                  </div>
                ) : null}
              </div>
            ) : null}
          </>
        ) : null}
        <div className="form-grid remote-endpoint-form">
          <label>
            用户自托管 HTTPS Origin
            <input
              autoComplete="off"
              disabled={remoteBusy || vaultLocked}
              onChange={(event) => {
                setRemoteOrigin(event.target.value);
                setRemoteValidation(null);
              }}
              placeholder="https://browser.example.com/"
              spellCheck={false}
              value={remoteOrigin}
            />
          </label>
          <label>
            Pin 类型
            <select
              disabled={remoteBusy || vaultLocked}
              onChange={(event) => {
                setRemotePinKind(
                  event.target.value as RemoteEndpoint["pin"]["kind"],
                );
                setRemoteValidation(null);
              }}
              value={remotePinKind}
            >
              <option value="spki_sha256">SPKI SHA-256</option>
              <option value="certificate_sha256">Certificate SHA-256</option>
            </select>
          </label>
          <label>
            SHA-256 pin（64 位小写十六进制）
            <input
              autoComplete="off"
              disabled={remoteBusy || vaultLocked}
              maxLength={64}
              onChange={(event) => {
                setRemotePinSha256(event.target.value);
                setRemoteValidation(null);
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
              !/^[a-f0-9]{64}$/u.test(remotePinSha256.trim().toLowerCase())
            }
            onClick={() => void validateRemoteEndpoint()}
            type="button"
          >
            仅在本地校验配置
          </button>
          {remoteStatus?.pairing !== null &&
          remoteStatus?.pairing !== undefined ? (
            <button
              className="button-danger"
              disabled={remoteBusy || vaultLocked}
              onClick={() => void revokeRemotePairing()}
              type="button"
            >
              撤销本地配对凭据
            </button>
          ) : null}
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
              <strong>用 Agent 签发的一次性令牌配对</strong>
              <span>
                令牌最长只接受五分钟有效期。点击“配对”会联网；令牌无论成功或失败都会在本机记为已使用并清空输入。
              </span>
            </div>
          </div>
          <div className="form-grid remote-pairing-form">
            <label>
              配对令牌 ID（UUID）
              <input
                autoComplete="off"
                disabled={
                  remoteBusy || vaultLocked || remoteStatus?.pairing !== null
                }
                onChange={(event) =>
                  setRemotePairingTokenId(event.target.value)
                }
                spellCheck={false}
                value={remotePairingTokenId}
              />
            </label>
            <label>
              一次性配对令牌
              <input
                autoComplete="off"
                disabled={
                  remoteBusy || vaultLocked || remoteStatus?.pairing !== null
                }
                onChange={(event) => setRemotePairingToken(event.target.value)}
                spellCheck={false}
                type="password"
                value={remotePairingToken}
              />
            </label>
            <label>
              Agent 声明的到期时间
              <input
                disabled={
                  remoteBusy || vaultLocked || remoteStatus?.pairing !== null
                }
                onChange={(event) =>
                  setRemotePairingExpiresAt(event.target.value)
                }
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
              令牌到期：
              {Number.isFinite(remotePairingExpiryMs)
                ? new Date(remotePairingExpiryMs).toLocaleString("zh-CN")
                : "格式无效"}
              。
              {remotePairingExpiryValid
                ? "当前剩余时间在协议允许的五分钟内。"
                : "令牌必须尚未过期，且从当前时刻起最多有效五分钟。"}
            </p>
          ) : null}
          <label className="remote-confirmation">
            <input
              checked={remotePairingApproved}
              disabled={
                remoteBusy || vaultLocked || remoteStatus?.pairing !== null
              }
              onChange={(event) =>
                setRemotePairingApproved(event.target.checked)
              }
              type="checkbox"
            />
            <span>
              我明确批准把这枚一次性令牌只发送到上方 Origin，并按上方 pin
              验证对端。此批准不代表接受创建费用。
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
                remotePairingToken.length < 32 ||
                !remotePairingExpiryValid ||
                !/^[0-9a-f-]{36}$/iu.test(remotePairingTokenId.trim()) ||
                remoteOrigin.trim() === "" ||
                !/^[a-f0-9]{64}$/u.test(remotePinSha256.trim().toLowerCase())
              }
              onClick={() => void pairRemoteEndpoint()}
              type="button"
            >
              {remoteBusy ? "正在执行…" : "明确批准并配对（会联网）"}
            </button>
          </div>
        </div>

        <div className="remote-rotation-panel">
          <div className="environment-console-heading">
            <div>
              <strong>同一 Origin 安全轮换 TLS pin</strong>
              <span>
                先在旧 pin 下用旧 bearer 做在线授权，再到新 pin 消费同一 token
                与 60 秒单次 challenge；新 Server ID
                还必须与旧配对及每条绑定完全一致。旧授权失败绝不会联系新 pin。
              </span>
            </div>
          </div>
          <p className="remote-rotation-boundary">
            轮换要求旧 pin 与新 pin
            在一次操作的一分钟授权窗口内依次可达。失败时旧端点、凭据身份和绑定不变，但新令牌
            ID
            永久记为已用；已认证发送的旧请求序号也不会回退。成功会签发新应用凭据，并清空全部本地交互授权。
          </p>
          <div className="form-grid remote-rotation-form">
            <label>
              固定 Origin（不可更改）
              <input
                disabled
                value={remoteStatus?.endpoint?.origin ?? "尚无已配对 Origin"}
              />
            </label>
            <label>
              新 Pin 类型
              <select
                disabled={remoteBusy || remoteStatus?.state !== "paired"}
                onChange={(event) =>
                  setRemoteRotationPinKind(
                    event.target.value as RemoteEndpoint["pin"]["kind"],
                  )
                }
                value={remoteRotationPinKind}
              >
                <option value="spki_sha256">SPKI SHA-256</option>
                <option value="certificate_sha256">Certificate SHA-256</option>
              </select>
            </label>
            <label>
              新 SHA-256 pin
              <input
                autoComplete="off"
                disabled={remoteBusy || remoteStatus?.state !== "paired"}
                maxLength={64}
                onChange={(event) =>
                  setRemoteRotationPinSha256(event.target.value)
                }
                spellCheck={false}
                value={remoteRotationPinSha256}
              />
            </label>
            <label>
              新一次性令牌 ID（UUID）
              <input
                autoComplete="off"
                disabled={remoteBusy || remoteStatus?.state !== "paired"}
                onChange={(event) =>
                  setRemoteRotationTokenId(event.target.value)
                }
                spellCheck={false}
                value={remoteRotationTokenId}
              />
            </label>
            <label>
              新 pin 下的一次性令牌
              <input
                autoComplete="off"
                disabled={remoteBusy || remoteStatus?.state !== "paired"}
                onInput={(event) =>
                  setRemoteRotationTokenReady(
                    event.currentTarget.value.length >= 32,
                  )
                }
                ref={remoteRotationTokenRef}
                spellCheck={false}
                type="password"
              />
            </label>
            <label>
              新令牌到期时间
              <input
                disabled={remoteBusy || remoteStatus?.state !== "paired"}
                onChange={(event) =>
                  setRemoteRotationExpiresAt(event.target.value)
                }
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
              新令牌到期：
              {Number.isFinite(remoteRotationExpiryMs)
                ? new Date(remoteRotationExpiryMs).toLocaleString("zh-CN")
                : "格式无效"}
              。
              {remoteRotationExpiryValid
                ? "当前剩余时间在协议允许的五分钟内。"
                : "新令牌必须尚未过期，且从当前时刻起最多有效五分钟。"}
            </p>
          ) : null}
          <label className="remote-confirmation remote-rotation-confirmation">
            <input
              checked={remoteRotationApproved}
              disabled={remoteBusy || remoteStatus?.state !== "paired"}
              onChange={(event) =>
                setRemoteRotationApproved(event.target.checked)
              }
              type="checkbox"
            />
            <span>
              我明确批准把这枚新一次性令牌只发送到当前 Origin，并按上方新 pin
              验证。只有 Agent
              身份与全部旧绑定一致才提交；我知道令牌即使失败也不能重用。
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
                !remoteRotationTokenReady ||
                !remoteRotationExpiryValid ||
                !remoteRotationPinValid ||
                !remoteRotationPinChanged ||
                !/^[0-9a-f-]{36}$/iu.test(remoteRotationTokenId.trim())
              }
              onClick={() => void rotateRemoteTlsPin()}
              type="button"
            >
              {remoteBusy ? "正在执行…" : "明确批准并安全轮换（会联网）"}
            </button>
          </div>
        </div>

        <div className="remote-lifecycle-console">
          <div className="environment-console-heading">
            <div>
              <strong>操作选中的 Silo 绑定</strong>
              <span>
                九项命令均为固定类型；不会发送 shell、路径、VM 镜像或任意 URL。
              </span>
            </div>
          </div>
          <div className="form-grid remote-operation-form">
            <label>
              现有 Silo 身份
              <select
                disabled={remoteBusy || vaultLocked}
                onChange={(event) => {
                  setSelectedRemoteSilo(event.target.value);
                  setRemoteDetachLocalConfirmed(false);
                  setRemoteDetachRiskAcknowledged(false);
                  setRemoteActionMessage(null);
                }}
                value={selectedRemoteSilo}
              >
                <option value="">
                  {vaultLocked ? "先解锁保险库" : "请选择 Silo"}
                </option>
                {silos.map((silo) => (
                  <option key={silo.id} value={silo.id}>
                    {silo.name} · {silo.id}
                  </option>
                ))}
              </select>
            </label>
            <label>
              来宾网络策略
              <select
                disabled={remoteBusy}
                onChange={(event) =>
                  setRemoteNetworkMode(
                    event.target.value as RemoteNetworkPolicy["mode"],
                  )
                }
                value={remoteNetworkMode}
              >
                <option value="direct">Direct</option>
                <option value="fixed_proxy">Agent 固定代理策略</option>
              </select>
            </label>
            {remoteNetworkMode === "fixed_proxy" ? (
              <label>
                Agent 代理策略 UUID
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
              创建 TTL（秒）
              <input
                disabled={remoteBusy}
                inputMode="numeric"
                max={2_592_000}
                min={60}
                onChange={(event) => setRemoteTtlSeconds(event.target.value)}
                placeholder="必须明确填写 60–2592000"
                type="number"
                value={remoteTtlSeconds}
              />
            </label>
            <label>
              日志条数上限
              <input
                disabled={remoteBusy}
                max={200}
                min={1}
                onChange={(event) => setRemoteLogsLimit(event.target.value)}
                type="number"
                value={remoteLogsLimit}
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
              <span>
                代理为强制策略；缺少、过期、泄漏或失败的来宾证据会阻止启动。
              </span>
            </label>
          ) : null}
          <div className="remote-cost-disclosure">
            <strong>本次创建费用披露</strong>
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
                    <dd>
                      {remoteStatus.pairing.node.keyCustody ===
                      "user_controlled"
                        ? "用户控制"
                        : remoteStatus.pairing.node.keyCustody}
                    </dd>
                  </div>
                  <div>
                    <dt>估算每小时费用</dt>
                    <dd>
                      {formatMicrosCurrency(
                        remoteStatus.pairing.node.cost.estimatedMicrosPerHour,
                        remoteStatus.pairing.node.cost.currency,
                      )}
                    </dd>
                  </div>
                </dl>
                <p>{remoteStatus.pairing.node.cost.notice}</p>
              </>
            ) : (
              <p>
                配对成功后才会显示由 Agent
                返回并经本地校验的运营者、区域、密钥保管和费用披露；没有这些数据时不能创建。
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
                我已查看上方 Agent
                返回的费用披露，并仅为下一次“创建”明确接受该费用。此确认不会由配对批准代替，创建尝试后会自动复位。
              </span>
            </label>
          </div>
          <div className="environment-operation-grid remote-operation-grid">
            {remoteStatus?.capabilities.map((capability) => {
              const available =
                capability.availability.availability === "available";
              const needsBinding = capability.operation !== "create";
              const bindingStateInvalid = needsBinding
                ? selectedRemoteBinding === undefined
                : selectedRemoteBinding !== undefined;
              return (
                <button
                  className={
                    capability.operation === "destroy"
                      ? "button-danger"
                      : "button-secondary"
                  }
                  disabled={
                    remoteBusy ||
                    vaultLocked ||
                    selectedRemoteSilo === "" ||
                    !available ||
                    bindingStateInvalid
                  }
                  key={capability.operation}
                  onClick={() => void runRemoteOperation(capability.operation)}
                  title={
                    !available &&
                    capability.availability.availability === "unavailable"
                      ? capability.availability.reason
                      : bindingStateInvalid
                        ? needsBinding
                          ? "此 Silo 尚无远程绑定。"
                          : "此 Silo 已有稳定远程绑定，不能再次创建。"
                        : undefined
                  }
                  type="button"
                >
                  {environmentOperationLabel(capability.operation)}
                </button>
              );
            })}
          </div>

          <div className="remote-interaction-console">
            <div className="environment-console-heading">
              <div>
                <strong>人类会话与显式自动化授权</strong>
                <span>
                  所有看屏/输入都绑定到下方授权
                  UUID。人类会话活动时，自动化输入在本地先暂停；Agent
                  仍会独立复核授权、作用域与到期时间。
                </span>
              </div>
            </div>

            <div className="remote-interaction-section">
              <div className="form-grid remote-interaction-form">
                <label>
                  人类会话有效期（秒）
                  <input
                    disabled={!remoteInteractionReady}
                    max={28_800}
                    min={60}
                    onChange={(event) =>
                      setRemoteHumanLifetime(event.target.value)
                    }
                    type="number"
                    value={remoteHumanLifetime}
                  />
                </label>
                <div className="remote-inline-actions">
                  <button
                    className="button-secondary"
                    disabled={
                      !remoteInteractionReady ||
                      activeRemoteHumanSession !== undefined
                    }
                    onClick={() => void runRemoteInteraction("open_human")}
                    type="button"
                  >
                    开启人类会话
                  </button>
                  <button
                    className="button-secondary"
                    disabled={
                      !remoteInteractionReady ||
                      activeRemoteHumanSession === undefined
                    }
                    onClick={() => void runRemoteInteraction("close_human")}
                    type="button"
                  >
                    关闭人类会话
                  </button>
                </div>
              </div>
              {selectedRemoteBinding?.humanSession !== undefined ? (
                <dl className="remote-interaction-facts">
                  <div>
                    <dt>人类授权</dt>
                    <dd>
                      {selectedRemoteBinding.humanSession.authorizationId}
                    </dd>
                  </div>
                  <div>
                    <dt>状态 / 到期</dt>
                    <dd>
                      {selectedRemoteBinding.humanSession.revoked
                        ? "已撤销"
                        : selectedRemoteBinding.humanSession.expiresAtUnixMs <=
                            remoteAuthorizationNow
                          ? "已过期"
                          : "活动"}{" "}
                      ·{" "}
                      {new Date(
                        selectedRemoteBinding.humanSession.expiresAtUnixMs,
                      ).toLocaleString("zh-CN")}
                    </dd>
                  </div>
                </dl>
              ) : (
                <p className="remote-interaction-note">
                  此绑定尚未保存人类会话授权。
                </p>
              )}
            </div>

            <div className="remote-interaction-section">
              <div className="form-grid remote-interaction-form automation">
                <label>
                  自动化有效期（秒）
                  <input
                    disabled={!remoteInteractionReady}
                    max={3_600}
                    min={60}
                    onChange={(event) =>
                      setRemoteAutomationLifetime(event.target.value)
                    }
                    type="number"
                    value={remoteAutomationLifetime}
                  />
                </label>
                <label className="remote-confirmation compact">
                  <input
                    checked={remoteAutomationReadScreen}
                    disabled={!remoteInteractionReady}
                    onChange={(event) =>
                      setRemoteAutomationReadScreen(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>允许读取屏幕通道元数据</span>
                </label>
                <label className="remote-confirmation compact">
                  <input
                    checked={remoteAutomationSendInput}
                    disabled={!remoteInteractionReady}
                    onChange={(event) =>
                      setRemoteAutomationSendInput(event.target.checked)
                    }
                    type="checkbox"
                  />
                  <span>允许发送受限输入事件</span>
                </label>
              </div>
              <label className="remote-confirmation">
                <input
                  checked={remoteAutomationApproved}
                  disabled={!remoteInteractionReady}
                  onChange={(event) =>
                    setRemoteAutomationApproved(event.target.checked)
                  }
                  type="checkbox"
                />
                <span>
                  我明确批准下一枚自动化授权、上方作用域和有效期。此批准不会从配对、人类会话或创建费用确认继承，尝试后会复位。
                </span>
              </label>
              <div className="remote-inline-actions remote-automation-actions">
                <button
                  className="button-secondary"
                  disabled={
                    !remoteInteractionReady ||
                    !remoteAutomationApproved ||
                    (!remoteAutomationReadScreen && !remoteAutomationSendInput)
                  }
                  onClick={() => void runRemoteInteraction("grant_automation")}
                  type="button"
                >
                  明确批准自动化
                </button>
                <label>
                  待撤销授权
                  <select
                    disabled={
                      !remoteInteractionReady ||
                      revocableRemoteAutomations.length === 0
                    }
                    onChange={(event) =>
                      setRemoteAutomationToRevoke(event.target.value)
                    }
                    value={remoteAutomationToRevoke}
                  >
                    <option value="">无可撤销授权</option>
                    {revocableRemoteAutomations.map((authorization) => (
                      <option
                        key={authorization.authorizationId}
                        value={authorization.authorizationId}
                      >
                        {authorization.scopes.join(" + ")} ·{" "}
                        {authorization.authorizationId}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  className="button-danger"
                  disabled={
                    !remoteInteractionReady || remoteAutomationToRevoke === ""
                  }
                  onClick={() => void runRemoteInteraction("revoke_automation")}
                  type="button"
                >
                  撤销自动化
                </button>
              </div>
              {selectedRemoteBinding !== undefined &&
              selectedRemoteBinding.automationAuthorizations.length > 0 ? (
                <ul className="remote-authorization-list">
                  {selectedRemoteBinding.automationAuthorizations.map(
                    (authorization) => (
                      <li key={authorization.authorizationId}>
                        <code>{authorization.authorizationId}</code>
                        <span>{authorization.scopes.join(" + ")}</span>
                        <strong>
                          {authorization.revoked
                            ? "已撤销"
                            : authorization.expiresAtUnixMs <=
                                remoteAuthorizationNow
                              ? "已过期"
                              : "活动至 " +
                                new Date(
                                  authorization.expiresAtUnixMs,
                                ).toLocaleString("zh-CN")}
                        </strong>
                      </li>
                    ),
                  )}
                </ul>
              ) : (
                <p className="remote-interaction-note">
                  此绑定尚未保存自动化授权。
                </p>
              )}
            </div>

            <div className="remote-interaction-section">
              <div className="form-grid remote-control-form">
                <label>
                  交互主体
                  <select
                    disabled={
                      !remoteInteractionReady ||
                      remotePrincipalOptions.length === 0
                    }
                    onChange={(event) =>
                      setRemotePrincipalSelection(event.target.value)
                    }
                    value={remotePrincipalSelection}
                  >
                    <option value="">无活动授权</option>
                    {remotePrincipalOptions.map((option) => (
                      <option key={option.value} value={option.value}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  单批文本输入（1–512 UTF-8 字节）
                  <input
                    autoComplete="off"
                    disabled={!remoteInteractionReady}
                    maxLength={512}
                    onChange={(event) => setRemoteInputText(event.target.value)}
                    value={remoteInputText}
                  />
                </label>
              </div>
              <div className="remote-inline-actions">
                <button
                  className="button-secondary"
                  disabled={
                    !remoteInteractionReady || !selectedPrincipalCanReadScreen
                  }
                  onClick={() => void runRemoteInteraction("open_screen")}
                  type="button"
                >
                  请求屏幕通道元数据
                </button>
                <button
                  className="button-secondary"
                  disabled={
                    !remoteInteractionReady ||
                    !selectedPrincipalCanSendInput ||
                    remoteInputText.length === 0
                  }
                  onClick={() => void runRemoteInteraction("send_input")}
                  type="button"
                >
                  发送受限文本事件
                </button>
              </div>
              <p className="remote-screen-boundary">
                “屏幕通道”回执只证明 Agent
                返回了与授权绑定的认证加密通道元数据。当前桌面不连接、解码或渲染该媒体流，因此这里不会冒充可见的远程桌面。
              </p>
              {selectedRemoteBinding?.lastScreenChannel !== undefined ? (
                <dl className="remote-interaction-facts">
                  <div>
                    <dt>Channel ID</dt>
                    <dd>{selectedRemoteBinding.lastScreenChannel.channelId}</dd>
                  </div>
                  <div>
                    <dt>绑定授权</dt>
                    <dd>
                      {selectedRemoteBinding.lastScreenChannel.authorizationId}
                    </dd>
                  </div>
                  <div>
                    <dt>传输声明</dt>
                    <dd>
                      {selectedRemoteBinding.lastScreenChannel.transport ===
                      "authenticated_encrypted_stream"
                        ? "认证加密流（仅元数据）"
                        : selectedRemoteBinding.lastScreenChannel.transport}
                    </dd>
                  </div>
                  <div>
                    <dt>到期</dt>
                    <dd>
                      {new Date(
                        selectedRemoteBinding.lastScreenChannel.expiresAtUnixMs,
                      ).toLocaleString("zh-CN")}
                    </dd>
                  </div>
                </dl>
              ) : null}
              {selectedRemoteBinding?.lastInteraction !== undefined ? (
                <p className="remote-interaction-note">
                  最近回执：
                  {remoteInteractionOperationLabel(
                    selectedRemoteBinding.lastInteraction.operation,
                  )}{" "}
                  ·{" "}
                  {remoteAgentResponseLabel(
                    selectedRemoteBinding.lastInteraction.response.type,
                  )}{" "}
                  ·{" "}
                  {new Date(
                    selectedRemoteBinding.lastInteraction.observedAtUnixMs,
                  ).toLocaleString("zh-CN")}
                </p>
              ) : null}
            </div>

            {remoteInteractionMessage !== null ? (
              <p
                className={`environment-action-message ${remoteInteractionMessage.tone}`}
                role={
                  remoteInteractionMessage.tone === "error" ? "alert" : "status"
                }
              >
                {remoteInteractionMessage.text}
              </p>
            ) : null}
          </div>

          {selectedRemoteSiloRecord !== undefined ? (
            <div className="remote-selected-state">
              <div className="remote-selected-heading">
                <div>
                  <span>当前 Silo</span>
                  <strong>{selectedRemoteSiloRecord.name}</strong>
                  <code>{selectedRemoteSiloRecord.id}</code>
                </div>
                <span
                  className={`provider-health ${selectedRemoteBinding === undefined ? "unavailable" : "healthy"}`}
                >
                  {selectedRemoteBinding === undefined
                    ? selectedRemoteResult?.state === "destroyed"
                      ? "已销毁并解除绑定"
                      : "尚未绑定"
                    : "稳定绑定已保存"}
                </span>
              </div>
              {selectedRemoteBinding !== undefined ? (
                <>
                  <dl className="remote-binding-facts">
                    <div>
                      <dt>Binding ID</dt>
                      <dd>{selectedRemoteBinding.bindingId}</dd>
                    </div>
                    <div>
                      <dt>Remote Environment ID</dt>
                      <dd>{selectedRemoteBinding.remoteEnvironmentId}</dd>
                    </div>
                    <div>
                      <dt>绑定 Origin</dt>
                      <dd>{selectedRemoteBinding.endpoint.origin}</dd>
                    </div>
                    <div>
                      <dt>网络</dt>
                      <dd>
                        {selectedRemoteBinding.network.mode === "direct"
                          ? "Direct"
                          : `Fixed proxy · ${selectedRemoteBinding.network.policyId} · ${selectedRemoteBinding.network.required ? "必需" : "可选"}`}
                      </dd>
                    </div>
                    <div>
                      <dt>加密卷</dt>
                      <dd>
                        {selectedRemoteBinding.volume.encrypted
                          ? "已加密"
                          : "未加密"}{" "}
                        · {selectedRemoteBinding.volume.volumeId}
                      </dd>
                    </div>
                    <div>
                      <dt>卷密钥</dt>
                      <dd>
                        {selectedRemoteBinding.volume.keyCustody ===
                        "user_controlled"
                          ? "用户控制"
                          : selectedRemoteBinding.volume.keyCustody}{" "}
                        · {selectedRemoteBinding.volume.keyId}
                      </dd>
                    </div>
                    <div>
                      <dt>Agent 最后活动时间</dt>
                      <dd>
                        {new Date(
                          selectedRemoteBinding.lastActivityAtUnixMs,
                        ).toLocaleString("zh-CN")}
                      </dd>
                    </div>
                  </dl>
                  <div className="remote-proof-recovery">
                    <strong>恢复已持久化的删除回执</strong>
                    <p>
                      该检查不会发起删除。只有 Agent 已经为同一环境保存了完整
                      typed Provider
                      回执时，客户端才会取回它并解除绑定；仍在运行的环境会要求另行确认销毁。
                    </p>
                    <button
                      className="button-secondary"
                      disabled={remoteBusy || vaultLocked}
                      onClick={() => void recoverRemoteDeletionProof()}
                      type="button"
                    >
                      检查并取回既有删除回执（不发起删除）
                    </button>
                  </div>
                  <div className="remote-force-detach">
                    <strong>灾难恢复：仅强制分离本地绑定</strong>
                    <p>
                      仅当旧 pin、凭据与 Agent
                      都无法恢复时使用。这不会联系远端、不会销毁环境或卷，也不会生成删除证明；远端资源可能继续运行并持续计费。
                    </p>
                    <label className="remote-confirmation compact">
                      <input
                        checked={remoteDetachLocalConfirmed}
                        disabled={remoteBusy}
                        onChange={(event) =>
                          setRemoteDetachLocalConfirmed(event.target.checked)
                        }
                        type="checkbox"
                      />
                      <span>
                        我确认只删除 Silo
                        与远端环境之间的本地绑定，并永久保留孤儿审计回执。
                      </span>
                    </label>
                    <label className="remote-confirmation compact">
                      <input
                        checked={remoteDetachRiskAcknowledged}
                        disabled={remoteBusy}
                        onChange={(event) =>
                          setRemoteDetachRiskAcknowledged(event.target.checked)
                        }
                        type="checkbox"
                      />
                      <span>
                        我理解 VeriSilo
                        没有证明远端已删除；环境、卷和其他资源可能仍存在、运行并继续计费，我会自行联系运营者清理。
                      </span>
                    </label>
                    <button
                      className="button-danger"
                      disabled={
                        remoteBusy ||
                        vaultLocked ||
                        !remoteDetachLocalConfirmed ||
                        !remoteDetachRiskAcknowledged
                      }
                      onClick={() => void forceDetachRemoteBinding()}
                      type="button"
                    >
                      强制分离本地绑定（绝不声称远端已删除）
                    </button>
                  </div>
                </>
              ) : null}
              {selectedRemoteResult !== undefined ? (
                <div className="remote-result-card">
                  <div>
                    <span>最近一次已持久化结果</span>
                    <strong>
                      {environmentOperationLabel(
                        selectedRemoteResult.operation,
                      )}{" "}
                      · {remoteResultStateLabel(selectedRemoteResult.state)}
                    </strong>
                    <span>
                      Agent 最后活动：
                      {new Date(
                        selectedRemoteResult.lastActivityAtUnixMs,
                      ).toLocaleString("zh-CN")}
                    </span>
                  </div>
                  {selectedRemoteResult.state === "destroyed" &&
                  selectedRemoteBinding === undefined ? (
                    selectedRemoteResult.deletionProof !== undefined ? (
                      <div className="remote-deletion-state">
                        <strong>已认证的 Provider 删除回执已加密持久化</strong>
                        <p>
                          Agent 在固定 pin 的认证 HTTPS
                          通道中返回这份回执；客户端核对
                          Server、Silo、Binding、Remote
                          Environment、卷与临时密钥标识后才移除本地绑定。
                          这是自托管 Provider
                          的认证声明，不等同于第三方独立审计或云厂商账单证明。
                        </p>
                        <dl className="remote-evidence-facts">
                          <div>
                            <dt>Proof ID</dt>
                            <dd>
                              {selectedRemoteResult.deletionProof.proofId}
                            </dd>
                          </div>
                          <div>
                            <dt>Provider Receipt</dt>
                            <dd>
                              {
                                selectedRemoteResult.deletionProof
                                  .providerReceiptId
                              }
                            </dd>
                          </div>
                          <div>
                            <dt>Volume ID</dt>
                            <dd>
                              {selectedRemoteResult.deletionProof.volumeId}
                            </dd>
                          </div>
                          <div>
                            <dt>删除原因</dt>
                            <dd>
                              {remoteDeletionReasonLabel(
                                selectedRemoteResult.deletionProof.reason,
                              )}
                            </dd>
                          </div>
                          <div>
                            <dt>删除时间</dt>
                            <dd>
                              {new Date(
                                selectedRemoteResult.deletionProof
                                  .deletedAtUnixMs,
                              ).toLocaleString("zh-CN")}
                            </dd>
                          </div>
                        </dl>
                        <ol className="remote-resource-deletions">
                          {selectedRemoteResult.deletionProof.resourceDeletions.map(
                            (resource) => (
                              <li key={resource.kind}>
                                <strong>
                                  {remoteDeletionResourceKindLabel(
                                    resource.kind,
                                  )}
                                </strong>
                                <span>
                                  {resource.status === "deleted"
                                    ? "Provider 声明已删除"
                                    : "Provider 声明不适用"}
                                </span>
                                <code>
                                  {resource.resourceId ?? "not_applicable"}
                                </code>
                              </li>
                            ),
                          )}
                        </ol>
                      </div>
                    ) : (
                      <p className="remote-deletion-state invalid">
                        destroyed 结果缺少 typed Provider
                        删除回执；本地控制器不应接受这种响应。
                      </p>
                    )
                  ) : null}
                  {selectedRemoteResult.evidence !== undefined ? (
                    <dl className="remote-evidence-facts">
                      <div>
                        <dt>来宾证据序号</dt>
                        <dd>{selectedRemoteResult.evidence.sequence}</dd>
                      </div>
                      <div>
                        <dt>代理</dt>
                        <dd>{selectedRemoteResult.evidence.proxy.state}</dd>
                      </div>
                      <div>
                        <dt>出口</dt>
                        <dd>
                          {selectedRemoteResult.evidence.exit.state} ·{" "}
                          {selectedRemoteResult.evidence.exit.publicAddresses.join(
                            "、",
                          ) || "未报告"}
                        </dd>
                      </div>
                      <div>
                        <dt>DNS</dt>
                        <dd>
                          {selectedRemoteResult.evidence.dns.state} · 泄漏
                          {selectedRemoteResult.evidence.dns.leakDetected
                            ? "是"
                            : "否"}
                        </dd>
                      </div>
                      <div>
                        <dt>WebRTC</dt>
                        <dd>
                          {selectedRemoteResult.evidence.webRtc.state} · 泄漏
                          {selectedRemoteResult.evidence.webRtc.leakDetected
                            ? "是"
                            : "否"}
                        </dd>
                      </div>
                      <div>
                        <dt>Guest Agent</dt>
                        <dd>
                          {selectedRemoteResult.evidence.health.state} ·{" "}
                          {selectedRemoteResult.evidence.health.agentVersion}
                        </dd>
                      </div>
                    </dl>
                  ) : null}
                  {selectedRemoteResult.logs !== undefined ? (
                    <ol className="remote-log-list">
                      {selectedRemoteResult.logs.map((entry) => (
                        <li key={`${entry.sequence}-${entry.observedAtUnixMs}`}>
                          <span>{entry.level}</span>
                          <code>{entry.message}</code>
                        </li>
                      ))}
                    </ol>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}
          {remoteStatus !== null && remoteStatus.orphanReceipts.length > 0 ? (
            <section
              aria-labelledby="remote-orphan-title"
              className="remote-orphan-history"
            >
              <div>
                <strong id="remote-orphan-title">永久孤儿环境审计回执</strong>
                <p>
                  这些记录只证明本地曾强制分离绑定，不是删除证明。即使对应本地
                  Silo 已永久删除，远端环境或其他可计费资源仍可能存在并运行。
                </p>
              </div>
              <ol>
                {remoteStatus.orphanReceipts.map((receipt) => (
                  <li key={receipt.receiptId}>
                    <div className="remote-orphan-heading">
                      <strong>
                        Silo {receipt.siloId} ·{" "}
                        {new Date(receipt.detachedAtUnixMs).toLocaleString(
                          "zh-CN",
                        )}
                      </strong>
                      <span>未验证远端删除 · 可能继续计费</span>
                    </div>
                    <dl className="remote-orphan-facts">
                      <div>
                        <dt>Receipt ID</dt>
                        <dd>{receipt.receiptId}</dd>
                      </div>
                      <div>
                        <dt>Binding ID</dt>
                        <dd>{receipt.bindingId}</dd>
                      </div>
                      <div>
                        <dt>Remote Environment ID</dt>
                        <dd>{receipt.remoteEnvironmentId}</dd>
                      </div>
                      <div>
                        <dt>Server ID</dt>
                        <dd>{receipt.serverId}</dd>
                      </div>
                      <div>
                        <dt>Endpoint</dt>
                        <dd>{receipt.endpoint.origin}</dd>
                      </div>
                      <div>
                        <dt>当时固定的 pin</dt>
                        <dd>
                          {receipt.endpoint.pin.kind} ·{" "}
                          {receipt.endpoint.pin.sha256}
                        </dd>
                      </div>
                    </dl>
                    <p className="remote-orphan-warning">
                      VeriSilo
                      未向该远端发送或验证销毁请求；请使用以上身份联系自托管运营者核查并停止后续费用。
                    </p>
                  </li>
                ))}
              </ol>
            </section>
          ) : null}
          {remoteActionMessage !== null ? (
            <p
              className={`environment-action-message ${remoteActionMessage.tone}`}
              role={remoteActionMessage.tone === "error" ? "alert" : "status"}
            >
              {remoteActionMessage.text}
            </p>
          ) : null}
        </div>
      </section>

      <section className="panel capability-panel">
        <div className="panel-heading">
          <div>
            <p className="eyebrow">逐项边界</p>
            <h2>你关心的参数，分别在哪一层实现</h2>
            <p>
              “计划实现”不是“现在可用”。每一项都同时说明当前事实、目标层级和以后怎样验收。
            </p>
          </div>
        </div>
        <div className="desktop-capability-table">
          {PRODUCT_CAPABILITIES.map((capability) => (
            <article className="capability-row" key={capability.id}>
              <div className="capability-name">
                <strong>{capability.name}</strong>
                <CapabilityBadge
                  label={capability.routeLabel}
                  tone={capability.tone}
                />
              </div>
              <p>{capability.currentReality}</p>
              <div className="evidence-rule">
                <span>验收证据</span>
                <p>{capability.evidenceRule}</p>
              </div>
            </article>
          ))}
        </div>
      </section>

      <section className="panel provider-readiness">
        <div>
          <p className="eyebrow">WSL 明确选择</p>
          <h2>先发现发行版，再核对固定 Guest Agent 与 WSLg</h2>
          <p>
            第一次检查只读取状态和发行版；随后只用固定 /usr/bin/test 核对 Guest
            Agent、Chromium 与 WSLg 路径，不接收任意 Linux 命令。
          </p>
          {wslStatus !== null ? (
            <div className="provider-result">
              <strong>{wslStatus.available ? "发现 WSL" : "尚不可用"}</strong>
              <span>{wslStatus.message}</span>
              {wslStatus.distributions.length > 0 ? (
                <label>
                  显式选择发行版
                  <select
                    disabled={wslBusy}
                    onChange={(event) =>
                      setSelectedWslDistribution(event.target.value)
                    }
                    value={selectedWslDistribution}
                  >
                    <option value="">请选择（不会默认第一项）</option>
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
                配置所选发行版
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
          {wslBusy ? "正在检查…" : "仅发现本机 WSL"}
        </button>
      </section>

      <section className="panel truth-panel">
        <div className="truth-icon" aria-hidden="true">
          i
        </div>
        <div>
          <h2>这条路线不等于“反检测”承诺</h2>
          <p>
            VeriSilo
            会参考成熟产品的环境分层、配置模板和远程会话思路，但不会照搬营销结论。
            即使进入专用引擎或
            VM，行为、账号关系、服务端历史和真实业务风控仍可能把会话关联起来。
          </p>
        </div>
      </section>
    </>
  );
}

function CapabilityBadge({
  label,
  tone,
}: {
  label: string;
  tone: CapabilityTone;
}) {
  return <span className={`capability-badge ${tone}`}>{label}</span>;
}

function engineAdapterLabel(
  adapter: EngineAdapterStatus["descriptor"]["id"],
): string {
  switch (adapter) {
    case "stock-chrome":
      return "Stock Google Chrome";
    case "stock-edge":
      return "Stock Microsoft Edge";
    case "controlled-chromium":
      return "受控 Chromium（实验）";
    case "camoufox":
      return "Camoufox（可选实验）";
  }
}

function siloEngineLabel(engine: SiloEngineConfig): string {
  switch (engine.adapter) {
    case "stock":
      return "Stock Chrome / Edge（旧 Silo 默认）";
    case "controlled-chromium":
      return `受控 Chromium · Template ${engine.identityTemplate.templateId}`;
    case "camoufox":
      return `Camoufox · Template ${engine.identityTemplate.templateId}`;
  }
}

function engineHealthLabel(
  state: EngineAdapterStatus["health"]["state"],
): string {
  switch (state) {
    case "healthy":
      return "健康";
    case "degraded":
      return "部分就绪";
    case "unavailable":
      return "不可用";
    case "emergency_disabled":
      return "紧急停用";
  }
}

function environmentBackendLabel(
  backend: EnvironmentBackendStatus["backend"],
): string {
  switch (backend) {
    case "wsl-chromium":
      return "WSL Chromium";
    case "windows-sandbox":
      return "Windows Sandbox";
    case "hyper-v":
      return "Hyper-V 持久 VM";
  }
}

function environmentPrerequisiteStateLabel(
  state: EnvironmentBackendStatus["prerequisites"][number]["state"],
): string {
  switch (state) {
    case "configured":
      return "已配置";
    case "guest_observed":
      return "来宾观测";
    case "verified":
      return "已验证";
    case "missing":
      return "缺失";
    case "unavailable":
      return "不可用";
    case "unknown":
      return "未知";
  }
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
      return "明确销毁";
    case "configureNetwork":
      return "配置网络/收集证据";
    case "health":
      return "检查健康";
    case "logs":
      return "导出有界日志";
  }
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
      return "凭据可用";
    case "credential_expired":
      return "凭据已过期";
    case "revoked":
      return "凭据已撤销";
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
      destroyed: "已销毁",
      network_configured: "网络已配置并报告证据",
      healthy: "健康检查完成",
      logs_returned: "日志已返回",
      blocked: "已阻止",
    } satisfies Record<
      RemoteEnvironmentStatus["lastResults"][number]["state"],
      string
    >
  )[state];
}

function remoteInteractionOperationLabel(
  operation: RemoteAgentControlOperation,
): string {
  return (
    {
      open_human_session: "开启人类会话",
      close_human_session: "关闭人类会话",
      grant_automation: "授予自动化",
      revoke_automation: "撤销自动化",
      open_screen: "请求屏幕通道",
      send_input: "发送输入",
    } satisfies Record<RemoteAgentControlOperation, string>
  )[operation];
}

function remoteDeletionResourceKindLabel(
  kind: NonNullable<
    RemoteEnvironmentStatus["lastResults"][number]["deletionProof"]
  >["resourceDeletions"][number]["kind"],
): string {
  return (
    {
      compute_instance: "Compute instance",
      persistent_volume: "Persistent volume",
      snapshot: "Snapshot",
      ephemeral_key: "Ephemeral key",
    } satisfies Record<typeof kind, string>
  )[kind];
}

function remoteDeletionReasonLabel(
  reason: NonNullable<
    RemoteEnvironmentStatus["lastResults"][number]["deletionProof"]
  >["reason"],
): string {
  return (
    {
      user_confirmed: "用户确认的新删除",
      ttl_expired: "TTL 到期自动清理",
      provider_policy: "Provider 策略清理",
    } satisfies Record<typeof reason, string>
  )[reason];
}

function remoteAgentResponseLabel(type: RemoteAgentResponse["type"]): string {
  return (
    {
      environment: "环境记录",
      deleted: "Provider 删除回执",
      human_session: "人类会话授权",
      automation: "自动化授权",
      screen: "屏幕通道元数据",
      input_accepted: "输入已接受",
      logs: "Agent 日志",
    } satisfies Record<RemoteAgentResponse["type"], string>
  )[type];
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

function networkProviderLabel(
  provider: RuntimeNetworkEvidence["provider"],
): string {
  return {
    direct: "直连",
    fixed_proxy: "固定代理",
    external_mihomo: "外部 Mihomo",
    pac: "PAC",
  }[provider];
}

function evidenceStateLabel(
  state: RuntimeNetworkEvidence["configuration"],
): string {
  return {
    not_applicable: "不适用",
    not_requested: "尚未验证",
    configured: "已配置",
    reachable: "端点可达",
    applied: "已应用",
    observed: "扩展声明已观测",
    verified: "已验证",
    failed: "失败",
    unavailable: "当前不可验证",
  }[state];
}

function evidenceTone(state: RuntimeNetworkEvidence["configuration"]): string {
  if (["reachable", "applied", "observed", "verified"].includes(state)) {
    return "good";
  }
  if (["failed", "unavailable"].includes(state)) {
    return "warn";
  }
  return "neutral";
}

function safeguardLabel(safeguard: string): string {
  return (
    {
      no_direct_fallback: "无 DIRECT 回退",
      browser_dns_through_proxy: "浏览器域名交给代理",
      quic_disabled: "QUIC 已禁用",
      non_proxied_webrtc_udp_disabled: "非代理 WebRTC UDP 已禁用",
    }[safeguard] ?? safeguard
  );
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
