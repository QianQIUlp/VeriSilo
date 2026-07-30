import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import {
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifestName = "SHA256SUMS";
const provenanceName = "provenance.json";
const excludedNames = new Set([manifestName, provenanceName]);
const provenanceProfiles = {
  windows: {
    inputPaths: [
      "package.json",
      "pnpm-lock.yaml",
      "apps/desktop/package.json",
      "apps/desktop/src-tauri/Cargo.toml",
      "apps/desktop/src-tauri/Cargo.lock",
      "apps/desktop/src-tauri/build.rs",
      "apps/desktop/src-tauri/resources/hyperv-image-manifest.schema.json",
      "crates/verisilo-remote-backend/Cargo.toml",
      "crates/verisilo-remote-backend/Cargo.lock",
      "apps/desktop/src-tauri/tauri.conf.json",
      "apps/extension/package.json",
      "apps/extension/manifest.json",
      ".github/workflows/windows-release.yml",
      ".github/workflows/windows-signed-release.yml",
      ".github/workflows/windows-e2e-real.yml",
      ".github/workflows/windows-e2e-harness-static.yml",
      "apps/desktop/src-tauri/tauri.release.conf.json",
      "apps/desktop/src-tauri/tauri.release-reset.conf.json",
      "apps/desktop/src-tauri/tauri.unsigned.conf.json",
      "apps/desktop/src-tauri/windows/release-hooks.nsh",
      "scripts/authenticode-gate.ps1",
      "scripts/download-windows-candidate.ps1",
      "scripts/generate-release-metadata.mjs",
      "scripts/generate-license-report.mjs",
      "scripts/generate-sbom.mjs",
      "scripts/install-native-host-release.ps1",
      "scripts/install-native-host.ps1",
      "scripts/package-extension-zip.mjs",
      "scripts/prepare-native-host-release.mjs",
      "scripts/stage-windows-bundle.mjs",
      "scripts/stage-hyperv-image.ps1",
      "scripts/uninstall-native-host.ps1",
      "scripts/verisilo-environment-probe.ps1",
      "scripts/verisilo-hyperv.ps1",
      "scripts/verisilo-sandbox-bootstrap.ps1",
      "scripts/verisilo-sandbox.ps1",
      "scripts/verisilo-wsl-guest-agent.sh",
      "scripts/verify-environment-source.mjs",
      "scripts/verify-engine-source.mjs",
      "scripts/verify-hyperv-image.mjs",
      "scripts/verify-native-host-install.ps1",
      "scripts/verify-release-policy.mjs",
      "scripts/verify-windows-promotion-candidate.mjs",
      "scripts/write-windows-promotion-attestation.mjs",
      "THIRD_PARTY_NOTICES.md",
      "LICENSE",
    ],
  },
  "remote-agent": {
    inputPaths: [
      "crates/verisilo-remote-backend/Cargo.toml",
      "crates/verisilo-remote-backend/Cargo.lock",
      "crates/verisilo-remote-backend/DEPLOYMENT.md",
      "crates/verisilo-remote-backend/verisilo-remote-agent.example.json",
      ".github/workflows/remote-agent-release.yml",
      "scripts/generate-release-metadata.mjs",
      "scripts/generate-license-report.mjs",
      "scripts/generate-sbom.mjs",
      "scripts/verify-remote-agent-source.mjs",
      "THIRD_PARTY_NOTICES.md",
      "LICENSE",
    ],
  },
};

function sha256(content) {
  return createHash("sha256").update(content).digest("hex");
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function normalizedSourceDate() {
  const epoch = Number(process.env.SOURCE_DATE_EPOCH ?? "0");
  if (!Number.isInteger(epoch) || epoch < 0) {
    throw new Error("SOURCE_DATE_EPOCH must be a non-negative integer.");
  }
  return new Date(epoch * 1000).toISOString().replace(".000Z", "Z");
}

function sourceRevision() {
  const revision = process.env.VERISILO_SOURCE_REVISION ?? "unversioned-source";
  if (!/^(?:[0-9a-f]{40}|unversioned-source)$/u.test(revision)) {
    throw new Error(
      "VERISILO_SOURCE_REVISION must be a full lowercase Git commit or unversioned-source.",
    );
  }
  return revision;
}

function hyperVImageSource(profileName) {
  if (profileName !== "windows") {
    return null;
  }
  const values = {
    repository: process.env.GITHUB_REPOSITORY,
    artifactId: process.env.VERISILO_HYPERV_ARTIFACT_ID,
    imageFile: process.env.VERISILO_HYPERV_IMAGE_FILE,
    imageSha256: process.env.VERISILO_HYPERV_IMAGE_SHA256,
    redistributionAcknowledged:
      process.env.VERISILO_HYPERV_REDISTRIBUTION_ACKNOWLEDGED,
  };
  if (Object.values(values).every((value) => value === undefined)) {
    return null;
  }
  const artifactId = Number(values.artifactId);
  if (
    typeof values.repository !== "string" ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(values.repository) ||
    typeof values.artifactId !== "string" ||
    !/^[1-9][0-9]{0,15}$/u.test(values.artifactId) ||
    !Number.isSafeInteger(artifactId) ||
    typeof values.imageFile !== "string" ||
    !/^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$/u.test(values.imageFile) ||
    values.imageFile.includes("..") ||
    typeof values.imageSha256 !== "string" ||
    !/^[0-9a-f]{64}$/u.test(values.imageSha256) ||
    /^0{64}$/u.test(values.imageSha256) ||
    values.redistributionAcknowledged !== "true"
  ) {
    throw new Error(
      "Windows provenance requires the complete verified Hyper-V image source tuple.",
    );
  }
  return {
    repository: values.repository,
    artifactId,
    imageFile: values.imageFile,
    imageSha256: values.imageSha256,
    redistributionAcknowledged: true,
  };
}

async function collectFiles(
  directory,
  relativeDirectory = "",
  excluded = excludedNames,
) {
  const entries = await readdir(path.join(directory, relativeDirectory), {
    withFileTypes: true,
  });
  const files = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    const absolutePath = path.join(directory, ...relativePath.split("/"));
    const metadata = await lstat(absolutePath);
    if (metadata.isSymbolicLink()) {
      throw new Error(
        `Release directories must not contain symlinks: ${relativePath}`,
      );
    }
    if (metadata.isDirectory()) {
      files.push(...(await collectFiles(directory, relativePath, excluded)));
    } else if (metadata.isFile() && !excluded.has(relativePath)) {
      files.push({
        path: relativePath,
        sha256: await sha256File(absolutePath),
        bytes: metadata.size,
      });
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

function formatManifest(files) {
  return files.map((file) => `${file.sha256}  ${file.path}\n`).join("");
}

async function sourceInputs(profileName) {
  const profile = provenanceProfiles[profileName];
  if (profile === undefined) {
    throw new Error("Provenance profile must be windows or remote-agent.");
  }
  return Promise.all(
    profile.inputPaths.map(async (relativePath) => ({
      path: relativePath,
      sha256: sha256(await readFile(path.join(root, relativePath))),
    })),
  );
}

async function readVersions(profileName) {
  if (profileName === "remote-agent") {
    const cargo = await readFile(
      path.join(root, "crates/verisilo-remote-backend/Cargo.toml"),
      "utf8",
    );
    const remoteAgent = cargo.match(/^version = "([^"]+)"$/mu)?.[1];
    if (remoteAgent === undefined) {
      throw new Error(
        "Unable to read the Remote Agent version from Cargo.toml.",
      );
    }
    return { remoteAgent };
  }
  if (profileName !== "windows") {
    throw new Error("Provenance profile must be windows or remote-agent.");
  }
  const [
    rootPackage,
    extensionPackage,
    extensionManifest,
    tauriConfig,
    cargo,
    remoteAgentCargo,
  ] = await Promise.all([
    readFile(path.join(root, "package.json"), "utf8").then(JSON.parse),
    readFile(path.join(root, "apps/extension/package.json"), "utf8").then(
      JSON.parse,
    ),
    readFile(path.join(root, "apps/extension/manifest.json"), "utf8").then(
      JSON.parse,
    ),
    readFile(
      path.join(root, "apps/desktop/src-tauri/tauri.conf.json"),
      "utf8",
    ).then(JSON.parse),
    readFile(path.join(root, "apps/desktop/src-tauri/Cargo.toml"), "utf8"),
    readFile(
      path.join(root, "crates/verisilo-remote-backend/Cargo.toml"),
      "utf8",
    ),
  ]);
  const cargoVersion = cargo.match(/^version = "([^"]+)"$/mu)?.[1];
  const remoteAgentVersion = remoteAgentCargo.match(
    /^version = "([^"]+)"$/mu,
  )?.[1];
  if (cargoVersion === undefined || remoteAgentVersion === undefined) {
    throw new Error(
      "Unable to read a release component version from Cargo.toml.",
    );
  }
  return {
    product: rootPackage.version,
    packageManager: rootPackage.packageManager,
    desktop: tauriConfig.version,
    cargo: cargoVersion,
    remoteAgent: remoteAgentVersion,
    extensionPackage: extensionPackage.version,
    extensionManifest: extensionManifest.version,
  };
}

