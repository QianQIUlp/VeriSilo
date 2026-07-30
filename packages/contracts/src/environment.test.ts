import { describe, expect, it } from "vitest";

import {
  environmentBackendStatusSchema,
  environmentOperationRequestSchema,
  guestNetworkEvidenceSchema,
  requiredProxyHasGuestEvidence,
} from "./environment.js";

const environmentId = "0f8fad5b-d9cb-469f-a165-70867728950e";
const runtimeId = "6b8a9da2-13e7-4f69-90cb-860f8d02e510";
const profilePath = `/var/lib/verisilo/silos/${environmentId}/chromium-profile`;
const agentSha256 = "a".repeat(64);
const evidenceBinding = { runtimeId, profilePath, agentSha256 };

describe("environment backend contracts", () => {
  it("requires every lifecycle capability exactly once", () => {
    const operations = [
      "create",
      "start",
      "stop",
      "pause",
      "snapshot",
      "destroy",
      "configureNetwork",
      "health",
      "logs",
    ] as const;
    expect(
      environmentBackendStatusSchema.parse({
        contractVersion: 1,
        backend: "windows-sandbox",
        capabilities: operations.map((operation) => ({
          operation,
          availability: ["pause", "snapshot"].includes(operation)
            ? {
                availability: "unavailable" as const,
                reason: "Disposable backend does not implement this operation.",
              }
            : { availability: "available" as const },
        })),
        prerequisites: [],
      }).capabilities,
    ).toHaveLength(9);

    expect(() =>
      environmentBackendStatusSchema.parse({
        contractVersion: 1,
        backend: "windows-sandbox",
        capabilities: operations.map(() => ({
          operation: "start",
          availability: { availability: "available" },
        })),
        prerequisites: [],
      }),
    ).toThrow(/exactly once/);
  });

  it("makes destructive confirmation part of the request schema", () => {
    expect(
      environmentOperationRequestSchema.parse({
        operation: "destroy",
        backend: "hyper-v",
        environmentId,
        confirmDestroy: true,
      }).operation,
    ).toBe("destroy");
    expect(() =>
      environmentOperationRequestSchema.parse({
        operation: "destroy",
        backend: "hyper-v",
        environmentId,
        confirmDestroy: false,
      }),
    ).toThrow();
  });

  it("accepts no host-origin alternative for guest exit and DNS evidence", () => {
    const evidence = {
      schemaVersion: 1 as const,
      evidenceId: "350fe840-911f-42dc-847d-8c2157396b74",
      environmentId,
      source: "guest_agent" as const,
      runtimeId,
      profilePath,
      proxyPort: 7890,
      agentSha256,
      proxy: "verified" as const,
      exit: "verified" as const,
      proxyDns: "verified" as const,
      guestResolver: "unavailable" as const,
      observedAt: "2026-07-28T00:00:00.000Z",
      validUntil: "2026-07-28T00:02:00.000Z",
    };
    expect(guestNetworkEvidenceSchema.parse(evidence)).toEqual(evidence);
    expect(() =>
      guestNetworkEvidenceSchema.parse({ ...evidence, source: "host" }),
    ).toThrow();
    expect(() =>
      guestNetworkEvidenceSchema.parse({
        ...evidence,
        agentSha256: "A".repeat(64),
      }),
    ).toThrow();
  });

  it("fails required proxies closed without matching complete guest evidence", () => {
    const network = {
      mode: "fixed_proxy" as const,
      proxyRequired: true,
      scheme: "socks5" as const,
      host: "127.0.0.1",
      port: 7890,
    };
    const validEvidence = {
      schemaVersion: 1 as const,
      evidenceId: "350fe840-911f-42dc-847d-8c2157396b74",
      environmentId,
      source: "guest_agent" as const,
      runtimeId,
      profilePath,
      proxyPort: 7890,
      agentSha256,
      proxy: "verified" as const,
      exit: "verified" as const,
      proxyDns: "verified" as const,
      guestResolver: "unavailable" as const,
      observedAt: "2026-07-28T00:00:00.000Z",
      validUntil: "2026-07-28T00:02:00.000Z",
    };
    expect(
      requiredProxyHasGuestEvidence(
        environmentId,
        network,
        undefined,
        evidenceBinding,
      ),
    ).toBe(false);
    expect(
      requiredProxyHasGuestEvidence(
        environmentId,
        network,
        { ...validEvidence, guestResolver: "verified" },
        evidenceBinding,
        new Date("2026-07-28T00:01:00.000Z"),
      ),
    ).toBe(false);
    expect(
      requiredProxyHasGuestEvidence(
        environmentId,
        network,
        validEvidence,
        evidenceBinding,
        new Date("2026-07-28T00:01:00.000Z"),
      ),
    ).toBe(true);
    expect(
      requiredProxyHasGuestEvidence(
        environmentId,
        network,
        { ...validEvidence, runtimeId: environmentId },
        evidenceBinding,
        new Date("2026-07-28T00:01:00.000Z"),
      ),
    ).toBe(false);
    expect(
      requiredProxyHasGuestEvidence(
        environmentId,
        network,
        {
          schemaVersion: 1,
          evidenceId: "350fe840-911f-42dc-847d-8c2157396b74",
          environmentId,
          source: "guest_agent",
          runtimeId,
          profilePath,
          proxyPort: 7890,
          agentSha256,
          proxy: "verified",
          exit: "verified",
          proxyDns: "failed",
          guestResolver: "unavailable",
          observedAt: "2026-07-28T00:00:00.000Z",
          validUntil: "2026-07-28T00:02:00.000Z",
        },
        evidenceBinding,
      ),
    ).toBe(false);
  });

  it("rejects unknown operation fields and command-like network input", () => {
    expect(() =>
      environmentOperationRequestSchema.parse({
        operation: "start",
        backend: "wsl-chromium",
        environmentId,
        command: "sh -c anything",
      }),
    ).toThrow();
    expect(() =>
      environmentOperationRequestSchema.parse({
        operation: "create",
        backend: "wsl-chromium",
        environmentId,
        network: {
          mode: "fixed_proxy",
          proxyRequired: true,
          scheme: "socks5",
          host: "127.0.0.1; touch /tmp/pwned",
          port: 7890,
        },
      }),
    ).toThrow();
  });
});
