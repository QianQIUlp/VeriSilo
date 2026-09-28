import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { type RuntimeActivation, type Silo } from "@verisilo/contracts";

import type {
  ManagedIdentityPreview,
} from "../../desktop-api.js";

import { previewManagedSilo, previewSilo } from "../../preview/fixtures.js";

import { SiloList } from "../silos/SiloList.js";

import {
  CurrentSessionIntegrity,
  NetworkEvidenceDetails,
  deriveCurrentSessionSummary,
} from "./CurrentSessionIntegrity.js";

const RUNTIME_ID = "22222222-2222-4222-8222-222222222222";
const OTHER_RUNTIME_ID = "33333333-3333-4333-8333-333333333333";
const NOW = Date.parse("2026-09-12T08:00:00.000Z");

const identityPreview: ManagedIdentityPreview = {
  userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0)",
  language: "en-US",
  timezone: "America/New_York",
  screenWidth: 1920,
  screenHeight: 1080,
  hardwareConcurrency: 8,
  webglVendor: "NVIDIA Corporation",
  webglRenderer: "NVIDIA GeForce RTX 3060, or similar",
  platform: "Win32",
  countryCode: "US",
  publicAddress: "203.0.113.7",
  latitude: null,
  longitude: null,
  networkBound: true,
};

const engineEvidence = {
  configuredAdapter: "camoufox" as const,
  launchedAdapter: "camoufox" as const,
  verifiedAdapter: "camoufox" as const,
  packageVerification: "verified" as const,
  packageVerificationDetails: {
    verifierId: "preview-verifier",
    artifactSha256: "a".repeat(64),
    digestVerified: true,
    signatureVerified: true,
    packageManifestSha256: "b".repeat(64),
    packageTreeSha256: "c".repeat(64),
    hostSha256: "d".repeat(64),
    signerCertificateSha256: "e".repeat(64),
    engineRevision: "preview-engine",
    verifiedAt: "2026-09-12T07:00:00.000Z",
  },
  bootstrapDelivery: "verified" as const,
  hostLaunch: "verified" as const,
  runtimeReceipts: "observed" as const,
  restoreReceipt: "not_applicable" as const,
  capabilities: [],
  phaseReceipts: [],
  fallbackReceipts: [],
};

function identityEvidence(
  overrides: Partial<
    NonNullable<RuntimeActivation["identityEvidence"]>
  > = {},
): NonNullable<RuntimeActivation["identityEvidence"]> {
  return {
    siloId: previewManagedSilo.id,
    runtimeId: RUNTIME_ID,
    sessionId: "session-preview",
    artifactId: "identity-preview-managed",
    artifactFileSha256: "b".repeat(64),
    engineAdapter: "camoufox" as const,
    observedAt: "2026-09-12T07:59:50.000Z",
    state: "matched" as const,
    signals: [],
    ...overrides,
  };
}

function networkEvidence(
  overrides: Partial<
    NonNullable<RuntimeActivation["networkEvidence"]>
  > = {},
): NonNullable<RuntimeActivation["networkEvidence"]> {
  return {
    runtimeId: RUNTIME_ID,
    evidenceId: "44444444-4444-4444-8444-444444444444",
    observedAt: "2026-09-12T07:59:40.000Z",
    expiresAt: "2026-09-12T08:04:40.000Z",
    provenance: "desktop_control_plane" as const,
    provider: "fixed_proxy" as const,
    configuration: "verified" as const,
    controllerBinding: "applied" as const,
    endpoint: "reachable" as const,
    authentication: "verified" as const,
    authenticationProvenance: "relay_observed" as const,
    browserRouting: "applied" as const,
    exit: "observed" as const,
    dns: "observed" as const,
    webRtc: "observed" as const,
    endpointLabel: "本机 Clash · 预览节点",
    safeguards: [],
    ...overrides,
  };
}

function activation(
  overrides: Partial<RuntimeActivation> = {},
): RuntimeActivation {
  return {
    activeSiloId: previewManagedSilo.id,
    state: "running" as const,
    updatedAt: "2026-09-12T08:00:00.000Z",
    message: null,
    engineEvidence,
    networkEvidence: networkEvidence(),
    identityEvidence: identityEvidence(),
    ...overrides,
  };
}

const directSilo: Silo = {
  ...previewManagedSilo,
  networkProfile: { mode: "direct", proxyRequired: false },
};

function summaryFor(
  silo: Silo,
  runtime: RuntimeActivation,
  managedEngineReady = true,
) {
  return deriveCurrentSessionSummary({
    silo,
    activation: runtime,
    managedEngineReady,
    now: NOW,
  });
}

