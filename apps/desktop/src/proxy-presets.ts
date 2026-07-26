import type { NetworkProfile } from "@verisilo/contracts";

export const MIHOMO_DEFAULT_HOST = "127.0.0.1" as const;
export const MIHOMO_DEFAULT_MIXED_PORT = 7890 as const;

export function localMihomoProfile(): NetworkProfile {
  return {
    mode: "fixed_proxy",
    proxyRequired: true,
    scheme: "socks5",
    host: MIHOMO_DEFAULT_HOST,
    port: MIHOMO_DEFAULT_MIXED_PORT,
    bypassList: [],
  };
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
