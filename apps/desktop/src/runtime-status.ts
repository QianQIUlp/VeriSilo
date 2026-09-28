import type { RuntimeActivation } from "@verisilo/contracts";
import type { DesktopStatus, RuntimeSessionStatus } from "./desktop-api.js";

export const idleRuntimeActivation: RuntimeActivation = {
  activeSiloId: null,
  state: "idle",
  updatedAt: new Date(0).toISOString(),
  message: null,
  engineEvidence: null,
  networkEvidence: null,
  identityEvidence: null,
};

export function sessionForSilo(
  status: DesktopStatus,
  siloId: string,
): RuntimeSessionStatus | undefined {
  return status.sessions.find((session) => session.siloId === siloId);
}

export function activationForSilo(
  status: DesktopStatus,
  siloId: string,
): RuntimeActivation {
  return sessionForSilo(status, siloId)?.activation ?? idleRuntimeActivation;
}

export function sessionNeedsManagement(session: RuntimeSessionStatus): boolean {
  return session.activation.activeSiloId === session.siloId ||
    ["preflight", "launching", "recovery_required"].includes(session.activation.state);
}
