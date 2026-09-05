import { userFacingErrorMessage } from "../user-errors.js";

export type Notice = {
  tone: "error" | "success" | "info";
  message: string;
} | null;

export function errorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  return userFacingErrorMessage(error, fallback);
}

export function managedErrorMessage(error: unknown): string {
  return errorMessage(
    error,
    "托管身份浏览器操作没有完成。请检查当前状态后重试。",
  );
}
