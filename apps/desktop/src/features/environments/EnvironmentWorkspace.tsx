import {
  type EnvironmentBackendStatus,
  type EnvironmentOperation,
  type EnvironmentOperationRequest,
  type RemoteEndpoint,
  type RemoteNetworkPolicy,
  type Silo,
} from "@verisilo/contracts";

import { useEffect, useRef, useState } from "react";

import {
  desktopApi,
  type EngineAdapterStatus,
  type RemoteEnvironmentStatus,
  type RemoteInteractivePrincipal,
  type WslStatus,
} from "../../desktop-api.js";

import { errorMessage } from "../../shared/notice.js";

import {
  canConfigureWslDistribution,
  requiresExplicitWslSelection,
} from "../../wsl-selection.js";

import {
  engineAdapterLabel,
  engineHealthDescription,
  engineHealthLabel,
  environmentBackendLabel,
  environmentNetworkForSilo,
  environmentOperationLabel,
  environmentPrerequisiteLabel,
  environmentPrerequisiteStateLabel,
  formatMicrosCurrency,
  isUserEnvironmentOperation,
  remoteResultStateLabel,
  remoteStateLabel,
} from "../../shared/presentation.js";

import { userFacingErrorDetail } from "../../user-errors.js";

import { InstrumentObject } from "../../shared/LivingWorkspace.js";

import { UserFacingError } from "../../user-errors.js";

