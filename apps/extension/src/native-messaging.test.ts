import { afterEach, describe, expect, it, vi } from "vitest";

import { sendNativeMessageWithTimeout } from "./native-messaging.js";

describe("Native Messaging timeout", () => {
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("returns a Native Host response and clears the timeout", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("chrome", {
      runtime: {
        sendNativeMessage: vi.fn(async () => ({ type: "handshake_ack" })),
      },
    });

    await expect(
      sendNativeMessageWithTimeout("io.verisilo.host", { type: "handshake" }),
    ).resolves.toEqual({ type: "handshake_ack" });
    expect(vi.getTimerCount()).toBe(0);
  });

  it("rejects a Native Host that never responds", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("chrome", {
      runtime: {
        sendNativeMessage: vi.fn(() => new Promise(() => undefined)),
      },
    });

    const result = sendNativeMessageWithTimeout(
      "io.verisilo.host",
      { type: "handshake" },
      25,
    );
    const assertion = expect(result).rejects.toThrow(/timed out/u);
    await vi.advanceTimersByTimeAsync(25);
    await assertion;
    expect(vi.getTimerCount()).toBe(0);
  });
});
