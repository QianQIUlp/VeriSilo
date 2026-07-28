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

const repositoryUrl = "https://github.com/QianQIUlp/VeriSilo";
const sha256Pattern = /^[0-9a-f]{64}$/u;
const revisionPattern = /^[0-9a-f]{40}$/u;
const receiptKeys = [
  "artifactId",
  "artifactSha256",
  "expiresAt",
  "repository",
  "schema",
  "schemaVersion",
  "sourceRevision",
];

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

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

function assertSafeRelativePath(value, label = "candidate path") {
  if (
    typeof value !== "string" ||
    value.length === 0 ||
    value.length > 512 ||
    !/^[A-Za-z0-9._+/\-]+$/u.test(value) ||
    value.includes("\\") ||
    value.includes(":") ||
    /[\0\r\n]/u.test(value) ||
    value.startsWith("/") ||
    value.endsWith("/") ||
    value.split("/").some((segment) => {
      const baseName = segment.split(".")[0];
      return (
        segment === "" ||
        segment === "." ||
        segment === ".." ||
        segment.endsWith(".") ||
        segment.endsWith(" ") ||
        /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])$/iu.test(baseName)
      );
    }) ||
    path.posix.normalize(value) !== value
  ) {
    throw new Error(
      `${label} is absolute, traversing, or non-canonical: ${value}`,
    );
  }
}

async function collectFiles(directory, relativeDirectory = "") {
  const entries = await readdir(path.join(directory, relativeDirectory), {
    withFileTypes: true,
  });
  const files = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    assertSafeRelativePath(relativePath);
    const absolutePath = path.join(directory, ...relativePath.split("/"));
    const metadata = await lstat(absolutePath);
    if (metadata.isSymbolicLink()) {
      throw new Error(`Candidate contains a symlink: ${relativePath}`);
    }
    if (metadata.isDirectory()) {
      files.push(...(await collectFiles(directory, relativePath)));
    } else if (metadata.isFile()) {
      files.push({
        path: relativePath,
        absolutePath,
        bytes: metadata.size,
        sha256: await sha256File(absolutePath),
      });
    } else {
      throw new Error(`Candidate contains a non-file entry: ${relativePath}`);
    }
  }
  files.sort((left, right) => left.path.localeCompare(right.path));
  const folded = files.map((file) => file.path.toLowerCase());
  if (new Set(folded).size !== folded.length) {
    throw new Error("Candidate contains case-colliding Windows paths.");
  }
  return files;
}

function parseChecksumManifest(content) {
  if (!content.endsWith("\n") || content.includes("\r")) {
    throw new Error("SHA256SUMS must be LF-terminated canonical text.");
  }
  const entries = content
    .slice(0, -1)
    .split("\n")
    .map((line) => {
      const match = line.match(/^([0-9a-f]{64})  (.+)$/u);
      if (match === null) {
        throw new Error("SHA256SUMS contains a malformed line.");
      }
      assertSafeRelativePath(match[2], "SHA256SUMS path");
      if (match[2] === "SHA256SUMS") {
        throw new Error("SHA256SUMS must not recursively list itself.");
      }
      return { sha256: match[1], path: match[2] };
    });
  if (
    entries.length === 0 ||
    new Set(entries.map((entry) => entry.path)).size !== entries.length
  ) {
    throw new Error("SHA256SUMS is empty or contains duplicate paths.");
  }
  const sorted = [...entries].sort((left, right) =>
    left.path.localeCompare(right.path),
  );
  if (JSON.stringify(entries) !== JSON.stringify(sorted)) {
    throw new Error("SHA256SUMS is not sorted canonically.");
  }
  return entries;
}

function assertExactFileSet(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw new Error(
      `${label} does not exactly cover the extracted candidate bytes.`,
    );
  }
}

function validateReceipt(receipt, expected) {
  if (
    receipt === null ||
    typeof receipt !== "object" ||
    Array.isArray(receipt) ||
    JSON.stringify(Object.keys(receipt).sort()) !==
      JSON.stringify(receiptKeys) ||
    receipt.schema !== "urn:verisilo:actions-artifact-receipt:1" ||
    receipt.schemaVersion !== 1 ||
    receipt.repository !== expected.repository ||
    receipt.artifactId !== expected.artifactId ||
    receipt.artifactSha256 !== expected.artifactSha256 ||
    receipt.sourceRevision !== expected.sourceRevision ||
    typeof receipt.expiresAt !== "string" ||
    !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/u.test(receipt.expiresAt) ||
    Date.parse(receipt.expiresAt) <= Date.now()
  ) {
    throw new Error(
      "Actions artifact receipt is stale, expired, or does not match the gate input.",
    );
  }
}

