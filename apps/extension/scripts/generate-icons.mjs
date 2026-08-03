import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import sharp from "sharp";

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = resolve(extensionRoot, "../site/public/favicon.svg");
const outputDirectory = resolve(extensionRoot, "icons");
// Keep a 256px source available for Windows' high-DPI executable and taskbar
// icon selection. The extension manifest can continue to reference its
// browser-specific sizes while the desktop generator consumes this frame.
const sizes = [16, 32, 48, 128, 256];
const source = await readFile(sourcePath);

async function render(size) {
  return sharp(source, { density: 288 })
    .resize(size, size, { fit: "fill", kernel: "lanczos3" })
    .png({
      adaptiveFiltering: false,
      compressionLevel: 9,
      effort: 10,
      palette: false,
    })
    .toBuffer();
}

const expected = await Promise.all(
  sizes.map(async (size) => ({
    path: resolve(outputDirectory, `verisilo-${size}.png`),
    bytes: await render(size),
    size,
  })),
);

if (process.argv.includes("--check")) {
  for (const icon of expected) {
    const actual = await readFile(icon.path).catch(() => null);
    if (actual === null || !actual.equals(icon.bytes)) {
      throw new Error(
        `Extension icon ${icon.size}px is missing or stale; run pnpm --filter @verisilo/extension icons:generate.`,
      );
    }
  }
  console.log("Verified extension icons match the website favicon.");
} else {
  await mkdir(outputDirectory, { recursive: true });
  await Promise.all(expected.map((icon) => writeFile(icon.path, icon.bytes)));
  console.log("Generated extension icons from apps/site/public/favicon.svg.");
}
