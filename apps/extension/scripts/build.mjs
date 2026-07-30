import { build, context } from "esbuild";
import { cp, mkdir, rm } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const outdir = resolve(root, "dist");
const options = {
  absWorkingDir: root,
  bundle: true,
  entryPoints: {
    background: "src/background.ts",
    content: "src/content.ts",
    "labs-bridge": "src/labs-bridge.ts",
    "main-world": "src/main-world.ts",
    sidepanel: "src/sidepanel.ts",
  },
  format: "esm",
  outdir,
  platform: "browser",
  sourcemap: true,
  target: ["chrome120", "edge120"],
  legalComments: "linked",
};

await rm(outdir, { recursive: true, force: true });
await mkdir(outdir, { recursive: true });
await cp(resolve(root, "manifest.json"), resolve(outdir, "manifest.json"));
await cp(resolve(root, "sidepanel.html"), resolve(outdir, "sidepanel.html"));
await cp(resolve(root, "icons"), resolve(outdir, "icons"), {
  recursive: true,
});

if (process.argv.includes("--watch")) {
  const buildContext = await context(options);
  await buildContext.watch();
  console.log("Watching VeriSilo extension sources…");
} else {
  await build(options);
}
