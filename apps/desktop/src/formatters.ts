import type {
  NetworkProfile,
  RuntimeActivation,
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
    idle: "现在没有打开的浏览器",
    preflight: "正在准备打开",
    launching: "正在打开浏览器",
    running: "浏览器正在运行",
    verification_failed: "这次运行已经结束。请点「结束会话」，然后再打开浏览器。",
    recovery_required: "上次浏览还没完全结束。请关掉残留窗口后再试。",
    stopped: "浏览器已停止",
    failed: "浏览器没有打开成功",
  };

  return labels[activation.state];
}

export function activationStatusLabel(
  state: RuntimeActivation["state"],
): string {
  const labels: Record<RuntimeActivation["state"], string> = {
    idle: "空闲",
    preflight: "启动中",
    launching: "启动中",
    running: "运行中",
    verification_failed: "已结束",
    recovery_required: "需要确认",
    stopped: "空闲",
    failed: "启动失败",
  };
  return labels[state];
}

export function describeNetwork(profile: NetworkProfile): string {
  switch (profile.mode) {
    case "direct":
      return "直连，不走代理";
    case "fixed_proxy":
      return profile.externalMihomo === undefined
        ? `${profile.scheme}://${profile.host}:${profile.port}${profile.proxyRequired ? "（必须走代理）" : ""}`
        : `Silo 专属代理 · 本机 Clash「${profile.externalMihomo.nodeName}」`;
    case "pac":
      return `PAC：${profile.pacUrl}${profile.proxyRequired ? "（必须代理）" : ""}`;
  }
}
