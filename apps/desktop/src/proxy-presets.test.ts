import { describe, expect, it } from "vitest";

import {
  clashControllerLabel,
  CLASH_VERGE_PIPE_URL,
  isClashPipeController,
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
      port: 7897,
      bypassList: [],
    });
  });

  it("labels Clash Verge's kernel pipe without exposing a fake 9097 port", () => {
    expect(isClashPipeController(CLASH_VERGE_PIPE_URL)).toBe(true);
    expect(clashControllerLabel(CLASH_VERGE_PIPE_URL)).toBe(
      "Clash Verge 内核管道",
    );
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
