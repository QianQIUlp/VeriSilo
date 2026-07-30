import { describe, expect, it } from "vitest";

import {
  isLoopbackProxyProfile,
  localMihomoProfile,
  MIHOMO_DEFAULT_MIXED_PORT,
} from "./proxy-presets.js";

describe("desktop proxy presets", () => {
  it("creates a fail-closed loopback Mihomo bridge", () => {
    expect(localMihomoProfile()).toEqual({
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme: "socks5",
      host: "127.0.0.1",
      port: MIHOMO_DEFAULT_MIXED_PORT,
      bypassList: [],
    });
  });

  it("does not mislabel a remote provider as a local core", () => {
    expect(
      isLoopbackProxyProfile({
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: "socks5",
        host: "proxy.example.test",
        port: 1080,
        bypassList: [],
      }),
    ).toBe(false);
  });
});
