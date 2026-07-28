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

const manifestSchema = "urn:verisilo:hyperv-image-source:1";
const manifestKeys = [
  "artifactId",
  "imageFile",
  "imageSha256",
  "redistributionAcknowledged",
  "repository",
  "schema",
  "schemaVersion",
];
const repositoryPattern = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const imageFilePattern = /^[a-z0-9][a-z0-9._-]{0,119}\.vhdx$/u;
const sha256Pattern = /^[0-9a-f]{64}$/u;

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function validateManifest(manifest, expected = {}) {
  if (
    manifest === null ||
    typeof manifest !== "object" ||
    Array.isArray(manifest) ||
    JSON.stringify(Object.keys(manifest).sort()) !==
      JSON.stringify(manifestKeys)
  ) {
    throw new Error("Hyper-V image manifest has unknown or missing fields.");
  }
  if (
    manifest.schema !== manifestSchema ||
    manifest.schemaVersion !== 1 ||
    typeof manifest.repository !== "string" ||
    !repositoryPattern.test(manifest.repository) ||
    !Number.isSafeInteger(manifest.artifactId) ||
    manifest.artifactId < 1 ||
    typeof manifest.imageFile !== "string" ||
    !imageFilePattern.test(manifest.imageFile) ||
    /^(?:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)/iu.test(
      manifest.imageFile,
    ) ||
    manifest.imageFile.includes("..") ||
    path.posix.basename(manifest.imageFile) !== manifest.imageFile ||
    path.win32.basename(manifest.imageFile) !== manifest.imageFile ||
    typeof manifest.imageSha256 !== "string" ||
    !sha256Pattern.test(manifest.imageSha256) ||
    /^0{64}$/u.test(manifest.imageSha256) ||
    manifest.redistributionAcknowledged !== true
  ) {
    throw new Error("Hyper-V image manifest violates schema version 1.");
  }
  for (const [field, expectedValue] of Object.entries(expected)) {
    if (expectedValue !== undefined && manifest[field] !== expectedValue) {
      throw new Error(
        `Hyper-V image manifest ${field} does not match the release input.`,
      );
    }
  }
  return manifest;
}

async function sha256File(filePath) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(filePath)) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

async function requireRegularFile(filePath, label) {
  const metadata = await lstat(filePath).catch(() => undefined);
  if (
    metadata === undefined ||
    !metadata.isFile() ||
    metadata.isSymbolicLink()
  ) {
    throw new Error(`${label} must be a regular, non-symlink file.`);
  }
}

async function verifyHyperVImage(manifestPath, imagesDirectory, expected = {}) {
  await requireRegularFile(manifestPath, "Hyper-V image manifest");
  const manifestText = await readFile(manifestPath, "utf8");
  const manifest = validateManifest(JSON.parse(manifestText), expected);
  if (manifestText !== stableJson(manifest)) {
    throw new Error(
      "Hyper-V image manifest must use canonical formatted JSON.",
    );
  }
  const imagesMetadata = await lstat(imagesDirectory).catch(() => undefined);
  if (
    imagesMetadata === undefined ||
    !imagesMetadata.isDirectory() ||
    imagesMetadata.isSymbolicLink()
  ) {
    throw new Error(
      "Hyper-V images root must be a regular, non-symlink directory.",
    );
  }
  const entries = await readdir(imagesDirectory, { withFileTypes: true });
  if (
    entries.length !== 1 ||
    entries[0].name !== manifest.imageFile ||
    !entries[0].isFile() ||
    entries[0].isSymbolicLink()
  ) {
    throw new Error(
      "Hyper-V images root must contain exactly the declared VHDX leaf file.",
    );
  }
  const imagePath = path.join(imagesDirectory, manifest.imageFile);
  await requireRegularFile(imagePath, "Hyper-V VHDX");
  const actualSha256 = await sha256File(imagePath);
  if (actualSha256 !== manifest.imageSha256) {
    throw new Error("Hyper-V VHDX SHA-256 does not match the strict manifest.");
  }
  return { manifest, imagePath, actualSha256 };
}

async function expectRejected(action, label) {
  let rejected = false;
  try {
    await action();
  } catch {
    rejected = true;
  }
  if (!rejected) {
    throw new Error(`Hyper-V image verifier self-test accepted ${label}.`);
  }
}

