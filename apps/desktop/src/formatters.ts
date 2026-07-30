import type {
  EngineCapabilityOperation,
  EngineControlPhaseReceipt,
  NetworkProfile,
  RuntimeActivation,
  RuntimeEngineEvidence,
  SiteFallbackReceipt,
  VaultState,
} from "@verisilo/contracts";

export function describeVault(state: VaultState): string {
  switch (state.state) {
    case "uninitialized":
      return "尚未创建本地保险库";
    case "locked":
      return "保险库已锁定";
    case "unlocked":
      return state.autoLockAt === null
        ? "保险库已解锁"
        : `保险库已解锁，将在 ${new Date(state.autoLockAt).toLocaleTimeString()} 自动锁定`;
  }
}

export function describeActivation(activation: RuntimeActivation): string {
  const labels: Record<RuntimeActivation["state"], string> = {
    idle: "没有运行中的 Silo",
    preflight: "正在进行启动前检查",
    launching: "正在启动浏览器",
    running: "Silo 正在运行",
    verification_failed: "运行网络路径已 fail-closed 阻断；旧端口不会自动恢复",
    recovery_required: "需要核对上次浏览器会话",
    stopped: "Silo 已停止",
    failed: "Silo 启动失败",
  };

  return activation.message ?? labels[activation.state];
}

export function describeEngineCapabilityOperation(
  operation: EngineCapabilityOperation,
): string {
  const labels: Record<EngineCapabilityOperation, string> = {
    not_configured: "未配置",
    configured: "已配置",
    applied: "已应用",
    verified: "已按收据核验",
    failed: "核验失败",
  };
  return labels[operation];
}

export function describeRuntimeEngineReceipts(
  evidence: RuntimeEngineEvidence,
): string {
  const phases = evidence.phaseReceipts
    .map((receipt) => receipt.phase)
    .join(" → ");
  return `运行收据：${evidence.runtimeReceipts}；阶段：${phases || "无"}；站点回退：${evidence.fallbackReceipts.length} 条；Restore：${evidence.restoreReceipt}`;
}

export function describeEnginePhaseReceipt(
  receipt: EngineControlPhaseReceipt,
): string {
  const capabilities = receipt.capabilities
    .map((capability) => `${capability.id} [${capability.evidence.join("；")}]`)
    .join("；");
  return `${receipt.phase} · ${new Date(receipt.recordedAt).toLocaleString()} · ${capabilities || "无目标能力"}`;
}

export function describeSiteFallbackReceipt(
  receipt: SiteFallbackReceipt,
): string {
  const capabilities = receipt.capabilities
    .map((capability) => `${capability.id} [${capability.evidence.join("；")}]`)
    .join("；");
  return `${receipt.site} ↔ ${receipt.matchedPattern} (${receipt.action}) · ${new Date(receipt.restoredAt).toLocaleString()} · ${capabilities}`;
}

export function describeNetwork(profile: NetworkProfile): string {
  switch (profile.mode) {
    case "direct":
      return "直连（不使用系统代理）";
    case "fixed_proxy":
      return profile.externalMihomo === undefined
        ? `${profile.scheme}://${profile.host}:${profile.port}${profile.proxyRequired ? "（必须代理）" : ""}`
        : `Mihomo「${profile.externalMihomo.nodeName}」· ${profile.host}:${profile.port}（必须代理）`;
    case "pac":
      return `PAC：${profile.pacUrl}${profile.proxyRequired ? "（必须代理）" : ""}`;
  }
}
