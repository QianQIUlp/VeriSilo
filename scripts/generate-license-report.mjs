import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  parseCargoPackages,
  parsePnpmPackages,
  parseUvPackages,
} from "./generate-sbom.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const managedPyInstallerVersion = "6.22.2";

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function sourceDate() {
  const epoch = Number(process.env.SOURCE_DATE_EPOCH ?? "0");
  if (!Number.isInteger(epoch) || epoch < 0) {
    throw new Error("SOURCE_DATE_EPOCH must be a non-negative integer.");
  }
  return new Date(epoch * 1_000).toISOString().replace(".000Z", "Z");
}

function sourceRevision() {
  const revision = process.env.VERISILO_SOURCE_REVISION ?? "unversioned-source";
  if (!/^(?:[0-9a-f]{40}|unversioned-source)$/u.test(revision)) {
    throw new Error(
      "VERISILO_SOURCE_REVISION must be a full lowercase Git commit or unversioned-source.",
    );
  }
  return revision;
}

function parseJson(raw, label) {
  try {
    return JSON.parse(raw.toString("utf8").replace(/^\uFEFF/u, ""));
  } catch (error) {
    throw new Error(`${label} is not valid JSON: ${String(error)}`);
  }
}

function npmPurl(name, version) {
  if (name.startsWith("@")) {
    const separator = name.indexOf("/");
    if (separator < 2) {
      throw new Error(`Invalid scoped npm package name: ${name}`);
    }
    return `pkg:npm/${encodeURIComponent(name.slice(0, separator))}/${encodeURIComponent(name.slice(separator + 1))}@${encodeURIComponent(version)}`;
  }
  return `pkg:npm/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function cargoPurl(name, version) {
  return `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function boundedOptional(value, maximum = 2_048) {
  return typeof value === "string" && value.trim() !== ""
    ? value.trim().slice(0, maximum)
    : null;
}

export function normalizeNpmLicenses(raw) {
  if (raw === null || typeof raw !== "object" || Array.isArray(raw)) {
    throw new Error(
      "pnpm license output must be an object grouped by license.",
    );
  }
  const evidence = new Map();
  for (const [groupLicense, entries] of Object.entries(raw)) {
    if (!Array.isArray(entries)) {
      throw new Error(`pnpm license group ${groupLicense} must be an array.`);
    }
    for (const entry of entries) {
      if (
        entry === null ||
        typeof entry !== "object" ||
        typeof entry.name !== "string" ||
        !Array.isArray(entry.versions)
      ) {
        throw new Error(
          `pnpm license group ${groupLicense} has an invalid entry.`,
        );
      }
      const declared = boundedOptional(entry.license) ?? groupLicense;
      if (declared !== groupLicense) {
        throw new Error(
          `pnpm license grouping disagrees for ${entry.name}: ${groupLicense} versus ${declared}.`,
        );
      }
      for (const version of entry.versions) {
        if (typeof version !== "string" || version.trim() === "") {
          throw new Error(
            `pnpm license entry ${entry.name} has an invalid version.`,
          );
        }
        const purl = npmPurl(entry.name, version);
        if (evidence.has(purl)) {
          throw new Error(`Duplicate pnpm license evidence for ${purl}.`);
        }
        evidence.set(purl, {
          purl,
          ecosystem: "npm",
          name: entry.name,
          version,
          declaredLicense: declared,
          licenseFile: null,
          repository: boundedOptional(entry.repository),
          homepage: boundedOptional(entry.homepage),
          evidenceSource: "pnpm-installed-package-metadata",
        });
      }
    }
  }
  return evidence;
}

export function normalizeCargoLicenses(raw) {
  if (raw === null || typeof raw !== "object" || !Array.isArray(raw.packages)) {
    throw new Error("cargo metadata output must contain a packages array.");
  }
  const evidence = new Map();
  for (const entry of raw.packages) {
    if (
      entry === null ||
      typeof entry !== "object" ||
      typeof entry.name !== "string" ||
      typeof entry.version !== "string" ||
      entry.source === null
    ) {
      continue;
    }
    const purl = cargoPurl(entry.name, entry.version);
    if (evidence.has(purl)) {
      throw new Error(`Duplicate Cargo license evidence for ${purl}.`);
    }
    const licenseFile = boundedOptional(entry.license_file);
    evidence.set(purl, {
      purl,
      ecosystem: "cargo",
      name: entry.name,
      version: entry.version,
      declaredLicense: boundedOptional(entry.license),
      licenseFile:
        licenseFile === null
          ? null
          : path.basename(licenseFile.replaceAll("\\", "/")),
      repository: boundedOptional(entry.repository),
      homepage: boundedOptional(entry.homepage),
      evidenceSource: "cargo-package-metadata",
    });
  }
  return evidence;
}

export function buildLicenseReport({
  npmLicenses,
  cargoMetadata,
  pnpmLock,
  cargoLock,
  pythonLock,
  extraComponents = [],
  includeNpm = true,
  target = "x86_64-pc-windows-msvc",
  generatedAt = sourceDate(),
  revision = sourceRevision(),
  inputDigests = {},
}) {
  const expected = [
    ...(includeNpm
      ? parsePnpmPackages(pnpmLock).filter((entry) => !entry.local)
      : []),
    ...parseCargoPackages(cargoLock).filter((entry) => !entry.local),
    ...(pythonLock === undefined
      ? []
      : parseUvPackages(pythonLock).filter((entry) => !entry.local)),
    ...extraComponents,
  ].sort((left, right) => left.purl.localeCompare(right.purl));
  const npmEvidence = normalizeNpmLicenses(npmLicenses);
  const cargoEvidence = normalizeCargoLicenses(cargoMetadata);
  const evidence = new Map([...npmEvidence, ...cargoEvidence]);
  const components = expected.map((component) => {
    const found = evidence.get(component.purl);
    return {
      purl: component.purl,
      ecosystem: component.ecosystem,
      name: component.name,
      version: component.version,
      resolved: found !== undefined,
      declaredLicense: found?.declaredLicense ?? null,
      licenseFile: found?.licenseFile ?? null,
      repository: found?.repository ?? null,
      homepage: found?.homepage ?? null,
      evidenceSource: found?.evidenceSource ?? null,
      requiresHumanReview: true,
      reviewReason:
        found === undefined
          ? "No installed-package metadata matched this locked component."
          : found.declaredLicense === null && found.licenseFile === null
            ? "Package metadata declares neither an SPDX expression nor a license file."
            : "Declared metadata is evidence, not a legal/distribution conclusion.",
    };
  });
  const expectedPurls = new Set(expected.map((component) => component.purl));
  const outOfLockEvidence = [...evidence.values()]
    .filter((component) => !expectedPurls.has(component.purl))
    .map((component) => component.purl)
    .sort();
  const resolved = components.filter((component) => component.resolved).length;
  return {
    schema: "urn:verisilo:dependency-license-evidence:1",
    schemaVersion: 1,
    sourceRevision: revision,
    generatedAt,
    target,
    scope:
      pythonLock !== undefined
        ? "Locked npm, Cargo, and uv/Python components with Formal-v3 runtime inputs. Installed package metadata remains evidence only; every entry still requires exact-artifact and legal review."
        : includeNpm
          ? "Locked npm and Cargo components cross-checked against installed package metadata. Includes build/dev/optional/target-specific lock entries; it does not claim that every entry is shipped."
          : "Locked Remote Agent Cargo components cross-checked against target-filtered package metadata. The report is candidate-specific but still requires exact-binary and legal review.",
    legalConclusion: false,
    releaseGate:
      "Every component remains human-review-required until the exact shipped artifact and required license/notice/source obligations are approved.",
    inputs: inputDigests,
    coverage: {
      lockedComponents: components.length,
      metadataResolved: resolved,
      metadataUnresolved: components.length - resolved,
      outOfLockEvidence: outOfLockEvidence.length,
    },
    outOfLockEvidence,
    components,
  };
}

function normalizedEvidenceDigest(evidence) {
  return sha256(
    stableJson(
      [...evidence.values()].sort((left, right) =>
        left.purl.localeCompare(right.purl),
      ),
    ),
  );
}

async function managedBrowserExtraComponents() {
  const sourcePath =
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json";
  const buildPath =
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json";
  const dependencyPath = "apps/camoufox-host/lock/dependencies.json";
  const [sourceRaw, buildRaw, dependencyRaw] = await Promise.all([
    readFile(path.join(root, sourcePath)),
    readFile(path.join(root, buildPath)),
    readFile(path.join(root, dependencyPath)),
  ]);
  const source = parseJson(sourceRaw, "Formal-v3 source lock");
  const build = parseJson(buildRaw, "Formal-v3 build result");
  const dependencies = parseJson(dependencyRaw, "Camoufox dependency lock");
  const firefox = source.firefoxSource;
  const archive = build.archive;
  if (
    firefox === null ||
    typeof firefox !== "object" ||
    typeof firefox.version !== "string" ||
    !/^[0-9]+\.[0-9]+\.[0-9]+$/u.test(firefox.version) ||
    !/^[0-9a-f]{128}$/u.test(firefox.sha512 ?? "") ||
    archive === null ||
    typeof archive !== "object" ||
    !/^[0-9a-f]{64}$/u.test(archive.sha256 ?? "") ||
    typeof build.engineRevision !== "string" ||
    typeof dependencies.python !== "string"
  ) {
    throw new Error(
      "Managed-browser source or runtime dependency binding is invalid.",
    );
  }
  return [
    {
      ecosystem: "generic",
      name: "VeriSilo Camoufox Formal-v3 runtime",
      version: build.engineRevision,
      purl: `pkg:generic/verisilo-camoufox@${encodeURIComponent(build.engineRevision)}`,
      source: buildPath,
      local: false,
    },
    {
      ecosystem: "generic",
      name: "Mozilla Firefox source",
      version: firefox.version,
      purl: `pkg:generic/firefox@${encodeURIComponent(firefox.version)}`,
      source: sourcePath,
      local: false,
    },
    {
      ecosystem: "generic",
      name: "CPython embedded runtime",
      version: dependencies.python,
      purl: `pkg:generic/cpython@${encodeURIComponent(dependencies.python)}`,
      source: dependencyPath,
      local: false,
    },
  ];
}

async function expectedReportFromRaw(
  npmRaw,
  cargoRaw,
  {
    includeNpm = true,
    target = "x86_64-pc-windows-msvc",
    cargoLockPath = "apps/desktop/src-tauri/Cargo.lock",
    pythonLockPath,
    extraComponents = [],
    requiredPyInstallerVersion,
  } = {},
) {
  const [pnpmLockRaw, cargoLockRaw, pythonLockRaw] = await Promise.all([
    includeNpm
      ? readFile(path.join(root, "pnpm-lock.yaml"))
      : Promise.resolve(
          Buffer.from("lockfileVersion: '9.0'\n\npackages:\n\nsnapshots:\n"),
        ),
    readFile(path.join(root, cargoLockPath)),
    pythonLockPath === undefined
      ? Promise.resolve(undefined)
      : readFile(path.join(root, pythonLockPath)),
  ]);
  const npmLicenses = parseJson(npmRaw, "pnpm license evidence");
  const cargoMetadata = parseJson(cargoRaw, "Cargo license evidence");
  if (requiredPyInstallerVersion !== undefined) {
    const pyinstallers = parseUvPackages(
      pythonLockRaw?.toString("utf8") ?? "",
    ).filter(
      (component) =>
        component.name.toLowerCase() === "pyinstaller" && !component.local,
    );
    if (
      pyinstallers.length !== 1 ||
      pyinstallers[0]?.version !== requiredPyInstallerVersion
    ) {
      throw new Error(
        `Managed-browser license evidence requires exactly PyInstaller ${requiredPyInstallerVersion} from uv.lock.`,
      );
    }
  }
  return buildLicenseReport({
    npmLicenses,
    cargoMetadata,
    pnpmLock: pnpmLockRaw.toString("utf8"),
    cargoLock: cargoLockRaw.toString("utf8"),
    pythonLock: pythonLockRaw?.toString("utf8"),
    extraComponents,
    includeNpm,
    target,
    inputDigests: {
      ...(includeNpm
        ? {
            normalizedNpmLicenseMetadataSha256: normalizedEvidenceDigest(
              normalizeNpmLicenses(npmLicenses),
            ),
            pnpmLockSha256: sha256(pnpmLockRaw),
          }
        : {}),
      normalizedCargoMetadataSha256: normalizedEvidenceDigest(
        normalizeCargoLicenses(cargoMetadata),
      ),
      cargoLockSha256: sha256(cargoLockRaw),
      ...(pythonLockRaw === undefined
        ? {}
        : { pythonLockSha256: sha256(pythonLockRaw) }),
    },
  });
}

async function expectedReport(npmPath, cargoPath, options) {
  const [npmRaw, cargoRaw] = await Promise.all([
    options.includeNpm ? readFile(npmPath) : Promise.resolve(Buffer.from("{}")),
    readFile(cargoPath),
  ]);
  return expectedReportFromRaw(npmRaw, cargoRaw, options);
}

function collect(command, args, label, env = process.env) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "buffer",
    env,
    maxBuffer: 64 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
  if (result.error !== undefined || result.status !== 0) {
    const stderr = result.stderr?.toString("utf8").trim().slice(0, 2_000);
    const stdout = result.stdout?.toString("utf8").trim().slice(0, 2_000);
    const detail = stderr === "" || stderr === undefined ? stdout : stderr;
    throw new Error(
      `${label} collection failed${detail === "" || detail === undefined ? "." : `: ${detail}`}`,
      { cause: result.error },
    );
  }
  return result.stdout;
}

