import { readFile } from "node:fs/promises";

const files = Object.fromEntries(
  await Promise.all(
    [
      "apps/desktop/src-tauri/src/native_host.rs",
      "apps/desktop/src-tauri/resources/native-host-manifest.template.json",
      "packages/contracts/src/protocol.ts",
      "scripts/install-native-host.ps1",
      "scripts/register-native-host.ps1",
      "scripts/uninstall-native-host.ps1",
      "scripts/verify-native-host-install.ps1",
    ].map(async (path) => [path, await readFile(path, "utf8")]),
  ),
);

function requirePattern(path, pattern, explanation) {
  if (!pattern.test(files[path])) {
    throw new Error(`${path}: ${explanation}`);
  }
}

function rejectPattern(path, pattern, explanation) {
  if (pattern.test(files[path])) {
    throw new Error(`${path}: ${explanation}`);
  }
}

const rustPath = "apps/desktop/src-tauri/src/native_host.rs";
for (const variable of [
  "VERISILO_CHROME_EXTENSION_ID",
  "VERISILO_EDGE_EXTENSION_ID",
]) {
  requirePattern(
    rustPath,
    new RegExp(`option_env!\\(\"${variable}\"\\)`, "u"),
    `${variable} is not compiled into the production origin allowlist.`,
  );
}
requirePattern(
  rustPath,
  /MAX_MESSAGE_BYTES:\s*usize\s*=\s*16\s*\*\s*1024/u,
  "Native Host message limit must remain 16 KiB.",
);
requirePattern(
  rustPath,
  /deny_unknown_fields/u,
  "Native Host request and snapshot DTOs must reject unknown fields.",
);

const protocolPath = "packages/contracts/src/protocol.ts";
requirePattern(
  protocolPath,
  /NATIVE_MESSAGE_MAX_BYTES\s*=\s*16\s*\*\s*1024/u,
  "TypeScript and Rust message limits have drifted.",
);

const installPath = "scripts/install-native-host.ps1";
for (const registryVendor of ["Google\\Chrome", "Microsoft\\Edge"]) {
  if (!files[installPath].includes(`HKCU:\\Software\\${registryVendor}`)) {
    throw new Error(
      `${installPath}: missing current-user ${registryVendor} registration.`,
    );
  }
}
rejectPattern(
  installPath,
  /HKLM:|ExtensionInstallForcelist|ExtensionSettings/iu,
  "production registration must not write machine or force-install policy.",
);

const developmentPath = "scripts/register-native-host.ps1";
requirePattern(
  developmentPath,
  /native-host-development-allowlist\.json/u,
  "development IDs must be isolated from production configuration.",
);

const uninstallPath = "scripts/uninstall-native-host.ps1";
rejectPattern(
  uninstallPath,
  /Remove-Item[^\r\n]*(?:vault\.json|browser-data|profiles?)/iu,
  "Native Host uninstall must not target Vault or browser Profile data.",
);

const templatePath =
  "apps/desktop/src-tauri/resources/native-host-manifest.template.json";
requirePattern(
  templatePath,
  /__VERISILO_EXTENSION_ID__/u,
  "manifest must retain an explicit build placeholder until store IDs exist.",
);
rejectPattern(
  templatePath,
  /chrome-extension:\/\/[a-p]{32}\//u,
  "do not commit a development or invented ID as a production origin.",
);

process.stdout.write(
  "Native Host source passed protocol, production-ID, HKCU, and data-retention checks.\n",
);
