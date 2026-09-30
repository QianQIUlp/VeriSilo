import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const desktop = resolve(root, "apps/desktop");
const { values } = parseArgs({
  options: {
    "engine-package": { type: "string" },
    standard: { type: "boolean", default: false },
    python: { type: "string", default: "python3" },
    "dry-run": { type: "boolean", default: false },
  },
});
const managed = values["engine-package"] !== undefined;
if (managed === values.standard) {
  throw new Error(
    "Pass --engine-package <signed-linux-package> for the RC5 Managed desktop, or --standard for an explicit build without the bundled engine.",
  );
}
const packageRoot = managed ? resolve(values["engine-package"]) : null;
const stagedPackage = resolve(
  desktop,
  "src-tauri/target/verisilo-managed-browser-resources/engine-package",
);
const args = [
  "build",
  "--ci",
  "--no-sign",
  "--target",
  "x86_64-unknown-linux-gnu",
  "--config",
  "src-tauri/tauri.linux.conf.json",
];
if (managed)
  args.push("--config", "src-tauri/tauri.managed-browser.linux.conf.json");
// Tauri forwards arguments after -- to Cargo; package builds use the committed lockfile.
args.push("--", "--locked");
const plan = {
  platform: "linux-x64",
  profile: managed ? "managed-browser-linux" : "desktop-linux",
  cwd: desktop,
  enginePackage: packageRoot,
  stagedPackage: managed ? stagedPackage : null,
  args,
};
if (values["dry-run"]) {
  console.log(JSON.stringify(plan, null, 2));
} else {
  if (process.platform !== "linux" || process.arch !== "x64") {
    throw new Error(
      "Linux x64 desktop packages must be built on native Linux x64, not cross-built from Windows.",
    );
  }
  const env = { ...process.env };
  if (managed) {
    const pins = (env.VERISILO_ENGINE_SIGNER_SHA256 ?? "")
      .split(",")
      .map((pin) => pin.trim());
    if (!pins.length || pins.some((pin) => !/^[a-f0-9]{64}$/u.test(pin))) {
      throw new Error(
        "VERISILO_ENGINE_SIGNER_SHA256 must contain the Linux package's public certificate SHA-256 pin.",
      );
    }
    const checked = spawnSync(
      values.python,
      [
        resolve(root, "scripts/build-camoufox-host-package.py"),
        "--check",
        packageRoot,
        "--require-signed",
      ],
      { cwd: root, encoding: "utf8", env },
    );
    if (checked.status !== 0)
      throw new Error(
        checked.stderr ||
          checked.error?.message ||
          "Linux engine package verification failed.",
      );
    const manifest = JSON.parse(
      readFileSync(resolve(packageRoot, "engine-package.json"), "utf8"),
    );
    const verification = JSON.parse(checked.stdout);
    if (
      manifest.platform !== "linux-x64" ||
      !pins.includes(manifest.signature?.keyId) ||
      verification.signatureVerified !== true ||
      verification.signerCertificateSha256 !== manifest.signature.keyId
    ) {
      throw new Error(
        "The engine package must be native linux-x64 and signed by a release-embedded certificate pin.",
      );
    }
    if (existsSync(stagedPackage))
      throw new Error(
        `Generated engine staging already exists: ${stagedPackage}. Remove this exact directory before rebuilding.`,
      );
    mkdirSync(dirname(stagedPackage), { recursive: true });
    cpSync(packageRoot, stagedPackage, {
      recursive: true,
      errorOnExist: true,
      force: false,
    });
    env.VERISILO_CAMOUFOX_LINUX_ASSET_LOCK = resolve(
      stagedPackage,
      "runtime-asset-lock.json",
    );
  }
  const require = createRequire(resolve(desktop, "package.json"));
  const cli = resolve(
    dirname(require.resolve("@tauri-apps/cli/package.json")),
    "tauri.js",
  );
  const built = spawnSync(process.execPath, [cli, ...args], {
    cwd: desktop,
    env,
    stdio: "inherit",
  });
  if (built.error) throw built.error;
  process.exitCode = built.status ?? 1;
}
