import { buildNetworkCheckResult } from "@verisilo/contracts";

// Only the website build aliases this module. The demo never probes a visitor's network.
export async function runDesktopNetworkCheck() {
  return buildNetworkCheckResult({
    ipPayload: {
      success: true,
      ip: "203.0.113.7",
      type: "IPv4",
      country: "Demo",
      country_code: "XX",
      city: "演示位置",
      latitude: 0,
      longitude: 0,
      connection: { asn: 0, org: "UI demo", isp: "UI demo" },
    },
    cloudflareDnsPayload: null,
    googleDnsPayload: null,
    errors: ["官网演示：这是模拟出口，不会探测你的 IP 或 DNS。"],
  });
}
