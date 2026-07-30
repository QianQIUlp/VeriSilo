export const NATIVE_MESSAGE_TIMEOUT_MS = 10_000;

export async function sendNativeMessageWithTimeout(
  hostName: string,
  message: Record<string, unknown>,
  timeoutMs = NATIVE_MESSAGE_TIMEOUT_MS,
): Promise<unknown> {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      chrome.runtime.sendNativeMessage(hostName, message),
      new Promise<never>((_resolve, reject) => {
        timeout = setTimeout(
          () => reject(new Error("VeriSilo Native Host timed out.")),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timeout !== undefined) {
      clearTimeout(timeout);
    }
  }
}
