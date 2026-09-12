import type { Silo } from "@verisilo/contracts";
import type {
  DesktopStatus,
  EngineAdapterStatus,
  ManagedIdentityPreview,
} from "../desktop-api.js";

export const previewSilo: Silo = {
  id: "c3e82c0e-83e9-49ee-b152-44f9e22f131b",
  schemaVersion: 3,
  name: "工作空间（示例）",
  color: "#5b5ce2",
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
  color: "#128f8b",
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
    vault: { state, autoLockAt: null },
    activation: {
      activeSiloId: null,
      state: "idle",
      updatedAt: "2026-09-01T00:00:00Z",
      message: null,
      engineEvidence: null,
      networkEvidence: null,
      identityEvidence: null,
    },
  };
}
