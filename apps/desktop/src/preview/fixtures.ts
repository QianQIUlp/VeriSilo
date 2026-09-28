import type { RuntimeActivation, Silo } from "@verisilo/contracts";
import type {
  DesktopStatus,
  EngineAdapterStatus,
  ManagedIdentityPreview,
} from "../desktop-api.js";

export const previewSilo: Silo = {
  id: "c3e82c0e-83e9-49ee-b152-44f9e22f131b",
  schemaVersion: 3,
  name: "工作空间（示例）",
  color: "#cc4c25",
  browser: {
    kind: "edge",
    executablePath: "C:\\Preview\\msedge.exe",
    version: "preview",
  },
  profileDirectory: "C:\\Preview\\profiles\\work",
  networkProfile: { mode: "direct", proxyRequired: false },
  executionTarget: { kind: "local" },
  engine: { adapter: "stock" },
  identityLockedAt: null,
  seedReference: "ba81b6fc-f048-4323-8671-20586907cb6b",
  createdAt: "2026-09-01T00:00:00Z",
  archivedAt: null,
};

export const previewManagedSilo: Silo = {
  id: "9f2c6a51-4b7e-4c1a-9d3e-5a1b2c3d4e5f",
  schemaVersion: 3,
  name: "托管空间（示例）",
  color: "#1553ff",
  browser: null,
  executionTarget: { kind: "local" },
  profileDirectory: "C:\\Preview\\profiles\\managed",
  networkProfile: {
    mode: "fixed_proxy",
    proxyRequired: true,
    scheme: "http",
    host: "proxy.example.test",
    port: 3128,
    bypassList: [],
    credentialRef: "7b1d0c3a-9e2f-4a8b-b5c6-1d2e3f4a5b6c",
  },
  engine: {
    adapter: "camoufox",
    artifactBinding: {
      artifactId: "identity-preview-managed",
      artifactFileSha256: "b".repeat(64),
      schema: "verisilo-camoufox-resolved-identity/v6",
    },
  },
  seedReference: "8c2e1f4a-0b3d-4c9e-a6f7-2b3c4d5e6f7a",
  createdAt: "2026-09-01T00:00:00Z",
  identityLockedAt: "2026-09-02T00:00:00Z",
  archivedAt: null,
};