function pnpmChildEnvironment() {
  const env = { ...process.env };
  for (const key of Object.keys(env)) {
    if (
      key.toLowerCase().startsWith("npm_") ||
      key.toUpperCase().startsWith("PNPM_") ||
      key === "INIT_CWD" ||
      key === "COREPACK_HOME"
    ) {
      delete env[key];
    }
  }
  return env;
}

async function collectExpectedReport(options) {
  const npmRaw = options.includeNpm
    ? collectPnpmLicenseMetadata()
    : Buffer.from("{}");
  const cargoRaw = collect(
    "cargo",
    [
      "metadata",
      "--locked",
      "--format-version",
      "1",
      "--filter-platform",
      options.target,
      "--manifest-path",
      options.cargoManifestPath,
    ],
    "Cargo license metadata",
  );
  return expectedReportFromRaw(npmRaw, cargoRaw, options);
}

function collectPnpmLicenseMetadata() {
  const args = ["licenses", "list", "--json"];
  if (process.platform !== "win32") {
    return collect(
      "pnpm",
      args,
      "pnpm license metadata",
      pnpmChildEnvironment(),
    );
  }

  // Node cannot execute a .cmd shim with shell:false on Windows (EINVAL).
  // pnpm sets npm_execpath for every package script, so invoke that pinned JS
  // entry point with the current Node executable instead of enabling a shell
  // around repository-controlled arguments.
  const pnpmEntrypoint = process.env.npm_execpath;
  if (
    pnpmEntrypoint === undefined ||
    !/\.(?:cjs|mjs|js)$/iu.test(pnpmEntrypoint)
  ) {
    throw new Error(
      "pnpm license metadata collection on Windows must run through the locked pnpm script; use pnpm licenses:report or provide --npm.",
    );
  }
  return collect(
    process.execPath,
    [pnpmEntrypoint, ...args],
    "pnpm license metadata",
    pnpmChildEnvironment(),
  );
}

