import { createHash } from "node:crypto";
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

const manifestSchema = "urn:verisilo:deterministic-extension-zip:1";
const utf8Flag = 0x0800;
const crcTable = new Uint32Array(256);
for (let index = 0; index < 256; index += 1) {
  let value = index;
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value & 1) === 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  crcTable[index] = value >>> 0;
}

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function sha256(content) {
  return createHash("sha256").update(content).digest("hex");
}

function crc32(content) {
  let value = 0xffffffff;
  for (const byte of content) {
    value = crcTable[(value ^ byte) & 0xff] ^ (value >>> 8);
  }
  return (value ^ 0xffffffff) >>> 0;
}

function sourceDateEpoch() {
  const value = process.env.SOURCE_DATE_EPOCH;
  if (value === undefined || !/^[0-9]{1,12}$/u.test(value)) {
    throw new Error(
      "SOURCE_DATE_EPOCH is required for deterministic extension ZIP packaging.",
    );
  }
  const epoch = Number(value);
  const date = new Date(epoch * 1000);
  if (
    !Number.isSafeInteger(epoch) ||
    epoch < 315532800 ||
    date.getUTCFullYear() > 2107
  ) {
    throw new Error(
      "SOURCE_DATE_EPOCH must fit the ZIP DOS timestamp range (1980-2107). ",
    );
  }
  return { epoch, date };
}

function dosTimestamp(date) {
  return {
    time:
      (date.getUTCHours() << 11) |
      (date.getUTCMinutes() << 5) |
      Math.floor(date.getUTCSeconds() / 2),
    date:
      ((date.getUTCFullYear() - 1980) << 9) |
      ((date.getUTCMonth() + 1) << 5) |
      date.getUTCDate(),
  };
}

function assertSafePath(value) {
  if (
    value.length === 0 ||
    value.length > 512 ||
    value.includes("\\") ||
    value.includes(":") ||
    /[\0\r\n]/u.test(value) ||
    value.startsWith("/") ||
    value.endsWith("/") ||
    value
      .split("/")
      .some(
        (segment) => segment === "" || segment === "." || segment === "..",
      ) ||
    path.posix.normalize(value) !== value
  ) {
    throw new Error(`Extension ZIP input path is unsafe: ${value}`);
  }
}

async function collectFiles(directory, relativeDirectory = "") {
  const entries = await readdir(path.join(directory, relativeDirectory), {
    withFileTypes: true,
  });
  const files = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    assertSafePath(relativePath);
    const absolutePath = path.join(directory, ...relativePath.split("/"));
    const metadata = await lstat(absolutePath);
    if (metadata.isSymbolicLink()) {
      throw new Error(
        `Extension ZIP input contains a symlink: ${relativePath}`,
      );
    }
    if (metadata.isDirectory()) {
      files.push(...(await collectFiles(directory, relativePath)));
    } else if (metadata.isFile()) {
      const content = await readFile(absolutePath);
      if (content.length > 0xffffffff) {
        throw new Error(
          `Extension ZIP input exceeds ZIP32 size: ${relativePath}`,
        );
      }
      files.push({ path: relativePath, content });
    } else {
      throw new Error(
        `Extension ZIP input is not a regular file: ${relativePath}`,
      );
    }
  }
  files.sort((left, right) =>
    Buffer.compare(
      Buffer.from(left.path, "utf8"),
      Buffer.from(right.path, "utf8"),
    ),
  );
  if (files.length === 0 || files.length > 65535) {
    throw new Error("Extension ZIP requires 1-65535 regular files.");
  }
  return files;
}

function localHeader(name, content, checksum, timestamp) {
  const header = Buffer.alloc(30);
  header.writeUInt32LE(0x04034b50, 0);
  header.writeUInt16LE(20, 4);
  header.writeUInt16LE(utf8Flag, 6);
  header.writeUInt16LE(0, 8);
  header.writeUInt16LE(timestamp.time, 10);
  header.writeUInt16LE(timestamp.date, 12);
  header.writeUInt32LE(checksum, 14);
  header.writeUInt32LE(content.length, 18);
  header.writeUInt32LE(content.length, 22);
  header.writeUInt16LE(name.length, 26);
  header.writeUInt16LE(0, 28);
  return header;
}

function centralHeader(name, content, checksum, timestamp, offset) {
  const header = Buffer.alloc(46);
  header.writeUInt32LE(0x02014b50, 0);
  header.writeUInt16LE(0x0314, 4);
  header.writeUInt16LE(20, 6);
  header.writeUInt16LE(utf8Flag, 8);
  header.writeUInt16LE(0, 10);
  header.writeUInt16LE(timestamp.time, 12);
  header.writeUInt16LE(timestamp.date, 14);
  header.writeUInt32LE(checksum, 16);
  header.writeUInt32LE(content.length, 20);
  header.writeUInt32LE(content.length, 24);
  header.writeUInt16LE(name.length, 28);
  header.writeUInt16LE(0, 30);
  header.writeUInt16LE(0, 32);
  header.writeUInt16LE(0, 34);
  header.writeUInt16LE(0, 36);
  header.writeUInt32LE(0x81a40000, 38);
  header.writeUInt32LE(offset, 42);
  return header;
}

function endOfCentralDirectory(entries, centralBytes, centralOffset) {
  const record = Buffer.alloc(22);
  record.writeUInt32LE(0x06054b50, 0);
  record.writeUInt16LE(0, 4);
  record.writeUInt16LE(0, 6);
  record.writeUInt16LE(entries, 8);
  record.writeUInt16LE(entries, 10);
  record.writeUInt32LE(centralBytes, 12);
  record.writeUInt32LE(centralOffset, 16);
  record.writeUInt16LE(0, 20);
  return record;
}

