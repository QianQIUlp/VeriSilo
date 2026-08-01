import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const websiteFaviconPath = resolve(root, "apps/site/public/favicon.svg");
const extensionIconDirectory = resolve(root, "apps/extension/icons");
const desktopPublicDirectory = resolve(root, "apps/desktop/public");
const desktopMarkPath = resolve(desktopPublicDirectory, "verisilo-mark.svg");
const tauriIconDirectory = resolve(root, "apps/desktop/src-tauri/icons");
const icoPath = resolve(tauriIconDirectory, "icon.ico");
const pngPath = resolve(tauriIconDirectory, "icon.png");
const iconSizes = [16, 32, 48, 128];

const websiteFavicon = await readFile(websiteFaviconPath);
const extensionIcons = await Promise.all(
  iconSizes.map(async (size) => ({
    bytes: await readFile(
      resolve(extensionIconDirectory, `verisilo-${size}.png`),
    ),
    size,
  })),
);

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

const ico = buildIco(extensionIcons);
const png = extensionIcons.find(({ size }) => size === 128)?.bytes;

if (png === undefined) {
  throw new Error("The 128px extension icon is required for the Tauri PNG.");
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
    !actualDesktopMark.equals(websiteFavicon)
  ) {
    throw new Error(
      "Desktop icon assets are missing or stale; run pnpm assets:generate.",
    );
  }

  console.log(
    "Verified desktop SVG, PNG, and multi-size ICO assets match the website favicon.",
  );
} else {
  await Promise.all([
    mkdir(tauriIconDirectory, { recursive: true }),
    mkdir(desktopPublicDirectory, { recursive: true }),
  ]);
  await Promise.all([
    writeFile(icoPath, ico),
    writeFile(pngPath, png),
    writeFile(desktopMarkPath, websiteFavicon),
  ]);
  console.log(
    "Generated desktop SVG, PNG, and multi-size ICO assets from apps/site/public/favicon.svg.",
  );
}
