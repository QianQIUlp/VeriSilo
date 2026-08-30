export class UserFacingError extends Error {
  override readonly name = "UserFacingError";
}

const stableBackendErrors = {
  managed_engine_unavailable:
    "托管浏览器组件不可用或未通过验证。请重新检查内置组件。",
  managed_identity_generation_failed:
    "身份配置生成失败。原有 Silo 数据没有改变，请检查网络后重试。",
  managed_artifact_unavailable:
    "已保存的身份配置不可用或已损坏。请恢复 Vault 备份或删除后重建该 Silo。",
  managed_network_mismatch:
    "代理不可达或网络结果与配置不匹配；已阻止启动且不会回退直连。",
  managed_another_silo_running:
    "当前已有一个 Silo 正在运行。请先停止它，再启动此 Silo。",
  managed_profile_in_use:
    "此 Silo 的浏览器数据正在使用中。请关闭对应浏览器后重试。",
  managed_runtime_recovery_required:
    "浏览器运行状态需要恢复核对。请先关闭残留浏览器，再重新检查状态。",
  managed_identity_preset_invalid: "身份预设不可用，请重新选择后重试。",
  managed_proxy_required:
    "托管身份浏览器必须使用 Direct，或完整配置 required HTTP / SOCKS5 代理。",
  managed_proxy_invalid: "代理地址或端口无效，请检查后重试。",
  managed_silo_active: "请先停止正在运行的 Silo，再修改托管配置。",
  managed_active_silo_limit: "当前已有一个 Silo 正在运行。请先停止它，再继续。",
  managed_create_failed: "托管身份浏览器创建失败，请检查设置后重试。",
} as const;

export function userFacingErrorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  const raw =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "";
  const normalized = raw.toLowerCase().replaceAll("-", "_");
  const code = (
    Object.keys(stableBackendErrors) as Array<keyof typeof stableBackendErrors>
  ).find((candidate) => normalized.includes(candidate));
  if (code !== undefined) {
    return stableBackendErrors[code];
  }
  return error instanceof UserFacingError ? error.message : fallback;
}
