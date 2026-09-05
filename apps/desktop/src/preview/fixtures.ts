import type { Silo } from "@verisilo/contracts";
import type { DesktopStatus } from "../desktop-api.js";

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
    },
  };
}
