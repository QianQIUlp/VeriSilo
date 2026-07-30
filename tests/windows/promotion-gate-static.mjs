import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
);
const read = (relativePath) => readFile(path.join(root, relativePath), "utf8");
const [
  promotion,
  unsignedRelease,
  signedRelease,
  imageStager,
  candidateDownloader,
  imageVerifier,
  candidateVerifier,
  attestationWriter,
  extensionPackager,
  releaseMetadata,
  releasePolicy,
  releaseConfig,
  resetConfig,
  unsignedConfig,
  releaseHooks,
] = await Promise.all([
  read(".github/workflows/windows-e2e-real.yml"),
  read(".github/workflows/windows-release.yml"),
  read(".github/workflows/windows-signed-release.yml"),
  read("scripts/stage-hyperv-image.ps1"),
  read("scripts/download-windows-candidate.ps1"),
  read("scripts/verify-hyperv-image.mjs"),
  read("scripts/verify-windows-promotion-candidate.mjs"),
  read("scripts/write-windows-promotion-attestation.mjs"),
  read("scripts/package-extension-zip.mjs"),
  read("scripts/generate-release-metadata.mjs"),
  read("scripts/verify-release-policy.mjs"),
  read("apps/desktop/src-tauri/tauri.release.conf.json"),
  read("apps/desktop/src-tauri/tauri.release-reset.conf.json"),
  read("apps/desktop/src-tauri/tauri.unsigned.conf.json"),
  read("apps/desktop/src-tauri/windows/release-hooks.nsh"),
]);

