import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// Canonical VeriSilo brand symbol (the kinetic asymmetric V used by the
// desktop UI); the Windows icon family and the desktop mark are derived from
// it via the extension-rendered PNG frames.
const brandSymbolPath = resolve(
  root,
  "apps/desktop/src/shared/verisilo-symbol.svg",
);
const extensionIconDirectory = resolve(root, "apps/extension/icons");
const desktopPublicDirectory = resolve(root, "apps/desktop/public");
const desktopMarkPath = resolve(desktopPublicDirectory, "verisilo-mark.svg");
const tauriIconDirectory = resolve(root, "apps/desktop/src-tauri/icons");
const icoPath = resolve(tauriIconDirectory, "icon.ico");
const pngPath = resolve(tauriIconDirectory, "icon.png");
// Tauri's Windows default-window path decodes only the first ICO entry into a
// single runtime image. Keep the complete Windows ladder, but put the largest
// canonical frame first so title bars and trays downsample instead of
// upscaling the 16px frame; Explorer/taskbar still select exact group frames.
const iconSizes = [256, 16, 20, 24, 32, 40, 48, 64, 96, 128];

const PNG_SIGNATURE = Buffer.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
]);

function readPngHeader(bytes, label) {
  if (bytes.length < 26 || !bytes.subarray(0, 8).equals(PNG_SIGNATURE)) {
    throw new Error(`${label} is not a PNG with an IHDR header.`);
  }
  return {
    width: bytes.readUInt32BE(16),
    height: bytes.readUInt32BE(20),
    bitDepth: bytes[24],
    colorType: bytes[25],
  };
}

const brandSymbol = await readFile(brandSymbolPath);
const extensionIcons = await Promise.all(
  iconSizes.map(async (size) => ({
    bytes: await readFile(
      resolve(extensionIconDirectory, `verisilo-${size}.png`),
    ),
    size,
  })),
);

for (const { bytes, size } of extensionIcons) {
  const header = readPngHeader(bytes, `verisilo-${size}.png`);
  if (
    header.width !== size ||
    header.height !== size ||
    header.bitDepth !== 8 ||
    header.colorType !== 6
  ) {
    throw new Error(
      `verisilo-${size}.png must be an ${size}x${size} 8-bit RGBA PNG.`,
    );
  }
}

function buildIco(images) {
  const headerSize = 6;
  const entrySize = 16;
  const dataOffset = headerSize + images.length * entrySize;
  const directory = Buffer.alloc(dataOffset);

  directory.writeUInt16LE(0, 0);
  directory.writeUInt16LE(1, 2);
  directory.writeUInt16LE(images.length, 4);

  let imageOffset = dataOffset;
  images.forEach(({ bytes, size }, index) => {
    const entryOffset = headerSize + index * entrySize;
    directory.writeUInt8(size === 256 ? 0 : size, entryOffset);
    directory.writeUInt8(size === 256 ? 0 : size, entryOffset + 1);
    directory.writeUInt8(0, entryOffset + 2);
    directory.writeUInt8(0, entryOffset + 3);
    directory.writeUInt16LE(1, entryOffset + 4);
    directory.writeUInt16LE(32, entryOffset + 6);
    directory.writeUInt32LE(bytes.length, entryOffset + 8);
    directory.writeUInt32LE(imageOffset, entryOffset + 12);
    imageOffset += bytes.length;
  });

  return Buffer.concat([directory, ...images.map(({ bytes }) => bytes)]);
}

function inspectIco(bytes) {
  if (
    bytes.length < 6 ||
    bytes.readUInt16LE(0) !== 0 ||
    bytes.readUInt16LE(2) !== 1
  ) {
    throw new Error("Desktop icon ICO header is invalid.");
  }
  const count = bytes.readUInt16LE(4);
  if (count !== iconSizes.length) {
    throw new Error(
      `Desktop icon ICO must contain ${iconSizes.length} frames; found ${count}.`,
    );
  }
  const frames = iconSizes.map((size, index) => {
    const offset = 6 + index * 16;
    const width = bytes[offset] || 256;
    const height = bytes[offset + 1] || 256;
    const bytesInRes = bytes.readUInt32LE(offset + 8);
    const imageOffset = bytes.readUInt32LE(offset + 12);
    const payload = bytes.subarray(imageOffset, imageOffset + bytesInRes);
    const expected = extensionIcons.find((icon) => icon.size === size).bytes;
    if (
      width !== size ||
      height !== size ||
      bytes.readUInt16LE(offset + 4) !== 1 ||
      bytes.readUInt16LE(offset + 6) !== 32 ||
      !payload.equals(expected)
    ) {
      throw new Error(`Desktop icon ICO frame ${size}px is invalid or stale.`);
    }
    return { width, height };
  });
  return frames;
}

const ico = buildIco(extensionIcons);
const png = extensionIcons.find(({ size }) => size === 256)?.bytes;

if (png === undefined) {
  throw new Error("The 256px extension icon is required for the Tauri PNG.");
}

if (process.argv.includes("--check")) {
  const [actualIco, actualPng, actualDesktopMark] = await Promise.all([
    readFile(icoPath).catch(() => null),
    readFile(pngPath).catch(() => null),
    readFile(desktopMarkPath).catch(() => null),
  ]);

  if (
    actualIco === null ||
    actualPng === null ||
    actualDesktopMark === null ||
    !actualIco.equals(ico) ||
    !actualPng.equals(png) ||
    !actualDesktopMark.equals(brandSymbol)
  ) {
    throw new Error(
      "Desktop icon assets are missing or stale; run pnpm assets:generate.",
    );
  }

  inspectIco(actualIco);

  console.log(
    "Verified desktop SVG, PNG, and multi-size ICO assets match the desktop brand symbol.",
  );
} else {
  await Promise.all([
    mkdir(tauriIconDirectory, { recursive: true }),
    mkdir(desktopPublicDirectory, { recursive: true }),
  ]);
  await Promise.all([
    writeFile(icoPath, ico),
    writeFile(pngPath, png),
    writeFile(desktopMarkPath, brandSymbol),
  ]);
  console.log(
    "Generated desktop SVG, PNG, and multi-size ICO assets from apps/desktop/src/shared/verisilo-symbol.svg.",
  );
}