function selfTest() {
  const report = buildLicenseReport({
    npmLicenses: {
      MIT: [
        {
          name: "example",
          versions: ["1.0.0"],
          license: "MIT",
          paths: ["/must/not/appear"],
        },
      ],
    },
    cargoMetadata: {
      packages: [
        {
          name: "sample",
          version: "2.0.0",
          source: "registry+https://github.com/rust-lang/crates.io-index",
          license: "Apache-2.0 OR MIT",
          license_file: "/runner/sample/LICENSE",
        },
      ],
    },
    pnpmLock:
      "lockfileVersion: '9.0'\n\npackages:\n\n  example@1.0.0:\n    resolution: {integrity: sha512-YQ==}\n\nsnapshots:\n",
    cargoLock:
      'version = 4\n\n[[package]]\nname = "sample"\nversion = "2.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"\n\n[[package]]\nname = "unresolved"\nversion = "3.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"\n',
    generatedAt: "1970-01-01T00:00:00Z",
    revision: "unversioned-source",
  });
  if (
    report.coverage.lockedComponents !== 3 ||
    report.coverage.metadataResolved !== 2 ||
    report.coverage.metadataUnresolved !== 1 ||
    report.components.some((component) =>
      JSON.stringify(component).includes("/must/not/appear"),
    ) ||
    report.components.find((component) => component.name === "sample")
      ?.licenseFile !== "LICENSE"
  ) {
    throw new Error("Dependency license report self-test failed.");
  }
  const remoteReport = buildLicenseReport({
    npmLicenses: {},
    cargoMetadata: { packages: [] },
    pnpmLock: "not parsed for a Cargo-only profile",
    cargoLock:
      'version = 4\n\n[[package]]\nname = "agent-dependency"\nversion = "1.0.0"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"\n',
    includeNpm: false,
    target: "x86_64-unknown-linux-gnu",
    generatedAt: "1970-01-01T00:00:00Z",
    revision: "unversioned-source",
  });
  if (
    remoteReport.target !== "x86_64-unknown-linux-gnu" ||
    remoteReport.coverage.lockedComponents !== 1 ||
    remoteReport.components[0]?.ecosystem !== "cargo"
  ) {
    throw new Error("Remote Agent license report profile self-test failed.");
  }
  const managedReport = buildLicenseReport({
    npmLicenses: {},
    cargoMetadata: { packages: [] },
    pnpmLock: "lockfileVersion: '9.0'\n\npackages:\n\nsnapshots:\n",
    cargoLock: "version = 4\n",
    pythonLock:
      'version = 1\n\n[[package]]\nname = "camoufox"\nversion = "0.5.4"\nsource = { registry = "https://pypi.org/simple" }\n',
    extraComponents: [
      {
        ecosystem: "generic",
        name: "Firefox source",
        version: "152.0.4",
        purl: "pkg:generic/firefox@152.0.4",
        source: "source-lock.json",
        local: false,
      },
    ],
    generatedAt: "1970-01-01T00:00:00Z",
    revision: "unversioned-source",
  });
  if (
    managedReport.coverage.lockedComponents !== 2 ||
    managedReport.components.some((component) => component.resolved) ||
    !managedReport.components.some(
      (component) => component.purl === "pkg:generic/firefox@152.0.4",
    )
  ) {
    throw new Error("Managed-browser Python/source license self-test failed.");
  }
  process.stdout.write("Dependency license report self-test passed.\n");
}