for (const workflow of [unsignedRelease, signedRelease]) {
  for (const required of [
    "hyperVImageArtifactId",
    "hyperVImageFile",
    "hyperVImageSha256",
    "I_HAVE_VERIFIED_REDISTRIBUTION_RIGHTS",
    "actions: read",
    "stage-hyperv-image.ps1",
    "verify-hyperv-image.mjs",
    "VERISILO_HYPERV_IMAGE_FILE",
    "VERISILO_HYPERV_IMAGE_SHA256",
    "package-extension-zip.mjs",
    "promotion-status.json",
    "NOT PROMOTABLE",
    "steps.upload-candidate.outputs.artifact-digest",
  ]) {
    assert.match(
      workflow,
      new RegExp(required.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
    );
  }
  assert.doesNotMatch(workflow, /Compress-Archive/u);
}

for (const required of [
  "workflow_call:",
  "actions: read",
  "candidateDigest",
  "sourceRevision",
  "verisilo-win10",
  "verisilo-win11",
  "expectedWindowsVersion: Windows 10",
  "expectedWindowsVersion: Windows 11",
  "browser: Chrome",
  "browser: Edge",
  "download-windows-candidate.ps1",
  "verify-windows-promotion-candidate.mjs",
  "acceptance-tests",
  "verisilo-acceptance-driver",
  "VERISILO_ACCEPTANCE_SOURCE_REVISION",
  "--acceptance-receipt",
  "-RequireAll",
  "write-windows-promotion-attestation.mjs",
  "--enforce",
]) {
  assert.match(
    promotion,
    new RegExp(required.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}
assert.equal(
  (promotion.match(/runnerLabel: verisilo-win10/gu) ?? []).length,
  2,
);
assert.equal(
  (promotion.match(/runnerLabel: verisilo-win11/gu) ?? []).length,
  2,
);
assert.equal((promotion.match(/browser: Chrome/gu) ?? []).length, 2);
assert.equal((promotion.match(/browser: Edge/gu) ?? []).length, 2);
for (const forbidden of [
  "nativeHostPath:",
  "releaseConfigPath:",
  "desktopExe:",
  "requireAll:",
  "runs-on: windows-latest",
]) {
  assert.doesNotMatch(promotion, new RegExp(forbidden, "u"));
}

for (const required of [
  "exactly one entry",
  "archive_download_url",
  "metadata.expired",
  "Get-FileHash",
  "ImageSha256",
  "redistributionAcknowledged",
]) {
  assert.match(imageStager, new RegExp(required, "u"));
}
for (const required of [
  "metadata.digest",
  "Get-FileHash",
  "Assert-SafeEntryName",
  "duplicate Windows path",
  "reparse point",
  "expires_at",
]) {
  assert.match(candidateDownloader, new RegExp(required, "u"));
}
assert.match(imageVerifier, /tampered bytes/u);
assert.match(imageVerifier, /path traversal/u);
assert.match(candidateVerifier, /SHA256SUMS/u);
assert.match(candidateVerifier, /provenance\.source\.revision/u);
assert.match(candidateVerifier, /promotion-status\.json/u);
assert.match(candidateVerifier, /acceptanceDriver/u);
assert.match(candidateVerifier, /cargoTarget: "verisilo-acceptance-driver"/u);
assert.match(attestationWriter, /counts\.SKIP > 0/u);
assert.match(attestationWriter, /counts\.BLOCKED > 0/u);
assert.match(attestationWriter, /validateAcceptanceReceipt/u);
assert.match(attestationWriter, /requiredEvidenceNames/u);
assert.match(attestationWriter, /acceptanceDriverReceiptSha256/u);
assert.match(extensionPackager, /sourceDateEpoch/u);
assert.match(extensionPackager, /zip32-store/u);
assert.match(extensionPackager, /crc32/u);
assert.doesNotMatch(extensionPackager, /node_modules|archiver|adm-zip/iu);
assert.match(releaseMetadata, /hyperVImageSource/u);
assert.match(releaseMetadata, /hermetic: false/u);
assert.match(releaseMetadata, /createReadStream/u);
assert.match(releasePolicy, /verifyHyperVImageResource/u);
assert.match(releasePolicy, /verifyExtensionZipEvidence/u);
assert.match(releasePolicy, /verifyPromotionStatus/u);
assert.match(releasePolicy, /path\.extname\(file\.path\).*\.vhdx/su);

const parsedReleaseConfig = JSON.parse(releaseConfig);
assert.equal(
  parsedReleaseConfig.bundle.resources[
    "target/verisilo-release-resources/environment/hyperv-image-manifest.json"
  ],
  "environment/hyperv-image-manifest.json",
);
assert.equal(
  parsedReleaseConfig.bundle.resources[
    "target/verisilo-release-resources/environment/images/"
  ],
  "environment/images/",
);
assert.deepEqual(JSON.parse(resetConfig).bundle.resources, []);
assert.deepEqual(JSON.parse(unsignedConfig).bundle.externalBin, []);
assert.deepEqual(JSON.parse(unsignedConfig).bundle.resources, []);
assert.equal(
  JSON.parse(unsignedConfig).bundle.windows.nsis.installerHooks,
  null,
);
assert.match(unsignedRelease, /tauri\.unsigned\.conf\.json/u);
assert.match(unsignedRelease, /desktop-only current-user NSIS installer/u);
assert.doesNotMatch(signedRelease, /tauri\.unsigned\.conf\.json/u);
assert.match(releaseHooks, /ExecutionPolicy AllSigned/gu);
assert.doesNotMatch(releaseHooks, /ExecutionPolicy (?:Bypass|RemoteSigned)/u);

for (const workflow of [promotion, unsignedRelease, signedRelease]) {
  for (const line of workflow
    .split(/\r?\n/u)
    .filter((value) => /\buses:/u.test(value))) {
    assert.match(
      line,
      /\buses:\s*[^@\s]+@[0-9a-f]{40}\s+#\s+v?\d+(?:\.\d+){0,2}\s*$/u,
    );
  }
}

process.stdout.write(
  "Windows promotion and Hyper-V image static gates passed; no Windows runner, browser, candidate, or VHDX was exercised.\n",
);