function rowState(summary: ReturnType<typeof summaryFor>, key: string) {
  const entry = summary.rows.find((row) => row.key === key);
  if (entry === undefined) {
    throw new Error(`missing row: ${key}`);
  }
  return entry;
}

describe("current session summary derivation", () => {
  it("reports no anomaly when identity matched, engine verified and exit observed", () => {
    const summary = summaryFor(previewManagedSilo, activation());
    expect(summary.overall).toBe("no_anomaly");
    expect(summary.overallLabel).toBe("当前未发现异常");
    expect(summary.overallDetail).toContain("不代表全部已验证");
    expect(rowState(summary, "identity").stateLabel).toBe("Matched");
    expect(rowState(summary, "engine").stateLabel).toBe("运行正常");
    expect(rowState(summary, "network").stateLabel).toBe("代理出口已观测");
    expect(rowState(summary, "attribution").stateLabel).toBe(
      "当前 Silo · 本次运行",
    );
  });

  it("raises attention on identity mismatched", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        identityEvidence: identityEvidence({ state: "mismatched" }),
      }),
    );
    expect(summary.overall).toBe("attention");
    expect(summary.overallLabel).toBe("需要注意");
    expect(rowState(summary, "identity").stateLabel).toBe("Mismatched");
  });

  it("keeps unavailable identity as partially unconfirmed, never a pass", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        identityEvidence: identityEvidence({ state: "unavailable" }),
      }),
    );
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "identity").stateLabel).toBe("Unavailable");
    expect(rowState(summary, "identity").tone).toBe("danger");
  });

  it("shows matched with unavailable fields as partial", () => {
    const summary = summaryFor(previewManagedSilo, activation({
      identityEvidence: identityEvidence({ signals: [
        { signal: "voices", expected: ["a"], observed: null, state: "unavailable" },
      ] }),
    }));
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "identity").stateLabel).toBe("Matched");
    expect(rowState(summary, "identity").detail).toContain("1 项字段不可用");
  });

  it("does not confirm current runtime attribution without network evidence", () => {
    const summary = summaryFor(previewManagedSilo, activation({ networkEvidence: null }));
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "identity").stateLabel).toBe("归属未确认");
    expect(rowState(summary, "attribution").stateLabel).toBe("尚未确认");
  });

  it("keeps an old matched verdict out of a stopped session", () => {
    const summary = summaryFor(previewManagedSilo, activation({ state: "stopped" }));
    expect(summary.overall).toBe("stopped");
    expect(rowState(summary, "identity").stateLabel).toBe("归属未确认");
    expect(rowState(summary, "identity").detail).toContain("最后已知");
  });

  it("keeps stale identity as partially unconfirmed, never a pass", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        identityEvidence: identityEvidence({
          state: "stale",
          reason: "这份网站身份观察不再属于当前 Silo 的 Artifact 或活动 runtime。",
        }),
      }),
    );
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "identity").stateLabel).toBe("Stale");
  });

  it("describes observation age without inferring a recheck or TTL", () => {
    const fresh = summaryFor(previewManagedSilo, activation());
    expect(rowState(fresh, "identity").detail).toContain("不到 1 分钟前观察");

    const minutesAgo = summaryFor(
      previewManagedSilo,
      activation({
        identityEvidence: identityEvidence({
          observedAt: "2026-09-12T07:50:00.000Z",
        }),
      }),
    );
    expect(rowState(minutesAgo, "identity").detail).toContain("10 分钟前观察");

    const old = summaryFor(
      previewManagedSilo,
      activation({
        identityEvidence: identityEvidence({
          observedAt: "2026-09-12T06:00:00.000Z",
        }),
      }),
    );
    expect(rowState(old, "identity").detail).toContain("上次观察于");
  });

  it("presents observed proxy exit with its validity window and provenance", () => {
    const summary = summaryFor(previewManagedSilo, activation());
    const network = rowState(summary, "network");
    expect(network.stateLabel).toBe("代理出口已观测");
    expect(network.detail).toContain("证据有效至");
    expect(network.detail).toContain("本机 Clash · 预览节点");
    expect(network.detail).toContain("桌面控制面");
  });

  it("treats expired required network evidence as attention, not a pass", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        networkEvidence: networkEvidence({
          expiresAt: "2026-09-12T07:55:00.000Z",
        }),
      }),
    );
    expect(summary.overall).toBe("attention");
    expect(rowState(summary, "network").stateLabel).toBe("无法确认当前出口");
    expect(rowState(summary, "network").detail).toContain("最近证据已过期");
  });

  it("keeps an observed exit partial when DNS is unavailable", () => {
    const summary = summaryFor(previewManagedSilo, activation({
      networkEvidence: networkEvidence({ dns: "unavailable" }),
    }));
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "network").detail).toContain("DNS尚未确认");
  });

  it("keeps a direct policy honest with and without exit observation", () => {
    const observed = summaryFor(
      directSilo,
      activation({
        networkEvidence: networkEvidence({
          provider: "direct" as const,
          endpointLabel: undefined,
          expiresAt: null,
        }),
      }),
    );
    expect(rowState(observed, "network").stateLabel).toBe("直连已观测");
    expect(rowState(observed, "network").detail).toContain("直连");

    const expired = summaryFor(
      directSilo,
      activation({
        networkEvidence: networkEvidence({
          provider: "direct" as const,
          endpointLabel: undefined,
          expiresAt: "2026-09-12T07:55:00.000Z",
        }),
      }),
    );
    expect(expired.overall).toBe("partial");
    expect(rowState(expired, "network").stateLabel).toBe("无法确认当前出口");

    const absent = summaryFor(
      directSilo,
      activation({ networkEvidence: null }),
    );
    expect(rowState(absent, "network").stateLabel).toBe("直连已配置");
    expect(rowState(absent, "network").detail).toContain("尚无出口观察");
    expect(absent.overall).toBe("partial");
  });

  it("does not call an optional proxy Direct when exit evidence is absent", () => {
    const optionalProxy: Silo = {
      ...previewManagedSilo,
      networkProfile: {
        mode: "fixed_proxy", proxyRequired: false, scheme: "http",
        host: "proxy.example.test", port: 3128, bypassList: [],
      },
    };
    const summary = summaryFor(optionalProxy, activation({ networkEvidence: null }));
    expect(rowState(summary, "network").stateLabel).toBe("代理策略已配置");
    expect(rowState(summary, "network").detail).toContain("尚无出口观察");
    expect(rowState(summary, "network").detail).not.toContain("直连");
  });

  it("maps verification failed and recovery required to non-running sessions", () => {
    const ended = summaryFor(
      previewManagedSilo,
      activation({
        state: "verification_failed" as const,
        message: "浏览器校验失败。",
      }),
    );
    expect(ended.overall).toBe("stopped");
    expect(ended.overallLabel).toBe("本次运行已结束");
    expect(ended.overallDetail).toContain("浏览器校验失败");

    const closed = summaryFor(
      previewManagedSilo,
      activation({
        activeSiloId: null,
        state: "verification_failed" as const,
        message: null,
        identityEvidence: null,
        networkEvidence: null,
      }),
    );
    expect(closed.overallDetail).toContain("浏览器已关闭");
    expect(closed.overallDetail).not.toContain("结束会话");

    const recovery = summaryFor(
      previewManagedSilo,
      activation({ state: "recovery_required" as const }),
    );
    expect(recovery.overall).toBe("attention");
    expect(rowState(recovery, "engine").stateLabel).toBe("需要确认");
  });

  it("shows launching as in progress instead of a completed session", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        state: "launching" as const,
        identityEvidence: null,
        networkEvidence: null,
      }),
    );
    expect(summary.overall).toBe("starting");
    expect(summary.overallLabel).toBe("正在启动");
    expect(rowState(summary, "identity").stateLabel).toBe("等待观察");
  });

  it("refuses to show evidence from another runtime as a current pass", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        networkEvidence: networkEvidence({ runtimeId: OTHER_RUNTIME_ID }),
      }),
    );
    expect(summary.overall).toBe("attention");
    expect(rowState(summary, "identity").stateLabel).toBe("Stale");
    expect(rowState(summary, "attribution").stateLabel).toBe(
      "证据不属于当前运行",
    );
  });

  it("treats missing engine evidence without a healthy package as unconfirmed", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        engineEvidence: null,
        identityEvidence: null,
        networkEvidence: null,
      }),
      false,
    );
    expect(summary.overall).toBe("partial");
    expect(rowState(summary, "engine").stateLabel).toBe("无法确认");
  });

  it("does not call the engine verified unless the run verified it", () => {
    const summary = summaryFor(
      previewManagedSilo,
      activation({
        engineEvidence: {
          ...engineEvidence,
          packageVerification: "observed" as const,
          packageVerificationDetails: null,
          hostLaunch: "observed" as const,
          verifiedAdapter: null,
        },
      }),
    );
    expect(rowState(summary, "engine").stateLabel).toBe("已启动");
    expect(rowState(summary, "engine").detail).not.toContain("校验通过");
    expect(summary.overall).toBe("partial");
  });
});

