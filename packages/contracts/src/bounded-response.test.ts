import { describe, expect, it, vi } from "vitest";

import { readBoundedUtf8Response } from "./bounded-response.js";

describe("bounded UTF-8 response reader", () => {
  it("decodes a multi-chunk UTF-8 body within the byte limit", async () => {
    const encoded = new TextEncoder().encode('{"label":"隔离"}');
    const response = new Response(
      new ReadableStream<Uint8Array>({
        start(controller) {
          controller.enqueue(encoded.slice(0, 12));
          controller.enqueue(encoded.slice(12));
          controller.close();
        },
      }),
    );

    await expect(
      readBoundedUtf8Response(response, encoded.byteLength),
    ).resolves.toBe('{"label":"隔离"}');
  });

  it("cancels before consuming bytes beyond the declared limit", async () => {
    const cancelled = vi.fn();
    let pullCount = 0;
    const response = new Response(
      new ReadableStream<Uint8Array>({
        pull(controller) {
          pullCount += 1;
          controller.enqueue(new Uint8Array(32));
        },
        cancel: cancelled,
      }),
    );

    await expect(readBoundedUtf8Response(response, 63)).rejects.toThrow(
      "response exceeded 63 bytes",
    );
    expect(cancelled).toHaveBeenCalledOnce();
    expect(pullCount).toBe(2);
  });
});
