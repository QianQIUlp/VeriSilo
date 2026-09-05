import {
  type EnvironmentBackendId,
  type EnvironmentBackendStatus,
  type EnvironmentNetworkProfile,
  type EnvironmentOperation,
  type NetworkCheckResult,
  type Silo,
} from "@verisilo/contracts";

import {
  type BrowserVerification,
  type DesktopStatus,
  type EngineAdapterStatus,
  type ManagedIdentityPreset,
  type ManagedIdentityPreview,
  type RemoteEnvironmentStatus,
} from "../desktop-api.js";

export function unboundEnvironmentControlsAvailable(): boolean {
  return false;
}

export function legacyEnvironmentLabel(backend: EnvironmentBackendId): string {
  switch (backend) {
    case "wsl-chromium":
      return "Linux 环境";
    case "windows-sandbox":
      return "Windows 临时环境";
    case "hyper-v":
      return "虚拟机环境";
  }
}

export function engineAdapterLabel(
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

export function browserVerificationMessage(
  verification: BrowserVerification,
): string {
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

export function activationNoticeTone(
  activation: DesktopStatus["activation"],
): "error" | "success" | "info" {
  if (activation.state === "running") {
    return "success";
  }
  return ["failed", "verification_failed"].includes(activation.state)
    ? "error"
    : "info";
}

export function activationStatusTone(
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

export function localePresetFromPreview(
  preview: ManagedIdentityPreview,
): ManagedIdentityPreset {
  if (preview.language.startsWith("zh")) {
    return "balanced-zh-cn";
  }
  if (preview.language.startsWith("de")) {
    return "balanced-de-de";
  }
  return "balanced-en-us";
}

export function siloBrowserLabel(silo: Silo): string {
  switch (silo.engine.adapter) {
    case "stock":
      return silo.browser?.kind === "chrome"
        ? "Google Chrome"
        : "Microsoft Edge";
    case "controlled-chromium":
      return "独立 Chromium";
    case "camoufox":
      return "独立 Firefox";
  }
}

export function siloExecutionTargetLabel(silo: Silo): string {
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

export function siloWebsiteIdentityBoundary(
  silo: Silo,
  preview?: ManagedIdentityPreview,
): string {
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
    if (preview !== undefined) {
      return [
        preview.language,
        preview.timezone,
        `${preview.screenWidth}×${preview.screenHeight}`,
        `${preview.hardwareConcurrency} 核`,
        preview.countryCode ?? (preview.networkBound ? "跟随出口" : "直连"),
      ].join(" · ");
    }
    return "已保存托管身份；打开详情可查看指纹";
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

export function engineHealthLabel(
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

export function engineHealthDescription(
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

export function environmentBackendLabel(
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

export function environmentPrerequisiteStateLabel(
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

export function environmentPrerequisiteLabel(id: string): string {
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

export function environmentOperationLabel(
  operation: EnvironmentOperation,
): string {
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

export function isUserEnvironmentOperation(
  operation: EnvironmentOperation,
): boolean {
  return operation !== "logs";
}

export function environmentNetworkForSilo(
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

export function remoteStateLabel(
  state: RemoteEnvironmentStatus["state"],
): string {
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

export function remoteResultStateLabel(
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

export function networkLocation(result: NetworkCheckResult): string {
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

export function networkOwner(result: NetworkCheckResult): string {
  if (result.ip === null) {
    return "未知";
  }
  return (
    [result.ip.asn, result.ip.organization ?? result.ip.isp]
      .filter((part): part is string => part !== null)
      .join(" · ") || "未知"
  );
}

export function dnsStateLabel(result: NetworkCheckResult): string {
  const labels: Record<NetworkCheckResult["dns"]["state"], string> = {
    consistent: "两家公共 DNS 结果一致",
    different: "两家公共 DNS 结果有差异",
    resolver_error: "公共 DNS 返回错误",
    partial: "仅一家公共 DNS 可用",
    failed: "公共 DNS 检查失败",
  };
  return labels[result.dns.state];
}

export function formatDate(isoDate: string): string {
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

export function formatBytes(bytes: number): string {
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

export function formatMicrosCurrency(micros: number, currency: string): string {
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

export function formatStorageSuffix(bytes: number | null | undefined): string {
  return bytes === undefined || bytes === null
    ? " · 大小暂不可用"
    : ` · ${formatBytes(bytes)}`;
}
