#!/usr/bin/env node

import { readdirSync, readFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const resourcesRoot = join(
  repositoryRoot,
  "apps",
  "desktop",
  "src-tauri",
  "resources",
);
const releaseMode = process.argv.slice(2).includes("--release");
const certificatePattern = /^[a-f0-9]{64}$/u;

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function walk(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = join(path, entry.name);
    return entry.isDirectory() ? walk(child) : [child];
  });
}

const policy = readJson(join(resourcesRoot, "engine-trusted-signers.json"));
if (
  policy.schemaVersion !== 1 ||
  !Array.isArray(policy.signers) ||
  policy.signers.length > 16
) {
  throw new Error("Engine signer policy schema or signer count is invalid.");
}

const policyPins = new Set();
for (const signer of policy.signers) {
  if (
    !signer ||
    Object.keys(signer).sort().join(",") !== "certificateSha256,publisher" ||
    !certificatePattern.test(signer.certificateSha256) ||
    typeof signer.publisher !== "string" ||
    signer.publisher.trim() !== signer.publisher ||
    signer.publisher.length === 0 ||
    signer.publisher.length > 200 ||
    policyPins.has(signer.certificateSha256)
  ) {
    throw new Error(
      "Engine signer policy contains an invalid or duplicate signer.",
    );
  }
  policyPins.add(signer.certificateSha256);
}

const buildPins = (process.env.VERISILO_ENGINE_SIGNER_SHA256 ?? "")
  .split(",")
  .map((pin) => pin.trim())
  .filter(Boolean);
if (buildPins.some((pin) => !certificatePattern.test(pin))) {
  throw new Error("VERISILO_ENGINE_SIGNER_SHA256 contains an invalid pin.");
}
const effectivePins = new Set([...policyPins, ...buildPins]);
if (releaseMode && effectivePins.size === 0) {
  throw new Error(
    "Release engine verification requires at least one pinned signer certificate.",
  );
}

const manifest = readJson(join(resourcesRoot, "engine-package.example.json"));
const expectedManifestKeys = [
  "artifactSha256",
  "capabilities",
  "channel",
  "engineId",
  "engineVersion",
  "executableRelativePath",
  "platform",
  "schemaVersion",
  "signature",
];
if (
  Object.keys(manifest).sort().join(",") !== expectedManifestKeys.join(",") ||
  manifest.schemaVersion !== 2 ||
  manifest.channel !== "experimental" ||
  manifest.platform !== "windows-x64" ||
  manifest.signature?.algorithm !== "cms-detached-sha256" ||
  !certificatePattern.test(manifest.signature?.keyId ?? "") ||
  !certificatePattern.test(manifest.artifactSha256 ?? "") ||
  !manifest.capabilities?.includes("identity_template") ||
  !manifest.capabilities?.includes("site_fallback") ||
  effectivePins.has(manifest.signature.keyId)
) {
  throw new Error(
    "Engine package example is structurally invalid or accidentally trusted.",
  );
}

const forbiddenArtifactExtensions = new Set([
  ".7z",
  ".cat",
  ".dll",
  ".exe",
  ".gz",
  ".p7s",
  ".tar",
  ".zip",
]);
const bundledArtifacts = walk(resourcesRoot).filter((path) =>
  forbiddenArtifactExtensions.has(extname(path).toLowerCase()),
);
if (bundledArtifacts.length !== 0) {
  throw new Error(
    `Controlled-engine binary/signature artifacts must not be committed here: ${bundledArtifacts.join(", ")}`,
  );
}

const engineSource = readFileSync(
  join(repositoryRoot, "apps", "desktop", "src-tauri", "src", "engine.rs"),
  "utf8",
);
for (const forbidden of [
  "std::process::Command",
  "cmd.exe",
  "powershell.exe",
  "powershell -",
]) {
  if (engineSource.toLowerCase().includes(forbidden.toLowerCase())) {
    throw new Error(`Engine verifier must not invoke a shell (${forbidden}).`);
  }
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      releaseMode,
      manifestSchemaVersion: manifest.schemaVersion,
      signatureAlgorithm: manifest.signature.algorithm,
      trustedSignerCount: effectivePins.size,
      controlledEngineArtifactsBundled: false,
      externalBlocker:
        effectivePins.size === 0
          ? "No release signer certificate pin is configured."
          : null,
    },
    null,
    2,
  )}\n`,
);
