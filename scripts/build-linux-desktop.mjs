import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
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
    bundles: { type: "string", default: "deb,appimage" },
    "bundle-only": { type: "boolean", default: false },
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
const bundles = values.bundles.split(",");
if (
  !bundles.length ||
  bundles.some((bundle) => !["deb", "appimage"].includes(bundle)) ||
  new Set(bundles).size !== bundles.length ||
  (values["bundle-only"] && bundles.length !== 1)
) {
  throw new Error(
    "Use --bundles deb, appimage or deb,appimage; --bundle-only requires one format.",
  );
}
const tauriArgs = (command, bundle) => {
  const args = [
    command,
    // Tauri's bundler streams linuxdeploy stderr at debug level, requiring -vv.
    "--verbose",
    "--verbose",
    "--ci",
    "--no-sign",
    "--target",
    "x86_64-unknown-linux-gnu",
    "--config",
    "src-tauri/tauri.linux.conf.json",
    "--bundles",
    bundle,
  ];
  if (managed) {
    args.push("--config", "src-tauri/tauri.managed-browser.linux.conf.json");
    if (bundle === "appimage")
      args.push(
        "--config",
        "src-tauri/tauri.managed-browser.appimage.linux.conf.json",
      );
  }
  // Only build invokes Cargo; bundle uses the binaries already compiled by build.
  if (command === "build") args.push("--", "--locked");
  return args;
};
const commands =
  bundles.length === 2
    ? [tauriArgs("build", "deb"), tauriArgs("bundle", "appimage")]
    : [tauriArgs(values["bundle-only"] ? "bundle" : "build", bundles[0])];
const appimageDirectory = resolve(
  desktop,
  "src-tauri/target/x86_64-unknown-linux-gnu/release/bundle/appimage",
);
const appimagePackage = resolve(
  appimageDirectory,
  "VeriSilo.AppDir/usr/share/VeriSilo/managed-browser/engine-package",
);
const plan = {
  platform: "linux-x64",
  profile: managed ? "managed-browser-linux" : "desktop-linux",
  cwd: desktop,
  enginePackage: packageRoot,
  stagedPackage: managed ? stagedPackage : null,
  args: commands[0],
  commands,
  appimagePackage:
    managed && bundles.includes("appimage") ? appimagePackage : null,
};
if (values["dry-run"]) {
  console.log(JSON.stringify(plan, null, 2));
} else {
  if (process.platform !== "linux" || process.arch !== "x64") {
    throw new Error(
      "Linux x64 desktop packages must be built on native Linux x64, not cross-built from Windows.",
    );
  }
  if (values["bundle-only"]) {
    for (const binary of ["verisilo", "verisilo-cli"]) {
      const path = resolve(
        desktop,
        "src-tauri/target/x86_64-unknown-linux-gnu/release",
        binary,
      );
      if (!existsSync(path) || !statSync(path).isFile())
        throw new Error(`Build the Linux desktop and CLI before bundling: ${path}`);
    }
  }
  const env = { ...process.env };
  let verifyPackage;
  const sameManifest = (candidate) => {
    if (
      !readFileSync(resolve(packageRoot, "engine-package.json")).equals(
        readFileSync(resolve(candidate, "engine-package.json")),
      )
    )
      throw new Error(
        "Bundling must retain the exact signed engine package used by this build.",
      );
  };
  if (managed) {
    const pins = (env.VERISILO_ENGINE_SIGNER_SHA256 ?? "")
      .split(",")
      .map((pin) => pin.trim());
    if (!pins.length || pins.some((pin) => !/^[a-f0-9]{64}$/u.test(pin))) {
      throw new Error(
        "VERISILO_ENGINE_SIGNER_SHA256 must contain the Linux package's public certificate SHA-256 pin.",
      );
    }
    verifyPackage = (candidate) => {
      const checked = spawnSync(
        values.python,
        [
          resolve(root, "scripts/build-camoufox-host-package.py"),
          "--check",
          candidate,
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
        readFileSync(resolve(candidate, "engine-package.json"), "utf8"),
      );
      const verification = JSON.parse(checked.stdout);
      if (
        manifest.platform !== "linux-x64" ||
        !pins.includes(manifest.signature?.keyId) ||
        verification.signatureVerified !== true ||
        verification.signerCertificateSha256 !== manifest.signature.keyId
      )
        throw new Error(
          "The engine package must be native linux-x64 and signed by a release-embedded certificate pin.",
        );
      return verification;
    };
    verifyPackage(packageRoot);
    if (values["bundle-only"]) {
      verifyPackage(stagedPackage);
      sameManifest(stagedPackage);
    } else {
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
    }
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
  for (const args of commands) {
    if (managed && args[0] === "bundle" && !values["bundle-only"]) {
      verifyPackage(stagedPackage);
      sameManifest(stagedPackage);
    }
    const built = spawnSync(process.execPath, [cli, ...args], {
      cwd: desktop,
      env,
      stdio: "inherit",
    });
    if (built.error) throw built.error;
    process.exitCode = built.status ?? 1;
    if (process.exitCode !== 0) break;
    if (managed && args[args.indexOf("--bundles") + 1] === "appimage") {
      const verification = verifyPackage(appimagePackage);
      sameManifest(appimagePackage);
      writeFileSync(
        resolve(appimageDirectory, "engine-package-check.json"),
        JSON.stringify(verification, null, 2),
      );
    }
  }
}
