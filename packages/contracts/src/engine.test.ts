import { describe, expect, it } from "vitest";

import {
  CAMOUFOX_ARTIFACT_SCHEMA_V6,
  CAMOUFOX_ARTIFACT_SCHEMA_V5,
  camoufoxArtifactBindingV1Schema,
  camoufoxHostPackageManifestSchema,
  camoufoxHostPackageTreeManifestSchema,
  derivedIdentityTokenSchema,
  engineCapabilityStateSchema,
  enginePackageManifestSchema,
  engineRuntimeReceiptFrameSchema,
  identityDerivationContextSchema,
  identityTemplateSchema,
  siloEngineConfigSchema,
} from "./engine.js";

const template = {
  schemaVersion: 1 as const,
  templateId: "6b8a9da2-13e7-4f69-90cb-860f8d02e510",
  os: {
    family: "windows" as const,
    version: "11" as const,
    architecture: "x64" as const,
  },
  browser: {
    family: "chromium" as const,
    majorVersion: 150,
    userAgent:
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/150.0.0.0 Safari/537.36",
    uaCh: {
      brands: [{ brand: "Chromium", version: "150" }],
      platform: "Windows" as const,
      platformVersion: "15.0.0",
      architecture: "x86" as const,
      bitness: "64" as const,
      mobile: false,
    },
  },
  languages: { primary: "zh-CN", accepted: ["zh-CN", "en-US"] },
  timezone: "Asia/Singapore",
  screen: {
    width: 1920,
    height: 1080,
    availableWidth: 1920,
    availableHeight: 1040,
    devicePixelRatio: 1,
    colorDepth: 24 as const,
  },
  render: {
    canvas: "controlled" as const,
    webGlVendor: "Google Inc. (NVIDIA)",
    webGlRenderer: "ANGLE (NVIDIA GeForce RTX)",
  },
  fonts: { families: ["Segoe UI", "Arial"] },
  media: { microphones: 1, cameras: 1, speakers: 1, labelsExposed: false },
  network: {
    proxyRequired: true,
    countryCode: "SG",
    timezone: "Asia/Singapore",
    locale: "zh-CN",
    desiredQuic: "browser_default" as const,
  },
};