function validateProvenance(
  provenance,
  expectedRevision,
  payloadFiles,
  hyperVImageSource,
) {
  if (
    provenance?.schema !== "urn:verisilo:release-provenance:1" ||
    provenance.schemaVersion !== 1 ||
    provenance.source?.repository !== repositoryUrl ||
    provenance.source.revision !== expectedRevision ||
    provenance.source.dirty !== false ||
    provenance.build?.artifactProfile !== "windows" ||
    provenance.build?.target !== "x86_64-pc-windows-msvc" ||
    provenance.build.promotionState !== "NOT_PROMOTABLE" ||
    JSON.stringify(provenance.build.hyperVImageSource) !==
      JSON.stringify(hyperVImageSource) ||
    !Array.isArray(provenance.artifacts)
  ) {
    throw new Error(
      "Candidate provenance does not bind the requested clean Windows source revision.",
    );
  }
  const actualArtifacts = provenance.artifacts.map((entry) => {
    if (
      entry === null ||
      typeof entry !== "object" ||
      Array.isArray(entry) ||
      JSON.stringify(Object.keys(entry).sort()) !==
        JSON.stringify(["bytes", "path", "sha256"]) ||
      !Number.isSafeInteger(entry.bytes) ||
      entry.bytes < 0 ||
      !sha256Pattern.test(entry.sha256)
    ) {
      throw new Error(
        "Candidate provenance contains a malformed artifact entry.",
      );
    }
    assertSafeRelativePath(entry.path, "provenance artifact path");
    return { path: entry.path, sha256: entry.sha256, bytes: entry.bytes };
  });
  const expectedArtifacts = payloadFiles.map(
    ({ path: filePath, sha256, bytes }) => ({
      path: filePath,
      sha256,
      bytes,
    }),
  );
  assertExactFileSet(
    actualArtifacts,
    expectedArtifacts,
    "provenance.artifacts",
  );
}

async function validateHyperVImageBinding(files) {
  const manifestFile = exactlyOne(
    files,
    (file) =>
      file.path === "bundle-resources/environment/hyperv-image-manifest.json",
    "bundled Hyper-V image manifest",
  );
  const manifestText = await readFile(manifestFile.absolutePath, "utf8");
  const manifest = JSON.parse(manifestText);
  if (
    JSON.stringify(Object.keys(manifest).sort()) !==
      JSON.stringify([
        "artifactId",
        "imageFile",
        "imageSha256",
        "redistributionAcknowledged",
        "repository",
        "schema",
        "schemaVersion",
      ]) ||
    manifest.schema !== "urn:verisilo:hyperv-image-source:1" ||
    manifest.schemaVersion !== 1 ||
    manifest.repository !== "QianQIUlp/VeriSilo" ||
    !Number.isSafeInteger(manifest.artifactId) ||
    manifest.artifactId < 1 ||
    typeof manifest.imageFile !== "string" ||
    !/^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$/u.test(manifest.imageFile) ||
    manifest.imageFile.includes("..") ||
    typeof manifest.imageSha256 !== "string" ||
    !sha256Pattern.test(manifest.imageSha256) ||
    /^0{64}$/u.test(manifest.imageSha256) ||
    manifest.redistributionAcknowledged !== true ||
    manifestText !== stableJson(manifest)
  ) {
    throw new Error(
      "Candidate Hyper-V image manifest is malformed or not canonical-repository bound.",
    );
  }
  const image = exactlyOne(
    files,
    (file) =>
      file.path === `bundle-resources/environment/images/${manifest.imageFile}`,
    "manifest-declared Hyper-V VHDX",
  );
  const imageFiles = files.filter((file) =>
    file.path.startsWith("bundle-resources/environment/images/"),
  );
  if (imageFiles.length !== 1 || image.sha256 !== manifest.imageSha256) {
    throw new Error(
      "Candidate Hyper-V image bytes do not match the sole manifest entry.",
    );
  }
  return {
    repository: manifest.repository,
    artifactId: manifest.artifactId,
    imageFile: manifest.imageFile,
    imageSha256: manifest.imageSha256,
    redistributionAcknowledged: true,
  };
}

