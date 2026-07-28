import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconDirectory = resolve(root, "apps/desktop/src-tauri/icons");
const icoPath = resolve(iconDirectory, "icon.ico");
const pngPath = resolve(iconDirectory, "icon.png");
const size = 32;
const xorBytes = size * size * 4;
const andBytes = ((size + 31) >> 5) * 4 * size;
const imageBytes = 40 + xorBytes + andBytes;
const output = Buffer.alloc(22 + imageBytes);

// ICONDIR + one ICONDIRENTRY.
output.writeUInt16LE(0, 0);
output.writeUInt16LE(1, 2);
output.writeUInt16LE(1, 4);
output.writeUInt8(size, 6);
output.writeUInt8(size, 7);
output.writeUInt8(0, 8);
output.writeUInt8(0, 9);
output.writeUInt16LE(1, 10);
output.writeUInt16LE(32, 12);
output.writeUInt32LE(imageBytes, 14);
output.writeUInt32LE(22, 18);

const bitmapOffset = 22;
output.writeUInt32LE(40, bitmapOffset);
output.writeInt32LE(size, bitmapOffset + 4);
output.writeInt32LE(size * 2, bitmapOffset + 8);
output.writeUInt16LE(1, bitmapOffset + 12);
output.writeUInt16LE(32, bitmapOffset + 14);
output.writeUInt32LE(0, bitmapOffset + 16);
output.writeUInt32LE(xorBytes, bitmapOffset + 20);

const isMarkPixel = (x, y) =>
  (x >= 7 && x <= 11 && y >= 7 && y <= 19) ||
  (x >= 20 && x <= 24 && y >= 7 && y <= 19) ||
  (y >= 18 && y <= 23 && x >= 10 && x <= 21 && Math.abs(x - 16) <= y - 16);

const pixelOffset = bitmapOffset + 40;
for (let row = 0; row < size; row += 1) {
  const y = size - 1 - row;
  for (let x = 0; x < size; x += 1) {
    const index = pixelOffset + (row * size + x) * 4;
    if (isMarkPixel(x, y)) {
      output[index] = 255;
      output[index + 1] = 255;
      output[index + 2] = 255;
      output[index + 3] = 255;
    } else {
      output[index] = 229;
      output[index + 1] = 70;
      output[index + 2] = 79;
      output[index + 3] = 255;
    }
  }
}

const crc32 = (bytes) => {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
};

const pngChunk = (type, data) => {
  const typeBytes = Buffer.from(type, "ascii");
  const chunk = Buffer.alloc(12 + data.length);
  chunk.writeUInt32BE(data.length, 0);
  typeBytes.copy(chunk, 4);
  data.copy(chunk, 8);
  chunk.writeUInt32BE(crc32(Buffer.concat([typeBytes, data])), 8 + data.length);
  return chunk;
};

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(size, 0);
ihdr.writeUInt32BE(size, 4);
ihdr[8] = 8;
ihdr[9] = 6;

const scanlines = Buffer.alloc((size * 4 + 1) * size);
for (let row = 0; row < size; row += 1) {
  const y = size - 1 - row;
  const rowOffset = row * (size * 4 + 1);
  for (let x = 0; x < size; x += 1) {
    const index = rowOffset + 1 + x * 4;
    const channel = isMarkPixel(x, y) ? 255 : undefined;
    scanlines[index] = channel ?? 79;
    scanlines[index + 1] = channel ?? 70;
    scanlines[index + 2] = channel ?? 229;
    scanlines[index + 3] = 255;
  }
}

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  pngChunk("IHDR", ihdr),
  pngChunk("IDAT", deflateSync(scanlines, { level: 9 })),
  pngChunk("IEND", Buffer.alloc(0)),
]);

if (process.argv.includes("--check")) {
  const [actualIco, actualPng] = await Promise.all([
    readFile(icoPath),
    readFile(pngPath),
  ]);
  if (!actualIco.equals(output) || !actualPng.equals(png)) {
    throw new Error(
      "Tauri icon assets are missing or stale; run pnpm assets:generate.",
    );
  }
  console.log("Verified reproducible Tauri ICO and PNG assets.");
} else {
  await mkdir(iconDirectory, { recursive: true });
  await Promise.all([writeFile(icoPath, output), writeFile(pngPath, png)]);
  console.log(`Generated ${icoPath} and ${pngPath}`);
}