if (process.argv.includes("--self-test")) {
  selfTest();
  process.exit(0);
}

function argument(name, required = true) {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (required && (value === undefined || value.startsWith("--"))) {
    throw new Error(`Missing ${name} argument.`);
  }
  return value === undefined || value.startsWith("--")
    ? undefined
    : path.resolve(root, value);
}

function valueArgument(name) {
  const index = process.argv.indexOf(name);
  const value = index === -1 ? undefined : process.argv[index + 1];
  if (value !== undefined && value.startsWith("--")) {
    throw new Error(`Missing ${name} argument.`);
  }
  return value;
}

const profile = valueArgument("--profile") ?? "windows";
const profileOptions =
  profile === "managed-browser-windows"
    ? {
        includeNpm: true,
        target: "x86_64-pc-windows-msvc",
        cargoManifestPath: "apps/desktop/src-tauri/Cargo.toml",
        cargoLockPath: "apps/desktop/src-tauri/Cargo.lock",
        pythonLockPath: "apps/camoufox-host/uv.lock",
        extraComponents: await managedBrowserExtraComponents(),
        requiredPyInstallerVersion: managedPyInstallerVersion,
      }
    : profile === "windows"
      ? {
          includeNpm: true,
          target: "x86_64-pc-windows-msvc",
          cargoManifestPath: "apps/desktop/src-tauri/Cargo.toml",
          cargoLockPath: "apps/desktop/src-tauri/Cargo.lock",
        }
      : profile === "remote-agent"
        ? {
            includeNpm: false,
            target: "x86_64-unknown-linux-gnu",
            cargoManifestPath: "crates/verisilo-remote-backend/Cargo.toml",
            cargoLockPath: "crates/verisilo-remote-backend/Cargo.lock",
          }
        : undefined;
if (profileOptions === undefined) {
  throw new Error(
    "--profile must be managed-browser-windows, windows, or remote-agent.",
  );
}

const outputPath = argument("--out");
const collectInputs = process.argv.includes("--collect");
const npmPath = argument("--npm", !collectInputs && profileOptions.includeNpm);
const cargoPath = argument("--cargo", !collectInputs);
const expected = stableJson(
  collectInputs
    ? await collectExpectedReport(profileOptions)
    : await expectedReport(npmPath, cargoPath, profileOptions),
);
if (process.argv.includes("--check")) {
  const actual = await readFile(outputPath, "utf8");
  if (actual !== expected) {
    throw new Error(
      "Dependency license report is missing, stale, or not deterministic.",
    );
  }
  process.stdout.write("Dependency license report verified.\n");
} else {
  await writeFile(outputPath, expected, "utf8");
  process.stdout.write(`Dependency license report written to ${outputPath}.\n`);
}