function exactlyOne(files, predicate, label) {
  const matches = files.filter(predicate);
  if (matches.length !== 1) {
    throw new Error(
      `Candidate must contain exactly one ${label}; found ${matches.length}.`,
    );
  }
  return matches[0];
}

async function verifyCandidate(
  directory,
  receiptPath,
  expected,
  descriptorPath,
) {
  const receiptText = await readFile(receiptPath, "utf8");
  const receipt = JSON.parse(receiptText);
  validateReceipt(receipt, expected);
  if (receiptText !== stableJson(receipt)) {
    throw new Error("Actions artifact receipt is not canonical JSON.");
  }

  const files = await collectFiles(directory);
  const checksumFile = exactlyOne(
    files,
    (file) => file.path === "SHA256SUMS",
    "root SHA256SUMS",
  );
  const provenanceFile = exactlyOne(
    files,
    (file) => file.path === "provenance.json",
    "root provenance.json",
  );
  const manifestEntries = parseChecksumManifest(
    await readFile(checksumFile.absolutePath, "utf8"),
  );
  const expectedManifestEntries = files
    .filter((file) => file.path !== "SHA256SUMS")
    .map(({ path: filePath, sha256 }) => ({ sha256, path: filePath }));
  assertExactFileSet(manifestEntries, expectedManifestEntries, "SHA256SUMS");

  const provenance = JSON.parse(
    await readFile(provenanceFile.absolutePath, "utf8"),
  );
  const payloadFiles = files.filter(
    (file) => file.path !== "SHA256SUMS" && file.path !== "provenance.json",
  );
  const hyperVImageSource = await validateHyperVImageBinding(files);
  validateProvenance(
    provenance,
    expected.sourceRevision,
    payloadFiles,
    hyperVImageSource,
  );

  const desktop = exactlyOne(
    files,
    (file) => file.path === "verisilo.exe",
    "root desktop executable",
  );
  const nativeHost = exactlyOne(
    files,
    (file) => file.path === "verisilo-native-host.exe",
    "root Native Messaging Host",
  );
  const releaseConfig = exactlyOne(
    files,
    (file) => file.path === "native-host-release-config.json",
    "root Native Host release config",
  );
  const installer = exactlyOne(
    files,
    (file) =>
      /^VeriSilo_[0-9]+\.[0-9]+\.[0-9]+_x64-setup\.exe$/u.test(file.path),
    "root x64 NSIS installer",
  );
  const promotionStatusFile = exactlyOne(
    files,
    (file) => file.path === "promotion-status.json",
    "root non-promotion status",
  );
  const promotionStatusText = await readFile(
    promotionStatusFile.absolutePath,
    "utf8",
  );
  const promotionStatus = JSON.parse(promotionStatusText);
  if (
    JSON.stringify(Object.keys(promotionStatus).sort()) !==
      JSON.stringify([
        "reason",
        "requiredWorkflow",
        "schema",
        "schemaVersion",
        "sourceRevision",
        "state",
      ]) ||
    promotionStatus.schema !== "urn:verisilo:windows-promotion-status:1" ||
    promotionStatus.schemaVersion !== 1 ||
    promotionStatus.state !== "NOT_PROMOTABLE" ||
    promotionStatus.sourceRevision !== expected.sourceRevision ||
    promotionStatus.requiredWorkflow !==
      ".github/workflows/windows-e2e-real.yml" ||
    typeof promotionStatus.reason !== "string" ||
    !promotionStatus.reason.includes("RequireAll") ||
    promotionStatusText !== stableJson(promotionStatus)
  ) {
    throw new Error(
      "Candidate non-promotion status is malformed or source-mismatched.",
    );
  }

  const descriptor = {
    schema: "urn:verisilo:windows-promotion-candidate:1",
    schemaVersion: 1,
    repository: expected.repository,
    artifactId: expected.artifactId,
    artifactSha256: expected.artifactSha256,
    sourceRevision: expected.sourceRevision,
    checksumManifestSha256: checksumFile.sha256,
    acceptanceDriver: {
      sourceRevision: expected.sourceRevision,
      cargoFeature: "acceptance-tests",
      cargoTarget: "verisilo-acceptance-driver",
    },
    files: {
      desktopExe: desktop.absolutePath,
      nativeHost: nativeHost.absolutePath,
      releaseConfig: releaseConfig.absolutePath,
      nsisInstaller: installer.absolutePath,
    },
  };
  await writeFile(descriptorPath, stableJson(descriptor), "utf8");
  return descriptor;
}

