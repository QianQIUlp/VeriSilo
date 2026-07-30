import { readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const targetTriple = "x86_64-pc-windows-msvc";
const resourceInputs = [
  [
    "scripts/install-native-host-release.ps1",
    "native-host/install-native-host-release.ps1",
  ],
  ["scripts/install-native-host.ps1", "native-host/install-native-host.ps1"],
  [
    "scripts/verify-native-host-install.ps1",
    "native-host/verify-native-host-install.ps1",
  ],
  [
    "scripts/uninstall-native-host.ps1",
    "native-host/uninstall-native-host.ps1",
  ],
  [
    "scripts/verisilo-environment-probe.ps1",
    "environment/verisilo-environment-probe.ps1",
  ],
  ["scripts/verisilo-hyperv.ps1", "environment/verisilo-hyperv.ps1"],
  ["scripts/verisilo-sandbox.ps1", "environment/verisilo-sandbox.ps1"],
  [
    "scripts/verisilo-sandbox-bootstrap.ps1",
    "environment/verisilo-sandbox-bootstrap.ps1",
  ],
  [
    "scripts/verisilo-wsl-guest-agent.sh",
    "environment/verisilo-wsl-guest-agent.sh",
  ],
];

function argument(name) {
  const index = process.argv.indexOf(name);
  return index === -1 ? undefined : process.argv[index + 1];
}

function stagingPaths() {
  const stagingRoot = path.resolve(
    root,
    argument("--staging-root") ?? "apps/desktop/src-tauri/target",
  );
  const resourceRoot = path.join(stagingRoot, "verisilo-release-resources");
  return {
    host: path.join(
      stagingRoot,
      "verisilo-release-sidecars",
      `verisilo-native-host-${targetTriple}.exe`,
    ),
    resourceRoot,
    config: path.join(
      resourceRoot,
      "native-host",
      "native-host-release-config.json",
    ),
    resources: resourceInputs.map(([source, destination]) => ({
      source: path.join(root, ...source.split("/")),
      destination: path.join(resourceRoot, ...destination.split("/")),
      relativeDestination: destination,
    })),
  };
}

async function expectedInputs() {
  const hostValue = argument("--host");
  const configValue = argument("--config");
  if (hostValue === undefined || configValue === undefined) {
    throw new Error(
      "Usage: node scripts/stage-windows-bundle.mjs --host <verisilo-native-host.exe> --config <release-config> [--check] | --self-test",
    );
  }
  const host = await readFile(path.resolve(root, hostValue));
  const config = await readFile(path.resolve(root, configValue));
  const parsedConfig = JSON.parse(config.toString("utf8"));
  for (const key of ["chromeExtensionId", "edgeExtensionId"]) {
    if (!/^[a-p]{32}$/u.test(parsedConfig[key] ?? "")) {
      throw new Error(`${key} is missing or invalid in the release config.`);
    }
  }
  const resources = await Promise.all(
    resourceInputs.map(async ([source, destination]) => ({
      destination,
      content: await readFile(path.join(root, ...source.split("/"))),
    })),
  );
  return { host, config, resources };
}

async function selfTest() {
  const {
    host: stagedHost,
    config: stagedConfig,
    resources: stagedResources,
  } = stagingPaths();
  const [releaseConfig, resetConfig, unsignedConfig] = await Promise.all([
    readFile(
      path.join(root, "apps/desktop/src-tauri/tauri.release.conf.json"),
      "utf8",
    ).then(JSON.parse),
    readFile(
      path.join(root, "apps/desktop/src-tauri/tauri.release-reset.conf.json"),
      "utf8",
    ).then(JSON.parse),
    readFile(
      path.join(root, "apps/desktop/src-tauri/tauri.unsigned.conf.json"),
      "utf8",
    ).then(JSON.parse),
  ]);
  const expectedResourceMap = Object.fromEntries([
    ...resourceInputs.map(([, destination]) => [
      `target/verisilo-release-resources/${destination}`,
      destination,
    ]),
    [
      "target/verisilo-release-resources/native-host/native-host-release-config.json",
      "native-host/native-host-release-config.json",
    ],
    [
      "target/verisilo-release-resources/environment/hyperv-image-manifest.json",
      "environment/hyperv-image-manifest.json",
    ],
    [
      "target/verisilo-release-resources/environment/images/",
      "environment/images/",
    ],
  ]);
  const normalizedEntries = (value) =>
    Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
  if (
    path.basename(stagedHost) !==
      "verisilo-native-host-x86_64-pc-windows-msvc.exe" ||
    path.basename(stagedConfig) !== "native-host-release-config.json" ||
    stagedResources.filter(({ destination }) =>
      destination.toLowerCase().endsWith(".ps1"),
    ).length !== 8 ||
    new Set(
      stagedResources.map(({ relativeDestination }) => relativeDestination),
    ).size !== stagedResources.length ||
    JSON.stringify(normalizedEntries(releaseConfig.bundle?.resources ?? {})) !==
      JSON.stringify(normalizedEntries(expectedResourceMap)) ||
    JSON.stringify(releaseConfig.bundle?.externalBin) !==
      JSON.stringify([
        "target/verisilo-release-sidecars/verisilo-native-host",
      ]) ||
    !Array.isArray(resetConfig.bundle?.resources) ||
    resetConfig.bundle.resources.length !== 0 ||
    !Array.isArray(unsignedConfig.bundle?.externalBin) ||
    unsignedConfig.bundle.externalBin.length !== 0 ||
    !Array.isArray(unsignedConfig.bundle?.resources) ||
    unsignedConfig.bundle.resources.length !== 0 ||
    unsignedConfig.bundle?.windows?.nsis?.installerHooks !== null
  ) {
    throw new Error(
      "Windows bundle staging or unsigned desktop-only overrides do not match Tauri conventions.",
    );
  }
  process.stdout.write("Windows bundle staging self-test passed.\n");
}

if (process.argv.includes("--self-test")) {
  await selfTest();
  process.exit(0);
}
const expected = await expectedInputs();
const {
  host: stagedHost,
  config: stagedConfig,
  resources: stagedResources,
} = stagingPaths();
if (process.argv.includes("--check")) {
  const [actualHost, actualConfig, ...actualResources] = await Promise.all([
    readFile(stagedHost).catch(() => undefined),
    readFile(stagedConfig).catch(() => undefined),
    ...stagedResources.map(({ destination }) =>
      readFile(destination).catch(() => undefined),
    ),
  ]);
  if (
    actualHost === undefined ||
    actualConfig === undefined ||
    !actualHost.equals(expected.host) ||
    !actualConfig.equals(expected.config) ||
    actualResources.some(
      (actual, index) =>
        actual === undefined ||
        !actual.equals(expected.resources[index].content),
    )
  ) {
    throw new Error(
      "Staged Windows sidecar, scripts, or release config are missing or stale.",
    );
  }
  process.stdout.write(
    "Verified staged Windows sidecar, scripts, and release config.\n",
  );
} else {
  await Promise.all([
    mkdir(path.dirname(stagedHost), { recursive: true }),
    mkdir(path.dirname(stagedConfig), { recursive: true }),
    ...stagedResources.map(({ destination }) =>
      mkdir(path.dirname(destination), { recursive: true }),
    ),
  ]);
  await Promise.all([
    writeFile(stagedHost, expected.host),
    writeFile(stagedConfig, expected.config),
    ...stagedResources.map(({ destination }, index) =>
      writeFile(destination, expected.resources[index].content),
    ),
  ]);
  process.stdout.write(
    `Staged Native Host sidecar for ${targetTriple}, release scripts, and config.\n`,
  );
}
