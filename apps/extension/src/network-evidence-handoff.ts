import type { NativeResponse } from "@verisilo/contracts";

export const RUNTIME_SNAPSHOT_MAX_AGE_MS = 45_000;
const RUNTIME_SNAPSHOT_FUTURE_TOLERANCE_MS = 5_000;

export type NetworkEvidenceHandoffReason =
  "desktop_unavailable" | "runtime_not_ready" | "submission_rejected";

export type NetworkEvidenceHandoffStatus =
  | {
      state: "local_only";
      checkedAt: string;
      reason: NetworkEvidenceHandoffReason;
    }
  | {
      state: "submitted";
      checkedAt: string;
      siloId: string;
      runtimeId: string;
      evidenceId: string;
      acceptedAt: string;
      expiresAt: string;
    };

type RuntimeStatusResponse = Extract<
  NativeResponse,
  { type: "runtime_status" }
>;

export interface RuntimeEvidenceBinding {
  siloId: string;
  runtimeId: string;
}

export function eligibleSiloForNetworkEvidence(
  response: NativeResponse,
  requestId: string,
  nowMs = Date.now(),
): RuntimeEvidenceBinding | null {
  if (response.type !== "runtime_status" || response.requestId !== requestId) {
    return null;
  }
  if (!isFreshRuntimeStatus(response, nowMs)) {
    return null;
  }
  if (
    response.vault.state !== "unlocked" ||
    response.vault.autoLockAt === null ||
    Date.parse(response.vault.autoLockAt) <= nowMs ||
    response.activation.state !== "running" ||
    response.activation.activeSiloId === null ||
    response.activation.networkEvidence === null
  ) {
    return null;
  }
  return {
    siloId: response.activation.activeSiloId,
    runtimeId: response.activation.networkEvidence.runtimeId,
  };
}

export function isNetworkEvidenceHandoffStatus(
  value: unknown,
  checkedAt?: string,
): value is NetworkEvidenceHandoffStatus {
  if (value === null || typeof value !== "object") {
    return false;
  }
  const candidate = value as Record<string, unknown>;
  if (
    !isDateTime(candidate.checkedAt) ||
    (checkedAt !== undefined && candidate.checkedAt !== checkedAt)
  ) {
    return false;
  }
  if (candidate.state === "local_only") {
    return (
      Object.keys(candidate).length === 3 &&
      [
        "desktop_unavailable",
        "runtime_not_ready",
        "submission_rejected",
      ].includes(String(candidate.reason))
    );
  }
  return (
    Object.keys(candidate).length === 7 &&
    candidate.state === "submitted" &&
    isUuid(candidate.siloId) &&
    isUuid(candidate.runtimeId) &&
    isUuid(candidate.evidenceId) &&
    isDateTime(candidate.acceptedAt) &&
    isDateTime(candidate.expiresAt)
  );
}

function isFreshRuntimeStatus(
  response: RuntimeStatusResponse,
  nowMs: number,
): boolean {
  const writtenAt = Date.parse(response.snapshotWrittenAt);
  if (!Number.isFinite(writtenAt)) {
    return false;
  }
  const age = nowMs - writtenAt;
  return (
    age >= -RUNTIME_SNAPSHOT_FUTURE_TOLERANCE_MS &&
    age <= RUNTIME_SNAPSHOT_MAX_AGE_MS
  );
}

function isDateTime(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.length <= 64 &&
    Number.isFinite(Date.parse(value))
  );
}

function isUuid(value: unknown): value is string {
  return (
    typeof value === "string" &&
    /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(
      value,
    )
  );
}
