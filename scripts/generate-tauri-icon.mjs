import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconPath = resolve(root, "apps/desktop/src-tauri/icons/icon.ico");
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

const pixelOffset = bitmapOffset + 40;
for (let row = 0; row < size; row += 1) {
  const y = size - 1 - row;
  for (let x = 0; x < size; x += 1) {
    const index = pixelOffset + (row * size + x) * 4;
    const isV =
      (x >= 7 && x <= 11 && y >= 7 && y <= 19) ||
      (x >= 20 && x <= 24 && y >= 7 && y <= 19) ||
      (y >= 18 && y <= 23 && x >= 10 && x <= 21 && Math.abs(x - 16) <= y - 16);
    if (isV) {
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

await mkdir(dirname(iconPath), { recursive: true });
await writeFile(iconPath, output);
console.log(`Generated ${iconPath}`);
