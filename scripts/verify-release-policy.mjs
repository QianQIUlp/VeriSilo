import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { readFile, readdir, lstat } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const extensionIdPattern = /^[a-p]{32}$/u;
const knownPlaceholderIds = new Set([
  "abcdefghijklmnopabcdefghijklmnop",
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
]);
const placeholderPatterns = [
  /__VERISILO_[A-Z0-9_]+__/u,
  /<published[-_ a-z]*id>/iu,
  /\b(?:change[_ -]?me|your[_ -]?(?:extension[_ -]?)?id)\b/iu,
];
const secretPatterns = [
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/u,
  /\bgithub_pat_[A-Za-z0-9_]{20,}\b/u,
  /\bgh[pousr]_[A-Za-z0-9]{20,}\b/u,
  /\bAKIA[0-9A-Z]{16}\b/u,
  /\bsk-[A-Za-z0-9_-]{20,}\b/u,
  /https?:\/\/[^\s/@:]+:[^\s/@]+@/iu,
  /https?:\/\/[^\s?#]+[?&](?:token|secret|password|api[_-]?key)=[^\s&#]{8,}/iu,
];
const forbiddenSecretExtensions = new Set([".pfx", ".p12", ".pem", ".key"]);
const expectedBundleResources = [
  "bundle-resources/environment/hyperv-image-manifest.json",
  "bundle-resources/environment/verisilo-environment-probe.ps1",
  "bundle-resources/environment/verisilo-hyperv.ps1",
  "bundle-resources/environment/verisilo-sandbox.ps1",
  "bundle-resources/environment/verisilo-sandbox-bootstrap.ps1",
  "bundle-resources/environment/verisilo-wsl-guest-agent.sh",
  "bundle-resources/native-host/install-native-host-release.ps1",
  "bundle-resources/native-host/install-native-host.ps1",
  "bundle-resources/native-host/native-host-release-config.json",
  "bundle-resources/native-host/uninstall-native-host.ps1",
  "bundle-resources/native-host/verify-native-host-install.ps1",
];
const pinnedWorkflowPaths = [
  ".github/workflows/ci.yml",
  ".github/workflows/windows-release.yml",
  ".github/workflows/windows-signed-release.yml",
  ".github/workflows/windows-e2e-real.yml",
];

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

function verifyWorkflowActionPins(relativePath, content) {
  const actionLines = content
    .split(/\r?\n/u)
    .filter((line) => /\buses:\s*/u.test(line));
  for (const line of actionLines) {
    const match = line.match(
      /\buses:\s*([^@\s]+)@([^\s#]+)\s+#\s+(v?\d+(?:\.\d+){0,2})\s*$/u,
    );
    if (match === null || !/^[0-9a-f]{40}$/u.test(match[2])) {
      throw new Error(
        `${relativePath}: every external action must use a full commit SHA and version comment.`,
      );
    }
  }
}

function workflowJobBlocks(content) {
  const jobsStart = content.indexOf("\njobs:\n");
  if (jobsStart === -1) {
    throw new Error("Workflow has no jobs mapping.");
  }
  const jobsSection = content.slice(jobsStart + 1);
  const matches = [...jobsSection.matchAll(/^  ([a-z0-9][a-z0-9-]*):\r?$/gmu)];
  const result = {};
  for (const [index, match] of matches.entries()) {
    const name = match[1];
    if (Object.hasOwn(result, name)) {
      throw new Error(`Workflow repeats job ${name}.`);
    }
    result[name] = jobsSection.slice(
      match.index,
      matches[index + 1]?.index ?? jobsSection.length,
    );
  }
  return result;
}

function verifySignedWorkflowOrder(content) {
  const releaseConfigSequence =
    "--config src-tauri/tauri.release-reset.conf.json --config src-tauri/tauri.release.conf.json";
  const expectedJobs = [
    "build-unsigned-inner",
    "sign-inner-isolated",
    "package-unsigned-outer",
    "sign-outer-isolated",
    "audit-signed-candidate",
  ];
  const jobs = workflowJobBlocks(content);
  if (JSON.stringify(Object.keys(jobs)) !== JSON.stringify(expectedJobs)) {
    throw new Error(
      "Signed Windows workflow must contain only the ordered build, isolated inner-sign, package, isolated outer-sign, and audit jobs.",
    );
  }
  for (const required of [
    "authenticodeSignerSha256:",
    "concurrency:",
    "cancel-in-progress: false",
    "if: github.ref == 'refs/heads/main'",
    "ref: ${{ github.sha }}",
    "fetch-depth: 0",
    "exactly match both GITHUB_SHA and origin/main",
    "secrets.VERISILO_AUTHENTICODE_PFX_BASE64",
    "secrets.VERISILO_AUTHENTICODE_PASSWORD",
    "VERISILO_AUTHENTICODE_SIGNER_SHA256",
    "VERISILO_ENGINE_SIGNER_SHA256",
    "pnpm engine:verify:release",
    "Self-test release tooling before any secret-bearing job",
    "verisilo-environment-probe.ps1 -SelfTest",
    "verisilo-hyperv.ps1 -SelfTest",
    "verisilo-sandbox.ps1 -SelfTest",
    "urn:verisilo:inner-signing-input:1",
    "urn:verisilo:signed-inner-payload:1",
    "urn:verisilo:outer-signing-input:1",
    "urn:verisilo:signed-outer-payload:1",
    "Import-PfxCertificate",
    "-Exportable:$false",
    "TimeStamperCertificate",
    "if: always()",
    "windows-x64-signed",
  ]) {
    if (!content.includes(required)) {
      throw new Error(`Signed Windows workflow is missing ${required}.`);
    }
  }

  const build = jobs["build-unsigned-inner"];
  const innerSign = jobs["sign-inner-isolated"];
  const packageOuter = jobs["package-unsigned-outer"];
  const outerSign = jobs["sign-outer-isolated"];
  const audit = jobs["audit-signed-candidate"];
  for (const [jobName, block] of Object.entries(jobs)) {
    const isSigningJob = [
      "sign-inner-isolated",
      "sign-outer-isolated",
    ].includes(jobName);
    if (isSigningJob) {
      for (const required of [
        "runs-on: windows-latest",
        "environment: windows-signing",
        "actions/download-artifact@",
        "actions/upload-artifact@",
        "secrets.VERISILO_AUTHENTICODE_PFX_BASE64",
        "secrets.VERISILO_AUTHENTICODE_PASSWORD",
        "Remove any residual signing material",
      ]) {
        if (!block.includes(required)) {
          throw new Error(
            `Isolated signing job ${jobName} is missing ${required}.`,
          );
        }
      }
      for (const forbidden of [
        "actions/checkout@",
        "pnpm",
        "setup-node@",
        "rust-toolchain@",
        "cargo ",
        "node ",
        "./scripts/",
      ]) {
        if (block.includes(forbidden)) {
          throw new Error(
            `Isolated signing job ${jobName} contains candidate/dependency execution marker ${forbidden}.`,
          );
        }
      }
    } else if (
      block.includes("environment: windows-signing") ||
      block.includes("secrets.VERISILO_AUTHENTICODE")
    ) {
      throw new Error(
        `Non-signing job ${jobName} must never receive signing environment secrets.`,
      );
    }
  }
  if (
    content.split("environment: windows-signing").length - 1 !== 2 ||
    content.split("secrets.VERISILO_AUTHENTICODE_PFX_BASE64").length - 1 !==
      2 ||
    content.split("secrets.VERISILO_AUTHENTICODE_PASSWORD").length - 1 !== 2
  ) {
    throw new Error(
      "Signing environment and both secrets must appear only in the two isolated signing jobs.",
    );
  }
  for (const [block, requiredNeed] of [
    [innerSign, "needs: build-unsigned-inner"],
    [packageOuter, "needs: sign-inner-isolated"],
    [outerSign, "needs: package-unsigned-outer"],
    [audit, "needs: sign-outer-isolated"],
  ]) {
    if (!block.includes(requiredNeed)) {
      throw new Error(
        `Signed workflow is missing strict topology edge ${requiredNeed}.`,
      );
    }
  }
  if (
    !build.includes("tauri build --ci --no-bundle --no-sign") ||
    !build.includes("-Mode Unsigned") ||
    !packageOuter.includes("tauri bundle --ci --no-sign") ||
    !packageOuter.includes("exactly one unsigned file") ||
    !innerSign.includes("exact fixed allowlist") ||
    !outerSign.includes("sign only installer") ||
    !audit.includes("-Mode VerifySigned -ReleaseDirectory artifacts/release")
  ) {
    throw new Error(
      "Signed workflow does not preserve unsigned build, allowlisted inner signing, bundle, installer-only signing, and final verification.",
    );
  }
  const finalGate = audit.indexOf(
    "Generate provenance and repeat every final content gate",
  );
  if (
    finalGate === -1 ||
    audit.indexOf("pnpm release:metadata", finalGate) === -1 ||
    audit.indexOf("pnpm licenses:check", finalGate) === -1 ||
    audit.indexOf("package-extension-zip.mjs", finalGate) === -1 ||
    audit.indexOf("verify-release-policy.mjs", finalGate) === -1
  ) {
    throw new Error(
      "Final no-secret audit must repeat provenance, license, extension ZIP, and release policy gates after all mutations.",
    );
  }
  if (content.split(releaseConfigSequence).length - 1 !== 2) {
    throw new Error(
      "Signed Windows workflow must reset source resources before both build and bundle release configs.",
    );
  }
  if (content.includes("tauri.unsigned.conf.json")) {
    throw new Error(
      "Signed Windows workflow must never load the desktop-only unsigned bundle override.",
    );
  }
}

function verifyPromotionWorkflow(content) {
  for (const required of [
    "workflow_call:",
    "actions: read",
    "candidateDigest",
    "sourceRevision",
    "download-windows-candidate.ps1",
    "verify-windows-promotion-candidate.mjs",
    "-RequireAll",
    "write-windows-promotion-attestation.mjs",
    "--enforce",
  ]) {
    if (!content.includes(required)) {
      throw new Error(`Windows promotion workflow is missing ${required}.`);
    }
  }
  for (const [marker, count] of [
    ["runnerLabel: verisilo-win10", 2],
    ["runnerLabel: verisilo-win11", 2],
    ["browser: Chrome", 2],
    ["browser: Edge", 2],
  ]) {
    if (content.split(marker).length - 1 !== count) {
      throw new Error(
        `Windows promotion workflow does not have the fixed ${marker} matrix.`,
      );
    }
  }
  for (const forbidden of [
    "nativeHostPath:",
    "releaseConfigPath:",
    "desktopExe:",
    "requireAll:",
    "runs-on: windows-latest",
  ]) {
    if (content.includes(forbidden)) {
      throw new Error(
        `Windows promotion workflow exposes forbidden runner-local input ${forbidden}.`,
      );
    }
  }
}

async function verifyWorkflowPolicies() {
  const requiredWorkflows = Object.fromEntries(
    await Promise.all(
      pinnedWorkflowPaths.map(async (relativePath) => [
        relativePath,
        await readFile(path.join(root, relativePath), "utf8"),
      ]),
    ),
  );
  const workflowDirectory = path.join(root, ".github", "workflows");
  const workflowPaths = (await readdir(workflowDirectory))
    .filter((name) => /\.ya?ml$/u.test(name))
    .map((name) => `.github/workflows/${name}`)
    .sort();
  const workflows = await Promise.all(
    workflowPaths.map(async (relativePath) => [
      relativePath,
      await readFile(path.join(root, relativePath), "utf8"),
    ]),
  );
  for (const [relativePath, content] of workflows) {
    verifyWorkflowActionPins(relativePath, content);
  }
  verifySignedWorkflowOrder(
    requiredWorkflows[".github/workflows/windows-signed-release.yml"],
  );
  verifyPromotionWorkflow(
    requiredWorkflows[".github/workflows/windows-e2e-real.yml"],
  );
  const unsigned = requiredWorkflows[".github/workflows/windows-release.yml"];
  const signed =
    requiredWorkflows[".github/workflows/windows-signed-release.yml"];
  const releaseConfigSequence =
    "--config src-tauri/tauri.release-reset.conf.json --config src-tauri/tauri.release.conf.json";
  const unsignedConfigSequence = `${releaseConfigSequence} --config src-tauri/tauri.unsigned.conf.json`;
  if (
    !unsigned.includes("-Mode Unsigned") ||
    !unsigned.includes("windows-x64-unsigned") ||
    !unsigned.includes("VERISILO_ENGINE_SIGNER_SHA256") ||
    !unsigned.includes("pnpm engine:verify:release") ||
    !unsigned.includes("stage-hyperv-image.ps1") ||
    !unsigned.includes("package-extension-zip.mjs") ||
    !unsigned.includes("promotion-status.json") ||
    !unsigned.includes("actions: read") ||
    unsigned.includes("Compress-Archive") ||
    unsigned.includes("secrets.VERISILO_AUTHENTICODE") ||
    unsigned.split(unsignedConfigSequence).length - 1 !== 1 ||
    !unsigned.includes("desktop-only current-user NSIS installer")
  ) {
    throw new Error(
      "Unsigned Windows workflow is not clearly separated from signing inputs and output.",
    );
  }
  if (
    !signed.includes("stage-hyperv-image.ps1") ||
    !signed.includes("package-extension-zip.mjs") ||
    !signed.includes("promotion-status.json") ||
    !signed.includes("actions: read") ||
    signed.includes("Compress-Archive")
  ) {
    throw new Error(
      "Signed Windows workflow lacks exact image, deterministic extension ZIP, or non-promotion gates.",
    );
  }
}

function assertReleaseId(name, value) {
  if (
    typeof value !== "string" ||
    !extensionIdPattern.test(value) ||
    knownPlaceholderIds.has(value) ||
    new Set(value).size < 4
  ) {
    throw new Error(
      `${name} must be an explicit published extension ID, not an example or placeholder.`,
    );
  }
}

async function readVersions() {
  const [
    rootPackage,
    desktopPackage,
    extensionPackage,
    extensionManifest,
    tauri,
    cargo,
  ] = await Promise.all([
    readFile(path.join(root, "package.json"), "utf8").then(JSON.parse),
    readFile(path.join(root, "apps/desktop/package.json"), "utf8").then(
      JSON.parse,
    ),
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
  ]);
  const cargoVersion = cargo.match(/^version = "([^"]+)"$/mu)?.[1];
  const desktopVersions = new Set([
    rootPackage.version,
    desktopPackage.version,
    tauri.version,
    cargoVersion,
  ]);
  if (desktopVersions.size !== 1 || desktopVersions.has(undefined)) {
    throw new Error(
      "Root, desktop package, Tauri, and Cargo desktop versions must match.",
    );
  }
  if (extensionPackage.version !== extensionManifest.version) {
    throw new Error("Extension package and MV3 manifest versions must match.");
  }
  const desktopVersion = rootPackage.version;
  const expectedVersion = process.env.VERISILO_RELEASE_VERSION;
  if (expectedVersion !== undefined && expectedVersion !== desktopVersion) {
    throw new Error(
      `VERISILO_RELEASE_VERSION ${expectedVersion} does not match desktop ${desktopVersion}.`,
    );
  }
  return { desktopVersion, extensionVersion: extensionManifest.version };
}

async function readReleaseConfig(configPath) {
  const config = JSON.parse(await readFile(configPath, "utf8"));
  const keys = Object.keys(config).sort();
  if (
    JSON.stringify(keys) !==
      JSON.stringify([
        "chromeExtensionId",
        "edgeExtensionId",
        "schemaVersion",
      ]) ||
    config.schemaVersion !== 1
  ) {
    throw new Error("Native Host release config has unknown/missing fields.");
  }
  assertReleaseId("chromeExtensionId", config.chromeExtensionId);
  assertReleaseId("edgeExtensionId", config.edgeExtensionId);
  for (const [environmentName, configName] of [
    ["VERISILO_CHROME_EXTENSION_ID", "chromeExtensionId"],
    ["VERISILO_EDGE_EXTENSION_ID", "edgeExtensionId"],
  ]) {
    const environmentValue = process.env[environmentName];
    if (
      environmentValue !== undefined &&
      environmentValue !== config[configName]
    ) {
      throw new Error(`${environmentName} does not match the release config.`);
    }
  }
  return config;
}

async function collectFiles(directory, relativeDirectory = "") {
  const entries = await readdir(path.join(directory, relativeDirectory), {
    withFileTypes: true,
  });
  const files = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    const absolutePath = path.join(directory, ...relativePath.split("/"));
    const metadata = await lstat(absolutePath);
    if (metadata.isSymbolicLink()) {
      throw new Error(`Release directory contains a symlink: ${relativePath}`);
    }
    if (metadata.isDirectory()) {
      files.push(...(await collectFiles(directory, relativePath)));
    } else if (metadata.isFile()) {
      files.push({ path: relativePath, absolutePath, bytes: metadata.size });
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

function scanContent(relativePath, content) {
  const extension = path.extname(relativePath).toLowerCase();
  if (forbiddenSecretExtensions.has(extension)) {
    throw new Error(
      `Release contains a forbidden key/certificate file: ${relativePath}`,
    );
  }
  const text = content.toString("utf8");
  for (const pattern of [...placeholderPatterns, ...secretPatterns]) {
    if (pattern.test(text)) {
      throw new Error(
        `Release policy pattern ${pattern} matched ${relativePath}.`,
      );
    }
  }
  const signingPassword = process.env.VERISILO_AUTHENTICODE_PASSWORD;
  if (
    signingPassword !== undefined &&
    signingPassword.length >= 8 &&
    content.includes(Buffer.from(signingPassword))
  ) {
    throw new Error(
      `Authenticode password bytes were found in ${relativePath}.`,
    );
  }
}

async function verifyHyperVImageResource(directory, files) {
  const manifestPath =
    "bundle-resources/environment/hyperv-image-manifest.json";
  const manifestFile = files.find((file) => file.path === manifestPath);
  if (manifestFile === undefined) {
    throw new Error("Release is missing the strict Hyper-V image manifest.");
  }
  const manifestText = await readFile(manifestFile.absolutePath, "utf8");
  const manifest = JSON.parse(manifestText);
  const keys = [
    "artifactId",
    "imageFile",
    "imageSha256",
    "redistributionAcknowledged",
    "repository",
    "schema",
    "schemaVersion",
  ];
  if (
    JSON.stringify(Object.keys(manifest).sort()) !== JSON.stringify(keys) ||
    manifest.schema !== "urn:verisilo:hyperv-image-source:1" ||
    manifest.schemaVersion !== 1 ||
    typeof manifest.repository !== "string" ||
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(manifest.repository) ||
    !Number.isSafeInteger(manifest.artifactId) ||
    manifest.artifactId < 1 ||
    typeof manifest.imageFile !== "string" ||
    !/^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$/u.test(manifest.imageFile) ||
    /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/iu.test(
      manifest.imageFile,
    ) ||
    manifest.imageFile.includes("..") ||
    typeof manifest.imageSha256 !== "string" ||
    !/^[0-9a-f]{64}$/u.test(manifest.imageSha256) ||
    /^0{64}$/u.test(manifest.imageSha256) ||
    manifest.redistributionAcknowledged !== true ||
    manifestText !== stableJson(manifest)
  ) {
    throw new Error(
      "Release Hyper-V image manifest is non-canonical or violates schema version 1.",
    );
  }
  const expectedRepository = process.env.GITHUB_REPOSITORY;
  const expectedArtifactId = process.env.VERISILO_HYPERV_ARTIFACT_ID;
  if (
    expectedRepository === undefined ||
    manifest.repository !== expectedRepository ||
    expectedArtifactId === undefined ||
    String(manifest.artifactId) !== expectedArtifactId ||
    manifest.imageFile !== process.env.VERISILO_HYPERV_IMAGE_FILE ||
    manifest.imageSha256 !== process.env.VERISILO_HYPERV_IMAGE_SHA256 ||
    process.env.VERISILO_HYPERV_REDISTRIBUTION_ACKNOWLEDGED !== "true"
  ) {
    throw new Error(
      "Release Hyper-V image manifest does not match the same-repository workflow inputs.",
    );
  }
  const imagePrefix = "bundle-resources/environment/images/";
  const imageFiles = files.filter((file) => file.path.startsWith(imagePrefix));
  const expectedImagePath = `${imagePrefix}${manifest.imageFile}`;
  if (imageFiles.length !== 1 || imageFiles[0].path !== expectedImagePath) {
    throw new Error(
      "Release must contain exactly the VHDX leaf declared by its Hyper-V manifest.",
    );
  }
  if ((await sha256File(imageFiles[0].absolutePath)) !== manifest.imageSha256) {
    throw new Error(
      "Release Hyper-V VHDX SHA-256 does not match its strict manifest.",
    );
  }
}

async function verifyExtensionZipEvidence(
  files,
  extensionZip,
  extensionVersion,
) {
  const manifestFile = files.find(
    (file) => file.path === "extension-zip-manifest.json",
  );
  if (manifestFile === undefined) {
    throw new Error("Release is missing extension-zip-manifest.json.");
  }
  const manifestText = await readFile(manifestFile.absolutePath, "utf8");
  const manifest = JSON.parse(manifestText);
  const expectedArchiveName = `VeriSilo-Companion-${extensionVersion}-chrome-edge.zip`;
  const epoch = Number(process.env.SOURCE_DATE_EPOCH);
  if (
    manifest?.schema !== "urn:verisilo:deterministic-extension-zip:1" ||
    manifest.schemaVersion !== 1 ||
    !Number.isSafeInteger(manifest.sourceDateEpoch) ||
    manifest.sourceDateEpoch !== epoch ||
    manifest.archive?.file !== expectedArchiveName ||
    extensionZip.path !== expectedArchiveName ||
    manifest.archive.format !== "zip32-store" ||
    manifest.archive.bytes !== extensionZip.bytes ||
    manifest.archive.sha256 !== (await sha256File(extensionZip.absolutePath)) ||
    !Array.isArray(manifest.files) ||
    manifestText !== stableJson(manifest)
  ) {
    throw new Error(
      "Deterministic extension ZIP manifest or archive hash is stale or invalid.",
    );
  }
  const extensionFiles = files
    .filter((file) => file.path.startsWith("extension/"))
    .map((file) => ({
      file,
      path: file.path.slice("extension/".length),
    }))
    .sort((left, right) =>
      Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)),
    );
  const expectedFiles = await Promise.all(
    extensionFiles.map(async ({ file, path: relativePath }) => ({
      path: relativePath,
      sha256: await sha256File(file.absolutePath),
      bytes: file.bytes,
    })),
  );
  if (JSON.stringify(manifest.files) !== JSON.stringify(expectedFiles)) {
    throw new Error(
      "Extension ZIP content manifest does not match the staged extension tree.",
    );
  }
}

async function verifyPromotionStatus(directory) {
  const statusText = await readFile(
    path.join(directory, "promotion-status.json"),
    "utf8",
  );
  const status = JSON.parse(statusText);
  if (
    JSON.stringify(Object.keys(status).sort()) !==
      JSON.stringify([
        "reason",
        "requiredWorkflow",
        "schema",
        "schemaVersion",
        "sourceRevision",
        "state",
      ]) ||
    status.schema !== "urn:verisilo:windows-promotion-status:1" ||
    status.schemaVersion !== 1 ||
    status.state !== "NOT_PROMOTABLE" ||
    status.sourceRevision !== process.env.VERISILO_SOURCE_REVISION ||
    status.requiredWorkflow !== ".github/workflows/windows-e2e-real.yml" ||
    typeof status.reason !== "string" ||
    !status.reason.includes("RequireAll") ||
    statusText !== stableJson(status)
  ) {
    throw new Error(
      "Candidate lacks a canonical exact-source NOT_PROMOTABLE status.",
    );
  }
}

function verifyAuthenticodeCoverage(files, report, checkEnvironment = true) {
  if (
    report?.schemaVersion !== 1 ||
    !["Unsigned", "VerifySigned"].includes(report.mode) ||
    !["unsigned", "signed-and-verified"].includes(report.signingState) ||
    !Array.isArray(report.files)
  ) {
    throw new Error(
      "authenticode-status.json is missing a final unsigned or signed verification result.",
    );
  }
  const expectedState =
    report.mode === "Unsigned" ? "unsigned" : "signed-and-verified";
  if (report.signingState !== expectedState) {
    throw new Error("Authenticode mode and signingState disagree.");
  }
  const expectedSigner = report.expectedSignerCertificateSha256;
  if (report.mode === "VerifySigned") {
    if (
      typeof expectedSigner !== "string" ||
      !/^[0-9a-f]{64}$/u.test(expectedSigner) ||
      /^0{64}$/u.test(expectedSigner)
    ) {
      throw new Error(
        "Signed Authenticode report lacks the release-pinned signer certificate SHA-256.",
      );
    }
    if (
      checkEnvironment &&
      process.env.VERISILO_AUTHENTICODE_SIGNER_SHA256 !== expectedSigner
    ) {
      throw new Error(
        "Final Authenticode signer does not match VERISILO_AUTHENTICODE_SIGNER_SHA256.",
      );
    }
  } else if (expectedSigner !== null) {
    throw new Error(
      "Unsigned Authenticode report must not claim a signer pin.",
    );
  }
  if (
    checkEnvironment &&
    process.env.VERISILO_SIGNING_STATE !== undefined &&
    process.env.VERISILO_SIGNING_STATE !== report.signingState
  ) {
    throw new Error(
      "VERISILO_SIGNING_STATE does not match the final Authenticode report.",
    );
  }
  const expectedPaths = files
    .map((file) => file.path)
    .filter((filePath) =>
      [".exe", ".ps1"].includes(path.extname(filePath).toLowerCase()),
    )
    .sort();
  const reportedPaths = report.files.map((entry) => entry.path).sort();
  if (
    new Set(reportedPaths).size !== reportedPaths.length ||
    JSON.stringify(reportedPaths) !== JSON.stringify(expectedPaths)
  ) {
    throw new Error(
      "Authenticode report does not cover every staged EXE and PS1 exactly once.",
    );
  }
  for (const entry of report.files) {
    if (report.mode === "Unsigned") {
      if (entry.status !== "NotSigned") {
        throw new Error("Unsigned Authenticode report contains a signed file.");
      }
    } else if (
      entry.status !== "Valid" ||
      typeof entry.signerThumbprint !== "string" ||
      entry.signerCertificateSha256 !== expectedSigner ||
      typeof entry.timestampThumbprint !== "string"
    ) {
      throw new Error(
        "Signed Authenticode report lacks a valid signer or timestamp for a file.",
      );
    }
  }
}

async function verifyRelease(directory, config, versions) {
  const files = await collectFiles(directory);
  const paths = files.map((file) => file.path);
  const nativeHost = files.find((file) =>
    /(?:^|\/)verisilo-native-host\.exe$/iu.test(file.path),
  );
  const desktop = files.find((file) =>
    /(?:^|\/)verisilo\.exe$/iu.test(file.path),
  );
  const installer = files.find((file) =>
    new RegExp(
      `(?:^|/)VeriSilo[_-]${versions.desktopVersion.replaceAll(".", "\\.")}.*(?:setup|installer).*\\.exe$`,
      "iu",
    ).test(file.path),
  );
  const extensionZip = files.find((file) =>
    new RegExp(
      `(?:^|/)VeriSilo-Companion-${versions.extensionVersion.replaceAll(".", "\\.")}.*\\.zip$`,
      "iu",
    ).test(file.path),
  );
  const extensionManifest = files.find(
    (file) => file.path === "extension/manifest.json",
  );
  for (const [label, file] of [
    ["desktop executable", desktop],
    ["Native Messaging Host executable", nativeHost],
    ["NSIS installer", installer],
    ["Companion ZIP", extensionZip],
    ["staged Companion manifest", extensionManifest],
  ]) {
    if (file === undefined) {
      throw new Error(`Release is missing ${label}.`);
    }
  }
  if (!paths.includes("native-host-release-config.json")) {
    throw new Error("Release is missing native-host-release-config.json.");
  }
  for (const required of [
    "LICENSE",
    "THIRD_PARTY_NOTICES.md",
    "dependency-licenses.json",
    "extension-zip-manifest.json",
    "promotion-status.json",
    "sbom/dependency-inventory.json",
    "sbom/bom.cyclonedx.json",
    "sbom/bom.spdx.json",
  ]) {
    if (!paths.includes(required)) {
      throw new Error(
        `Release is missing required audit artifact ${required}.`,
      );
    }
  }
  if (
    paths.includes("npm-licenses.raw.json") ||
    paths.includes("cargo-metadata.raw.json")
  ) {
    throw new Error(
      "Raw license metadata with runner-local paths must not ship.",
    );
  }
  const licenseReport = JSON.parse(
    await readFile(path.join(directory, "dependency-licenses.json"), "utf8"),
  );
  if (
    licenseReport.schema !== "urn:verisilo:dependency-license-evidence:1" ||
    licenseReport.schemaVersion !== 1 ||
    licenseReport.target !== "x86_64-pc-windows-msvc" ||
    licenseReport.legalConclusion !== false ||
    !Array.isArray(licenseReport.components) ||
    licenseReport.components.length === 0 ||
    licenseReport.components.some(
      (component) =>
        component.requiresHumanReview !== true ||
        typeof component.purl !== "string" ||
        component.purl === "",
    ) ||
    licenseReport.coverage?.lockedComponents !==
      licenseReport.components.length ||
    licenseReport.coverage.metadataResolved +
      licenseReport.coverage.metadataUnresolved !==
      licenseReport.components.length
  ) {
    throw new Error(
      "dependency-licenses.json is malformed or falsely presents metadata as a legal conclusion.",
    );
  }
  for (const resourcePath of expectedBundleResources) {
    if (!paths.includes(resourcePath)) {
      throw new Error(
        `Release is missing staged bundle resource ${resourcePath}.`,
      );
    }
  }
  await verifyHyperVImageResource(directory, files);
  await verifyExtensionZipEvidence(
    files,
    extensionZip,
    versions.extensionVersion,
  );
  await verifyPromotionStatus(directory);
  const stagedConfig = JSON.parse(
    await readFile(
      path.join(directory, "native-host-release-config.json"),
      "utf8",
    ),
  );
  if (JSON.stringify(stagedConfig) !== JSON.stringify(config)) {
    throw new Error(
      "Staged Native Host config differs from the verified config.",
    );
  }
  const stagedExtensionManifest = JSON.parse(
    await readFile(extensionManifest.absolutePath, "utf8"),
  );
  if (stagedExtensionManifest.version !== versions.extensionVersion) {
    throw new Error("Staged Companion manifest version is stale.");
  }

  for (const file of files) {
    if (path.extname(file.path).toLowerCase() !== ".vhdx") {
      scanContent(file.path, await readFile(file.absolutePath));
    }
  }
  const authenticodeReport = JSON.parse(
    await readFile(path.join(directory, "authenticode-status.json"), "utf8"),
  );
  verifyAuthenticodeCoverage(files, authenticodeReport);
  const hostContent = await readFile(nativeHost.absolutePath);
  for (const id of [config.chromeExtensionId, config.edgeExtensionId]) {
    if (!hostContent.includes(Buffer.from(id, "ascii"))) {
      throw new Error(
        "Native Host does not contain one of the verified production extension IDs.",
      );
    }
  }
  process.stdout.write(
    `Release policy passed for ${files.length} staged files (desktop ${versions.desktopVersion}, extension ${versions.extensionVersion}).\n`,
  );
}

async function selfTest() {
  assertReleaseId("test", "ponmlkjihgfedcbaponmlkjihgfedcba");
  for (const invalid of [
    "abcdefghijklmnopabcdefghijklmnop",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "too-short",
  ]) {
    let rejected = false;
    try {
      assertReleaseId("test", invalid);
    } catch {
      rejected = true;
    }
    if (!rejected) {
      throw new Error(`Release ID self-test accepted ${invalid}.`);
    }
  }
  let secretRejected = false;
  try {
    scanContent("fixture.txt", Buffer.from("-----BEGIN PRIVATE KEY-----"));
  } catch {
    secretRejected = true;
  }
  if (!secretRejected) {
    throw new Error("Secret scanner self-test failed.");
  }
  verifyAuthenticodeCoverage(
    [
      { path: "VeriSilo_0.1.0_x64-setup.exe" },
      { path: "bundle-resources/native-host/install-native-host.ps1" },
    ],
    {
      schemaVersion: 1,
      mode: "VerifySigned",
      signingState: "signed-and-verified",
      expectedSignerCertificateSha256: "a".repeat(64),
      files: [
        {
          path: "VeriSilo_0.1.0_x64-setup.exe",
          status: "Valid",
          signerThumbprint: "signer",
          signerCertificateSha256: "a".repeat(64),
          timestampThumbprint: "timestamp",
        },
        {
          path: "bundle-resources/native-host/install-native-host.ps1",
          status: "Valid",
          signerThumbprint: "signer",
          signerCertificateSha256: "a".repeat(64),
          timestampThumbprint: "timestamp",
        },
      ],
    },
    false,
  );
  verifyWorkflowActionPins(
    "fixture.yml",
    "- uses: example/action@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa # v1.2.3",
  );
  await verifyWorkflowPolicies();
  process.stdout.write("Release policy self-test passed.\n");
}

if (process.argv.includes("--self-test")) {
  await selfTest();
  process.exit(0);
}
if (!process.argv.includes("--check")) {
  throw new Error(
    "Release policy is read-only; pass --check (or --self-test) explicitly.",
  );
}
const configIndex = process.argv.indexOf("--config");
const configValue =
  configIndex === -1 ? undefined : process.argv[configIndex + 1];
if (configValue === undefined) {
  throw new Error(
    "Usage: node scripts/verify-release-policy.mjs --check --config <release-config> [--release <directory>] | --self-test",
  );
}
const versions = await readVersions();
await verifyWorkflowPolicies();
const config = await readReleaseConfig(path.resolve(root, configValue));
const releaseIndex = process.argv.indexOf("--release");
const releaseValue =
  releaseIndex === -1 ? undefined : process.argv[releaseIndex + 1];
if (releaseValue === undefined) {
  process.stdout.write(
    `Source release policy passed (desktop ${versions.desktopVersion}, extension ${versions.extensionVersion}); artifact checks were not requested.\n`,
  );
} else {
  await verifyRelease(path.resolve(root, releaseValue), config, versions);
}
