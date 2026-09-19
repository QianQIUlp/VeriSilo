import {
  UserFacingError,
  userFacingErrorDetail,
  userFacingErrorMessage,
} from "../user-errors.js";

export type Notice = {
  tone: "error" | "success" | "info";
  message: string;
  /** Secondary detail line (smaller, muted) shown under the primary message. */
  detail?: string;
} | null;

export function errorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  return userFacingErrorMessage(error, fallback);
}

export type ErrorNotice = { tone: "error"; message: string; detail?: string };

export function errorNotice(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): ErrorNotice {
  const notice: ErrorNotice = {
    tone: "error",
    message: userFacingErrorMessage(error, fallback),
  };
  const detail = userFacingErrorDetail(error, fallback);
  if (detail !== null) {
    notice.detail = detail;
  }
  return notice;
}

export function managedErrorMessage(error: unknown): string {
  return errorMessage(
    error,
    "托管身份浏览器操作没有完成。请检查当前状态后重试。",
  );
}

/** Attaches a secondary detail line to an explicitly composed primary message. */
export function withErrorDetail(
  message: string,
  error: unknown,
): { message: string; detail?: string } {
  if (error instanceof UserFacingError && error.detail !== null) {
    return { message, detail: error.detail };
  }
  const detail = userFacingErrorDetail(error);
  return detail === null || detail === message
    ? { message }
    : { message, detail };
}
