import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const managedPyInstallerVersion = "6.22.2";
const outputFiles = [
  "dependency-inventory.json",
  "bom.cyclonedx.json",
  "bom.spdx.json",
];

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function npmPurl(name, version) {
  if (name.startsWith("@")) {
    const separator = name.indexOf("/");
    if (separator === -1) {
      throw new Error(`Invalid scoped npm package name: ${name}`);
    }
    return `pkg:npm/${encodeURIComponent(name.slice(0, separator))}/${encodeURIComponent(name.slice(separator + 1))}@${encodeURIComponent(version)}`;
  }
  return `pkg:npm/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function parseNpmLockKey(key) {
  const separator = key.lastIndexOf("@");
  if (separator <= 0 || separator === key.length - 1) {
    throw new Error(`Unsupported pnpm package key: ${key}`);
  }
  return { name: key.slice(0, separator), version: key.slice(separator + 1) };
}

function unquoteYamlKey(value) {
  if (value.startsWith('"')) {
    return JSON.parse(value);
  }
  if (value.startsWith("'")) {
    return value.slice(1, -1).replaceAll("''", "'");
  }
  return value;
}

export function parsePnpmPackages(lockText) {
  const sectionStart = lockText.indexOf("\npackages:\n");
  if (sectionStart === -1) {
    throw new Error("pnpm-lock.yaml does not contain a packages section.");
  }
  const contentStart = sectionStart + "\npackages:\n".length;
  const sectionEndCandidate = lockText.indexOf("\nsnapshots:\n", contentStart);
  const sectionEnd =
    sectionEndCandidate === -1 ? lockText.length : sectionEndCandidate;
  const section = lockText.slice(contentStart, sectionEnd);
  const entryPattern =
    /^ {2}((?:"(?:[^"\\]|\\.)*"|'[^']*'|[^\s][^:\n]*)):\s*$/gmu;
  const matches = [...section.matchAll(entryPattern)];

  return matches.map((match, index) => {
    const rawKey = match[1];
    if (rawKey === undefined || match.index === undefined) {
      throw new Error("Unable to read a pnpm package entry.");
    }
    const key = unquoteYamlKey(rawKey);
    const nextIndex = matches[index + 1]?.index ?? section.length;
    const block = section.slice(match.index, nextIndex);
    const { name, version } = parseNpmLockKey(key);
    const integrity = block.match(
      /integrity:\s*(sha(?:256|384|512)-[A-Za-z0-9+/=]+)/u,
    )?.[1];
    const component = {
      ecosystem: "npm",
      name,
      version,
      purl: npmPurl(name, version),
      source: "pnpm-lock.yaml",
      local: false,
    };
    if (integrity !== undefined) {
      const separator = integrity.indexOf("-");
      const algorithm = integrity.slice(0, separator).toUpperCase();
      component.hash = {
        algorithm,
        value: Buffer.from(integrity.slice(separator + 1), "base64").toString(
          "hex",
        ),
      };
      component.integrity = integrity;
    }
    return component;
  });
}

function parseTomlString(block, key) {
  const rawValue = block.match(
    new RegExp(`^${key} = ("(?:[^"\\\\]|\\\\.)*")$`, "mu"),
  )?.[1];
  return rawValue === undefined ? undefined : JSON.parse(rawValue);
}

export function parseCargoPackages(lockText) {
  return lockText
    .split(/^\[\[package\]\]\s*$/mu)
    .slice(1)
    .map((block) => {
      const name = parseTomlString(block, "name");
      const version = parseTomlString(block, "version");
      if (name === undefined || version === undefined) {
        throw new Error("Cargo.lock contains a package without name/version.");
      }
      const checksum = parseTomlString(block, "checksum");
      const source = parseTomlString(block, "source");
      const component = {
        ecosystem: "cargo",
        name,
        version,
        purl: `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`,
        source: source ?? "apps/desktop/src-tauri/Cargo.lock (workspace)",
        local: source === undefined,
      };
      if (checksum !== undefined) {
        component.hash = { algorithm: "SHA256", value: checksum };
      }
      return component;
    });
}