async function verifiedSigning(directory, profileName) {
  if (profileName === "remote-agent") {
    const signingState = process.env.VERISILO_SIGNING_STATE ?? "unsigned";
    if (signingState !== "unsigned") {
      throw new Error(
        "Remote Agent candidate provenance must remain explicitly unsigned.",
      );
    }
    return { signingState, signerCertificateSha256: null };
  }
  const report = await readFile(
    path.join(directory, "authenticode-status.json"),
    "utf8",
  )
    .then(JSON.parse)
    .catch(() => undefined);
  const signingState = report?.signingState ?? "not-checked";
  const allowed = new Set([
    "not-checked",
    "unsigned",
    "dry-run-inputs-validated-not-signed",
    "signed-and-verified",
  ]);
  if (!allowed.has(signingState)) {
    throw new Error(
      "authenticode-status.json has an unsupported or ambiguous signing state.",
    );
  }
  if (
    process.env.VERISILO_SIGNING_STATE !== undefined &&
    process.env.VERISILO_SIGNING_STATE !== signingState
  ) {
    throw new Error(
      "VERISILO_SIGNING_STATE does not match the Authenticode verification report.",
    );
  }
  const signerCertificateSha256 =
    signingState === "signed-and-verified"
      ? report?.expectedSignerCertificateSha256
      : null;
  if (
    signingState === "signed-and-verified" &&
    (typeof signerCertificateSha256 !== "string" ||
      !/^[0-9a-f]{64}$/u.test(signerCertificateSha256) ||
      /^0{64}$/u.test(signerCertificateSha256) ||
      process.env.VERISILO_AUTHENTICODE_SIGNER_SHA256 !==
        signerCertificateSha256)
  ) {
    throw new Error(
      "Signed provenance requires the exact release-pinned Authenticode signer SHA-256.",
    );
  }
  return { signingState, signerCertificateSha256 };
}

