import { describe, expect, it } from "vitest";

import type { Silo } from "@verisilo/contracts";

import type {
  RuntimeSessionStatus,
  SiloNetworkEvidence,
} from "../../desktop-api.js";
import {
  collectParallelExitFindings,
  configuredExitKey,
  latestUnexpiredEvidenceBySilo,
  normalizeExitAddress,
  parallelExitLaunchHint,
} from "./parallel-exit.js";

let siloCounter = 0;

function nextUuid(): string {
  siloCounter += 1;
  return `00000000-0000-4000-8000-${String(siloCounter).padStart(12, "0")}`;
}

function directSilo(overrides: Partial<Silo> = {}): Silo {
  return {
    id: nextUuid(),
    schemaVersion: 3,
    name: `直连空间 ${siloCounter}`,
    color: "#1553ff",
    browser: null,
    executionTarget: { kind: "local" },
    profileDirectory: `C:\\profiles\\${siloCounter}`,
    networkProfile: { mode: "direct", proxyRequired: false },
    engine: { adapter: "camoufox" },
    seedReference: "8c2e1f4a-0b3d-4c9e-a6f7-2b3c4d5e6f7a",
    createdAt: "2026-09-01T00:00:00Z",
    identityLockedAt: null,
    archivedAt: null,
    ...overrides,
  } as Silo;
}

function proxySilo(
  host = "proxy.example.test",
  port = 3128,
  scheme: "http" | "socks5" = "http",
  overrides: Partial<Silo> = {},
): Silo {
  return directSilo({
    networkProfile: {
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme,
      host,
      port,
      bypassList: [],
    },
    ...overrides,
  } as Partial<Silo>);
}

function stockSilo(): Silo {
  return directSilo({
    name: `本机空间 ${siloCounter}`,
    browser: {
      kind: "chrome",
      executablePath: "C:\\Program Files\\Google\\Chrome\\chrome.exe",
    },
    engine: { adapter: "stock" },
  } as Partial<Silo>);
}

function runningSession(
  silo: Silo,
  state: RuntimeSessionStatus["activation"]["state"] = "running",
): RuntimeSessionStatus {
  return {
    siloId: silo.id,
    activation: {
      activeSiloId: state === "idle" || state === "stopped" ? null : silo.id,
      state,
      updatedAt: "2026-09-28T00:00:00Z",
      message: null,
      engineEvidence: null,
      networkEvidence: null,
      identityEvidence: null,
    },
  };
}

const NOW = Date.parse("2026-09-28T12:00:00Z");

function freshEvidence(
  silo: Silo,
  address: string,
  options: { expiresAt?: string; receivedAt?: string } = {},
): SiloNetworkEvidence {
  return {
    schemaVersion: 1,
    protocolVersion: 1,
    evidenceId: `11111111-1111-4111-8111-${String(siloCounter++).padStart(12, "0")}`,
    requestId: `22222222-2222-4222-8222-${String(siloCounter++).padStart(12, "0")}`,
    siloId: silo.id,
    receivedAt: options.receivedAt ?? "2026-09-28T11:55:00Z",
    expiresAt: options.expiresAt ?? "2026-09-28T13:00:00Z",
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
      schemaVersion: 2,
      checkedAt: options.receivedAt ?? "2026-09-28T11:55:00Z",
      ip: {
        address,
        version: "IPv4",
        country: null,
        countryCode: null,
        region: null,
        city: null,
        asn: null,
        organization: null,
        isp: null,
        timezone: null,
        networkHint: "unknown",
        latitude: null,
        longitude: null,
      },
      dns: {
        state: "failed",
        dnssec: "unavailable",
        queryName: "example.com",
        providers: [],
      },
      reputation: {
        state: "not_scored",
        explanation:
          "未查询商业信誉库或黑名单；运营商与机房线索不能代表 IP 一定干净或一定有风险。",
      },
      errors: [],
    },
  };
}

describe("configuredExitKey", () => {
  it("normalizes direct profiles to a single key", () => {
    expect(configuredExitKey({ mode: "direct", proxyRequired: false })).toBe(
      "direct",
    );
  });

  it("lowercases proxy hosts and keeps scheme and port", () => {
    expect(
      configuredExitKey({
        mode: "fixed_proxy",
        proxyRequired: true,
        scheme: "socks5",
        host: "Proxy.Example.test",
        port: 1080,
        bypassList: [],
      }),
    ).toBe("proxy:socks5:proxy.example.test:1080");
  });

  it("distinguishes schemes and ports on the same host", () => {
    const http = configuredExitKey({
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme: "http",
      host: "proxy.example.test",
      port: 3128,
      bypassList: [],
    });
    const socks = configuredExitKey({
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme: "socks5",
      host: "proxy.example.test",
      port: 3128,
      bypassList: [],
    });
    const otherPort = configuredExitKey({
      mode: "fixed_proxy",
      proxyRequired: true,
      scheme: "http",
      host: "proxy.example.test",
      port: 4128,
      bypassList: [],
    });
    expect(new Set([http, socks, otherPort]).size).toBe(3);
  });

  it("returns null for PAC profiles because they cannot be compared statically", () => {
    expect(
      configuredExitKey({
        mode: "pac",
        proxyRequired: true,
        pacUrl: "https://example.test/proxy.pac",
      }),
    ).toBeNull();
  });
});