describe("current session claim boundaries", () => {
  const noAction = async () => {};

  function freshActivation(
    networkOverrides: Partial<
      NonNullable<RuntimeActivation["networkEvidence"]>
    > = {},
  ): RuntimeActivation {
    return activation({
      identityEvidence: identityEvidence({
        observedAt: new Date(Date.now() - 10_000).toISOString(),
      }),
      networkEvidence: networkEvidence({
        observedAt: new Date(Date.now() - 20_000).toISOString(),
        expiresAt: new Date(Date.now() + 300_000).toISOString(),
        ...networkOverrides,
      }),
    });
  }

  function renderSummary(silo: Silo, runtime: RuntimeActivation) {
    return renderToStaticMarkup(
      createElement(CurrentSessionIntegrity, {
        activation: runtime,
        managedEngineReady: true,
        silo,
      }),
    );
  }

  it("presents Matched as a reconciliation result, not a verification", () => {
    const rendered = renderSummary(previewManagedSilo, freshActivation());
    expect(rendered).toContain("当前会话完整性");
    expect(rendered).toContain("Matched");
    expect(rendered).toContain("不到 1 分钟前观察");
    expect(rendered).toContain("证据有效至");
    expect(rendered).not.toMatch(/>已验证<\/strong>/u);
    expect(rendered).not.toContain("Verified");
    expect(rendered).not.toContain("安全");
    expect(rendered).not.toContain("分数");
    expect(rendered).not.toContain("Trust");
    expect(rendered).not.toContain("Anonymous");
  });

  it("keeps provenance wording for extension-asserted exit observations", () => {
    const rendered = renderSummary(
      previewManagedSilo,
      freshActivation({ provenance: "extension_asserted" as const }),
    );
    expect(rendered).toContain("浏览器内检查断言");
  });

  it("renders network stages, provenance, observed time and expiry for reuse", () => {
    const rendered = renderToStaticMarkup(createElement(NetworkEvidenceDetails, {
      activation: freshActivation({ dns: "unavailable" }),
      silo: previewManagedSilo,
    }));
    expect(rendered).toContain("桌面控制面");
    expect(rendered).toContain("浏览器路由");
    expect(rendered).toContain("DNS");
    expect(rendered).toContain("不可用");
    expect(rendered).toContain("观察于");
    expect(rendered).toContain("有效至");
  });

  it("renders keyboard buttons for lens navigation when callbacks are wired", () => {
    const rendered = renderToStaticMarkup(createElement(CurrentSessionIntegrity, {
      activation: freshActivation(), managedEngineReady: true, silo: previewManagedSilo,
      onInspectIdentity: () => {}, onInspectNetwork: () => {},
    }));
    expect(rendered).toContain("<button type=\"button\"");
    expect(rendered).toContain("查看身份字段");
    expect(rendered).toContain("查看网络证据");
  });

  it("defers the session summary until its detail pane is opened", () => {
    const baseProps = {
      busy: false,
      siloBusyIds: new Set<string>(),
      launchingSiloIds: new Set<string>(),
      managedSessionLimit: 2,
      managedEngineReady: true,
      networkEvidence: [],
      storageUsage: {},
      onArchive: noAction,
      onCreate: noAction,
      onEdit: noAction,
      onLaunch: noAction,
      onRebindMihomo: noAction,
      onRecheckBrowser: noAction,
      onRecheckRuntime: noAction,
      onStop: noAction,
    };
    const active = renderToStaticMarkup(
      createElement(SiloList, {
        ...baseProps,
        sessions: [{ siloId: previewManagedSilo.id, activation: freshActivation() }],
        silos: [previewManagedSilo],
        identityPreviews: { [previewManagedSilo.id]: identityPreview },
        onCreateIdentity: () => {},
      }),
    );
    expect(active).not.toContain("当前会话完整性");
    expect(active).not.toContain("当前未发现异常");
    expect(active).toContain("重新检查");
    expect(active).toContain("创建新身份");

    const inactive = renderToStaticMarkup(
      createElement(SiloList, {
        ...baseProps,
        sessions: [],
        silos: [previewManagedSilo],
        identityPreviews: { [previewManagedSilo.id]: identityPreview },
        onCreateIdentity: () => {},
      }),
    );
    expect(inactive).not.toContain("当前会话完整性");
    expect(inactive).not.toContain("当前未发现异常");
    expect(inactive).toContain("创建新身份");
    expect(inactive).toContain("打开浏览器");

    const standard = renderToStaticMarkup(
      createElement(SiloList, {
        ...baseProps,
        sessions: [{ siloId: previewSilo.id, activation: { ...freshActivation(), activeSiloId: previewSilo.id } }],
        silos: [previewSilo],
        identityPreviews: {},
      }),
    );
    expect(standard).not.toContain("当前会话完整性");
  });
});
