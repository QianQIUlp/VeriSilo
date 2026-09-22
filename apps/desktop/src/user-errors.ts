export class UserFacingError extends Error {
  override readonly name = "UserFacingError";
  /** Optional secondary detail line shown smaller and muted under the message. */
  readonly detail: string | null;

  constructor(message: string, detail?: string) {
    super(message);
    this.detail = detail ?? null;
  }
}

/** Backend detail lines are capped so a runaway native message cannot flood the UI. */
const errorDetailLimit = 300;

export function truncateErrorDetail(text: string): string {
  const trimmed = text.trim();
  if (trimmed.length <= errorDetailLimit) {
    return trimmed;
  }
  return `${trimmed.slice(0, errorDetailLimit)}…`;
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
    "已经有一个浏览器在运行。请先停掉它，再打开这一个。",
  managed_profile_in_use:
    "这个浏览器的数据正在使用。请先关掉对应窗口再试。",
  managed_runtime_recovery_required:
    "浏览器运行状态需要恢复核对。请先关闭残留浏览器，再重新检查状态。",
  managed_browser_open_failed: "浏览器没有打开成功，请再试一次。",
  managed_identity_preset_invalid: "身份设置不可用，请重新选择后重试。",
  managed_identity_locked: "这个身份已在首次启动后锁定。如需另一套指纹，请创建新的浏览器空间。",
  managed_proxy_required:
    "托管身份浏览器必须使用 Direct，或完整配置 required HTTP / SOCKS5 代理。",
  managed_proxy_invalid: "代理地址或端口无效，请检查后重试。",
  managed_silo_active: "请先停掉正在运行的浏览器，再改这个身份。",
  managed_active_silo_limit: "已经有一个浏览器在运行。请先停掉它，再继续。",
  managed_create_failed: "托管身份浏览器创建失败，请检查设置后重试。",
} as const;

function structuredBackendError(
  error: unknown,
): { code: keyof typeof stableBackendErrors; detail: string | null } | null {
  if (typeof error !== "object" || error === null || !("code" in error)) {
    return null;
  }
  const { code, detail } = error as { code: unknown; detail?: unknown };
  if (typeof code !== "string" || !(code in stableBackendErrors)) {
    return null;
  }
  return {
    code: code as keyof typeof stableBackendErrors,
    detail: typeof detail === "string" ? detail : null,
  };
}

function rawErrorText(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "object" && error !== null) {
    const record = error as { message?: unknown; error?: unknown };
    if (typeof record.message === "string") {
      return record.message;
    }
    if (typeof record.error === "string") {
      return record.error;
    }
  }
  return "";
}

function isProductNativeMessage(raw: string): boolean {
  return (
    raw.startsWith("Mihomo") ||
    raw.startsWith("无法连接本机 Mihomo") ||
    raw.startsWith("本机 Mihomo") ||
    raw.startsWith("本机没有可用的 Clash") ||
    raw.startsWith("无法绑定所选") ||
    raw.startsWith("代理启动前") ||
    raw.startsWith("无法启动禁止直连") ||
    raw.startsWith("无法启动或验证本机代理") ||
    raw.startsWith("本机代理中继") ||
    raw.startsWith("网络配置无效") ||
    raw.startsWith("Clash ") ||
    raw.startsWith("身份") ||
    raw.startsWith("保险库") ||
    raw.startsWith("无法通过当前代理") ||
    raw.startsWith("当前代理出口") ||
    raw.startsWith("内置浏览器") ||
    raw.startsWith("浏览器启动") ||
    raw.startsWith("浏览器引擎") ||
    raw.startsWith("受控引擎") ||
    raw.startsWith("无法启动所选浏览器") ||
    raw.includes("是 Clash 给浏览器走流量的代理端口") ||
    raw.includes("读取代理组") ||
    raw.includes("内核管道")
  );
}

export function userFacingErrorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  const structured = structuredBackendError(error);
  if (structured !== null) {
    return stableBackendErrors[structured.code];
  }
  const raw = rawErrorText(error);
  const normalized = raw.toLowerCase().replaceAll("-", "_");
  const code = (
    Object.keys(stableBackendErrors) as Array<keyof typeof stableBackendErrors>
  ).find((candidate) => normalized.includes(candidate));
  if (code !== undefined) {
    return stableBackendErrors[code];
  }
  if (error instanceof UserFacingError) {
    return error.message;
  }
  if (isProductNativeMessage(raw)) {
    return raw;
  }
  return fallback;
}

/**
 * Secondary detail for an error that was mapped to a curated fallback: the raw
 * backend message stays visible instead of being silently dropped. Returns
 * null when the primary text already explains the failure (matched code,
 * product-native copy, or an explicit UserFacingError), or when the error
 * carries no usable text.
 */
export function userFacingErrorDetail(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string | null {
  if (error instanceof UserFacingError && error.detail !== null) {
    return error.detail;
  }
  const structured = structuredBackendError(error);
  if (structured !== null) {
    return structured.detail === null || structured.detail.trim() === ""
      ? null
      : truncateErrorDetail(structured.detail);
  }
  if (userFacingErrorMessage(error, fallback) !== fallback) {
    return null;
  }
  const raw = rawErrorText(error);
  if (raw.trim() === "") {
    return null;
  }
  const detail = truncateErrorDetail(raw);
  return detail === fallback ? null : detail;
}