async function expectedProvenance(files, directory, profileName) {
  const { signingState, signerCertificateSha256 } = await verifiedSigning(
    directory,
    profileName,
  );
  const versions = await readVersions(profileName);
  return {
    schema: "urn:verisilo:release-provenance:1",
    schemaVersion: 1,
    generatedAt: normalizedSourceDate(),
    source: {
      repository: "https://github.com/QianQIUlp/VeriSilo",
      revision: sourceRevision(),
      dirty:
        process.env.VERISILO_SOURCE_DIRTY === "false"
          ? false
          : process.env.VERISILO_SOURCE_DIRTY === "true"
            ? true
            : null,
      inputs: await sourceInputs(profileName),
    },
    versions,
    build: {
      artifactProfile: profileName,
      target: process.env.VERISILO_BUILD_TARGET ?? "x86_64-pc-windows-msvc",
      signingState,
      signerCertificateSha256,
      hyperVImageSource: hyperVImageSource(profileName),
      promotionState: profileName === "windows" ? "NOT_PROMOTABLE" : null,
      node: process.version,
      packageManager: versions.packageManager ?? null,
      rustToolchain: "1.88.0",
      workflow: process.env.GITHUB_WORKFLOW ?? null,
      workflowRef: process.env.GITHUB_WORKFLOW_REF ?? null,
      runId: process.env.GITHUB_RUN_ID ?? null,
      runnerOs: process.env.RUNNER_OS ?? null,
      runnerArch: process.env.RUNNER_ARCH ?? null,
      reproducibility: {
        hermetic: false,
        deterministicSubcomponents:
          profileName === "windows"
            ? ["companion-zip:sorted-zip32-store-source-date-epoch"]
            : [],
        limitations:
          profileName === "windows"
            ? [
                "hosted-runner-image-and-tool-downloads",
                "pe-nsis-toolchain-output",
                "authenticode-timestamp-service-when-signed",
              ]
            : ["hosted-runner-image-and-rust-toolchain-output"],
      },
    },
    artifacts: files,
  };
}