async function writeFixture(directory, revision) {
  const imageDirectory = path.join(
    directory,
    "bundle-resources",
    "environment",
    "images",
  );
  await mkdir(imageDirectory, { recursive: true });
  const imageBytes = Buffer.from("not a real disk image\n", "utf8");
  const imageSha256 = createHash("sha256").update(imageBytes).digest("hex");
  await writeFile(path.join(imageDirectory, "licensed-base.vhdx"), imageBytes);
  await writeFile(
    path.join(
      directory,
      "bundle-resources",
      "environment",
      "hyperv-image-manifest.json",
    ),
    stableJson({
      schema: "urn:verisilo:hyperv-image-source:1",
      schemaVersion: 1,
      repository: "QianQIUlp/VeriSilo",
      artifactId: 99,
      imageFile: "licensed-base.vhdx",
      imageSha256,
      redistributionAcknowledged: true,
    }),
  );
  for (const [name, content] of [
    ["verisilo.exe", "desktop"],
    ["verisilo-native-host.exe", "host"],
    ["native-host-release-config.json", "{}\n"],
    ["VeriSilo_0.1.0_x64-setup.exe", "installer"],
    [
      "promotion-status.json",
      stableJson({
        schema: "urn:verisilo:windows-promotion-status:1",
        schemaVersion: 1,
        state: "NOT_PROMOTABLE",
        sourceRevision: revision,
        requiredWorkflow: ".github/workflows/windows-e2e-real.yml",
        reason: "Fixture requires the exact RequireAll gate.",
      }),
    ],
  ]) {
    await writeFile(path.join(directory, name), content);
  }
  const initialFiles = await collectFiles(directory);
  const provenance = {
    schema: "urn:verisilo:release-provenance:1",
    schemaVersion: 1,
    generatedAt: "2026-01-01T00:00:00Z",
    source: { repository: repositoryUrl, revision, dirty: false, inputs: [] },
    versions: {},
    build: {
      artifactProfile: "windows",
      target: "x86_64-pc-windows-msvc",
      promotionState: "NOT_PROMOTABLE",
      hyperVImageSource: {
        repository: "QianQIUlp/VeriSilo",
        artifactId: 99,
        imageFile: "licensed-base.vhdx",
        imageSha256,
        redistributionAcknowledged: true,
      },
    },
    artifacts: initialFiles.map(({ path: filePath, sha256, bytes }) => ({
      path: filePath,
      sha256,
      bytes,
    })),
  };
  await writeFile(
    path.join(directory, "provenance.json"),
    stableJson(provenance),
  );
  const checksumFiles = await collectFiles(directory);
  await writeFile(
    path.join(directory, "SHA256SUMS"),
    checksumFiles.map((file) => `${file.sha256}  ${file.path}\n`).join(""),
  );
}

async function expectRejected(action, label) {
  let rejected = false;
  try {
    await action();
  } catch {
    rejected = true;
  }
  if (!rejected) {
    throw new Error(`Promotion candidate self-test accepted ${label}.`);
  }
}

