import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const extensionIdPattern = /^[a-p]{32}$/u;
const knownPlaceholderIds = new Set([
  "abcdefghijklmnopabcdefghijklmnop",
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
]);

function validateExtensionId(name, value) {
  if (
    value === undefined ||
    !extensionIdPattern.test(value) ||
    knownPlaceholderIds.has(value) ||
    new Set(value).size < 4
  ) {
    throw new Error(
      `${name} must be an explicit 32-character release extension ID (letters a-p), not an example or placeholder.`,
    );
  }
}

function expectedConfig() {
  const chromeExtensionId = process.env.VERISILO_CHROME_EXTENSION_ID;
  const edgeExtensionId = process.env.VERISILO_EDGE_EXTENSION_ID;
  validateExtensionId("VERISILO_CHROME_EXTENSION_ID", chromeExtensionId);
  validateExtensionId("VERISILO_EDGE_EXTENSION_ID", edgeExtensionId);
  return {
    schemaVersion: 1,
    chromeExtensionId,
    edgeExtensionId,
  };
}

function selfTest() {
  validateExtensionId("fixture", "ponmlkjihgfedcbaponmlkjihgfedcba");
  let rejected = false;
  try {
    validateExtensionId("fixture", "abcdefghijklmnopabcdefghijklmnop");
  } catch {
    rejected = true;
  }
  if (!rejected) {
    throw new Error(
      "Native Host release config self-test accepted a placeholder ID.",
    );
  }
  process.stdout.write("Native Host release config self-test passed.\n");
}

if (process.argv.includes("--self-test")) {
  selfTest();
  process.exit(0);
}
const outputArgumentIndex = process.argv.indexOf("--out");
if (
  outputArgumentIndex === -1 ||
  process.argv[outputArgumentIndex + 1] === undefined
) {
  throw new Error(
    "Usage: node scripts/prepare-native-host-release.mjs --out <directory> [--check] | --self-test",
  );
}

const outputDirectory = path.resolve(process.argv[outputArgumentIndex + 1]);
const outputPath = path.join(
  outputDirectory,
  "native-host-release-config.json",
);
const serialized = `${JSON.stringify(expectedConfig(), null, 2)}\n`;

if (process.argv.includes("--check")) {
  const actual = await readFile(outputPath, "utf8").catch(() => undefined);
  if (actual !== serialized) {
    throw new Error(
      `${outputPath} is missing or does not match the release IDs.`,
    );
  }
  process.stdout.write(`Verified Native Host release config: ${outputPath}\n`);
} else {
  await mkdir(outputDirectory, { recursive: true });
  await writeFile(outputPath, serialized, "utf8");
  process.stdout.write(
    `Prepared Native Host release configuration: ${outputPath}\n` +
      "Build verisilo-native-host in the same environment so these IDs are compiled into the Host.\n",
  );
}
