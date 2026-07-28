import { describe, expect, it } from "vitest";

import { PROTOCOL_VERSION, type NativeResponse } from "@verisilo/contracts";

import {
  eligibleSiloForNetworkEvidence,
  isNetworkEvidenceHandoffStatus,
} from "./network-evidence-handoff.js";

const requestId = "6b8a9da2-13e7-4f69-90cb-860f8d02e510";
const siloId = "6c0bd487-a28a-49f4-8996-1fe16d7f010e";
const runtimeId = "11111111-1111-4111-8111-111111111111";
const now = Date.parse("2026-07-28T12:00:00.000Z");

function runtimeStatus(
  overrides: Partial<Extract<NativeResponse, { type: "runtime_status" }>> = {},
): Extract<NativeResponse, { type: "runtime_status" }> {
  return {
    type: "runtime_status",
    protocolVersion: PROTOCOL_VERSION,
    requestId,
    snapshotWrittenAt: "2026-07-28T11:59:30.000Z",
    activation: {
      activeSiloId: siloId,
      state: "running",
      updatedAt: "2026-07-28T11:59:30.000Z",
      networkEvidence: {
        runtimeId,
        evidenceId: "55555555-5555-4555-8555-555555555555",
        observedAt: "2026-07-28T11:59:30.000Z",
        expiresAt: null,
        provenance: "desktop_control_plane",
        provider: "fixed_proxy",
        configuration: "configured",
        controllerBinding: "not_applicable",
        endpoint: "reachable",
        authentication: "configured",
        authenticationProvenance: "desktop_control_plane",
        browserRouting: "applied",
        exit: "not_requested",
        dns: "not_requested",
        webRtc: "not_requested",
        safeguards: [],
      },
    },
    vault: { state: "unlocked", autoLockAt: "2026-07-28T12:10:00.000Z" },
    ...overrides,
  };
}

describe("network evidence handoff eligibility", () => {
  it("accepts only a fresh, unlocked and running Silo snapshot", () => {
    expect(
      eligibleSiloForNetworkEvidence(runtimeStatus(), requestId, now),
    ).toEqual({ siloId, runtimeId });
  });

  it("rejects stale, locked, idle and mismatched status responses", () => {
    expect(
      eligibleSiloForNetworkEvidence(
        runtimeStatus({ snapshotWrittenAt: "2026-07-28T11:59:00.000Z" }),
        requestId,
        now,
      ),
    ).toBeNull();
    expect(
      eligibleSiloForNetworkEvidence(
        runtimeStatus({ vault: { state: "locked", autoLockAt: null } }),
        requestId,
        now,
      ),
    ).toBeNull();
    expect(
      eligibleSiloForNetworkEvidence(
        runtimeStatus({
          vault: {
            state: "unlocked",
            autoLockAt: "2026-07-28T11:59:59.000Z",
          },
        }),
        requestId,
        now,
      ),
    ).toBeNull();
    expect(
      eligibleSiloForNetworkEvidence(
        runtimeStatus({
          activation: {
            activeSiloId: null,
            state: "idle",
            updatedAt: "2026-07-28T11:59:30.000Z",
            networkEvidence: null,
          },
        }),
        requestId,
        now,
      ),
    ).toBeNull();
    expect(
      eligibleSiloForNetworkEvidence(runtimeStatus(), crypto.randomUUID(), now),
    ).toBeNull();
  });

  it("parses only a handoff tied to the current network check", () => {
    const handoff = {
      state: "submitted",
      checkedAt: "2026-07-28T12:00:00.000Z",
      siloId,
      runtimeId,
      evidenceId: "6930220c-15a4-49e6-a310-b296e1499d27",
      acceptedAt: "2026-07-28T12:00:01.000Z",
      expiresAt: "2026-07-28T12:15:01.000Z",
    };
    expect(
      isNetworkEvidenceHandoffStatus(handoff, "2026-07-28T12:00:00.000Z"),
    ).toBe(true);
    expect(
      isNetworkEvidenceHandoffStatus(handoff, "2026-07-28T12:01:00.000Z"),
    ).toBe(false);
    expect(isNetworkEvidenceHandoffStatus({ ...handoff, unknown: true })).toBe(
      false,
    );
  });
});
