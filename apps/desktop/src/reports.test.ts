import { describe, expect, it } from "vitest";

import type { RuntimeActivation, Silo } from "@verisilo/contracts";
import { NETWORK_REPUTATION_EXPLANATION } from "@verisilo/contracts";

import {
  buildLocalSiloReport,
  renderLocalSiloReportHtml,
  serializeLocalSiloReport,
  type LocalSiloReportInput,
} from "./reports.js";

const silo: Silo = {
  id: "11111111-1111-4111-8111-111111111111",
  schemaVersion: 1,
  name: "<img src=x onerror=alert('name')>",
  color: "#5b5ce2",
  browser: {
    kind: "chrome",
    executablePath: "C:\\Users\\alice\\AppData\\Chrome\\chrome.exe",
    version: "127.0.6533.89",
  },
  engine: { adapter: "stock" },
  profileDirectory: "C:\\Users\\alice\\AppData\\VeriSilo\\browser-data\\work",
  networkProfile: {
    mode: "fixed_proxy",
    proxyRequired: true,
    scheme: "socks5",
    host: "proxy.private.example",
    port: 1080,
    bypassList: [],
    credentialRef: "22222222-2222-4222-8222-222222222222",
    externalMihomo: {
      controllerUrl: "http://127.0.0.1:9090/",
      selectorGroup: "Secret group",
      nodeName: "Secret node",
      controllerSecretRef: "33333333-3333-4333-8333-333333333333",
    },
  },
  seedReference: "44444444-4444-4444-8444-444444444444",
  createdAt: "2026-07-28T10:00:00.000Z",
  archivedAt: null,
};

const activation: RuntimeActivation = {
  activeSiloId: silo.id,
  state: "running",
  updatedAt: "2026-07-28T11:00:00.000Z",
  message: "raw upstream error with secret endpoint",
  engineEvidence: null,
  networkEvidence: {
    runtimeId: "11111111-1111-4111-8111-111111111111",
    evidenceId: "55555555-5555-4555-8555-555555555555",
    observedAt: "2026-07-28T10:59:30.000Z",
    expiresAt: "2026-07-28T11:10:30.000Z",
    provenance: "extension_asserted",
    provider: "external_mihomo",
    configuration: "configured",
    controllerBinding: "applied",
    endpoint: "reachable",
    authentication: "verified",
    authenticationProvenance: "relay_observed",
    browserRouting: "applied",
    exit: "observed",
    dns: "unavailable",
    webRtc: "not_requested",
    endpointLabel: "127.0.0.1:43210 -> proxy.private.example:1080",
    safeguards: ["contains untrusted labels but never enters the report"],
  },
};

function input(): LocalSiloReportInput {
  return {
    generatedAt: "2026-07-28T12:00:00.000Z",
    silo,
    activation,
    vaultEvidence: [
      {
        siloId: silo.id,
        receivedAt: "2026-07-28T11:01:00.000Z",
        coverage: {
          trigger: "user_initiated",
          transport: "companion_extension_fetch",
          ip: "third_party_https_observation",
          publicDns: "public_doh_answer_comparison",
          actualDnsPath: "not_observed",
          webRtc: "not_observed",
          quic: "not_observed",
        },
        result: {
          schemaVersion: 1,
          checkedAt: "2026-07-28T11:00:30.000Z",
          ip: {
            address: "203.0.113.240",
            version: "IPv4",
            country: "Exampleland",
            countryCode: "EX",
            region: "Secret region",
            city: "Secret city",
            asn: "AS64500",
            organization: "Secret ISP",
            isp: "Secret ISP",
            timezone: "Example/Secret",
            networkHint: "cloud_or_hosting",
          },
          dns: {
            state: "consistent",
            dnssec: "validated",
            queryName: "example.com",
            providers: [
              {
                provider: "Cloudflare",
                status: 0,
                dnssecAuthenticated: true,
                addresses: ["93.184.216.34"],
              },
              {
                provider: "Google",
                status: 0,
                dnssecAuthenticated: true,
                addresses: ["93.184.216.34"],
              },
            ],
          },
          reputation: {
            state: "not_scored",
            explanation: NETWORK_REPUTATION_EXPLANATION,
          },
          errors: ["Raw network error that must not be exported"],
        },
      },
      {
        siloId: "55555555-5555-4555-8555-555555555555",
        receivedAt: "2026-07-28T11:02:00.000Z",
        coverage: {
          trigger: "user_initiated",
          transport: "companion_extension_fetch",
          ip: "third_party_https_observation",
          publicDns: "public_doh_answer_comparison",
          actualDnsPath: "not_observed",
          webRtc: "not_observed",
          quic: "not_observed",
        },
        result: {
          schemaVersion: 1,
          checkedAt: "2026-07-28T11:02:00.000Z",
          ip: null,
          dns: {
            state: "failed",
            dnssec: "unavailable",
            queryName: "example.com",
            providers: [],
          },
          reputation: {
            state: "not_scored",
            explanation: NETWORK_REPUTATION_EXPLANATION,
          },
          errors: [],
        },
      },
    ],
  };
}

