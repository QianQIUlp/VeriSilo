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
    idle: "没有运行中的 Silo",
    preflight: "正在进行启动前检查",
    launching: "正在启动浏览器",
    running: "Silo 正在运行",
    stopped: "Silo 已停止",
    failed: "Silo 启动失败",
  };

  return activation.message ?? labels[activation.state];
}

export function describeNetwork(profile: NetworkProfile): string {
  switch (profile.mode) {
    case "direct":
      return "直连（不使用系统代理）";
    case "fixed_proxy":
      return `${profile.scheme}://${profile.host}:${profile.port}${profile.proxyRequired ? "（必须代理）" : ""}`;
    case "pac":
      return `PAC：${profile.pacUrl}${profile.proxyRequired ? "（必须代理）" : ""}`;
  }
}
