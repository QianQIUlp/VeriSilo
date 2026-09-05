import type { NetworkProfile } from "@verisilo/contracts";

export const MIHOMO_DEFAULT_HOST = "127.0.0.1" as const;
/** Clash Verge / 常见中文客户端 mixed 口。Mihomo 原版默认仍是 7890。 */
export const MIHOMO_DEFAULT_MIXED_PORT = 7897 as const;
export const COMMON_CLASH_MIXED_PORTS = [7897, 7890, 7891, 7880] as const;
export const COMMON_CLASH_CONTROLLER_PORTS = [9097, 9090, 9091] as const;
/** Clash Verge Rev 默认关闭 9097，改走本机内核管道。 */
export const CLASH_VERGE_PIPE_URL = "pipe://verge-mihomo/";

export function localMihomoProfile(
  mixedPort: number = MIHOMO_DEFAULT_MIXED_PORT,
): NetworkProfile {
  return {
    mode: "fixed_proxy",
    proxyRequired: true,
    scheme: "socks5",
    host: MIHOMO_DEFAULT_HOST,
    port: mixedPort,
    bypassList: [],
  };
}

export function clashControllerUrl(port: number): string {
  return `http://127.0.0.1:${port}/`;
}

export function isClashPipeController(url: string): boolean {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "pipe:" && parsed.hostname === "verge-mihomo";
  } catch {
    return false;
  }
}

export function clashControllerLabel(url: string): string {
  if (isClashPipeController(url)) {
    return "Clash Verge 内核管道";
  }
  try {
    const parsed = new URL(url);
    if (
      parsed.protocol === "http:" &&
      parsed.hostname === "127.0.0.1" &&
      parsed.port !== ""
    ) {
      return parsed.port;
    }
  } catch {
    // ignore
  }
  return url;
}

export function clashControllerPort(url: string): string {
  if (isClashPipeController(url)) {
    return "";
  }
  try {
    return new URL(url).port;
  } catch {
    return "";
  }
}

export function isCommonClashMixedPort(port: number): boolean {
  return (COMMON_CLASH_MIXED_PORTS as readonly number[]).includes(port);
}

export function isLoopbackProxyProfile(
  profile: NetworkProfile,
): profile is Extract<NetworkProfile, { mode: "fixed_proxy" }> {
  return (
    profile.mode === "fixed_proxy" &&
    ["127.0.0.1", "localhost", "::1"].includes(
      profile.host.trim().toLowerCase(),
    )
  );
}