async function generate(directory, profileName) {
  const payloadFiles = await collectFiles(directory);
  if (payloadFiles.length === 0) {
    throw new Error("Release directory is empty.");
  }
  await writeFile(
    path.join(directory, provenanceName),
    stableJson(await expectedProvenance(payloadFiles, directory, profileName)),
    "utf8",
  );
  const checksumFiles = await collectFiles(
    directory,
    "",
    new Set([manifestName]),
  );
  await writeFile(
    path.join(directory, manifestName),
    formatManifest(checksumFiles),
    "utf8",
  );
  process.stdout.write(
    `Generated provenance for ${payloadFiles.length} payload files and SHA-256 checksums for ${checksumFiles.length} files.\n`,
  );
}

async function check(directory, profileName) {
  const payloadFiles = await collectFiles(directory);
  const checksumFiles = await collectFiles(
    directory,
    "",
    new Set([manifestName]),
  );
  const expectedManifest = formatManifest(checksumFiles);
  const actualManifest = await readFile(
    path.join(directory, manifestName),
    "utf8",
  );
  if (actualManifest !== expectedManifest) {
    throw new Error("SHA256SUMS is stale, incomplete, unsorted, or invalid.");
  }
  const provenance = JSON.parse(
    await readFile(path.join(directory, provenanceName), "utf8"),
  );
  if (
    provenance.schema !== "urn:verisilo:release-provenance:1" ||
    provenance.schemaVersion !== 1 ||
    stableJson(provenance) !==
      stableJson(await expectedProvenance(payloadFiles, directory, profileName))
  ) {
    throw new Error(
      "provenance.json does not describe the current profile, inputs, build, and artifacts.",
    );
  }
  if (
    !new Set([
      "not-checked",
      "unsigned",
      "dry-run-inputs-validated-not-signed",
      "signed-and-verified",
    ]).has(provenance.build?.signingState)
  ) {
    throw new Error("provenance.json has an unsupported signing state.");
  }
  process.stdout.write(
    `Verified provenance for ${payloadFiles.length} payload files and SHA-256 checksums for ${checksumFiles.length} files.\n`,
  );
}