export function EnvironmentWorkspace({
  silos,
  vaultLocked,
}: {
  silos: Silo[];
  vaultLocked: boolean;
}) {
  const [locationOpen, setLocationOpen] = useState(false);
  const [inventoryLoading, setInventoryLoading] = useState(!vaultLocked);
  const [inventoryRetry, setInventoryRetry] = useState(0);
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
    detail?: string;
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
    detail?: string;
  } | null>(null);
  const [selectedEnvironmentBackend, setSelectedEnvironmentBackend] =
    useState<EnvironmentBackendStatus["backend"]>("wsl-chromium");
  const [selectedEnvironmentSilo, setSelectedEnvironmentSilo] = useState("");
  const visibleEngineStatuses = engineStatuses;

  useEffect(() => {
    if (vaultLocked) {
      setEngineStatuses([]);
      setEnvironmentStatuses([]);
      setRemoteStatus(null);
      setTechnologyError(null);
      return;
    }
    let current = true;
    setInventoryLoading(true);
    void Promise.allSettled([
      desktopApi.listEngineAdapters(),
      desktopApi.environmentBackendStatuses(),
      desktopApi.remoteEnvironmentStatus(),
    ]).then(([engines, environments, remote]) => {
      if (!current) return;
      setEngineStatuses(engines.status === "fulfilled" ? engines.value : []);
      setEnvironmentStatuses(
        environments.status === "fulfilled" ? environments.value : [],
      );
      if (remote.status === "fulfilled") {
        setRemoteStatus(remote.value);
        if (remote.value.endpoint !== null) {
          setRemoteOrigin(remote.value.endpoint.origin);
          setRemotePinKind(remote.value.endpoint.pin.kind);
          setRemotePinSha256(remote.value.endpoint.pin.sha256);
        }
      }
      setTechnologyError(
        [engines, environments, remote].some(
          (result) => result.status === "rejected",
        )
          ? "部分运行环境的状态暂时无法读取，请稍后重试。"
          : null,
      );
      setInventoryLoading(false);
    });
    return () => {
      current = false;
    };
  }, [vaultLocked, inventoryRetry]);

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
      const detail = userFacingErrorDetail(
        error,
        "填写内容有误，请检查服务地址和安全指纹。",
      );
      setRemoteValidation({
        tone: "error",
        text: "填写内容有误，请检查服务地址和安全指纹。",
        ...(detail === null ? {} : { detail }),
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
      const detail = userFacingErrorDetail(
        error,
        "连接没有完成。请检查服务地址、安全指纹和配对码后重试。",
      );
      setRemoteActionMessage({
        tone: "error",
        text: "连接没有完成。请检查服务地址、安全指纹和配对码后重试。",
        ...(detail === null ? {} : { detail }),
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
    <section
      className={`location-observatory living-page${locationOpen ? " location-open" : ""}`}
    >
      <header className="living-heading">
        <p className="eyebrow">04 / EXECUTION MAP</p>
        <h1>每个身份，都有落点。</h1>
        <p>在地图里查看浏览器与可选运行位置，进入后准备或修复。</p>
      </header>
      {technologyError !== null && (
        <div className="location-read-error" role="alert">
          {technologyError}
          <button
            type="button"
            className="button-secondary"
            disabled={inventoryLoading}
            onClick={() => setInventoryRetry((value) => value + 1)}
          >
            重新读取状态
          </button>
        </div>
      )}
      <nav className="location-map" aria-label="运行位置设置类别">
        <div className="map-connection" aria-hidden="true" />
        <button
          className="map-place place-browser"
          aria-pressed={environmentSection === "browser" && locationOpen}
          aria-expanded={environmentSection === "browser" && locationOpen}
          aria-controls="location-console"
          onClick={() => {
            setEnvironmentSection("browser");
            setLocationOpen(true);
          }}
          type="button"
        >
          <InstrumentObject
            kind="browser"
            active={environmentSection === "browser" && locationOpen}
          />
          <span className="place-index">01 / WINDOWS</span>
          <strong>浏览器准备</strong>
          <small>
            {vaultLocked
              ? "解锁后读取浏览器状态"
              : inventoryLoading
                ? "正在读取…"
                : `${visibleEngineStatuses.length} 个引擎状态`}
          </small>
          <i aria-hidden="true">进入 ↗</i>
        </button>
        <button
          className="map-place place-linux"
          aria-pressed={environmentSection === "local" && locationOpen}
          aria-expanded={environmentSection === "local" && locationOpen}
          aria-controls="location-console"
          onClick={() => {
            setEnvironmentSection("local");
            setLocationOpen(true);
          }}
          type="button"
        >
          <InstrumentObject
            kind="linux"
            active={environmentSection === "local" && locationOpen}
          />
          <span className="place-index">02 / LOCAL LINUX</span>
          <strong>Linux 环境</strong>
          <small>
            {wslStatus === null
              ? "进入后检查本机 WSL"
              : wslStatus.available
                ? `${wslStatus.distributions.length} 个发行版`
                : "尚不可用"}
          </small>
          <i aria-hidden="true">进入 ↗</i>
        </button>
        {(remoteStatus?.bindings.length ?? 0) > 0 && (
          <button
            className="map-place place-remote"
            aria-pressed={environmentSection === "remote" && locationOpen}
            aria-expanded={environmentSection === "remote" && locationOpen}
            aria-controls="location-console"
            onClick={() => {
              setEnvironmentSection("remote");
              setLocationOpen(true);
            }}
            type="button"
          >
            <InstrumentObject
              kind="remote"
              active={environmentSection === "remote" && locationOpen}
            />
            <span className="place-index">03 / EXISTING REMOTE</span>
            <strong>旧远程环境</strong>
            <small>{remoteStatus?.bindings.length} 个已有连接</small>
            <i aria-hidden="true">进入 ↗</i>
          </button>
        )}
      </nav>
      {!locationOpen && (
        <p className="map-boundary">
          选择一个落点进入查看。Silo 的运行位置仍在创建时选择。
        </p>
      )}
      <div
        id="location-console"
        className="location-console"
        hidden={!locationOpen}
      >
        <div className="location-console-bar">
          <span>
            {environmentSection === "browser"
              ? "WINDOWS / 浏览器准备"
              : environmentSection === "local"
                ? "LOCAL / Linux 环境"
                : "REMOTE / 已有远程环境"}
          </span>
          <button
            type="button"
            className="button-secondary"
            onClick={() => setLocationOpen(false)}
          >
            返回地图 ↗
          </button>
        </div>
        <div className="location-console-body" key={environmentSection}>
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
                      <strong>
                        {engineAdapterLabel(engine.descriptor.id)}
                      </strong>
                      <span
                        className={`provider-health ${engine.health.state}`}
                      >
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
                      Force Detach
                      只移除这台电脑上的连接记录，不会删除远程环境。
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
                  role={
                    remoteActionMessage.tone === "error" ? "alert" : "status"
                  }
                >
                  {remoteActionMessage.text}
                  {remoteActionMessage.detail !== undefined ? (
                    <span className="error-detail">
                      {remoteActionMessage.detail}
                    </span>
                  ) : null}
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
                    <strong>
                      {wslStatus.available ? "发现 WSL" : "尚不可用"}
                    </strong>
                    <span>
                      {wslStatus.available
                        ? `发现 ${wslStatus.distributions.length} 个可选发行版。`
                        : "请先在 Windows 中安装并启用 WSL。"}
                    </span>
                    {wslStatus.distributions.length > 0 ? (
                      <fieldset className="distribution-picker">
                        <legend>Linux 发行版</legend>
                        {wslStatus.distributions.map((distribution) => (
                          <label
                            className="distribution-option"
                            key={distribution}
                          >
                            <input
                              type="radio"
                              name="workspace-wsl-distribution"
                              disabled={wslBusy}
                              checked={selectedWslDistribution === distribution}
                              onChange={() =>
                                setSelectedWslDistribution(distribution)
                              }
                              value={distribution}
                            />
                            <span aria-hidden="true">◇</span>
                            <strong>{distribution}</strong>
                            <small>
                              {selectedWslDistribution === distribution
                                ? "已选择 · 等待应用"
                                : "选择此落点"}
                            </small>
                          </label>
                        ))}
                      </fieldset>
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
        </div>
      </div>
    </section>
  );
}

type EnvironmentSection = "browser" | "local" | "remote";
