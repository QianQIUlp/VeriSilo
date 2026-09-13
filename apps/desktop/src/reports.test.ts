import { describe, expect, it } from "vitest";

import type {
  RuntimeActivation,
  RuntimeEngineEvidence,
  RuntimeIdentityEvidence,
  Silo,
} from "@verisilo/contracts";
import { NETWORK_REPUTATION_EXPLANATION } from "@verisilo/contracts";

import {
  buildLocalSiloReport,
  LOCAL_REPORT_SCHEMA_VERSION,
  renderLocalSiloReportHtml,
  serializeLocalSiloReport,
  type LocalSiloReportInput,
} from "./reports.js";

const silo: Silo = {
  id: "11111111-1111-4111-8111-111111111111",
  schemaVersion: 3,
  name: "<img src=x onerror=alert('name')>",
  color: "#5b5ce2",
  browser: {
    kind: "chrome",
    executablePath: "C:\\Users\\alice\\AppData\\Chrome\\chrome.exe",
    version: "127.0.6533.89",
  },
  engine: { adapter: "stock" },
  executionTarget: { kind: "local" },
  identityLockedAt: null,
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

const managedSilo: Silo = {
  ...silo,
  browser: null,
  engine: { adapter: "camoufox" },
};

const otherSiloId = "55555555-5555-4555-8555-555555555555";

const identityEvidence: RuntimeIdentityEvidence = {
  siloId: silo.id,
  runtimeId: "99999999-9999-4999-8999-999999999999",
  sessionId: "session-secret-identifier",
  artifactId: "identity-artifact-7733",
  artifactFileSha256:
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
  engineAdapter: "camoufox",
  observedAt: "2026-07-28T11:05:00.000Z",
  state: "matched",
  signals: [
    {
      signal: "userAgent",
      expected: "Mozilla/5.0 SecretFingerprintUA",
      observed: "Mozilla/5.0 SecretFingerprintUA",
      state: "matched",
    },
    {
      signal: "timezone",
      expected: "Asia/Shanghai",
      observed: "Asia/Shanghai",
      state: "matched",
    },
    {
      signal: "fonts",
      expected: null,
      observed: null,
      state: "unavailable",
      reason: "fontMode=inherit：字体指标由主机提供，当前不能诚实比较。",
    },
  ],
};

const otherSiloIdentityEvidence: RuntimeIdentityEvidence = {
  ...identityEvidence,
  siloId: otherSiloId,
};

const camoufoxEngineEvidence: RuntimeEngineEvidence = {
  configuredAdapter: "camoufox",
  launchedAdapter: "camoufox",
  verifiedAdapter: "camoufox",
  packageVerification: "verified",
  packageVerificationDetails: null,
  bootstrapDelivery: "not_applicable",
  hostLaunch: "verified",
  runtimeReceipts: "not_applicable",
  restoreReceipt: "not_applicable",
  capabilities: [
    {
      id: "fonts",
      availability: "unavailable",
      operation: "not_configured",
      reason: "capability diagnostic with C:\\Engine\\secret\\path",
      verifiedAt: null,
      evidence: [],
    },
  ],
  phaseReceipts: [],
  fallbackReceipts: [
    {
      site: "secret-browsing.example",
      matchedPattern: "*.example",
      action: "restore_experimental_controls",
      restoredAt: "2026-07-28T11:00:00.000Z",
      capabilities: [
        {
          id: "fonts",
          evidence: ["fallback capability evidence line"],
        },
      ],
    },
  ],
};

const stockEngineEvidence: RuntimeEngineEvidence = {
  configuredAdapter: "stock-chrome",
  launchedAdapter: "stock-chrome",
  verifiedAdapter: null,
  packageVerification: "not_applicable",
  packageVerificationDetails: null,
  bootstrapDelivery: "not_applicable",
  hostLaunch: "not_applicable",
  runtimeReceipts: "not_applicable",
  restoreReceipt: "not_applicable",
  capabilities: [],
  phaseReceipts: [],
  fallbackReceipts: [],
};

const activation: RuntimeActivation = {
  activeSiloId: silo.id,
  state: "running",
  updatedAt: "2026-07-28T11:00:00.000Z",
  message: "raw upstream error with secret endpoint",
  engineEvidence: null,
  networkEvidence: {
    runtimeId: "11111111-1111-4111-8111-111111111111",
    evidenceId: "66666666-6666-4666-8666-666666666666",
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
  identityEvidence: null,
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
        siloId: otherSiloId,
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

function inputWith(overrides: {
  silo?: Silo;
  activation: Partial<RuntimeActivation>;
}): LocalSiloReportInput {
  const base = input();
  return {
    ...base,
    silo: overrides.silo ?? base.silo,
    activation: { ...base.activation, ...overrides.activation },
  };
}

function managedInput(
  activationOverrides: Partial<RuntimeActivation>,
): LocalSiloReportInput {
  return inputWith({
    silo: managedSilo,
    activation: {
      identityEvidence,
      engineEvidence: camoufoxEngineEvidence,
      ...activationOverrides,
    },
  });
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

  it("uses schema version 2 in the constant, the model, and the serialized JSON", () => {
    expect(LOCAL_REPORT_SCHEMA_VERSION).toBe(2);
    const report = buildLocalSiloReport(input());

    expect(report.schemaVersion).toBe(2);
    expect(JSON.parse(serializeLocalSiloReport(report)).schemaVersion).toBe(2);
  });

  it("exports current matched identity evidence with binding conclusions and signal states only", () => {
    const report = buildLocalSiloReport(managedInput({}));

    expect(report.identity).toEqual({
      attribution: "current_for_selected_silo",
      state: "matched",
      observedAt: "2026-07-28T11:05:00.000Z",
      engineAdapter: "camoufox",
      binding: {
        present: true,
        current: true,
        selectedSiloAttributionValid: true,
      },
      signals: [
        { signal: "userAgent", state: "matched" },
        { signal: "timezone", state: "matched" },
        { signal: "fonts", state: "unavailable" },
      ],
      signalTotals: { matched: 2, mismatched: 0, unavailable: 1, total: 3 },
    });
    expect(report.summary).toContain(
      "网站可见身份与当前声明一致（matched），观察于 2026-07-28T11:05:00.000Z；证据属于当前正在运行的所选 Silo。",
    );
  });

  it("never exports identity identifiers, raw signal values, or upstream reason text", () => {
    const serialized = serializeLocalSiloReport(buildLocalSiloReport(managedInput({})));

    for (const prohibited of [
      "identity-artifact-7733",
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
      "99999999-9999-4999-8999-999999999999",
      "session-secret-identifier",
      "SecretFingerprintUA",
      "Asia/Shanghai",
      "字体指标由主机提供",
      "identityEvidence",
      "runtimeId",
      "artifactFileSha256",
    ]) {
      expect(serialized).not.toContain(prohibited);
    }
  });

  it("surfaces mismatched signals in the summary without hiding them", () => {
    const report = buildLocalSiloReport(
      managedInput({
        identityEvidence: {
          ...identityEvidence,
          state: "mismatched",
          signals: [
            {
              signal: "userAgent",
              expected: "Mozilla/5.0 SecretFingerprintUA",
              observed: "Mozilla/5.0 OtherUA",
              state: "mismatched",
            },
            {
              signal: "timezone",
              expected: "Asia/Shanghai",
              observed: "Asia/Shanghai",
              state: "matched",
            },
          ],
        },
      }),
    );

    expect(report.identity.state).toBe("mismatched");
    expect(report.identity.binding.current).toBe(true);
    expect(report.identity.signalTotals).toEqual({
      matched: 1,
      mismatched: 1,
      unavailable: 0,
      total: 2,
    });
    expect(
      report.summary.some((line) =>
        line.startsWith("身份对账发现不一致（mismatched）"),
      ),
    ).toBe(true);
    expect(
      report.summary.some((line) =>
        line.includes("本次未确认：身份不一致项：userAgent"),
      ),
    ).toBe(true);
  });

  it("marks stale identity evidence as no longer current", () => {
    const report = buildLocalSiloReport(
      managedInput({
        identityEvidence: { ...identityEvidence, state: "stale", signals: [] },
      }),
    );

    expect(report.identity.state).toBe("stale");
    expect(report.identity.binding.current).toBe(false);
    expect(
      report.summary.some((line) => line.includes("身份观察已过期（stale）")),
    ).toBe(true);
  });

  it("reflects the latest observedAt after a fresh recheck without triggering one", () => {
    const before = buildLocalSiloReport(managedInput({}));
    expect(before.identity.observedAt).toBe("2026-07-28T11:05:00.000Z");

    const afterRecheck = managedInput({
      identityEvidence: {
        ...identityEvidence,
        observedAt: "2026-07-28T11:55:00.000Z",
      },
    });
    const after = buildLocalSiloReport(afterRecheck);
    expect(after.identity.observedAt).toBe("2026-07-28T11:55:00.000Z");
    expect(after.summary).toContain(
      "网站可见身份与当前声明一致（matched），观察于 2026-07-28T11:55:00.000Z；证据属于当前正在运行的所选 Silo。",
    );
  });

  it("does not borrow identity or execution evidence when a different Silo is active", () => {
    const report = buildLocalSiloReport(
      inputWith({
        activation: {
          activeSiloId: otherSiloId,
          identityEvidence: otherSiloIdentityEvidence,
          engineEvidence: camoufoxEngineEvidence,
          browserVerification: {
            state: "verified",
            expectedKind: "chrome",
            expectedVersion: "127.0.6533.89",
            actualVersion: "127.0.6533.89",
            executablePath: "C:\\Users\\bob\\chrome.exe",
            checkedAt: "2026-07-28T11:00:00.000Z",
            message: "ok",
          },
        },
      }),
    );

    expect(report.runtime.state).toBe("not_active_for_selected_silo");
    expect(report.runtime.networkEvidence).toBeNull();
    expect(report.identity).toEqual({
      attribution: "belongs_to_different_silo",
      state: null,
      observedAt: null,
      engineAdapter: null,
      binding: {
        present: false,
        current: null,
        selectedSiloAttributionValid: false,
      },
      signals: [],
      signalTotals: { matched: 0, mismatched: 0, unavailable: 0, total: 0 },
    });
    expect(report.execution).toEqual({
      attribution: "belongs_to_different_silo",
      engineAdapter: null,
      engineStages: null,
      browserVerification: null,
    });
    const serialized = serializeLocalSiloReport(report);
    expect(serialized).not.toContain("2026-07-28T11:05:00.000Z");
    expect(serialized).not.toContain("secret-browsing.example");
    expect(serialized).not.toContain("C:\\Users\\bob\\chrome.exe");
  });

  it("reports no attributable runtime evidence when nothing is active", () => {
    const report = buildLocalSiloReport(
      inputWith({
        activation: {
          activeSiloId: null,
          state: "stopped",
          engineEvidence: null,
          networkEvidence: null,
          identityEvidence: null,
        },
      }),
    );

    expect(report.identity.attribution).toBe("no_evidence_available");
    expect(report.execution.attribution).toBe("no_evidence_available");
    expect(report.summary).toContain(
      "所选 Silo 当前没有可归属的运行时身份证据。",
    );
    expect(report.summary).toContain(
      "所选 Silo 当前没有可归属的活跃运行时执行证据。",
    );
  });

  it("keeps last-known selected-Silo identity evidence clearly non-current for archived Silos", () => {
    const report = buildLocalSiloReport(
      inputWith({
        silo: { ...managedSilo, archivedAt: "2026-07-29T09:00:00.000Z" },
        activation: {
          activeSiloId: null,
          state: "stopped",
          identityEvidence,
          engineEvidence: null,
          networkEvidence: null,
        },
      }),
    );

    expect(report.silo.lifecycle).toBe("archived");
    expect(report.identity.attribution).toBe(
      "last_known_for_selected_silo_not_active",
    );
    expect(report.identity.state).toBe("matched");
    expect(report.identity.binding).toEqual({
      present: true,
      current: false,
      selectedSiloAttributionValid: true,
    });
    expect(
      report.summary.some((line) =>
        line.includes("当前没有活跃的运行时证据，不作为当前运行结论"),
      ),
    ).toBe(true);
  });

  it("exports engine adapters, engine stages, and browser verification without paths or diagnostics", () => {
    const report = buildLocalSiloReport(managedInput({}));

    expect(report.execution).toMatchObject({
      attribution: "current_for_selected_silo",
      engineAdapter: {
        configured: "camoufox",
        launched: "camoufox",
        verified: "camoufox",
      },
      engineStages: {
        packageVerification: "verified",
        bootstrapDelivery: "not_applicable",
        hostLaunch: "verified",
        runtimeReceipts: "not_applicable",
        restoreReceipt: "not_applicable",
      },
      browserVerification: null,
    });
    const serialized = serializeLocalSiloReport(report);
    for (const prohibited of [
      "secret-browsing.example",
      "C:\\Engine\\secret\\path",
      "capability diagnostic",
      "fallbackReceipts",
      "packageVerificationDetails",
    ]) {
      expect(serialized).not.toContain(prohibited);
    }
  });

  it("exports browser verification state without executable path or raw message", () => {
    const report = buildLocalSiloReport(
      inputWith({
        activation: {
          engineEvidence: stockEngineEvidence,
          browserVerification: {
            state: "version_drift",
            expectedKind: "chrome",
            expectedVersion: "126.0.0.1",
            actualVersion: "127.0.6533.89",
            executablePath: "C:\\Users\\alice\\AppData\\Chrome\\chrome.exe",
            checkedAt: "2026-07-28T11:00:00.000Z",
            message: "browser executable moved to C:\\elsewhere\\chrome.exe",
          },
        },
      }),
    );

    expect(report.execution.browserVerification).toEqual({
      state: "version_drift",
      expectedKind: "chrome",
      expectedVersion: "126.0.0.1",
      actualVersion: "127.0.6533.89",
      checkedAt: "2026-07-28T11:00:00.000Z",
    });
    const serialized = serializeLocalSiloReport(report);
    expect(serialized).not.toContain("C:\\elsewhere\\chrome.exe");
    expect(serialized).not.toContain("browser executable moved");
  });

  it("renders identity, execution, network, and boundary sections in HTML", () => {
    const html = renderLocalSiloReportHtml(buildLocalSiloReport(managedInput({})));

    expect(html).toContain("<h2>身份证据</h2>");
    expect(html).toContain("current_for_selected_silo");
    expect(html).toContain(">userAgent</code></td><td><code>matched</code>");
    expect(html).toContain("对账计数：matched 2 · mismatched 0 · unavailable 1");
    expect(html).toContain("<h2>执行与引擎</h2>");
    expect(html).toContain("包验证 <code>verified</code>");
    expect(html).toContain("<h2>运行时证据</h2>");
    expect(html).toContain("exit=observed");
    expect(html).toContain("<h2>导出边界</h2>");
    expect(html).toContain("本报告不证明");
    expect(html).toContain("TLS ClientHello 已验证");
    expect(html).not.toContain("<script");
    expect(html).not.toMatch(/https?:\/\//u);
  });

  it("explains non-attributable identity evidence in HTML without leaking the other Silo's data", () => {
    const html = renderLocalSiloReportHtml(
      buildLocalSiloReport(
        inputWith({
          activation: {
            activeSiloId: otherSiloId,
            identityEvidence: otherSiloIdentityEvidence,
          },
        }),
      ),
    );

    expect(html).toContain("belongs_to_different_silo");
    expect(html).toContain("未纳入本报告");
    expect(html).not.toContain("SecretFingerprintUA");
    expect(html).not.toContain("2026-07-28T11:05:00.000Z");
  });
});