export const previewManagedIdentity: ManagedIdentityPreview = {
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

const previewEngineDescriptor = {
  contractVersion: 1,
  id: "camoufox",
  adapterVersion: "preview",
  engineVersion: "preview",
  channel: "development",
  browserFamily: "firefox",
  platform: "windows-x64",
  externallyPackaged: true,
  emergencyDisabled: false,
} as const;

export const previewEngineStatuses: EngineAdapterStatus[] = [
  {
    descriptor: previewEngineDescriptor,
    negotiation: {
      adapter: previewEngineDescriptor,
      capabilities: [],
      accepted: [],
      rejected: [],
    },
    health: {
      state: "healthy",
      checkedAt: "2026-09-01T00:00:00Z",
      message: "预览模拟：独立浏览器可用。",
    },
  },
];

export function previewStatus(
  state: DesktopStatus["vault"]["state"] = "unlocked",
): DesktopStatus {
  return {
    // An unlocked preview vault carries a future auto-lock deadline so that
    // user-gated flows (e.g. local report export) are exercisable in preview.
    vault: {
      state,
      autoLockAt:
        state === "unlocked"
          ? new Date(Date.now() + 10 * 60_000).toISOString()
          : null,
    },
    activation: {
      activeSiloId: null,
      state: "idle",
      updatedAt: "2026-09-01T00:00:00Z",
      message: null,
      engineEvidence: null,
      networkEvidence: null,
      identityEvidence: null,
    },
    sessions: [],
    managedSessionLimit: 2,
  };
}

// Synthetic observations for visual QA. These never reach the production entry.
export function previewIdentityEvidence(
  silo: Silo,
  state: "matched" | "mismatched" | "unavailable" | "stale" = "matched",
  runtimeId = "22222222-2222-4222-8222-222222222222",
): NonNullable<RuntimeActivation["identityEvidence"]> {
  return {
    siloId: silo.id,
    runtimeId,
    sessionId: "preview-session",
    artifactId:
      silo.engine.adapter === "camoufox"
        ? silo.engine.artifactBinding!.artifactId
        : "preview-artifact",
    artifactFileSha256: silo.engine.adapter === "camoufox"
      ? silo.engine.artifactBinding!.artifactFileSha256 : "b".repeat(64),
    engineAdapter: "camoufox",
    observedAt: new Date(
      Date.now() - (state === "stale" ? 3600000 : 10000),
    ).toISOString(),
    state,
    ...(state === "unavailable"
      ? { reason: "模拟：此次没有取得页面观察。可重新检查，不能视为通过。" }
      : state === "stale"
        ? { reason: "模拟：证据来自上一次运行，请重新检查。" }
        : state === "mismatched"
          ? {
              reason:
                "模拟：观察到的时区与声明不一致。请检查身份配置后重新检查。",
            }
          : {}),
    signals:
      state === "unavailable" || state === "stale"
        ? []
        : [
            {
              signal: "language",
              expected: "en-US",
              observed: "en-US",
              state: "matched",
            },
            {
              signal: "timezone",
              expected: "America/New_York",
              observed:
                state === "mismatched" ? "Asia/Singapore" : "America/New_York",
              state: state === "mismatched" ? "mismatched" : "matched",
            },
            {
              signal: "screen",
              expected: { width: 1920, height: 1080 },
              observed: { width: 1920, height: 1080 },
              state: "matched",
            },
            {
              signal: "hardwareConcurrency",
              expected: 8,
              observed: 8,
              state: "matched",
            },
            {
              signal: "webglVendor",
              expected: "NVIDIA Corporation",
              observed: "NVIDIA Corporation",
              state: "matched",
            },
            {
              signal: "fonts",
              expected: null,
              observed: null,
              state: "unavailable",
              reason: "当前身份没有可直接比较的字体宽度期望值。",
            },
          ],
  };
}

/** Consistent synthetic bindings for product interaction checks only. */
export function previewRuntimeActivation(
  silo: Silo,
  runtimeId = "22222222-2222-4222-8222-222222222222",
): RuntimeActivation {
  const now = new Date().toISOString();
  const managed = silo.engine.adapter === "camoufox";
  const adapter = managed ? "camoufox" : silo.browser?.kind === "edge" ? "stock-edge" : "stock-chrome";
  const proxy = silo.networkProfile.mode !== "direct";
  return {
    activeSiloId: silo.id, state: "running", updatedAt: now, message: null,
    identityEvidence: managed ? previewIdentityEvidence(silo, "matched", runtimeId) : null,
    engineEvidence: {
      configuredAdapter: adapter, launchedAdapter: adapter, verifiedAdapter: null,
      packageVerification: managed ? "configured" : "not_applicable",
      packageVerificationDetails: null,
      bootstrapDelivery: "applied", hostLaunch: "observed",
      runtimeReceipts: "observed", restoreReceipt: "not_applicable",
      capabilities: [], phaseReceipts: [], fallbackReceipts: [],
    },
    networkEvidence: {
      runtimeId,
      evidenceId: "44444444-4444-4444-8444-444444444444",
      observedAt: now, expiresAt: new Date(Date.now() + 300000).toISOString(),
      provenance: "desktop_control_plane",
      provider: silo.networkProfile.mode === "fixed_proxy" && silo.networkProfile.externalMihomo
        ? "external_mihomo" : silo.networkProfile.mode,
      configuration: "configured", controllerBinding: "not_applicable",
      endpoint: proxy ? "reachable" : "not_applicable",
      authentication: "not_applicable", authenticationProvenance: "desktop_control_plane",
      browserRouting: proxy ? "applied" : "not_applicable", exit: "observed",
      dns: "unavailable", webRtc: "unavailable", safeguards: [],
      endpointLabel: proxy ? "预览代理节点（非出口 IP）" : "直连",
    },
  };
}