async function selfTest() {
  const fixture = [
    { path: "a.txt", sha256: "a".repeat(64) },
    { path: "nested/b.bin", sha256: "b".repeat(64) },
  ];
  const manifest = formatManifest(fixture);
  if (
    manifest !== `${"a".repeat(64)}  a.txt\n${"b".repeat(64)}  nested/b.bin\n`
  ) {
    throw new Error("Release manifest self-test failed.");
  }

  const temporaryRoot = await mkdtemp(
    path.join(tmpdir(), "verisilo-release-metadata-"),
  );
  const previousSigningState = process.env.VERISILO_SIGNING_STATE;
  const previousTarget = process.env.VERISILO_BUILD_TARGET;
  const hyperVEnvironmentNames = [
    "GITHUB_REPOSITORY",
    "VERISILO_HYPERV_ARTIFACT_ID",
    "VERISILO_HYPERV_IMAGE_FILE",
    "VERISILO_HYPERV_IMAGE_SHA256",
    "VERISILO_HYPERV_REDISTRIBUTION_ACKNOWLEDGED",
  ];
  const previousHyperVEnvironment = Object.fromEntries(
    hyperVEnvironmentNames.map((name) => [name, process.env[name]]),
  );
  try {
    for (const profileName of Object.keys(provenanceProfiles)) {
      const candidate = path.join(temporaryRoot, profileName);
      await mkdir(candidate, { recursive: true });
      await writeFile(
        path.join(candidate, "payload.txt"),
        `${profileName} fixture\n`,
        "utf8",
      );
      if (profileName === "remote-agent") {
        process.env.VERISILO_SIGNING_STATE = "unsigned";
        process.env.VERISILO_BUILD_TARGET = "x86_64-unknown-linux-gnu";
      } else {
        delete process.env.VERISILO_SIGNING_STATE;
        process.env.VERISILO_BUILD_TARGET = "x86_64-pc-windows-msvc";
        process.env.GITHUB_REPOSITORY = "QianQIUlp/VeriSilo";
        process.env.VERISILO_HYPERV_ARTIFACT_ID = "123";
        process.env.VERISILO_HYPERV_IMAGE_FILE = "licensed-base.vhdx";
        process.env.VERISILO_HYPERV_IMAGE_SHA256 = "a".repeat(64);
        process.env.VERISILO_HYPERV_REDISTRIBUTION_ACKNOWLEDGED = "true";
      }
      await generate(candidate, profileName);
      await check(candidate, profileName);
    }
  } finally {
    if (previousSigningState === undefined) {
      delete process.env.VERISILO_SIGNING_STATE;
    } else {
      process.env.VERISILO_SIGNING_STATE = previousSigningState;
    }
    if (previousTarget === undefined) {
      delete process.env.VERISILO_BUILD_TARGET;
    } else {
      process.env.VERISILO_BUILD_TARGET = previousTarget;
    }
    for (const [name, value] of Object.entries(previousHyperVEnvironment)) {
      if (value === undefined) {
        delete process.env[name];
      } else {
        process.env[name] = value;
      }
    }
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  process.stdout.write(
    "Release metadata self-test passed for windows and remote-agent profiles.\n",
  );
}

if (process.argv.includes("--self-test")) {
  await selfTest();
  process.exit(0);
}
const directoryIndex = process.argv.indexOf("--dir");
const directoryValue =
  directoryIndex === -1 ? undefined : process.argv[directoryIndex + 1];
const profileIndex = process.argv.indexOf("--profile");
const profileName =
  profileIndex === -1 ? "windows" : process.argv[profileIndex + 1];
if (directoryValue === undefined) {
  throw new Error(
    "Usage: node scripts/generate-release-metadata.mjs --dir <release-directory> [--profile windows|remote-agent] [--check] | --self-test",
  );
}
if (
  profileName === undefined ||
  provenanceProfiles[profileName] === undefined
) {
  throw new Error("Provenance profile must be windows or remote-agent.");
}
const releaseDirectory = path.resolve(root, directoryValue);
if (process.argv.includes("--check")) {
  await check(releaseDirectory, profileName);
} else {
  await generate(releaseDirectory, profileName);
}
