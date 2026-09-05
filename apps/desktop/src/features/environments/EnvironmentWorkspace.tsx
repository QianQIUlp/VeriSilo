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
  unboundEnvironmentControlsAvailable,
} from "../../shared/presentation.js";

import { UserFacingError } from "../../user-errors.js";

export function EnvironmentWorkspace({
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

type EnvironmentSection = "browser" | "local" | "remote";