describe("local Silo report", () => {
  it("redacts network observations and keeps evidence-stage distinctions", () => {
    const report = buildLocalSiloReport(input());

    expect(report.companionEvidence).toHaveLength(1);
    expect(report.companionEvidence[0]?.exit).toEqual({
      state: "observed",
      addressPrefix: "203.0.113.0/24",
      version: "IPv4",
      countryCode: "EX",
      asn: "AS64500",
      networkHint: "cloud_or_hosting",
    });
    expect(report.runtime.networkEvidence?.stages).toEqual({
      configuration: {
        configuration: "configured",
        controllerBinding: "applied",
        endpoint: "reachable",
        authentication: "verified",
      },
      application: { browserRouting: "applied" },
      verification: {
        exit: "observed",
        dns: "unavailable",
        webRtc: "not_requested",
      },
    });
    expect(report.runtime.observedAt).toBe("2026-07-28T11:00:30.000Z");
    expect(report.runtime.observationSource).toBe("vault_companion_checked_at");
  });

  it("contains no source paths, endpoints, references, raw errors, or full IPs", () => {
    const serialized = serializeLocalSiloReport(buildLocalSiloReport(input()));

    for (const prohibited of [
      "profileDirectory",
      "executablePath",
      "requestId",
      "proxy.private.example",
      "127.0.0.1:9090",
      "Secret group",
      "Secret node",
      "203.0.113.240",
      "93.184.216.34",
      "Secret city",
      "Secret region",
      "Raw network error",
      "credentialRef",
      "controllerSecretRef",
      "seedReference",
      "11111111-1111-4111-8111-111111111111",
    ]) {
      expect(serialized).not.toContain(prohibited);
    }
  });

  it("creates deterministic output for fixed input and redacts IPv6 to /48", () => {
    const ipv6Input = input();
    ipv6Input.vaultEvidence[0]!.result.ip = {
      ...ipv6Input.vaultEvidence[0]!.result.ip!,
      address: "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
      version: "IPv6",
    };
    const first = serializeLocalSiloReport(buildLocalSiloReport(ipv6Input));
    const second = serializeLocalSiloReport(buildLocalSiloReport(ipv6Input));

    expect(first).toBe(second);
    expect(first).toContain("2001:0db8:85a3::/48");
    expect(first).not.toContain("2001:0db8:85a3:0000:0000:8a2e:0370:7334");
  });

  it("escapes every report value in self-contained HTML without scripts or remote resources", () => {
    const html = renderLocalSiloReportHtml(buildLocalSiloReport(input()));

    expect(html).toContain("&lt;img src=x onerror=alert(&#39;name&#39;)&gt;");
    expect(html).not.toContain("<img src=x");
    expect(html).not.toContain("<script");
    expect(html).not.toMatch(/https?:\/\//u);
  });
});