function buildZip(files, date) {
  const timestamp = dosTimestamp(date);
  const localParts = [];
  const centralParts = [];
  let offset = 0;
  for (const file of files) {
    const name = Buffer.from(file.path, "utf8");
    if (name.length > 65535) {
      throw new Error(`Extension ZIP path is too long: ${file.path}`);
    }
    const checksum = crc32(file.content);
    const local = localHeader(name, file.content, checksum, timestamp);
    localParts.push(local, name, file.content);
    centralParts.push(
      centralHeader(name, file.content, checksum, timestamp, offset),
      name,
    );
    offset += local.length + name.length + file.content.length;
    if (offset > 0xffffffff) {
      throw new Error("Extension ZIP exceeds the ZIP32 archive limit.");
    }
  }
  const central = Buffer.concat(centralParts);
  const local = Buffer.concat(localParts);
  return Buffer.concat([
    local,
    central,
    endOfCentralDirectory(files.length, central.length, local.length),
  ]);
}

function expectedManifest(files, archive, archiveName, epoch) {
  return {
    schema: manifestSchema,
    schemaVersion: 1,
    sourceDateEpoch: epoch,
    archive: {
      file: archiveName,
      format: "zip32-store",
      sha256: sha256(archive),
      bytes: archive.length,
    },
    files: files.map((file) => ({
      path: file.path,
      sha256: sha256(file.content),
      bytes: file.content.length,
    })),
  };
}

async function packageExtension(input, output, manifestPath, check) {
  const outputName = path.basename(output);
  if (
    outputName !== path.win32.basename(outputName) ||
    !/^VeriSilo-Companion-[0-9]+\.[0-9]+\.[0-9]+-chrome-edge\.zip$/u.test(
      outputName,
    )
  ) {
    throw new Error(
      "Extension ZIP output must use the strict release leaf filename.",
    );
  }
  const { epoch, date } = sourceDateEpoch();
  const files = await collectFiles(input);
  const archive = buildZip(files, date);
  const manifest = expectedManifest(files, archive, outputName, epoch);
  if (check) {
    const [actualArchive, actualManifestText] = await Promise.all([
      readFile(output),
      readFile(manifestPath, "utf8"),
    ]);
    if (
      !actualArchive.equals(archive) ||
      actualManifestText !== stableJson(manifest)
    ) {
      throw new Error(
        "Extension ZIP bytes or content/hash manifest are stale or invalid.",
      );
    }
  } else {
    await Promise.all([
      mkdir(path.dirname(output), { recursive: true }),
      mkdir(path.dirname(manifestPath), { recursive: true }),
    ]);
    await Promise.all([
      writeFile(output, archive),
      writeFile(manifestPath, stableJson(manifest), "utf8"),
    ]);
  }
  return manifest;
}

async function selfTest() {
  const temporaryRoot = await mkdtemp(
    path.join(tmpdir(), "verisilo-extension-zip-"),
  );
  const input = path.join(temporaryRoot, "input");
  const output = path.join(
    temporaryRoot,
    "VeriSilo-Companion-0.1.0-chrome-edge.zip",
  );
  const manifest = path.join(temporaryRoot, "extension-zip-manifest.json");
  const previousEpoch = process.env.SOURCE_DATE_EPOCH;
  try {
    process.env.SOURCE_DATE_EPOCH = "1767225600";
    await mkdir(path.join(input, "nested"), { recursive: true });
    await writeFile(path.join(input, "z.txt"), "z\n");
    await writeFile(path.join(input, "nested", "a.txt"), "a\n");
    await packageExtension(input, output, manifest, false);
    await packageExtension(input, output, manifest, true);
    await writeFile(path.join(input, "z.txt"), "tampered\n");
    let rejected = false;
    try {
      await packageExtension(input, output, manifest, true);
    } catch {
      rejected = true;
    }
    if (!rejected) {
      throw new Error("Extension ZIP self-test accepted stale source bytes.");
    }
  } finally {
    if (previousEpoch === undefined) {
      delete process.env.SOURCE_DATE_EPOCH;
    } else {
      process.env.SOURCE_DATE_EPOCH = previousEpoch;
    }
    await rm(temporaryRoot, { recursive: true, force: true });
  }
  process.stdout.write(
    "Deterministic extension ZIP self-test passed (sorted stored entries, fixed timestamp, content manifest, and archive hash).\n",
  );
}

export { buildZip, packageExtension };

if (process.argv.includes("--self-test")) {
  await selfTest();
} else {
  const inputValue = argument("--input");
  const outputValue = argument("--out");
  const manifestValue = argument("--manifest");
  if (
    inputValue === undefined ||
    outputValue === undefined ||
    manifestValue === undefined
  ) {
    throw new Error(
      "Usage: node scripts/package-extension-zip.mjs --input <dist> --out <release.zip> --manifest <manifest.json> [--check] | --self-test",
    );
  }
  const result = await packageExtension(
    path.resolve(inputValue),
    path.resolve(outputValue),
    path.resolve(manifestValue),
    process.argv.includes("--check"),
  );
  process.stdout.write(
    `${process.argv.includes("--check") ? "Verified" : "Wrote"} deterministic extension ZIP ${result.archive.file} (${result.archive.sha256}).\n`,
  );
}
