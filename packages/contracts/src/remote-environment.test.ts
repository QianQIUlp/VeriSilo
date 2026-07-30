import { describe, expect, it } from "vitest";

import {
  REMOTE_AGENT_MAX_ACTIVITY_ENTRIES,
  REMOTE_AGENT_MAX_AUTOMATION_SECONDS,
  REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS,
  REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS,
  REMOTE_AGENT_MAX_INPUT_EVENTS,
  REMOTE_ENVIRONMENT_MAX_PAIRING_TOKEN_LIFETIME_MS,
  remoteActivityLogSchema,
  remoteAgentRequestSchema,
  remoteAgentResponseEnvelopeSchema,
  remoteAgentResponseSchema,
  remoteAutomationAuthorizationSchema,
  remoteBackendSnapshotSchema,
  remoteCapabilitySetSchema,
  remoteDeletionProofSchema,
  remoteEndpointSchema,
  remoteEnvironmentRecordSchema,
  remoteGuestEvidenceSchema,
  remoteNodeDisclosureSchema,
  remoteOperationRequestSchema,
  remoteOperationResultSchema,
  remotePairingRequestSchema,
  remotePairingResponseSchema,
  remoteSessionAuthorizationSchema,
  remoteSiloBindingSchema,
  remoteVolumeAttestationSchema,
  requiredRemoteProxyHasGuestEvidence,
} from "./remote-environment.js";

const siloId = "0f8fad5b-d9cb-469f-a165-70867728950e";
const bindingId = "6b8a9da2-13e7-4f69-90cb-860f8d02e510";
const remoteEnvironmentId = "2d931510-d99f-494a-8c67-87feb05e1594";
const policyId = "73c16720-9a53-4e4f-a6c1-4c34bc02d638";
const nodeId = "a80b7719-36b4-4cfa-97fd-2851dd7fd6a2";
const volumeId = "86ca5097-2c75-4a51-9576-cc0f5ddc1093";
const keyId = "524cbee2-2464-4b36-b268-47f9c82a880e";
const authorizationId = "c0f7f26d-4c92-4bc3-92af-a3e5f50230f6";
const credentialId = "265e41db-3707-4f68-b69a-e821a67ef6b1";
const requestId = "0d3f7ba5-f545-4aa4-836a-99f6af661775";

function evidence() {
  return {
    protocolVersion: 1 as const,
    evidenceId: "5d5f21ae-c0a3-4c0c-96c1-674570618fbd",
    bindingId,
    remoteEnvironmentId,
    source: "guest_agent" as const,
    sequence: 7,
    observedAtUnixMs: 1_785_196_800_000,
    proxy: { state: "enforced" as const, policyId },
    exit: {
      state: "verified" as const,
      publicAddresses: ["203.0.113.7"],
    },
    dns: {
      state: "verified" as const,
      resolvers: ["192.0.2.53"],
      leakDetected: false,
    },
    webRtc: {
      state: "verified" as const,
      observedCandidates: ["relay 203.0.113.7"],
      leakDetected: false,
    },
    health: {
      state: "healthy" as const,
      agentVersion: "0.9.0-prototype",
      checks: ["browser process owned by guest agent"],
    },
  };
}