describe("normalizeExitAddress", () => {
  it("trims and lowercases", () => {
    expect(normalizeExitAddress(" 198.51.100.7 ")).toBe("198.51.100.7");
    expect(normalizeExitAddress("2001:DB8::1")).toBe("2001:db8::1");
  });
});

describe("latestUnexpiredEvidenceBySilo", () => {
  it("keeps only records with an exit address that have not expired", () => {
    const a = directSilo();
    const b = directSilo();
    const evidence = [
      freshEvidence(a, "198.51.100.7", {
        expiresAt: "2026-09-28T11:00:00Z",
      }),
      freshEvidence(b, "198.51.100.7"),
      {
        ...freshEvidence(directSilo(), "203.0.113.9"),
        result: {
          ...freshEvidence(directSilo(), "203.0.113.9").result,
          ip: null,
        },
      } as SiloNetworkEvidence,
    ];
    const latest = latestUnexpiredEvidenceBySilo(evidence, NOW);
    expect(latest.has(a.id)).toBe(false);
    expect(latest.has(b.id)).toBe(true);
    expect(latest.size).toBe(1);
  });

  it("prefers the most recently received record per silo", () => {
    const a = directSilo();
    const evidence = [
      freshEvidence(a, "198.51.100.7", {
        receivedAt: "2026-09-28T10:00:00Z",
      }),
      freshEvidence(a, "203.0.113.9", {
        receivedAt: "2026-09-28T11:30:00Z",
      }),
    ];
    const latest = latestUnexpiredEvidenceBySilo(evidence, NOW);
    expect(latest.get(a.id)?.result.ip?.address).toBe("203.0.113.9");
  });
});

