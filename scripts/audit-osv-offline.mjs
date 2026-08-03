import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import semver from "semver";

const supportedEcosystems = new Map([
  ["cargo", "crates.io"],
  ["npm", "npm"],
]);

function stableJson(value) {
  return `${JSON.stringify(value, null, 2)}\n`;
}

function normalizeVersion(value) {
  return semver.valid(value, { loose: true, includePrerelease: true });
}

function compareVersions(left, right) {
  const normalizedLeft = normalizeVersion(left);
  const normalizedRight = normalizeVersion(right);
  if (normalizedLeft === null || normalizedRight === null) {
    throw new Error(
      `Unsupported semantic version boundary: ${left} / ${right}`,
    );
  }
  return semver.compare(normalizedLeft, normalizedRight);
}

export function rangeAffectsVersion(range, version) {
  if (range?.type !== "SEMVER" && range?.type !== "ECOSYSTEM") {
    return undefined;
  }
  if (!Array.isArray(range.events) || range.events.length === 0) {
    throw new Error("OSV range must contain at least one event.");
  }

  let affected = false;
  let opened = false;
  for (const event of range.events) {
    const keys = ["introduced", "fixed", "last_affected", "limit"].filter(
      (key) => typeof event?.[key] === "string",
    );
    if (keys.length !== 1) {
      throw new Error(
        "Each OSV range event must contain exactly one boundary.",
      );
    }

    const [kind] = keys;
    const boundary = event[kind];
    if (kind === "introduced") {
      opened = true;
      if (boundary === "0" || compareVersions(version, boundary) >= 0) {
        affected = true;
      }
      continue;
    }
    if (!opened) {
      throw new Error("An OSV range must start with an introduced event.");
    }

    const comparison = compareVersions(version, boundary);
    if (kind === "last_affected") {
      if (comparison > 0) {
        affected = false;
      }
    } else if (comparison >= 0) {
      affected = false;
    }
  }
  return affected;
}

export function affectedEntryAffectsVersion(entry, version) {
  const versions = Array.isArray(entry?.versions) ? entry.versions : [];
  if (versions.includes(version)) {
    return true;
  }

  const ranges = Array.isArray(entry?.ranges) ? entry.ranges : [];
  let supportedRangeSeen = false;
  for (const range of ranges) {
    const result = rangeAffectsVersion(range, version);
    if (result === undefined) {
      continue;
    }
    supportedRangeSeen = true;
    if (result) {
      return true;
    }
  }
  if (supportedRangeSeen || versions.length > 0) {
    return false;
  }
  return undefined;
}

function parseArguments(argv) {
  const result = { selfTest: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--self-test") {
      result.selfTest = true;
      continue;
    }
    const key = new Map([
      ["--inventory", "inventory"],
      ["--npm-db", "npmDb"],
      ["--crates-db", "cratesDb"],
      ["--out", "out"],
      ["--cargo-manifest", "cargoManifest"],
      ["--cargo-target", "cargoTarget"],
    ]).get(argument);
    if (key === undefined) {
      throw new Error(`Unknown argument: ${argument}`);
    }
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) {
      throw new Error(`${argument} requires a path.`);
    }
    result[key] = key === "cargoTarget" ? value : path.resolve(value);
    index += 1;
  }
  return result;
}