describe("remote environment V0.9 contract", () => {
  it("accepts only a pinned user-self-hosted HTTPS origin", () => {
    const pin = { kind: "spki_sha256", sha256: "a".repeat(64) };
    expect(
      remoteEndpointSchema.parse({
        ownership: "user_self_hosted",
        origin: "https://browser.example.test:8443",
        pin,
      }).origin,
    ).toBe("https://browser.example.test:8443");

    for (const origin of [
      "http://browser.example.test",
      "https://user:secret@browser.example.test",
      "https://browser.example.test/api",
    ]) {
      expect(() =>
        remoteEndpointSchema.parse({
          ownership: "user_self_hosted",
          origin,
          pin,
        }),
      ).toThrow();
    }
    expect(() =>
      remoteEndpointSchema.parse({
        ownership: "vendor_hosted",
        origin: "https://browser.example.test",
        pin,
      }),
    ).toThrow();
  });

  it("requires explicit approval and a token lifetime no longer than five minutes", () => {
    const request = {
      protocolVersion: 1 as const,
      requestId: "0d3f7ba5-f545-4aa4-836a-99f6af661775",
      nonce: "n".repeat(32),
      sentAtUnixMs: 1_000_000,
      body: {
        operation: "pair" as const,
        approvedByUser: true as const,
        pairingTokenId: "748b1e8d-05c6-49df-90e7-850dd30d1a1c",
        pairingToken: "t".repeat(32),
        pairingTokenExpiresAtUnixMs:
          1_000_000 + REMOTE_ENVIRONMENT_MAX_PAIRING_TOKEN_LIFETIME_MS,
      },
    };
    expect(remotePairingRequestSchema.parse(request)).toEqual(request);
    expect(() =>
      remotePairingRequestSchema.parse({
        ...request,
        body: { ...request.body, approvedByUser: false },
      }),
    ).toThrow();
    expect(() =>
      remotePairingRequestSchema.parse({
        ...request,
        body: {
          ...request.body,
          pairingTokenExpiresAtUnixMs:
            request.sentAtUnixMs +
            REMOTE_ENVIRONMENT_MAX_PAIRING_TOKEN_LIFETIME_MS +
            1,
        },
      }),
    ).toThrow(/five minutes/);
  });

  it("negotiates all nine capabilities and preserves unavailable as data", () => {
    const capabilities = [
      "create",
      "start",
      "stop",
      "pause",
      "snapshot",
      "destroy",
      "configureNetwork",
      "health",
      "logs",
    ].map((operation) => ({
      operation,
      availability: ["pause", "snapshot"].includes(operation)
        ? { availability: "unavailable", reason: "Provider has no pause API." }
        : { availability: "available" },
    }));
    expect(remoteCapabilitySetSchema.parse(capabilities)).toHaveLength(9);
    expect(() =>
      remoteCapabilitySetSchema.parse(
        capabilities.map((capability) => ({
          ...capability,
          operation: "start",
        })),
      ),
    ).toThrow(/exactly once/);
  });

  it("keeps pairing, request sequencing, volume evidence, and stored bindings aligned", () => {
    const capabilities = [
      "create",
      "start",
      "stop",
      "pause",
      "snapshot",
      "destroy",
      "configureNetwork",
      "health",
      "logs",
    ].map((operation) => ({
      operation,
      availability: { availability: "available" as const },
    }));
    const node = {
      nodeId,
      ownership: "user_self_hosted" as const,
      operatorLabel: "My Windows host",
      dataRegion: "home-sg",
      keyCustody: "user_controlled" as const,
      cost: {
        currency: "USD",
        estimatedMicrosPerHour: 125_000,
        notice: "Estimated infrastructure cost; no VeriSilo hosting fee.",
      },
    };
    const pairing = {
      protocolVersion: 1 as const,
      responseId: "4385fe89-dc5f-48cb-a3b8-1a130b471cb0",
      inReplyTo: requestId,
      nonce: "r".repeat(32),
      sentAtUnixMs: 1_785_196_800_000,
      sequence: 1,
      body: {
        status: "success" as const,
        serverId: "331f2604-53ce-4b5e-8527-b9c3f3498b70",
        clientCredentialId: credentialId,
        node,
        clientCredential: "c".repeat(64),
        credentialExpiresAtUnixMs: 1_787_788_800_000,
        capabilities,
      },
    };
    expect(remotePairingResponseSchema.parse(pairing)).toEqual(pairing);
    expect(() =>
      remotePairingResponseSchema.parse({
        ...pairing,
        body: { ...pairing.body, node: undefined },
      }),
    ).toThrow();

    const createRequest = {
      protocolVersion: 1 as const,
      requestId,
      nonce: "n".repeat(32),
      sequence: 1,
      sentAtUnixMs: 1_785_196_800_000,
      siloId,
      body: {
        operation: "create" as const,
        network: { mode: "direct" as const },
        ttlSeconds: 3_600,
        costAcknowledged: true as const,
      },
    };
    expect(remoteOperationRequestSchema.parse(createRequest)).toEqual(
      createRequest,
    );
    expect(() =>
      remoteOperationRequestSchema.parse({ ...createRequest, sequence: 0 }),
    ).toThrow();
    expect(() =>
      remoteOperationRequestSchema.parse({
        ...createRequest,
        body: { ...createRequest.body, costAcknowledged: false },
      }),
    ).toThrow();
    const recoveryRequest = {
      ...createRequest,
      body: {
        operation: "destroy" as const,
        bindingId,
        remoteEnvironmentId,
        confirmDestroy: false,
      },
    };
    expect(remoteOperationRequestSchema.parse(recoveryRequest)).toEqual(
      recoveryRequest,
    );

    const result = {
      operation: "create" as const,
      siloId,
      bindingId,
      remoteEnvironmentId,
      serverId: pairing.body.serverId,
      lastActivityAtUnixMs: 1_785_196_800_000,
      state: "created" as const,
      volume: volumeAttestation(),
      evidence: evidence(),
    };
    expect(remoteOperationResultSchema.parse(result)).toEqual(result);
    expect(() =>
      remoteOperationResultSchema.parse({ ...result, volume: undefined }),
    ).toThrow(/volume attestation/);
    expect(() =>
      remoteOperationResultSchema.parse({
        ...result,
        operation: "stop",
        state: "stopped",
      }),
    ).toThrow(/Only create/);

    const binding = {
      siloId,
      bindingId,
      remoteEnvironmentId,
      serverId: pairing.body.serverId,
      endpoint: {
        ownership: "user_self_hosted" as const,
        origin: "https://browser.example.test",
        pin: { kind: "spki_sha256" as const, sha256: "a".repeat(64) },
      },
      network: { mode: "direct" as const },
      volume: volumeAttestation(),
      lastActivityAtUnixMs: 1_785_196_800_000,
      automationAuthorizations: [],
      lastEvidence: evidence(),
    };
    expect(remoteSiloBindingSchema.parse(binding)).toEqual(binding);

    const snapshot = {
      pairing: {
        serverId: pairing.body.serverId,
        clientCredentialId: pairing.body.clientCredentialId,
        node,
        clientCredential: pairing.body.clientCredential,
        credentialExpiresAtUnixMs: pairing.body.credentialExpiresAtUnixMs,
        capabilities,
        lastClientSequence: 7,
        lastServerSequence: 9,
      },
      usedPairingTokenIds: ["748b1e8d-05c6-49df-90e7-850dd30d1a1c"],
      bindings: [binding],
    };
    expect(remoteBackendSnapshotSchema.parse(snapshot)).toEqual(snapshot);
    expect(() =>
      remoteBackendSnapshotSchema.parse({
        ...snapshot,
        usedPairingTokenIds: [
          snapshot.usedPairingTokenIds[0],
          snapshot.usedPairingTokenIds[0],
        ],
      }),
    ).toThrow(/unique/);
  });

  it("has no command, shell, argument-list, or path escape hatch", () => {
    const start = {
      protocolVersion: 1,
      requestId: "0d3f7ba5-f545-4aa4-836a-99f6af661775",
      nonce: "n".repeat(32),
      sequence: 1,
      sentAtUnixMs: 1_785_196_800_000,
      siloId,
      body: {
        operation: "start",
        bindingId,
        remoteEnvironmentId,
      },
    };
    expect(remoteOperationRequestSchema.parse(start).body.operation).toBe(
      "start",
    );
    for (const forbidden of ["command", "shell", "args", "path"]) {
      expect(() =>
        remoteOperationRequestSchema.parse({
          ...start,
          body: { ...start.body, [forbidden]: "/tmp/or sh -c" },
        }),
      ).toThrow();
    }
    expect(() =>
      remoteOperationRequestSchema.parse({
        ...start,
        body: {
          operation: "logs",
          bindingId,
          remoteEnvironmentId,
          cursor: "/tmp/server-log",
          limit: 20,
        },
      }),
    ).toThrow();
  });

  it("rejects unknown fields, unknown versions, and over-limit logs", () => {
    expect(() =>
      remoteOperationRequestSchema.parse({
        protocolVersion: 2,
        requestId: "0d3f7ba5-f545-4aa4-836a-99f6af661775",
        nonce: "n".repeat(32),
        sequence: 1,
        sentAtUnixMs: 1_785_196_800_000,
        siloId,
        body: {
          operation: "logs",
          bindingId,
          remoteEnvironmentId,
          limit: 201,
        },
        forwardCompatibleGuess: true,
      }),
    ).toThrow();
  });

  it("fails required proxy verification closed on DNS or WebRTC leakage", () => {
    const binding = {
      bindingId,
      remoteEnvironmentId,
      network: { mode: "fixed_proxy" as const, required: true, policyId },
    };
    const validEvidence = remoteGuestEvidenceSchema.parse(evidence());
    expect(requiredRemoteProxyHasGuestEvidence(binding, validEvidence)).toBe(
      true,
    );
    expect(
      requiredRemoteProxyHasGuestEvidence(binding, {
        ...validEvidence,
        dns: { ...validEvidence.dns, leakDetected: true },
      }),
    ).toBe(false);
    expect(
      requiredRemoteProxyHasGuestEvidence(binding, {
        ...validEvidence,
        webRtc: { ...validEvidence.webRtc, state: "unavailable" },
      }),
    ).toBe(false);
    expect(requiredRemoteProxyHasGuestEvidence(binding, undefined)).toBe(false);
  });
});

