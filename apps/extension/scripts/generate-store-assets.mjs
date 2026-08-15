import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import sharp from "sharp";

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const faviconPath = resolve(extensionRoot, "../site/public/favicon.svg");
const outputDirectory = resolve(extensionRoot, "../../assets/store");
const favicon = await readFile(faviconPath);

const outputs = [
  {
    name: "verisilo-store-logo-300.png",
    render: () =>
      sharp(favicon, { density: 288 })
        .resize(300, 300, { fit: "fill", kernel: "lanczos3" })
        .png({ compressionLevel: 9, effort: 10 })
        .toBuffer(),
  },
  {
    name: "verisilo-store-tile-440x280.png",
    render: () =>
      sharp(
        Buffer.from(
          [
            '<svg xmlns="http://www.w3.org/2000/svg" width="440" height="280" viewBox="0 0 440 280">',
            '<rect width="440" height="280" fill="#171c35"/>',
            '<rect x="20" y="20" width="400" height="240" rx="16" fill="#f4f6f2"/>',
            '<circle cx="92" cy="140" r="44" fill="#4f46e5"/>',
            '<path d="M70 128h44v24H70z" fill="none" stroke="#cbd0ff" stroke-width="1.5"/>',
            '<path d="M74 134l18 12 18-12" fill="none" stroke="#fff" stroke-width="4" stroke-linecap="square" stroke-linejoin="miter"/>',
            '<text x="156" y="132" font-family="Bahnschrift, Segoe UI, Arial, sans-serif" font-size="40" font-weight="700" fill="#14182b">VeriSilo</text>',
            '<text x="156" y="170" font-family="Segoe UI, Arial, sans-serif" font-size="20" fill="#626c71">Companion — browser exposure inspector</text>',
            "</svg>",
          ].join(""),
        ),
      )
        .png({ compressionLevel: 9, effort: 10 })
        .toBuffer(),
  },
];

if (process.argv.includes("--check")) {
  for (const output of outputs) {
    const expected = await output.render();
    const actual = await readFile(resolve(outputDirectory, output.name)).catch(
      () => null,
    );
    if (actual === null || !actual.equals(expected)) {
      throw new Error(
        `Store asset ${output.name} is missing or stale; run pnpm --filter @verisilo/extension store-assets:generate.`,
      );
    }
  }
  console.log("Verified store assets match the website favicon and brand.");
} else {
  await mkdir(outputDirectory, { recursive: true });
  for (const output of outputs) {
    await writeFile(
      resolve(outputDirectory, output.name),
      await output.render(),
    );
  }
  console.log(`Generated store assets in ${outputDirectory}.`);
}