function cargoPurl(name, version) {
  return `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function targetCargoPurls(options) {
  if (
    (options.cargoManifest === undefined) !==
    (options.cargoTarget === undefined)
  ) {
    throw new Error(
      "--cargo-manifest and --cargo-target must be supplied together.",
    );
  }
  if (options.cargoManifest === undefined) {
    return undefined;
  }

  const result = spawnSync(
    "cargo",
    [
      "metadata",
      "--locked",
      "--offline",
      "--format-version",
      "1",
      "--filter-platform",
      options.cargoTarget,
      "--manifest-path",
      options.cargoManifest,
    ],
    {
      encoding: "utf8",
      maxBuffer: 128 * 1024 * 1024,
      shell: false,
      windowsHide: true,
    },
  );
  if (result.error !== undefined || result.status !== 0) {
    const detail = (result.stderr || result.stdout || "")
      .trim()
      .slice(0, 2_000);
    throw new Error(
      `Target-filtered Cargo metadata failed${detail === "" ? "." : `: ${detail}`}`,
      { cause: result.error },
    );
  }
  const metadata = JSON.parse(result.stdout);
  if (!Array.isArray(metadata?.packages)) {
    throw new Error("Cargo metadata did not contain a package list.");
  }
  return new Set(
    metadata.packages
      .filter(
        (entry) =>
          typeof entry?.name === "string" && typeof entry?.version === "string",
      )
      .map((entry) => cargoPurl(entry.name, entry.version)),
  );
}

function inventoryIndex(document, allowedCargoPurls) {
  if (
    document?.schema !== "urn:verisilo:dependency-inventory:1" ||
    !Array.isArray(document.components)
  ) {
    throw new Error("Input is not a VeriSilo dependency inventory.");
  }

  const index = new Map();
  for (const component of document.components) {
    if (component?.local === true) {
      continue;
    }
    const osvEcosystem = supportedEcosystems.get(component?.ecosystem);
    if (
      osvEcosystem === undefined ||
      typeof component?.name !== "string" ||
      typeof component?.version !== "string" ||
      typeof component?.purl !== "string"
    ) {
      continue;
    }
    if (
      component.ecosystem === "cargo" &&
      allowedCargoPurls !== undefined &&
      !allowedCargoPurls.has(component.purl)
    ) {
      continue;
    }
    const key = `${osvEcosystem}\u0000${component.name}`;
    const versions = index.get(key) ?? new Map();
    versions.set(component.version, component);
    index.set(key, versions);
  }
  return index;
}

function severityFor(advisory) {
  const severity = advisory?.database_specific?.severity;
  if (typeof severity === "string" && severity.length > 0) {
    return severity.toUpperCase();
  }
  if (Array.isArray(advisory?.severity) && advisory.severity.length > 0) {
    return advisory.severity
      .map((entry) => `${entry.type}:${entry.score}`)
      .join(", ");
  }
  return "UNKNOWN";
}

function severityRank(value) {
  return (
    new Map([
      ["CRITICAL", 5],
      ["HIGH", 4],
      ["MODERATE", 3],
      ["MEDIUM", 3],
      ["LOW", 2],
      ["UNKNOWN", 0],
    ]).get(value) ?? 1
  );
}

function preferredId(ids) {
  const priority = (id) => {
    if (id.startsWith("CVE-")) return 0;
    if (id.startsWith("GHSA-")) return 1;
    if (id.startsWith("RUSTSEC-")) return 2;
    return 3;
  };
  return [...ids].sort(
    (left, right) =>
      priority(left) - priority(right) || left.localeCompare(right),
  )[0];
}

function consolidateMatches(matches) {
  const groups = new Map();
  for (const finding of matches) {
    const ids = new Set([finding.id, ...finding.aliases]);
    const key = `${[...ids].sort().join("|")}\u0000${finding.purl}`;
    const current = groups.get(key);
    if (current === undefined) {
      groups.set(key, {
        ids,
        summaries: new Set(finding.summary === "" ? [] : [finding.summary]),
        severities: new Set([finding.severity]),
        informational: new Set(
          finding.informational === null ? [] : [finding.informational],
        ),
        ecosystem: finding.ecosystem,
        package: finding.package,
        version: finding.version,
        purl: finding.purl,
        modified: finding.modified,
      });
      continue;
    }
    for (const id of ids) current.ids.add(id);
    if (finding.summary !== "") current.summaries.add(finding.summary);
    current.severities.add(finding.severity);
    if (finding.informational !== null) {
      current.informational.add(finding.informational);
    }
    if (
      typeof finding.modified === "string" &&
      (current.modified === null || finding.modified > current.modified)
    ) {
      current.modified = finding.modified;
    }
  }

  return [...groups.values()]
    .map((group) => {
      const severity = [...group.severities].sort(
        (left, right) =>
          severityRank(right) - severityRank(left) || left.localeCompare(right),
      )[0];
      const informational = [...group.informational].sort();
      return {
        id: preferredId(group.ids),
        aliases: [...group.ids]
          .filter((id) => id !== preferredId(group.ids))
          .sort(),
        summary: [...group.summaries].sort()[0] ?? "",
        severity,
        informational,
        ecosystem: group.ecosystem,
        package: group.package,
        version: group.version,
        purl: group.purl,
        modified: group.modified,
      };
    })
    .sort((left, right) =>
      `${left.id}\u0000${left.purl}`.localeCompare(
        `${right.id}\u0000${right.purl}`,
      ),
    );
}

async function scanDatabase(directory, expectedEcosystem, index) {
  const entries = (await readdir(directory, { withFileTypes: true }))
    .filter((entry) => entry.isFile() && entry.name.endsWith(".json"))
    .sort((left, right) => left.name.localeCompare(right.name));
  if (entries.length === 0) {
    throw new Error(`OSV database is empty: ${directory}`);
  }

  const vulnerabilities = [];
  const incomplete = [];
  let withdrawn = 0;
  let matchedAdvisories = 0;

  for (const entry of entries) {
    const file = path.join(directory, entry.name);
    const advisory = JSON.parse(await readFile(file, "utf8"));
    if (
      typeof advisory?.id !== "string" ||
      !Array.isArray(advisory?.affected)
    ) {
      throw new Error(`Invalid OSV advisory: ${file}`);
    }
    if (advisory.withdrawn !== undefined) {
      withdrawn += 1;
      continue;
    }

    let advisoryMatched = false;
    for (const affected of advisory.affected) {
      const packageData = affected?.package;
      if (
        packageData?.ecosystem !== expectedEcosystem ||
        typeof packageData?.name !== "string"
      ) {
        continue;
      }
      const versions = index.get(
        `${expectedEcosystem}\u0000${packageData.name}`,
      );
      if (versions === undefined) {
        continue;
      }
      advisoryMatched = true;

      for (const [version, component] of versions) {
        let isAffected;
        let error;
        try {
          isAffected = affectedEntryAffectsVersion(affected, version);
        } catch (caught) {
          error = caught instanceof Error ? caught.message : String(caught);
        }
        if (isAffected === true) {
          vulnerabilities.push({
            id: advisory.id,
            aliases: Array.isArray(advisory.aliases)
              ? [...advisory.aliases].sort()
              : [],
            summary: advisory.summary ?? "",
            severity: severityFor(advisory),
            ecosystem: expectedEcosystem,
            package: packageData.name,
            version,
            purl: component.purl,
            modified: advisory.modified ?? null,
            informational:
              typeof affected?.database_specific?.informational === "string"
                ? affected.database_specific.informational
                : null,
          });
        } else if (isAffected === undefined || error !== undefined) {
          incomplete.push({
            id: advisory.id,
            ecosystem: expectedEcosystem,
            package: packageData.name,
            version,
            reason:
              error ??
              "No explicit versions or supported SEMVER/ECOSYSTEM range.",
          });
        }
      }
    }
    if (advisoryMatched) {
      matchedAdvisories += 1;
    }
  }

  return {
    advisoriesScanned: entries.length,
    withdrawnSkipped: withdrawn,
    matchedAdvisories,
    vulnerabilities,
    incomplete,
  };
}

async function audit(options) {
  for (const key of ["inventory", "npmDb", "cratesDb", "out"]) {
    if (options[key] === undefined) {
      throw new Error(
        "Usage: node scripts/audit-osv-offline.mjs --inventory <dependency-inventory.json> --npm-db <extracted npm OSV directory> --crates-db <extracted crates.io OSV directory> --out <report.json> [--cargo-manifest <Cargo.toml> --cargo-target <target triple>]",
      );
    }
  }

  const inventory = JSON.parse(await readFile(options.inventory, "utf8"));
  const allowedCargoPurls = targetCargoPurls(options);
  const index = inventoryIndex(inventory, allowedCargoPurls);
  const [npm, crates] = await Promise.all([
    scanDatabase(options.npmDb, "npm", index),
    scanDatabase(options.cratesDb, "crates.io", index),
  ]);
  const matches = consolidateMatches([
    ...npm.vulnerabilities,
    ...crates.vulnerabilities,
  ]);
  const warnings = matches.filter(
    (finding) =>
      finding.informational.length > 0 &&
      finding.informational.every((value) => value === "unmaintained"),
  );
  const vulnerabilities = matches.filter(
    (finding) => !warnings.includes(finding),
  );
  const incomplete = [...npm.incomplete, ...crates.incomplete].sort(
    (left, right) =>
      `${left.id}\u0000${left.ecosystem}\u0000${left.package}\u0000${left.version}`.localeCompare(
        `${right.id}\u0000${right.ecosystem}\u0000${right.package}\u0000${right.version}`,
      ),
  );
  const report = {
    schema: "urn:verisilo:offline-osv-audit:1",
    generatedAt: new Date().toISOString(),
    inventory: options.inventory,
    scope: {
      cargo:
        allowedCargoPurls === undefined
          ? { target: "all-lockfile-entries" }
          : {
              target: options.cargoTarget,
              manifest: options.cargoManifest,
              resolvedPackages: allowedCargoPurls.size,
            },
      npm: { target: "all-lockfile-entries" },
    },
    databases: {
      npm: {
        path: options.npmDb,
        advisoriesScanned: npm.advisoriesScanned,
        withdrawnSkipped: npm.withdrawnSkipped,
        matchedAdvisories: npm.matchedAdvisories,
      },
      cratesIo: {
        path: options.cratesDb,
        advisoriesScanned: crates.advisoriesScanned,
        withdrawnSkipped: crates.withdrawnSkipped,
        matchedAdvisories: crates.matchedAdvisories,
      },
    },
    result:
      vulnerabilities.length > 0 || incomplete.length > 0
        ? "fail"
        : warnings.length > 0
          ? "pass_with_warnings"
          : "pass",
    vulnerabilities,
    warnings,
    incomplete,
  };

  await mkdir(path.dirname(options.out), { recursive: true });
  await writeFile(options.out, stableJson(report), "utf8");
  console.log(
    `Offline OSV audit ${report.result.toUpperCase()}: ${vulnerabilities.length} vulnerable component match(es), ${warnings.length} maintenance warning(s), ${incomplete.length} incomplete classification(s), ${npm.advisoriesScanned + crates.advisoriesScanned} advisories scanned.`,
  );
  if (report.result === "fail") {
    process.exitCode = 1;
  }
}

function selfTest() {
  assert.equal(
    rangeAffectsVersion(
      {
        type: "SEMVER",
        events: [{ introduced: "0" }, { fixed: "1.2.0" }],
      },
      "1.1.9",
    ),
    true,
  );
  assert.equal(
    rangeAffectsVersion(
      {
        type: "SEMVER",
        events: [{ introduced: "0" }, { fixed: "1.2.0" }],
      },
      "1.2.0",
    ),
    false,
  );
  assert.equal(
    rangeAffectsVersion(
      {
        type: "ECOSYSTEM",
        events: [{ introduced: "1.2.0" }, { last_affected: "1.3.0" }],
      },
      "1.3.0",
    ),
    true,
  );
  assert.equal(
    rangeAffectsVersion(
      {
        type: "ECOSYSTEM",
        events: [{ introduced: "1.2.0" }, { last_affected: "1.3.0" }],
      },
      "1.3.1",
    ),
    false,
  );
  assert.equal(
    rangeAffectsVersion(
      {
        type: "SEMVER",
        events: [
          { introduced: "0" },
          { fixed: "1.0.0" },
          { introduced: "1.1.0" },
          { limit: "2.0.0" },
        ],
      },
      "1.5.0",
    ),
    true,
  );
  assert.equal(
    affectedEntryAffectsVersion(
      {
        versions: ["3.4.5-custom"],
        ranges: [{ type: "GIT", events: [{ introduced: "deadbeef" }] }],
      },
      "3.4.5-custom",
    ),
    true,
  );
  assert.equal(
    affectedEntryAffectsVersion(
      { ranges: [{ type: "GIT", events: [{ introduced: "deadbeef" }] }] },
      "1.0.0",
    ),
    undefined,
  );
  assert.throws(
    () =>
      rangeAffectsVersion(
        { type: "SEMVER", events: [{ fixed: "1.0.0" }] },
        "0.9.0",
      ),
    /start with an introduced/u,
  );
  const consolidated = consolidateMatches([
    {
      id: "RUSTSEC-2024-0001",
      aliases: ["GHSA-aaaa-bbbb-cccc"],
      summary: "same issue",
      severity: "UNKNOWN",
      informational: "unsound",
      ecosystem: "crates.io",
      package: "sample",
      version: "1.0.0",
      purl: "pkg:cargo/sample@1.0.0",
      modified: "2024-01-01T00:00:00Z",
    },
    {
      id: "GHSA-aaaa-bbbb-cccc",
      aliases: ["RUSTSEC-2024-0001"],
      summary: "same issue",
      severity: "HIGH",
      informational: null,
      ecosystem: "crates.io",
      package: "sample",
      version: "1.0.0",
      purl: "pkg:cargo/sample@1.0.0",
      modified: "2024-01-02T00:00:00Z",
    },
  ]);
  assert.equal(consolidated.length, 1);
  assert.equal(consolidated[0].id, "GHSA-aaaa-bbbb-cccc");
  assert.equal(consolidated[0].severity, "HIGH");
  assert.deepEqual(consolidated[0].informational, ["unsound"]);
  console.log("Offline OSV audit self-test passed.");
}

const options = parseArguments(process.argv.slice(2));
if (options.selfTest) {
  selfTest();
} else {
  await audit(options);
}