function controlRequest(command: Record<string, unknown>) {
  return {
    protocolVersion: 1 as const,
    requestId,
    nonce: "n".repeat(32),
    sequence: 1,
    sentAtUnixMs: 1_785_196_800_000,
    principal: {
      kind: "control_plane" as const,
      credentialId,
      authorizationId: null,
    },
    command,
  };
}

function volumeAttestation() {
  return {
    encrypted: true as const,
    keyCustody: "user_controlled" as const,
    volumeId,
    keyId,
  };
}

describe("remote Agent V0.9 domain contract", () => {
  it("requires a self-hosted node, user-held keys, and an explicit cost disclosure", () => {
    const node = {
      nodeId,
      ownership: "user_self_hosted" as const,
      operatorLabel: "My Windows host",
      dataRegion: "home-sg",
      keyCustody: "user_controlled" as const,
      cost: {
        currency: "USD",
        estimatedMicrosPerHour: 125_000,
        notice: "Estimated infrastructure cost; no VeriSilo hosting fee.",
      },
    };
    expect(remoteNodeDisclosureSchema.parse(node)).toEqual(node);
    expect(() =>
      remoteNodeDisclosureSchema.parse({
        ...node,
        ownership: "vendor_hosted",
      }),
    ).toThrow();
    expect(() =>
      remoteNodeDisclosureSchema.parse({
        ...node,
        operatorLabel: " padded ",
      }),
    ).toThrow(/without surrounding whitespace/);
    expect(() =>
      remoteNodeDisclosureSchema.parse({
        ...node,
        operatorLabel: "界".repeat(41),
      }),
    ).toThrow(/UTF-8 bytes/);
    expect(() =>
      remoteNodeDisclosureSchema.parse({
        ...node,
        cost: { ...node.cost, currency: "usd" },
      }),
    ).toThrow();
    expect(() =>
      remoteNodeDisclosureSchema.parse({ ...node, managedByVeriSilo: true }),
    ).toThrow();
  });

  it("only accepts affirmative encrypted-volume evidence and coherent deletion records", () => {
    expect(remoteVolumeAttestationSchema.parse(volumeAttestation())).toEqual(
      volumeAttestation(),
    );
    expect(() =>
      remoteVolumeAttestationSchema.parse({
        ...volumeAttestation(),
        encrypted: false,
      }),
    ).toThrow();
    expect(() =>
      remoteVolumeAttestationSchema.parse({
        ...volumeAttestation(),
        keyId: "00000000-0000-0000-0000-000000000000",
      }),
    ).toThrow(/nil UUID/);

    const record = {
      siloId,
      bindingId,
      remoteEnvironmentId,
      nodeId,
      state: "created" as const,
      network: { mode: "direct" as const },
      volume: volumeAttestation(),
      createdAtUnixMs: 1_000_000,
      expiresAtUnixMs: 1_060_000,
      lastActivityAtUnixMs: 1_000_000,
      deletionProofId: null,
    };
    expect(remoteEnvironmentRecordSchema.parse(record)).toEqual(record);
    expect(() =>
      remoteEnvironmentRecordSchema.parse({
        ...record,
        expiresAtUnixMs:
          record.createdAtUnixMs +
          (REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS + 1) * 1_000,
      }),
    ).toThrow(/30 days/);
    expect(() =>
      remoteEnvironmentRecordSchema.parse({
        ...record,
        state: "deleted",
      }),
    ).toThrow(/deletion proof/);

    const proof = {
      proofId: "7f94d6b8-5284-4630-ae77-930344e2ce87",
      siloId,
      bindingId,
      remoteEnvironmentId,
      volumeId,
      providerReceiptId: "9ced6158-b1c0-4d58-a3b9-95baeeaa1433",
      resourceDeletions: [
        {
          kind: "compute_instance" as const,
          resourceId: remoteEnvironmentId,
          status: "deleted" as const,
        },
        {
          kind: "persistent_volume" as const,
          resourceId: volumeId,
          status: "deleted" as const,
        },
        {
          kind: "snapshot" as const,
          status: "not_applicable" as const,
        },
        {
          kind: "ephemeral_key" as const,
          resourceId: keyId,
          status: "deleted" as const,
        },
      ],
      deletedAtUnixMs: 1_060_000,
      reason: "user_confirmed" as const,
    };
    expect(remoteDeletionProofSchema.parse(proof)).toEqual(proof);
    expect(
      remoteDeletionProofSchema.parse({
        ...proof,
        reason: "provider_policy",
      }).reason,
    ).toBe("provider_policy");
    expect(() =>
      remoteDeletionProofSchema.parse({
        ...proof,
        resourceDeletions: proof.resourceDeletions.slice(0, 3),
      }),
    ).toThrow();
    expect(() =>
      remoteDeletionProofSchema.parse({
        ...proof,
        resourceDeletions: [
          ...proof.resourceDeletions.slice(0, 3),
          proof.resourceDeletions[0],
        ],
      }),
    ).toThrow(/exactly once/);
    expect(() =>
      remoteDeletionProofSchema.parse({
        ...proof,
        resourceDeletions: proof.resourceDeletions.map((resource) =>
          resource.kind === "snapshot"
            ? { ...resource, kind: "unknown_resource" }
            : resource,
        ),
      }),
    ).toThrow();
    expect(() =>
      remoteDeletionProofSchema.parse({
        ...proof,
        resourceDeletions: proof.resourceDeletions.map((resource) =>
          resource.kind === "snapshot"
            ? { ...resource, status: "unknown_status" }
            : resource,
        ),
      }),
    ).toThrow();
    expect(() =>
      remoteDeletionProofSchema.parse({
        ...proof,
        resourcesDeleted: ["legacy-untyped-resource"],
      }),
    ).toThrow();
  });

  it("separates bounded human sessions from short, explicitly scoped automation", () => {
    const issuedAtUnixMs = 1_000_000;
    const session = {
      authorizationId,
      siloId,
      remoteEnvironmentId,
      issuedAtUnixMs,
      expiresAtUnixMs:
        issuedAtUnixMs + REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS * 1_000,
      revoked: false,
    };
    expect(remoteSessionAuthorizationSchema.parse(session)).toEqual(session);
    expect(() =>
      remoteSessionAuthorizationSchema.parse({
        ...session,
        expiresAtUnixMs: session.expiresAtUnixMs + 1,
      }),
    ).toThrow(/Authorization lifetime/);

    const automation = {
      ...session,
      expiresAtUnixMs:
        issuedAtUnixMs + REMOTE_AGENT_MAX_AUTOMATION_SECONDS * 1_000,
      scopes: ["read_screen", "send_input"] as const,
      approvedByUser: true as const,
    };
    expect(remoteAutomationAuthorizationSchema.parse(automation)).toEqual(
      automation,
    );
    expect(() =>
      remoteAutomationAuthorizationSchema.parse({
        ...automation,
        scopes: ["read_screen", "read_screen"],
      }),
    ).toThrow(/unique/);
    expect(() =>
      remoteAutomationAuthorizationSchema.parse({
        ...automation,
        approvedByUser: false,
      }),
    ).toThrow();
  });

  it("bounds environment/session TTLs and requires affirmative user decisions", () => {
    const create = controlRequest({
      operation: "create",
      siloId,
      bindingId,
      remoteEnvironmentId,
      ttlSeconds: REMOTE_AGENT_MAX_ENVIRONMENT_TTL_SECONDS,
      network: { mode: "direct" },
      costAcknowledged: true,
    });
    expect(remoteAgentRequestSchema.parse(create)).toEqual(create);
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...create,
        command: { ...create.command, ttlSeconds: 59 },
      }),
    ).toThrow();
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...create,
        command: { ...create.command, costAcknowledged: false },
      }),
    ).toThrow();

    const openSession = controlRequest({
      operation: "openHumanSession",
      siloId,
      lifetimeSeconds: REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS,
    });
    expect(remoteAgentRequestSchema.parse(openSession)).toEqual(openSession);
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...openSession,
        command: {
          ...openSession.command,
          lifetimeSeconds: REMOTE_AGENT_MAX_HUMAN_SESSION_SECONDS + 1,
        },
      }),
    ).toThrow();

    const grant = controlRequest({
      operation: "grantAutomation",
      siloId,
      lifetimeSeconds: REMOTE_AGENT_MAX_AUTOMATION_SECONDS,
      scopes: ["read_screen", "send_input"],
      approvedByUser: true,
    });
    expect(remoteAgentRequestSchema.parse(grant)).toEqual(grant);
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...grant,
        command: {
          ...grant.command,
          scopes: ["send_input", "send_input"],
        },
      }),
    ).toThrow(/unique/);
  });

  it("uses typed screen/input commands with bounded payloads and no shell or path escape", () => {
    const automationPrincipal = {
      kind: "automation" as const,
      credentialId,
      authorizationId,
    };
    const sendInput = {
      ...controlRequest({
        operation: "sendInput",
        siloId,
        events: [
          { type: "key", code: "KeyA", pressed: true },
          { type: "pointer_move", x: 100, y: 200 },
          { type: "pointer_button", button: "primary", pressed: false },
          { type: "text", value: "hello\nworld" },
        ],
      }),
      principal: automationPrincipal,
    };
    expect(remoteAgentRequestSchema.parse(sendInput)).toEqual(sendInput);

    expect(() =>
      remoteAgentRequestSchema.parse({
        ...sendInput,
        command: {
          operation: "sendInput",
          siloId,
          events: Array.from(
            { length: REMOTE_AGENT_MAX_INPUT_EVENTS + 1 },
            () => ({ type: "key", code: "KeyA", pressed: true }),
          ),
        },
      }),
    ).toThrow();
    for (const badEvent of [
      { type: "key", code: "Key-A", pressed: true },
      { type: "pointer_move", x: 16_385, y: 0 },
      { type: "text", value: "bad\u0000input" },
    ]) {
      expect(() =>
        remoteAgentRequestSchema.parse({
          ...sendInput,
          command: { operation: "sendInput", siloId, events: [badEvent] },
        }),
      ).toThrow();
    }

    const start = controlRequest({ operation: "start", siloId });
    for (const forbidden of ["command", "shell", "args", "path"] as const) {
      expect(() =>
        remoteAgentRequestSchema.parse({
          ...start,
          command: {
            ...start.command,
            [forbidden]: "C:\\Windows\\System32\\cmd.exe",
          },
        }),
      ).toThrow();
    }
  });

  it("enforces principal roles, positive sequence numbers, and the 64 KiB envelope", () => {
    const start = controlRequest({ operation: "start", siloId });
    expect(() =>
      remoteAgentRequestSchema.parse({ ...start, sequence: 0 }),
    ).toThrow();
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...start,
        principal: {
          kind: "automation",
          credentialId,
          authorizationId,
        },
      }),
    ).toThrow(/not authorized/);
    expect(() =>
      remoteAgentRequestSchema.parse({
        ...start,
        principal: {
          kind: "control_plane",
          credentialId,
          authorizationId,
        },
      }),
    ).toThrow();

    const oversized = {
      ...start,
      principal: {
        kind: "automation" as const,
        credentialId,
        authorizationId,
      },
      command: {
        operation: "sendInput",
        siloId,
        events: Array.from({ length: REMOTE_AGENT_MAX_INPUT_EVENTS }, () => ({
          type: "text",
          value: "x".repeat(512),
        })),
      },
    };
    expect(() => remoteAgentRequestSchema.parse(oversized)).toThrow(/64 KiB/);
  });

  it("bounds the activity ledger and authenticates screen-channel response shape", () => {
    const activity = {
      activityId: "31c67fef-a414-4bf6-8729-dc91d0c83d05",
      siloId,
      principal: "human_session" as const,
      operation: "open_screen" as const,
      accepted: true,
      occurredAtUnixMs: 1_000_000,
    };
    expect(remoteActivityLogSchema.parse([activity])).toEqual([activity]);
    expect(() =>
      remoteActivityLogSchema.parse(
        Array.from(
          { length: REMOTE_AGENT_MAX_ACTIVITY_ENTRIES + 1 },
          () => activity,
        ),
      ),
    ).toThrow();

    const response = {
      type: "screen" as const,
      channel: {
        channelId: "9a451e62-1036-41ad-bbad-687f1875df3c",
        remoteEnvironmentId,
        authorizationId,
        expiresAtUnixMs: 1_300_000,
        transport: "authenticated_encrypted_stream" as const,
      },
    };
    expect(remoteAgentResponseSchema.parse(response)).toEqual(response);
    const envelope = {
      protocolVersion: 1 as const,
      responseId: "4385fe89-dc5f-48cb-a3b8-1a130b471cb0",
      inReplyTo: requestId,
      nonce: "r".repeat(32),
      sentAtUnixMs: 1_785_196_800_000,
      sequence: 12,
      body: { status: "success" as const, response },
    };
    expect(remoteAgentResponseEnvelopeSchema.parse(envelope)).toEqual(envelope);
    expect(() =>
      remoteAgentResponseEnvelopeSchema.parse({
        ...envelope,
        body: {
          status: "rejected",
          code: "arbitrary_server_error",
          message: "not allowed",
        },
      }),
    ).toThrow();
    expect(() =>
      remoteAgentResponseSchema.parse({
        ...response,
        channel: { ...response.channel, transport: "plain_websocket" },
      }),
    ).toThrow();
    expect(() =>
      remoteAgentResponseSchema.parse({
        type: "logs",
        entries: ["x".repeat(1_025)],
        lastActivityAtUnixMs: 1_000_000,
      }),
    ).toThrow(/1,024 UTF-8 bytes/);
  });
});