describe("collectParallelExitFindings", () => {
  it("warns two concurrently running direct Managed sessions about the shared machine exit", () => {
    const a = directSilo();
    const b = directSilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [],
      now: NOW,
    });
    const aFindings = findings.get(a.id);
    expect(aFindings).toHaveLength(1);
    expect(aFindings![0]).toMatchObject({
      basis: "configured",
      peerSiloId: b.id,
      peerName: b.name,
      sharedExitLabel: "直连（本机网络）",
    });
    expect(findings.get(b.id)?.[0]?.peerSiloId).toBe(a.id);
  });

  it("does not warn a single running session", () => {
    const a = directSilo();
    const findings = collectParallelExitFindings({
      silos: [a],
      sessions: [runningSession(a)],
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.size).toBe(0);
  });

  it("does not warn direct and proxy sessions about each other by configuration", () => {
    const a = directSilo();
    const b = proxySilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.size).toBe(0);
  });

  it("warns sessions that share a configured proxy entry", () => {
    const a = proxySilo();
    const b = proxySilo();
    const other = proxySilo("relay.example.test", 3128);
    const findings = collectParallelExitFindings({
      silos: [a, b, other],
      sessions: [runningSession(a), runningSession(b), runningSession(other)],
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.get(a.id)).toHaveLength(1);
    expect(findings.get(a.id)?.[0]).toMatchObject({
      basis: "configured",
      peerSiloId: b.id,
      sharedExitLabel: "proxy.example.test:3128",
    });
    expect(findings.get(other.id)).toBeUndefined();
  });

  it("prefers an observed shared address over the configured basis and records when it was received", () => {
    const a = proxySilo();
    const b = proxySilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [
        freshEvidence(a, "198.51.100.7"),
        freshEvidence(b, " 198.51.100.7 "),
      ],
      now: NOW,
    });
    expect(findings.get(a.id)?.[0]).toMatchObject({
      basis: "observed",
      peerSiloId: b.id,
      sharedExitLabel: "198.51.100.7",
      observedAt: "2026-09-28T11:55:00Z",
    });
  });

  it("falls back to the configured basis when the latest records show different addresses", () => {
    const a = directSilo();
    const b = directSilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [
        freshEvidence(a, "198.51.100.7"),
        freshEvidence(b, "203.0.113.9"),
      ],
      now: NOW,
    });
    expect(findings.get(a.id)?.[0]?.basis).toBe("configured");
  });

  it("ignores expired records and falls back to the configured basis instead of claiming an observation", () => {
    const a = directSilo();
    const b = directSilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [
        freshEvidence(a, "198.51.100.7", {
          expiresAt: "2026-09-28T11:00:00Z",
        }),
        freshEvidence(b, "198.51.100.7", {
          expiresAt: "2026-09-28T11:30:00Z",
        }),
      ],
      now: NOW,
    });
    expect(findings.get(a.id)?.[0]?.basis).toBe("configured");
    expect(findings.get(a.id)?.[0]?.observedAt).toBeUndefined();
  });

  it("produces no finding when the only shared basis was an expired record on different configured exits", () => {
    const a = directSilo();
    const b = proxySilo();
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions: [runningSession(a), runningSession(b)],
      networkEvidence: [
        freshEvidence(a, "198.51.100.7", {
          expiresAt: "2026-09-28T11:00:00Z",
        }),
        freshEvidence(b, "198.51.100.7", {
          expiresAt: "2026-09-28T11:30:00Z",
        }),
      ],
      now: NOW,
    });
    expect(findings.size).toBe(0);
  });

  it("does not treat stopped, failed, or recovery sessions as traffic-carrying peers", () => {
    const a = directSilo();
    const b = directSilo();
    const sessions = [runningSession(a), runningSession(b, "stopped")];
    const findings = collectParallelExitFindings({
      silos: [a, b],
      sessions,
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.size).toBe(0);
    expect(
      collectParallelExitFindings({
        silos: [a, b],
        sessions: [runningSession(a), runningSession(b, "recovery_required")],
        networkEvidence: [],
        now: NOW,
      }).size,
    ).toBe(0);
    expect(
      collectParallelExitFindings({
        silos: [a, b],
        sessions: [runningSession(a), runningSession(b, "failed")],
        networkEvidence: [],
        now: NOW,
      }).size,
    ).toBe(0);
  });

  it("cautions only Managed targets, while a stock session can still be the peer", () => {
    const managed = directSilo();
    const stock = stockSilo();
    const findings = collectParallelExitFindings({
      silos: [managed, stock],
      sessions: [runningSession(managed), runningSession(stock)],
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.get(managed.id)).toHaveLength(1);
    expect(findings.has(stock.id)).toBe(false);
  });

  it("ignores sessions whose silo no longer exists or runs outside this machine", () => {
    const managed = directSilo();
    const wsl = directSilo({
      name: "WSL 空间",
      executionTarget: { kind: "wsl", distribution: "Ubuntu-22.04" },
    } as Partial<Silo>);
    const findings = collectParallelExitFindings({
      silos: [managed, wsl],
      sessions: [runningSession(managed), runningSession(wsl)],
      networkEvidence: [],
      now: NOW,
    });
    expect(findings.size).toBe(0);
  });
});

describe("parallelExitLaunchHint", () => {
  it("hints when opening a direct Managed Silo would share the machine exit with a running session", () => {
    const idle = directSilo();
    const running = directSilo();
    const hint = parallelExitLaunchHint(
      idle,
      [idle, running],
      [runningSession(running)],
    );
    expect(hint).toMatchObject({
      basis: "configured",
      peerSiloId: running.id,
      peerName: running.name,
      sharedExitLabel: "直连（本机网络）",
    });
  });

  it("hints when the configured proxy entry matches a running session", () => {
    const idle = proxySilo();
    const running = proxySilo();
    expect(
      parallelExitLaunchHint(idle, [idle, running], [runningSession(running)]),
    ).toMatchObject({
      basis: "configured",
      sharedExitLabel: "proxy.example.test:3128",
    });
  });

  it("returns null when configured exits differ", () => {
    const idle = proxySilo("other.example.test", 8080);
    const running = directSilo();
    expect(
      parallelExitLaunchHint(idle, [idle, running], [runningSession(running)]),
    ).toBeNull();
  });

  it("returns null when nothing is running or the target already runs", () => {
    const idle = directSilo();
    expect(parallelExitLaunchHint(idle, [idle], [])).toBeNull();
    expect(
      parallelExitLaunchHint(idle, [idle], [runningSession(idle)]),
    ).toBeNull();
  });

  it("returns null for stock or remote targets", () => {
    const stock = stockSilo();
    const running = directSilo();
    expect(
      parallelExitLaunchHint(
        stock,
        [stock, running],
        [runningSession(running)],
      ),
    ).toBeNull();
    const remote = directSilo({
      name: "远程空间",
      executionTarget: {
        kind: "remote",
        endpointOrigin: "https://agent.example.test",
      },
    } as Partial<Silo>);
    expect(
      parallelExitLaunchHint(
        remote,
        [remote, running],
        [runningSession(running)],
      ),
    ).toBeNull();
  });
});