describe("EngineAdapter contracts", () => {
  it("keeps capability availability separate from runtime operation", () => {
    expect(
      engineCapabilityStateSchema.parse({
        id: "canvas",
        availability: "experimental",
        operation: "configured",
        reason: "A controlled engine package declared a configuration surface.",
        verifiedAt: null,
        evidence: [],
      }).operation,
    ).toBe("configured");
    expect(() =>
      engineCapabilityStateSchema.parse({
        id: "tls_client_hello",
        availability: "unavailable",
        operation: "verified",
        reason: "No packet evidence exists.",
        verifiedAt: new Date().toISOString(),
        evidence: ["configuration only"],
      }),
    ).toThrow(/unavailable/);
  });

  it("accepts a constrained identity and rejects cross-signal contradictions", () => {
    expect(identityTemplateSchema.parse(template).templateId).toBe(
      template.templateId,
    );
    expect(() =>
      identityTemplateSchema.parse({
        ...template,
        browser: { ...template.browser, majorVersion: 149 },
      }),
    ).toThrow(/majorVersion/);
    expect(() =>
      identityTemplateSchema.parse({
        ...template,
        network: { ...template.network, timezone: "America/New_York" },
      }),
    ).toThrow(/timezone/);
    expect(() =>
      identityTemplateSchema.parse({
        ...template,
        network: { ...template.network, desiredQuic: "disabled" },
      }),
    ).toThrow();
  });

  it("rejects package traversal, unknown fields and unsigned shapes", () => {
    const manifest = {
      schemaVersion: 2,
      engineId: "controlled-chromium",
      engineVersion: "150.0.0",
      channel: "experimental",
      platform: "windows-x64",
      executableRelativePath: "bin/chromium.exe",
      artifactSha256: "a".repeat(64),
      signature: {
        algorithm: "cms-detached-sha256",
        keyId: "b".repeat(64),
        value: "A".repeat(256),
      },
      capabilities: ["identity_template", "canvas", "site_fallback"],
    };
    expect(enginePackageManifestSchema.parse(manifest).engineId).toBe(
      "controlled-chromium",
    );
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        executableRelativePath: "../chromium.exe",
      }),
    ).toThrow(/executableRelativePath|Invalid enum/);
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        downloadUrl: "https://example.test",
      }),
    ).toThrow();
    expect(() =>
      enginePackageManifestSchema.parse({ ...manifest, signature: null }),
    ).toThrow();
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        signature: { ...manifest.signature, algorithm: "ed25519" },
      }),
    ).toThrow();
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        executableRelativePath: "bin/camoufox.exe",
      }),
    ).toThrow(/adapter/);
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        capabilities: [
          "identity_template",
          "tls_client_hello",
          "site_fallback",
        ],
      }),
    ).toThrow();
  });

  it("binds Camoufox Host v3 packages to a raw entrypoint and tree", () => {
    const manifest = {
      schemaVersion: 3,
      engineId: "camoufox",
      engineVersion: "152.0.4-beta.28",
      channel: "experimental",
      platform: "windows-x64",
      artifactSha256: "a".repeat(64),
      signature: {
        algorithm: "cms-detached-sha256",
        keyId: "b".repeat(64),
        value: "A".repeat(256),
      },
      capabilities: [
        "identity_template",
        "ua_ua_ch",
        "language_timezone",
        "screen",
        "canvas",
        "webgl",
        "fonts",
        "media_devices",
        "request_headers",
        "window",
        "iframe",
        "dedicated_worker",
      ],
      entrypoint: {
        kind: "camoufox-host-v1",
        relativePath: "host/camoufox-host.exe",
        protocol: "verisilo-camoufox-host/v1",
        sha256: "a".repeat(64),
      },
      treeManifest: {
        relativePath: "package-tree.json",
        sha256: "c".repeat(64),
      },
      browserTreeManifest: {
        relativePath: "browser-tree-manifest.json",
        sha256: "e".repeat(64),
      },
      hostVersion: "0.1.0",
      browserRelease: "v152.0.4-beta.28",
      browserAssetSha256: "d".repeat(64),
    };
    expect(
      camoufoxHostPackageManifestSchema.parse(manifest).entrypoint.kind,
    ).toBe("camoufox-host-v1");
    expect(enginePackageManifestSchema.parse(manifest).engineId).toBe(
      "camoufox",
    );
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        artifactSha256: "e".repeat(64),
      }),
    ).toThrow(/artifactSha256/);
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        capabilities: ["identity_template", "site_fallback"],
      }),
    ).toThrow(/site_fallback/);
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        entrypoint: { ...manifest.entrypoint, protocol: "native-bootstrap-v1" },
      }),
    ).toThrow();
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        browserRelease: "152.0.4-beta.28",
      }),
    ).toThrow(/browserRelease/);
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        browserTreeManifest: undefined,
      }),
    ).toThrow();
    expect(() =>
      enginePackageManifestSchema.parse({
        ...manifest,
        schemaVersion: 2,
        executableRelativePath: "bin/camoufox.exe",
      }),
    ).toThrow();
    expect(
      camoufoxHostPackageTreeManifestSchema.parse({
        schema: "verisilo-camoufox-host-package-tree/v1",
        entries: [{ path: "host/camoufox-host.exe", sha256: "a".repeat(64) }],
      }).entries.length,
    ).toBe(1);
  });

  it("allows only short-lived opaque seed-handle derivation contexts", () => {
    const issuedAt = "2026-07-28T00:00:00.000Z";
    expect(
      identityDerivationContextSchema.parse({
        siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
        seedReference: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        templateId: template.templateId,
        sessionId: "6930220c-15a4-49e6-a310-b296e1499d27",
        issuedAt,
        expiresAt: "2026-07-28T00:30:00.000Z",
      }).seedReference,
    ).toBe("7c9e6679-7425-40de-944b-e07fc1f90ae7");
    expect(() =>
      identityDerivationContextSchema.parse({
        siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
        seedReference: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        templateId: template.templateId,
        sessionId: "6930220c-15a4-49e6-a310-b296e1499d27",
        issuedAt,
        expiresAt: "2026-07-28T02:00:00.000Z",
        rawSeed: "must-not-exist",
      }),
    ).toThrow();
  });

  it("keeps token secrets native-only and strictly binds per-Silo adapters", () => {
    expect(
      derivedIdentityTokenSchema.parse({
        tokenId: "6930220c-15a4-49e6-a310-b296e1499d27",
        delivery: "secure_stdin_before_navigation",
        expiresAt: "2026-07-28T00:30:00.000Z",
      }).tokenId,
    ).toBe("6930220c-15a4-49e6-a310-b296e1499d27");
    expect(() =>
      derivedIdentityTokenSchema.parse({
        tokenId: "6930220c-15a4-49e6-a310-b296e1499d27",
        delivery: "secure_stdin_before_navigation",
        expiresAt: "2026-07-28T00:30:00.000Z",
        token: "must-never-reach-the-webview",
      }),
    ).toThrow();

    expect(siloEngineConfigSchema.parse({ adapter: "stock" })).toEqual({
      adapter: "stock",
    });
    expect(
      siloEngineConfigSchema.parse({
        adapter: "controlled-chromium",
        identityTemplate: template,
        fallbackRules: [],
      }).adapter,
    ).toBe("controlled-chromium");
    expect(siloEngineConfigSchema.parse({ adapter: "camoufox" })).toEqual({
      adapter: "camoufox",
    });
    expect(
      siloEngineConfigSchema.parse({
        adapter: "camoufox",
        artifactBinding: {
          artifactId: "identity-camoufox-m3",
          artifactFileSha256: "a".repeat(64),
          schema: "verisilo-camoufox-resolved-identity/v3",
        },
      }).artifactBinding?.artifactId,
    ).toBe("identity-camoufox-m3");
    expect(() =>
      siloEngineConfigSchema.parse({
        adapter: "camoufox",
        identityTemplate: template,
        fallbackRules: [],
      }),
    ).toThrow(/Unrecognized/);
    expect(
      camoufoxArtifactBindingV1Schema.parse({
        artifactId: "identity-camoufox-m3",
        artifactFileSha256: "a".repeat(64),
        schema: CAMOUFOX_ARTIFACT_SCHEMA_V6,
      }).schema,
    ).toBe(CAMOUFOX_ARTIFACT_SCHEMA_V6);
    expect(
      camoufoxArtifactBindingV1Schema.parse({
        artifactId: "identity-camoufox-m3",
        artifactFileSha256: "a".repeat(64),
        schema: CAMOUFOX_ARTIFACT_SCHEMA_V5,
      }).schema,
    ).toBe(CAMOUFOX_ARTIFACT_SCHEMA_V5);
    expect(() =>
      camoufoxArtifactBindingV1Schema.parse({
        artifactId: "identity-Camoufox",
        artifactFileSha256: "a".repeat(64),
        schema: "verisilo-camoufox-resolved-identity/v3",
      }),
    ).toThrow();
    expect(() =>
      siloEngineConfigSchema.parse({
        adapter: "controlled-chromium",
        identityTemplate: template,
      }),
    ).toThrow();
    expect(() =>
      siloEngineConfigSchema.parse({
        adapter: "controlled-chromium",
        identityTemplate: template,
        fallbackRules: [
          {
            sitePattern: "bad..example.test",
            disableCapabilities: ["canvas"],
            action: "restore_then_reload",
          },
        ],
      }),
    ).toThrow();
    expect(() =>
      siloEngineConfigSchema.parse({
        adapter: "arbitrary-adapter",
        launchArguments: ["--unsafe"],
      }),
    ).toThrow();
  });

  it("defines strict, short-lived and bounded runtime receipt frames", () => {
    const frame = {
      receiptVersion: 1 as const,
      contractVersion: 1 as const,
      adapterId: "controlled-chromium" as const,
      siloId: "0f8fad5b-d9cb-469f-a165-70867728950e",
      sessionId: "6930220c-15a4-49e6-a310-b296e1499d27",
      tokenId: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
      package: {
        engineVersion: "150.0.0",
        artifactSha256: "a".repeat(64),
        verifierId: "pinned-cms-verifier",
        verifiedAt: "2026-07-28T12:00:00.000Z",
      },
      sequence: 1,
      issuedAt: "2026-07-28T12:00:01.000Z",
      expiresAt: "2026-07-28T12:00:11.000Z",
      receipt: {
        kind: "phase" as const,
        phase: "observe" as const,
        capabilities: [
          { id: "canvas" as const, evidence: ["canvas baseline observed"] },
        ],
      },
    };

    expect(engineRuntimeReceiptFrameSchema.parse(frame).sequence).toBe(1);
    expect(() =>
      engineRuntimeReceiptFrameSchema.parse({
        ...frame,
        token: "must-never-appear",
      }),
    ).toThrow();
    expect(() =>
      engineRuntimeReceiptFrameSchema.parse({
        ...frame,
        receipt: { ...frame.receipt, unexpected: true },
      }),
    ).toThrow();
    expect(() =>
      engineRuntimeReceiptFrameSchema.parse({
        ...frame,
        expiresAt: "2026-07-28T12:00:32.000Z",
      }),
    ).toThrow(/30 seconds/);
    expect(() =>
      engineRuntimeReceiptFrameSchema.parse({
        ...frame,
        receipt: {
          ...frame.receipt,
          capabilities: Array.from({ length: 17 }, (_, capabilityIndex) => ({
            id: "canvas" as const,
            evidence: Array.from(
              { length: 16 },
              (_, evidenceIndex) =>
                `${capabilityIndex}-${evidenceIndex}-${"x".repeat(490)}`,
            ),
          })),
        },
      }),
    ).toThrow(/32 KiB/);
  });
});