async function selfTest() {
  const temporaryRoot = await mkdtemp(
    path.join(tmpdir(), "verisilo-hyperv-image-"),
  );
  const manifestPath = path.join(temporaryRoot, "hyperv-image-manifest.json");
  const imagesDirectory = path.join(temporaryRoot, "images");
  const imageFile = "licensed-windows-base.vhdx";
  const imagePath = path.join(imagesDirectory, imageFile);
  const bytes = Buffer.from("not a real disk image\n", "utf8");
  const imageSha256 = createHash("sha256").update(bytes).digest("hex");
  const manifest = {
    schema: manifestSchema,
    schemaVersion: 1,
    repository: "QianQIUlp/VeriSilo",
    artifactId: 123,
    imageFile,
    imageSha256,
    redistributionAcknowledged: true,
  };
  try {
    await mkdir(imagesDirectory);
    await writeFile(imagePath, bytes);
    await writeFile(manifestPath, stableJson(manifest), "utf8");
    await verifyHyperVImage(manifestPath, imagesDirectory, {
      repository: manifest.repository,
      artifactId: manifest.artifactId,
      imageFile,
      imageSha256,
      redistributionAcknowledged: true,
    });

    await writeFile(imagePath, Buffer.from("tampered\n", "utf8"));
    await expectRejected(
      () => verifyHyperVImage(manifestPath, imagesDirectory),
      "tampered bytes",
    );
    await writeFile(imagePath, bytes);

    await writeFile(
      manifestPath,
      stableJson({ ...manifest, imageSha256: "f".repeat(64) }),
      "utf8",
    );
    await expectRejected(
      () => verifyHyperVImage(manifestPath, imagesDirectory),
      "a wrong manifest hash",
    );

    await writeFile(
      manifestPath,
      stableJson({ ...manifest, imageFile: "../escape.vhdx" }),
      "utf8",
    );
    await expectRejected(
      () => verifyHyperVImage(manifestPath, imagesDirectory),
      "path traversal",
    );

    await writeFile(manifestPath, stableJson(manifest), "utf8");
    await writeFile(path.join(imagesDirectory, "extra.vhdx"), bytes);
    await expectRejected(
      () => verifyHyperVImage(manifestPath, imagesDirectory),
      "multiple image files",
    );
    await rm(path.join(imagesDirectory, "extra.vhdx"));
    await rm(imagePath);
    await expectRejected(
      () => verifyHyperVImage(manifestPath, imagesDirectory),
      "a missing image",
    );
  } finally {
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  process.stdout.write(
    "Hyper-V image manifest self-test passed (missing, tampered, wrong-hash, traversal, and multi-file fixtures were rejected).\n",
  );
}

export { validateManifest, verifyHyperVImage };

if (process.argv.includes("--self-test")) {
  await selfTest();
} else {
  if (!process.argv.includes("--check")) {
    throw new Error(
      "Hyper-V image verifier is read-only; pass --check or --self-test.",
    );
  }
  const manifestValue = argument("--manifest");
  const imagesValue = argument("--images");
  if (manifestValue === undefined || imagesValue === undefined) {
    throw new Error(
      "Usage: node scripts/verify-hyperv-image.mjs --check --manifest <manifest.json> --images <directory> [--repository owner/repo --artifact-id N --image-file leaf.vhdx --sha256 hex] | --self-test",
    );
  }
  const artifactIdValue = argument("--artifact-id");
  const artifactId =
    artifactIdValue === undefined ? undefined : Number(artifactIdValue);
  const expected = {
    repository: argument("--repository"),
    artifactId,
    imageFile: argument("--image-file"),
    imageSha256: argument("--sha256"),
    redistributionAcknowledged: process.argv.includes(
      "--require-redistribution-ack",
    )
      ? true
      : undefined,
  };
  if (
    artifactIdValue !== undefined &&
    (!/^[1-9][0-9]{0,15}$/u.test(artifactIdValue) ||
      !Number.isSafeInteger(artifactId))
  ) {
    throw new Error("Expected Actions artifact ID is invalid.");
  }
  const result = await verifyHyperVImage(
    path.resolve(manifestValue),
    path.resolve(imagesValue),
    expected,
  );
  process.stdout.write(
    `Verified Hyper-V image ${result.manifest.imageFile} from same-repository Actions artifact ${result.manifest.artifactId}.\n`,
  );
}
