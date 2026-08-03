export class UserFacingError extends Error {
  override readonly name = "UserFacingError";
}

export function userFacingErrorMessage(
  error: unknown,
  fallback = "操作没有完成。请检查当前设置后重试。",
): string {
  return error instanceof UserFacingError ? error.message : fallback;
}
