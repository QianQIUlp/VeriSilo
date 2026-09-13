import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import sharp from "sharp";

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// Canonical VeriSilo brand symbol: the kinetic asymmetric V used by the
// desktop UI. The extension icons and the Windows desktop icon family are
// rasterized from this single source so the variants cannot drift apart.
const sourcePath = resolve(
  extensionRoot,
  "../desktop/src/shared/verisilo-symbol.svg",
);
const outputDirectory = resolve(extensionRoot, "icons");
// The manifest references the browser-specific sizes it needs, while the full
// Windows system family (title bar, taskbar, Explorer, installer) ships so the
// desktop generator can pack every size Windows asks for without rescaling.
const sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 256];
const source = await readFile(sourcePath);

// The symbol art keeps generous margins in its 96-unit canvas for page
// layouts; icon frames need the glyph to fill the canvas instead. Measure the
// drawn alpha bounds once at high resolution and derive a square viewport
// around them with a small optical margin, so every rendered size shares the
// same tightly framed composition without hand-tuned per-size geometry.
//
// At 16px the downscaled strokes turn soft and the glyph would graze the
// canvas edge, so that size alone gets a same-color stroke expansion (a
// stronger outline, not a redesign); the viewport accounts for the expansion.
const OPTICAL_MARGIN = 0.08;
const SMALL_ICON_STROKE_UNITS = 4;
const SMALL_ICON_MAX_SIZE = 16;

async function measureGlyphBounds(svg) {
  const canvasUnits = 96;
  const probePixels = 768;
  const density = (probePixels / canvasUnits) * 72;
  const { data, info } = await sharp(svg, { density })
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });
  if (info.width !== probePixels || info.height !== probePixels) {
    throw new Error(
      `Unexpected symbol probe size ${info.width}x${info.height}; expected ${probePixels}x${probePixels}.`,
    );
  }

  let minX = probePixels;
  let minY = probePixels;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < info.height; y += 1) {
    for (let x = 0; x < info.width; x += 1) {
      if (data[(y * info.width + x) * info.channels + 3] >= 8) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  if (maxX < 0) {
    throw new Error("The brand symbol rendered no visible pixels.");
  }

  const unitsPerPixel = canvasUnits / probePixels;
  return {
    x: minX * unitsPerPixel,
    y: minY * unitsPerPixel,
    width: (maxX - minX + 1) * unitsPerPixel,
    height: (maxY - minY + 1) * unitsPerPixel,
  };
}

const round3 = (value) => Math.round(value * 1000) / 1000;

function frameSymbol(svg, bounds, strokeUnits) {
  let framed = svg;
  if (strokeUnits > 0) {
    framed = svg.replace(/<path\b([^>]*?)(\/?)>/g, (tag, attrs, selfClose) => {
      const fill = attrs.match(/fill="([^"]*)"/);
      if (!fill) {
        throw new Error("A brand symbol path is missing a fill to stroke.");
      }
      const stroke = ` stroke="${fill[1]}" stroke-width="${strokeUnits}" stroke-linejoin="round"`;
      return `<path${attrs}${stroke}${selfClose}>`;
    });
  }
  const side =
    (Math.max(bounds.width, bounds.height) + strokeUnits) *
    (1 + OPTICAL_MARGIN * 2);
  const viewBox = `viewBox="${round3(bounds.x + bounds.width / 2 - side / 2)} ${round3(
    bounds.y + bounds.height / 2 - side / 2,
  )} ${round3(side)} ${round3(side)}"`;
  const result = framed.replace(/viewBox="[^"]*"/, viewBox);
  if (result === framed) {
    throw new Error("The brand symbol does not declare a viewBox to reframe.");
  }
  return result;
}

const bounds = await measureGlyphBounds(source);
const framedByStroke = new Map(
  [0, SMALL_ICON_STROKE_UNITS].map((strokeUnits) => [
    strokeUnits,
    frameSymbol(source.toString("utf8"), bounds, strokeUnits),
  ]),
);

async function render(size) {
  const strokeUnits = size <= SMALL_ICON_MAX_SIZE ? SMALL_ICON_STROKE_UNITS : 0;
  return sharp(Buffer.from(framedByStroke.get(strokeUnits)), { density: 576 })
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
  console.log("Verified extension icons match the desktop brand symbol.");
} else {
  await mkdir(outputDirectory, { recursive: true });
  await Promise.all(expected.map((icon) => writeFile(icon.path, icon.bytes)));
  console.log(
    "Generated extension icons from apps/desktop/src/shared/verisilo-symbol.svg.",
  );
}
