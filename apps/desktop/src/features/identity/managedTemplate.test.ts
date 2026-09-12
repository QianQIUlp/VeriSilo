import type { Silo } from "@verisilo/contracts";
import { describe, expect, it } from "vitest";
import type { ManagedIdentityPreview } from "../../desktop-api.js";
import {
  managedCreateTemplateFromSilo,
  newIdentityNameFor,
  type ManagedSiloTemplate,
} from "./managedTemplate.js";

const preview: ManagedIdentityPreview = {
  userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
  language: "zh-CN",
  timezone: "Asia/Shanghai",
  screenWidth: 1920,
  screenHeight: 1080,
  hardwareConcurrency: 8,
  webglVendor: "NVIDIA Corporation",
  webglRenderer: "NVIDIA GeForce RTX 3060, or similar",
  platform: "Win32",
  countryCode: null,
  publicAddress: null,
  latitude: null,
  longitude: null,
  networkBound: false,
};

const managedSilo = (overrides: Partial<Silo>): Silo => ({
  id: "11111111-1111-4111-8111-111111111111",
  schemaVersion: 3,
  name: "工作空间",
  color: "#5b5ce2",
  browser: null,
  executionTarget: { kind: "local" },
  profileDirectory: "C:\\Vault\\profiles\\source",
  networkProfile: { mode: "direct", proxyRequired: false },
  engine: {
    adapter: "camoufox",
    artifactBinding: {
      artifactId: "identity-source-artifact",
      artifactFileSha256: "a".repeat(64),
      schema: "verisilo-camoufox-resolved-identity/v6",
    },
  },
  seedReference: "22222222-2222-4222-8222-222222222222",
  createdAt: "2026-09-01T00:00:00Z",
  identityLockedAt: "2026-09-02T00:00:00Z",
  archivedAt: null,
  ...overrides,
});

const templateKeysOf = (template: ManagedSiloTemplate): string[] =>
  Object.keys(template).sort();

describe("managed create template mapping", () => {
  it("maps a direct Managed Silo onto the creation form defaults it actually used", () => {
    const template = managedCreateTemplateFromSilo(
      managedSilo({}),
      preview,
    );
    expect(template).not.toBeNull();
    expect(template).toMatchObject({
      sourceSiloId: "11111111-1111-4111-8111-111111111111",
      sourceSiloName: "工作空间",
      name: "工作空间 - 新身份",
      color: "#5b5ce2",
      identityPreset: "balanced-zh-cn",
      followNetworkExit: false,
      screenWidth: 1920,
      screenHeight: 1080,
      hardwareConcurrency: 8,
      gpuPreset: "nvidia-rtx-3060",
      timezone: "Asia/Shanghai",
      networkMode: "direct",
    });
  });

  it("maps a remote fixed proxy without copying its credentials", () => {
    const template = managedCreateTemplateFromSilo(
      managedSilo({
        networkProfile: {
          mode: "fixed_proxy",
          proxyRequired: true,
          scheme: "http",
          host: "proxy.example.test",
          port: 3128,
          bypassList: [],
          credentialRef: "33333333-3333-4333-8333-333333333333",
        },
      }),
      preview,
    );
    expect(template).toMatchObject({
      networkMode: "remote",
      proxyScheme: "http",
      proxyHost: "proxy.example.test",
      proxyPort: "3128",
      proxyCredentialUsed: true,
      mihomoSecretUsed: false,
      controllerUrl: "",
      selectorGroup: "",
      nodeName: "",
      followNetworkExit: true,
    });
  });

  it("maps a bound local Clash endpoint including its group selection but no secret", () => {
    const template = managedCreateTemplateFromSilo(
      managedSilo({
        networkProfile: {
          mode: "fixed_proxy",
          proxyRequired: true,
          scheme: "socks5",
          host: "127.0.0.1",
          port: 7897,
          bypassList: [],
          externalMihomo: {
            controllerUrl: "http://127.0.0.1:9097",
            selectorGroup: "全局",
            nodeName: "香港 01",
            controllerSecretRef: "44444444-4444-4444-8444-444444444444",
          },
        },
      }),
      preview,
    );
    expect(template).toMatchObject({
      networkMode: "clash",
      mixedPort: "7897",
      controllerUrl: "http://127.0.0.1:9097",
      selectorGroup: "全局",
      nodeName: "香港 01",
      mihomoSecretUsed: true,
      proxyCredentialUsed: false,
    });
  });

  it("recovers the GPU preset and language preset from the resolved identity", () => {
    const enPreview: ManagedIdentityPreview = {
      ...preview,
      language: "en-US",
      timezone: "America/New_York",
      webglVendor: "unavailable",
      webglRenderer: "unavailable",
    };
    const template = managedCreateTemplateFromSilo(managedSilo({}), enPreview);
    expect(template).toMatchObject({
      identityPreset: "balanced-en-us",
      gpuPreset: "auto",
      timezone: "America/New_York",
    });
  });

  it("falls back to the preset default when the resolved timezone is not offered", () => {
    const template = managedCreateTemplateFromSilo(managedSilo({}), {
      ...preview,
      language: "de-DE",
      timezone: "Europe/Nowhere",
    });
    expect(template).toMatchObject({
      identityPreset: "balanced-de-de",
      timezone: "Europe/Berlin",
    });
  });

  it("never carries identity, profile, or secret material", () => {
    const template = managedCreateTemplateFromSilo(
      managedSilo({
        networkProfile: {
          mode: "fixed_proxy",
          proxyRequired: true,
          scheme: "socks5",
          host: "proxy.example.test",
          port: 1080,
          bypassList: [],
          credentialRef: "33333333-3333-4333-8333-333333333333",
        },
      }),
      preview,
    );
    expect(templateKeysOf(template as ManagedSiloTemplate)).toEqual([
      "color",
      "controllerUrl",
      "followNetworkExit",
      "gpuPreset",
      "hardwareConcurrency",
      "identityPreset",
      "mihomoSecretUsed",
      "mixedPort",
      "name",
      "networkMode",
      "nodeName",
      "proxyCredentialUsed",
      "proxyHost",
      "proxyPort",
      "proxyScheme",
      "screenHeight",
      "screenWidth",
      "selectorGroup",
      "sourceSiloId",
      "sourceSiloName",
      "timezone",
    ]);
    const serialized = JSON.stringify(template);
    expect(serialized).not.toContain("identity-source-artifact");
    expect(serialized).not.toContain("a".repeat(64));
    expect(serialized).not.toContain("22222222-2222-4222-8222-222222222222");
    expect(serialized).not.toContain("C:\\Vault\\profiles\\source");
  });

  it("refuses to template what the managed create path cannot accept", () => {
    expect(
      managedCreateTemplateFromSilo(
        managedSilo({
          engine: {
            adapter: "stock",
          },
          browser: {
            kind: "chrome",
            executablePath: "C:\\Chrome\\chrome.exe",
            version: "1",
          },
        }),
        preview,
      ),
    ).toBeNull();
    expect(
      managedCreateTemplateFromSilo(
        managedSilo({
          networkProfile: {
            mode: "pac",
            proxyRequired: false,
            pacUrl: "https://proxy.example.test/proxy.pac",
          },
        }),
        preview,
      ),
    ).toBeNull();
  });

  it("derives a distinct, bounded default name", () => {
    expect(newIdentityNameFor("Alice")).toBe("Alice - 新身份");
    expect(newIdentityNameFor("  Alice  ")).toBe("Alice - 新身份");
    expect(newIdentityNameFor("长".repeat(70)).length).toBe(64);
  });
});