async function selfTest() {
  const temporaryRoot = await mkdtemp(
    path.join(tmpdir(), "verisilo-promotion-candidate-"),
  );
  const candidate = path.join(temporaryRoot, "candidate");
  const receiptPath = path.join(temporaryRoot, "receipt.json");
  const descriptorPath = path.join(temporaryRoot, "descriptor.json");
  const expected = {
    repository: "QianQIUlp/VeriSilo",
    artifactId: 123,
    artifactSha256: "a".repeat(64),
    sourceRevision: "b".repeat(40),
  };
  try {
    await mkdir(candidate);
    await writeFixture(candidate, expected.sourceRevision);
    const receipt = {
      schema: "urn:verisilo:actions-artifact-receipt:1",
      schemaVersion: 1,
      repository: expected.repository,
      artifactId: expected.artifactId,
      artifactSha256: expected.artifactSha256,
      sourceRevision: expected.sourceRevision,
      expiresAt: "2099-01-01T00:00:00Z",
    };
    await writeFile(receiptPath, stableJson(receipt));
    const descriptor = await verifyCandidate(
      candidate,
      receiptPath,
      expected,
      descriptorPath,
    );
    if (
      descriptor.acceptanceDriver.sourceRevision !== expected.sourceRevision ||
      descriptor.acceptanceDriver.cargoFeature !== "acceptance-tests" ||
      descriptor.acceptanceDriver.cargoTarget !== "verisilo-acceptance-driver"
    ) {
      throw new Error(
        "Promotion candidate descriptor did not bind the acceptance driver contract.",
      );
    }

    const desktopPath = path.join(candidate, "verisilo.exe");
    await writeFile(desktopPath, "tampered");
    await expectRejected(
      () => verifyCandidate(candidate, receiptPath, expected, descriptorPath),
      "tampered candidate bytes",
    );
    await writeFile(desktopPath, "desktop");

    await expectRejected(
      () =>
        verifyCandidate(
          candidate,
          receiptPath,
          { ...expected, sourceRevision: "c".repeat(40) },
          descriptorPath,
        ),
      "a mismatched source revision",
    );

    const checksumPath = path.join(candidate, "SHA256SUMS");
    const originalChecksums = await readFile(checksumPath, "utf8");
    await writeFile(checksumPath, `${"d".repeat(64)}  ../escape.exe\n`);
    await expectRejected(
      () => verifyCandidate(candidate, receiptPath, expected, descriptorPath),
      "checksum path traversal",
    );
    await writeFile(checksumPath, originalChecksums);

    await expectRejected(
      () =>
        verifyCandidate(
          candidate,
          receiptPath,
          { ...expected, artifactSha256: "e".repeat(64) },
          descriptorPath,
        ),
      "a mismatched artifact digest",
    );
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  process.stdout.write(
    "Windows promotion candidate self-test passed (digest, revision, hash, and traversal mismatches were rejected).\n",
  );
}

export { parseChecksumManifest, verifyCandidate };

if (process.argv.includes("--self-test")) {
  await selfTest();
} else {
  if (!process.argv.includes("--check")) {
    throw new Error(
      "Promotion candidate verifier is read-only; pass --check or --self-test.",
    );
  }
  const directoryValue = argument("--dir");
  const receiptValue = argument("--receipt");
  const descriptorValue = argument("--descriptor");
  const repository = argument("--repository");
  const artifactIdValue = argument("--artifact-id");
  const artifactSha256 = argument("--artifact-sha256");
  const sourceRevision = argument("--source-revision");
  if (
    directoryValue === undefined ||
    receiptValue === undefined ||
    descriptorValue === undefined ||
    repository === undefined ||
    artifactIdValue === undefined ||
    artifactSha256 === undefined ||
    sourceRevision === undefined
  ) {
    throw new Error(
      "Usage: node scripts/verify-windows-promotion-candidate.mjs --check --dir <candidate> --receipt <json> --descriptor <json> --repository owner/repo --artifact-id N --artifact-sha256 hex --source-revision commit | --self-test",
    );
  }
  const artifactId = Number(artifactIdValue);
  if (
    !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u.test(repository) ||
    !/^[1-9][0-9]{0,15}$/u.test(artifactIdValue) ||
    !Number.isSafeInteger(artifactId) ||
    !sha256Pattern.test(artifactSha256) ||
    /^0{64}$/u.test(artifactSha256) ||
    !revisionPattern.test(sourceRevision)
  ) {
    throw new Error("Promotion gate inputs are invalid.");
  }
  const descriptor = await verifyCandidate(
    path.resolve(directoryValue),
    path.resolve(receiptValue),
    { repository, artifactId, artifactSha256, sourceRevision },
    path.resolve(descriptorValue),
  );
  process.stdout.write(
    `Verified exact Windows candidate Actions artifact ${descriptor.artifactId} at source ${descriptor.sourceRevision}.\n`,
  );
}