export function parseUvPackages(lockText) {
  return lockText
    .split(/^\[\[package\]\]\s*$/mu)
    .slice(1)
    .map((block) => {
      const name = parseTomlString(block, "name");
      const version = parseTomlString(block, "version");
      if (name === undefined || version === undefined) {
        throw new Error("uv.lock contains a package without name/version.");
      }
      const digest = block.match(/\bhash = "sha256:([0-9a-f]{64})"/u)?.[1];
      const local = /source = \{\s*(?:virtual|editable)\s*=/u.test(block);
      const normalizedName = name.toLowerCase().replace(/[-_.]+/gu, "-");
      return {
        ecosystem: "pypi",
        name,
        version,
        purl: `pkg:pypi/${encodeURIComponent(normalizedName)}@${encodeURIComponent(version)}`,
        source: local
          ? "apps/camoufox-host/pyproject.toml"
          : "apps/camoufox-host/uv.lock",
        local,
        ...(digest === undefined
          ? {}
          : { hash: { algorithm: "SHA256", value: digest } }),
      };
    });
}

function sourceDate() {
  const epoch = Number(process.env.SOURCE_DATE_EPOCH ?? "0");
  if (!Number.isInteger(epoch) || epoch < 0) {
    throw new Error("SOURCE_DATE_EPOCH must be a non-negative integer.");
  }
  return new Date(epoch * 1000).toISOString().replace(".000Z", "Z");
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

function deterministicUuid(hexDigest) {
  const bytes = Buffer.from(hexDigest.slice(0, 32), "hex");
  bytes[6] = (bytes[6] & 0x0f) | 0x50;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = bytes.toString("hex");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function spdxId(component) {
  return `SPDXRef-Package-${sha256(component.purl).slice(0, 20)}`;
}

const profiles = {
  "managed-browser-windows": {
    inputPaths: [
      "package.json",
      "apps/desktop/package.json",
      "packages/contracts/package.json",
      "pnpm-lock.yaml",
      "apps/desktop/src-tauri/Cargo.lock",
      "apps/desktop/src-tauri/Cargo.toml",
      "apps/camoufox-host/windows-supervisor/Cargo.lock",
      "apps/camoufox-host/windows-supervisor/Cargo.toml",
      "apps/desktop/src-tauri/tauri.conf.json",
      "apps/desktop/src-tauri/tauri.release-reset.conf.json",
      "apps/desktop/src-tauri/tauri.managed-browser.conf.json",
      "apps/camoufox-host/pyproject.toml",
      "apps/camoufox-host/uv.lock",
      "apps/camoufox-host/lock/dependencies.json",
      "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json",
      "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json",
      "scripts/build-camoufox-host-package.py",
    ],
    npmManifests: [
      "package.json",
      "apps/desktop/package.json",
      "packages/contracts/package.json",
    ],
    cargoManifests: [
      "apps/desktop/src-tauri/Cargo.toml",
      "apps/camoufox-host/windows-supervisor/Cargo.toml",
    ],
    cargoLocks: [
      "apps/desktop/src-tauri/Cargo.lock",
      "apps/camoufox-host/windows-supervisor/Cargo.lock",
    ],
    pythonLocks: ["apps/camoufox-host/uv.lock"],
    extraComponents: [],
    rootManifest: "package.json",
    productLabel: "VeriSilo Managed Browser",
    includeNpm: true,
    scope:
      "Runtime pnpm, desktop Cargo, uv/Python, Firefox source, and the fixed Formal-v3 Camoufox binding. The package-tree manifest separately inventories every shipped Host/browser file.",
  },
  windows: {
    inputPaths: [
      "package.json",
      "apps/desktop/package.json",
      "apps/extension/package.json",
      "packages/contracts/package.json",
      "pnpm-lock.yaml",
      "apps/desktop/src-tauri/Cargo.lock",
      "apps/desktop/src-tauri/Cargo.toml",
      "crates/verisilo-remote-backend/Cargo.lock",
      "crates/verisilo-remote-backend/Cargo.toml",
      "apps/desktop/src-tauri/tauri.conf.json",
      "apps/extension/manifest.json",
    ],
    npmManifests: [
      "package.json",
      "apps/desktop/package.json",
      "apps/extension/package.json",
      "packages/contracts/package.json",
    ],
    cargoManifests: [
      "apps/desktop/src-tauri/Cargo.toml",
      "crates/verisilo-remote-backend/Cargo.toml",
    ],
    cargoLocks: [
      "apps/desktop/src-tauri/Cargo.lock",
      "crates/verisilo-remote-backend/Cargo.lock",
    ],
    rootManifest: "package.json",
    pythonLocks: [],
    extraComponents: [],
    productLabel: "VeriSilo",
    includeNpm: true,
    scope:
      "Complete pnpm and merged desktop/Remote-Agent Cargo lockfile inventory, including development, optional, target-specific, and transitive entries.",
  },
  "remote-agent": {
    inputPaths: [
      "crates/verisilo-remote-backend/Cargo.lock",
      "crates/verisilo-remote-backend/Cargo.toml",
    ],
    npmManifests: [],
    cargoManifests: ["crates/verisilo-remote-backend/Cargo.toml"],
    cargoLocks: ["crates/verisilo-remote-backend/Cargo.lock"],
    rootManifest: "crates/verisilo-remote-backend/Cargo.toml",
    pythonLocks: [],
    extraComponents: [],
    productLabel: "VeriSilo Remote Agent",
    includeNpm: false,
    scope:
      "Remote Agent Cargo lockfile inventory for the Linux x64 candidate, including target-specific and transitive entries; exact binary linkage still requires artifact review.",
  },
};

function managedBrowserComponents(input) {
  const sourcePath =
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json";
  const buildPath =
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json";
  const dependencyPath = "apps/camoufox-host/lock/dependencies.json";
  const source = JSON.parse(input[sourcePath].content.toString("utf8"));
  const build = JSON.parse(input[buildPath].content.toString("utf8"));
  const dependencies = JSON.parse(
    input[dependencyPath].content.toString("utf8"),
  );
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
    typeof build.engineRevision !== "string" ||
    !/^[0-9a-f]{64}$/u.test(archive.sha256 ?? "") ||
    typeof dependencies.python !== "string" ||
    !/^3\.[0-9]+\.[0-9]+$/u.test(dependencies.python)
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
      hash: { algorithm: "SHA256", value: archive.sha256 },
    },
    {
      ecosystem: "generic",
      name: "Mozilla Firefox source",
      version: firefox.version,
      purl: `pkg:generic/firefox@${encodeURIComponent(firefox.version)}`,
      source: sourcePath,
      local: false,
      hash: { algorithm: "SHA512", value: firefox.sha512 },
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

async function buildDocuments(profileName = "windows") {
  const profile = profiles[profileName];
  if (profile === undefined) {
    throw new Error(
      "SBOM profile must be managed-browser-windows, windows, or remote-agent.",
    );
  }
  const { inputPaths } = profile;
  const inputEntries = await Promise.all(
    inputPaths.map(async (relativePath) => {
      const content = await readFile(path.join(root, relativePath));
      return { path: relativePath, content, sha256: sha256(content) };
    }),
  );
  const input = Object.fromEntries(
    inputEntries.map((entry) => [entry.path, entry]),
  );
  const workspaceManifestPaths = profile.npmManifests;
  const localNpmComponents = workspaceManifestPaths.map((manifestPath) => {
    const manifestEntry = input[manifestPath];
    if (manifestEntry === undefined) {
      throw new Error(`Missing manifest input: ${manifestPath}`);
    }
    const manifest = JSON.parse(manifestEntry.content.toString("utf8"));
    if (
      typeof manifest.name !== "string" ||
      typeof manifest.version !== "string"
    ) {
      throw new Error(`${manifestPath} must declare name and version.`);
    }
    return {
      ecosystem: "npm",
      name: manifest.name,
      version: manifest.version,
      purl: npmPurl(manifest.name, manifest.version),
      source: manifestPath,
      local: true,
    };
  });
  const localCargoComponents = profile.cargoManifests.map((manifestPath) => {
    const manifestEntry = input[manifestPath];
    if (manifestEntry === undefined) {
      throw new Error(`Missing Cargo manifest input: ${manifestPath}`);
    }
    const manifest = manifestEntry.content.toString("utf8");
    const name = manifest.match(/^name = "([^"]+)"$/mu)?.[1];
    const version = manifest.match(/^version = "([^"]+)"$/mu)?.[1];
    if (name === undefined || version === undefined) {
      throw new Error(`${manifestPath} must declare package name and version.`);
    }
    return {
      ecosystem: "cargo",
      name,
      version,
      purl: `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`,
      source: manifestPath,
      local: true,
    };
  });
  const pnpmLock = input["pnpm-lock.yaml"];
  if (profile.includeNpm && pnpmLock === undefined) {
    throw new Error("Dependency lockfiles are missing.");
  }
  const lockedCargoComponents = new Map();
  const cargoLockEntries = profile.cargoLocks.map((lockPath) => {
    const lock = input[lockPath];
    if (lock === undefined) {
      throw new Error(`Missing Cargo lockfile input: ${lockPath}`);
    }
    return lock;
  });
  for (const component of cargoLockEntries
    .flatMap((lock) => parseCargoPackages(lock.content.toString("utf8")))
    .filter((component) => !component.local)) {
    const existing = lockedCargoComponents.get(component.purl);
    if (
      existing !== undefined &&
      (existing.source !== component.source ||
        existing.hash?.algorithm !== component.hash?.algorithm ||
        existing.hash?.value !== component.hash?.value)
    ) {
      throw new Error(
        `Rust lockfiles disagree about dependency identity ${component.purl}.`,
      );
    }
    lockedCargoComponents.set(component.purl, component);
  }
  const pythonComponents = profile.pythonLocks.flatMap((lockPath) => {
    const lock = input[lockPath];
    if (lock === undefined) {
      throw new Error(`Missing Python lockfile input: ${lockPath}`);
    }
    return parseUvPackages(lock.content.toString("utf8"));
  });
  if (profileName === "managed-browser-windows") {
    const pyinstallers = pythonComponents.filter(
      (component) =>
        component.name.toLowerCase() === "pyinstaller" && !component.local,
    );
    if (
      pyinstallers.length !== 1 ||
      pyinstallers[0]?.version !== managedPyInstallerVersion
    ) {
      throw new Error(
        `Managed-browser SBOM requires exactly PyInstaller ${managedPyInstallerVersion} from uv.lock.`,
      );
    }
  }
  const extraComponents =
    profileName === "managed-browser-windows"
      ? managedBrowserComponents(input)
      : profile.extraComponents;
  const components = [
    ...localNpmComponents,
    ...localCargoComponents,
    ...(profile.includeNpm
      ? parsePnpmPackages(pnpmLock.content.toString("utf8"))
      : []),
    ...lockedCargoComponents.values(),
    ...pythonComponents,
    ...extraComponents,
  ].sort((left, right) => left.purl.localeCompare(right.purl));
  const duplicate = components.find(
    (component, index) => components[index - 1]?.purl === component.purl,
  );
  if (duplicate !== undefined) {
    throw new Error(`Duplicate dependency identity: ${duplicate.purl}`);
  }

  const rootComponent = [...localNpmComponents, ...localCargoComponents].find(
    (component) => component.source === profile.rootManifest,
  );
  if (rootComponent === undefined) {
    throw new Error("Root package component is missing.");
  }
  const revision = sourceRevision();
  const created = sourceDate();
  const inputs = inputEntries.map(({ path: inputPath, sha256: digest }) => ({
    path: inputPath,
    sha256: digest,
  }));
  const lockDigest = sha256(
    inputs.map((entry) => `${entry.sha256}  ${entry.path}\n`).join(""),
  );
  const serial = deterministicUuid(lockDigest);

  const inventory = {
    schema: "urn:verisilo:dependency-inventory:1",
    schemaVersion: 1,
    sourceRevision: revision,
    generatedAt: created,
    generatedFrom: inputs,
    artifactProfile: profileName,
    scope: profile.scope,
    licenseEvidence:
      "Lockfiles do not prove declared licenses. Consult THIRD_PARTY_NOTICES.md and upstream package metadata before release.",
    componentCount: components.length,
    components,
  };

  const cycloneComponents = components
    .filter((component) => component.purl !== rootComponent.purl)
    .map((component) => ({
      type: component.local ? "application" : "library",
      "bom-ref": component.purl,
      name: component.name,
      version: component.version,
      purl: component.purl,
      ...(component.hash === undefined
        ? {}
        : {
            hashes: [
              {
                alg: component.hash.algorithm.replace("SHA", "SHA-"),
                content: component.hash.value,
              },
            ],
          }),
      properties: [
        { name: "verisilo:ecosystem", value: component.ecosystem },
        { name: "verisilo:source", value: component.source },
        { name: "verisilo:local", value: String(component.local) },
      ],
    }));
  const cyclonedx = {
    $schema: "https://cyclonedx.org/schema/bom-1.6.schema.json",
    bomFormat: "CycloneDX",
    specVersion: "1.6",
    serialNumber: `urn:uuid:${serial}`,
    version: 1,
    metadata: {
      timestamp: created,
      tools: {
        components: [
          {
            type: "application",
            name: "VeriSilo lockfile SBOM generator",
            version: "1",
          },
        ],
      },
      component: {
        type: "application",
        "bom-ref": rootComponent.purl,
        name: profile.productLabel,
        version: rootComponent.version,
        purl: rootComponent.purl,
      },
      properties: [
        { name: "verisilo:artifact-profile", value: profileName },
        { name: "verisilo:source-revision", value: revision },
        { name: "verisilo:input-digest", value: lockDigest },
      ],
    },
    components: cycloneComponents,
  };

  const spdxPackages = components.map((component) => ({
    name: component.name,
    SPDXID: spdxId(component),
    versionInfo: component.version,
    downloadLocation: "NOASSERTION",
    filesAnalyzed: false,
    licenseConcluded: "NOASSERTION",
    licenseDeclared: "NOASSERTION",
    copyrightText: "NOASSERTION",
    primaryPackagePurpose: component.local ? "APPLICATION" : "LIBRARY",
    externalRefs: [
      {
        referenceCategory: "PACKAGE-MANAGER",
        referenceType: "purl",
        referenceLocator: component.purl,
      },
    ],
    ...(component.hash === undefined
      ? {}
      : {
          checksums: [
            {
              algorithm: component.hash.algorithm,
              checksumValue: component.hash.value,
            },
          ],
        }),
    comment: `Inventory source: ${component.source}. License is NOASSERTION until separately verified from upstream metadata.`,
  }));
  const spdx = {
    spdxVersion: "SPDX-2.3",
    dataLicense: "CC0-1.0",
    SPDXID: "SPDXRef-DOCUMENT",
    name: `${profile.productLabel}-${rootComponent.version}-dependency-inventory`,
    documentNamespace: `https://github.com/QianQIUlp/VeriSilo/sbom/${profileName}/${revision}/${lockDigest}`,
    creationInfo: {
      created,
      creators: ["Tool: VeriSilo lockfile SBOM generator-1"],
    },
    documentDescribes: spdxPackages.map((component) => component.SPDXID),
    packages: spdxPackages,
  };

  return new Map([
    [outputFiles[0], stableJson(inventory)],
    [outputFiles[1], stableJson(cyclonedx)],
    [outputFiles[2], stableJson(spdx)],
  ]);
}

function selfTest() {
  const npm = parsePnpmPackages(
    `lockfileVersion: '9.0'\n\npackages:\n  "@scope/pkg@1.2.3":\n    resolution: {integrity: sha512-YWJj}\n\n  plain@2.0.0:\n    resolution: {}\n\nsnapshots:\n`,
  );
  if (
    npm.length !== 2 ||
    npm[0]?.purl !== "pkg:npm/%40scope/pkg@1.2.3" ||
    npm[0]?.hash?.value !== Buffer.from("abc").toString("hex")
  ) {
    throw new Error("pnpm parser self-test failed.");
  }
  const cargo = parseCargoPackages(
    `version = 4\n\n[[package]]\nname = "crate-a"\nversion = "1.0.0"\nsource = "registry+https://example.invalid"\nchecksum = "abcd"\n\n[[package]]\nname = "local"\nversion = "0.1.0"\n`,
  );
  if (cargo.length !== 2 || cargo[1]?.local !== true) {
    throw new Error("Cargo parser self-test failed.");
  }
  const uv = parseUvPackages(
    `version = 1\n\n[[package]]\nname = "example_pkg"\nversion = "1.2.3"\nsource = { registry = "https://pypi.org/simple" }\nsdist = { hash = "sha256:${"a".repeat(64)}" }\n\n[[package]]\nname = "local"\nversion = "0.1.0"\nsource = { virtual = "." }\n`,
  );
  if (
    uv.length !== 2 ||
    uv[0]?.purl !== "pkg:pypi/example-pkg@1.2.3" ||
    uv[0]?.hash?.value !== "a".repeat(64) ||
    uv[1]?.local !== true
  ) {
    throw new Error("uv parser self-test failed.");
  }
  const editable = parseUvPackages(
    `version = 1\n\n[[package]]\nname = "editable"\nversion = "0.1.0"\nsource = { editable = "." }\n`,
  );
  if (editable[0]?.local !== true) {
    throw new Error("uv editable-package self-test failed.");
  }
  process.stdout.write("SBOM parser self-test passed.\n");
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  const argumentsSet = new Set(process.argv.slice(2));
  if (argumentsSet.has("--self-test")) {
    selfTest();
  } else {
    const outIndex = process.argv.indexOf("--out");
    const outValue = outIndex === -1 ? undefined : process.argv[outIndex + 1];
    const profileIndex = process.argv.indexOf("--profile");
    const profileValue =
      profileIndex === -1 ? "windows" : process.argv[profileIndex + 1];
    if (outValue === undefined) {
      throw new Error(
        "Usage: node scripts/generate-sbom.mjs --out <directory> [--profile managed-browser-windows|windows|remote-agent] [--check] | --self-test",
      );
    }
    if (profileValue === undefined || profiles[profileValue] === undefined) {
      throw new Error(
        "--profile must be managed-browser-windows, windows, or remote-agent.",
      );
    }
    const outputDirectory = path.resolve(root, outValue);
    const documents = await buildDocuments(profileValue);
    if (argumentsSet.has("--check")) {
      for (const [name, expected] of documents) {
        const actual = await readFile(
          path.join(outputDirectory, name),
          "utf8",
        ).catch(() => undefined);
        if (actual !== expected) {
          throw new Error(
            `${path.join(outputDirectory, name)} is missing or stale; regenerate the SBOM.`,
          );
        }
      }
      process.stdout.write(
        `Verified ${documents.size} reproducible dependency/SBOM files in ${outputDirectory}.\n`,
      );
    } else {
      await mkdir(outputDirectory, { recursive: true });
      for (const [name, content] of documents) {
        await writeFile(path.join(outputDirectory, name), content, "utf8");
      }
      process.stdout.write(
        `Generated ${documents.size} dependency/SBOM files in ${outputDirectory}.\n`,
      );
    }
  }
}
