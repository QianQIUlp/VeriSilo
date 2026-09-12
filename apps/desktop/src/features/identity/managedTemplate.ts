import type { Silo } from "@verisilo/contracts";

import type {
  ManagedIdentityPreset,
  ManagedIdentityPreview,
} from "../../desktop-api.js";

import { gpuPresetFromWebgl, type GpuPresetId } from "../../gpu-presets.js";

import { isLoopbackProxyHost, MIHOMO_DEFAULT_MIXED_PORT } from "../../proxy-presets.js";

import { localePresetFromPreview } from "../../shared/presentation.js";

import {
  defaultTimezoneForPreset,
  isSupportedTimezone,
} from "../../timezone-presets.js";

const MAX_SILO_NAME_LENGTH = 64;

export const NEW_IDENTITY_NAME_SUFFIX = " - 新身份";

export type ManagedTemplateNetworkMode = "direct" | "clash" | "remote";

/**
 * Initial values for the existing managed creation form, recovered from an
 * existing Managed Silo. This is a template, not a clone: seeds, Artifacts,
 * Profiles, and any runtime evidence are never part of it, and stored secrets
 * stay in the Vault — it carries where a proxy lives, never how to
 * authenticate to it.
 */
export interface ManagedSiloTemplate {
  sourceSiloId: string;
  sourceSiloName: string;
  name: string;
  color: string;
  identityPreset: ManagedIdentityPreset;
  followNetworkExit: boolean;
  screenWidth: number;
  screenHeight: number;
  hardwareConcurrency: number;
  gpuPreset: GpuPresetId;
  timezone: string;
  networkMode: ManagedTemplateNetworkMode;
  proxyScheme: "http" | "socks5";
  proxyHost: string;
  proxyPort: string;
  mixedPort: string;
  controllerUrl: string;
  selectorGroup: string;
  nodeName: string;
  proxyCredentialUsed: boolean;
  mihomoSecretUsed: boolean;
}

export function newIdentityNameFor(sourceName: string): string {
  return `${sourceName.trim()}${NEW_IDENTITY_NAME_SUFFIX}`.slice(
    0,
    MAX_SILO_NAME_LENGTH,
  );
}

/**
 * Map a Managed (Camoufox) Silo's user-visible configuration onto the
 * existing managed creation form. Returns null for anything the managed
 * create path could not accept verbatim, so the form never opens with
 * settings it would reject on submit.
 */
export function managedCreateTemplateFromSilo(
  silo: Silo,
  preview: ManagedIdentityPreview,
): ManagedSiloTemplate | null {
  if (silo.engine.adapter !== "camoufox") {
    return null;
  }
  const profile = silo.networkProfile;
  let network: Pick<
    ManagedSiloTemplate,
    | "networkMode"
    | "proxyScheme"
    | "proxyHost"
    | "proxyPort"
    | "mixedPort"
    | "controllerUrl"
    | "selectorGroup"
    | "nodeName"
    | "mihomoSecretUsed"
  >;
  if (profile.mode === "direct") {
    network = {
      networkMode: "direct",
      proxyScheme: "socks5",
      proxyHost: "",
      proxyPort: "8080",
      mixedPort: String(MIHOMO_DEFAULT_MIXED_PORT),
      controllerUrl: "",
      selectorGroup: "",
      nodeName: "",
      mihomoSecretUsed: false,
    };
  } else if (profile.mode === "fixed_proxy") {
    if (isLoopbackProxyHost(profile.host)) {
      const binding = profile.externalMihomo;
      network = {
        networkMode: "clash",
        proxyScheme: "socks5",
        proxyHost: profile.host,
        proxyPort: String(profile.port),
        mixedPort: String(profile.port),
        controllerUrl: binding?.controllerUrl ?? "",
        selectorGroup: binding?.selectorGroup ?? "",
        nodeName: binding?.nodeName ?? "",
        mihomoSecretUsed: binding?.controllerSecretRef !== undefined,
      };
    } else if (profile.scheme === "http" || profile.scheme === "socks5") {
      network = {
        networkMode: "remote",
        proxyScheme: profile.scheme,
        proxyHost: profile.host,
        proxyPort: String(profile.port),
        mixedPort: String(MIHOMO_DEFAULT_MIXED_PORT),
        controllerUrl: "",
        selectorGroup: "",
        nodeName: "",
        mihomoSecretUsed: false,
      };
    } else {
      // Managed Silos cannot carry other proxy schemes; refuse to guess
      // instead of prefilling something the create path would reject.
      return null;
    }
  } else {
    // Managed Silos cannot carry PAC; refuse to guess instead of prefilling
    // something the create path would reject.
    return null;
  }
  const identityPreset = localePresetFromPreview(preview);
  return {
    sourceSiloId: silo.id,
    sourceSiloName: silo.name,
    name: newIdentityNameFor(silo.name),
    color: silo.color,
    identityPreset,
    followNetworkExit: profile.proxyRequired,
    screenWidth: preview.screenWidth,
    screenHeight: preview.screenHeight,
    hardwareConcurrency: preview.hardwareConcurrency,
    gpuPreset: gpuPresetFromWebgl(preview.webglVendor, preview.webglRenderer),
    timezone: isSupportedTimezone(preview.timezone)
      ? preview.timezone
      : defaultTimezoneForPreset(identityPreset),
    proxyCredentialUsed:
      profile.mode === "fixed_proxy" && profile.credentialRef !== undefined,
    ...network,
  };
}
