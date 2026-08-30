import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const profile = "managed-browser-windows";
const releaseVersion = "v0.1.0-rc1";
const installerName = `VeriSilo-Managed-Browser-${releaseVersion}-x64-setup.exe`;
const runtimeStepNames = [
  "installCurrentUser",
  "vaultInitializeUnlock",
  "enginePackageVerified",
  "siloAProxySiteSmoke",
  "siloAStatePersisted",
  "siloBIsolation",
  "singleActiveLimit",
  "siloAReplayStable",
  "requiredProxyFailClosed",
  "applicationRestart",
  "repairReinstallPreservesData",
  "uninstallPreservesData",
  "reinstallReopensVault",
];
const lifecycleNames = [
  "hostExitZero",
  "browserProcessTreeEmpty",
  "jobActiveCountZero",
  "relayClosed",
  "profileOwnershipReleased",
  "residualPidEmpty",
  "userClosePassed",
  "stopPassed",
  "applicationExitPassed",
  "failedLaunchPassed",
  "applicationRemoved",
  "dataPreserved",
];
const requiredFiles = new Set([
  "verisilo.exe",
  installerName,
  "README.txt",
  "LICENSE",
  "THIRD_PARTY_NOTICES.md",
  "windows-acceptance-report.json",
  "windows-acceptance-report.md",
  "authenticode-status.json",
  "dependency-licenses.json",
  "sbom/dependency-inventory.json",
  "sbom/bom.cyclonedx.json",
  "sbom/bom.spdx.json",
  "SHA256SUMS",
  "provenance.json",
]);
const allowedTopLevel = new Set([...requiredFiles, "sbom", "engine-package"]);
const forbiddenPath =
  /(?:^|\/)(?:hyper[-_ ]?v|vhdx?|environment|extension|native[-_ ]?host|hooks?|wsl|sandbox|portable|updater)(?:\/|$)/iu;
const checksumLinePattern = /^([0-9a-f]{64})  (.+)$/u;

function fail(message) {
  throw new Error(message);
}

function sha256(raw) {
  return createHash("sha256").update(raw).digest("hex");
}

function parseJson(raw, label) {
  try {
    return JSON.parse(raw.toString("utf8"));
  } catch (error) {
    fail(`${label} is not valid JSON: ${String(error)}`);
  }
}

async function collectFiles(directory, relativeDirectory = "") {
  const entries = await readdir(path.join(directory, relativeDirectory), {
    withFileTypes: true,
  });
  const files = [];
  for (const entry of entries) {
    const relativePath = path.posix.join(relativeDirectory, entry.name);
    const absolutePath = path.join(directory, ...relativePath.split("/"));
    const metadata = await lstat(absolutePath);
    if (metadata.isSymbolicLink()) {
      fail(`Managed-browser release contains a symlink: ${relativePath}`);
    }
    if (metadata.isDirectory()) {
      files.push(...(await collectFiles(directory, relativePath)));
    } else if (metadata.isFile()) {
      files.push({
        path: relativePath,
        absolutePath,
        bytes: metadata.size,
      });
    } else {
      fail(
        `Managed-browser release contains an irregular member: ${relativePath}`,
      );
    }
  }
  return files.sort((left, right) => left.path.localeCompare(right.path));
}

function assertReleaseShape(files) {
  const paths = new Set(files.map((file) => file.path));
  for (const required of requiredFiles) {
    if (!paths.has(required)) {
      fail(`Managed-browser release is missing ${required}.`);
    }
  }
  if (!files.some((file) => file.path.startsWith("engine-package/"))) {
    fail("Managed-browser release is missing the engine-package tree.");
  }
  for (const relativePath of paths) {
    const topLevel = relativePath.split("/", 1)[0];
    if (!allowedTopLevel.has(topLevel) && !relativePath.startsWith("sbom/")) {
      fail(
        `Managed-browser release contains an unexpected member: ${relativePath}`,
      );
    }
    if (
      !relativePath.startsWith("engine-package/") &&
      forbiddenPath.test(relativePath)
    ) {
      fail(
        `Managed-browser release contains a forbidden profile member: ${relativePath}`,
      );
    }
  }
  const executablePaths = files
    .map((file) => file.path)
    .filter(
      (relativePath) =>
        !relativePath.startsWith("engine-package/") &&
        path.extname(relativePath).toLowerCase() === ".exe",
    )
    .sort();
  if (
    JSON.stringify(executablePaths) !==
    JSON.stringify([installerName, "verisilo.exe"].sort())
  ) {
    fail(
      "Managed-browser release must stage only verisilo.exe and its NSIS installer.",
    );
  }
  if (
    files.some(
      (file) =>
        !file.path.startsWith("engine-package/") &&
        path.extname(file.path).toLowerCase() === ".ps1",
    )
  ) {
    fail("Managed-browser release must not stage PowerShell hooks or scripts.");
  }
}

async function verifyChecksums(directory, files) {
  const checksumText = await readFile(
    path.join(directory, "SHA256SUMS"),
    "utf8",
  );
  if (!checksumText.endsWith("\n") || checksumText.includes("\r")) {
    fail("SHA256SUMS must be UTF-8 LF text with a trailing newline.");
  }
  const entries = checksumText
    .trimEnd()
    .split("\n")
    .map((line) => {
      const match = checksumLinePattern.exec(line);
      if (match === null) {
        fail("SHA256SUMS contains a malformed entry.");
      }
      return { path: match[2], sha256: match[1] };
    });
  const payload = files
    .filter((file) => file.path !== "SHA256SUMS")
    .sort((left, right) => left.path.localeCompare(right.path));
  if (
    entries.length !== payload.length ||
    new Set(entries.map((entry) => entry.path)).size !== entries.length ||
    JSON.stringify(entries.map((entry) => entry.path)) !==
      JSON.stringify(payload.map((file) => file.path))
  ) {
    fail("SHA256SUMS does not cover the managed-browser release exactly once.");
  }
  for (const [index, file] of payload.entries()) {
    const digest = sha256(await readFile(file.absolutePath));
    if (digest !== entries[index].sha256) {
      fail(`SHA256SUMS digest mismatch for ${file.path}.`);
    }
  }
}

function verifyAuthenticodeReport(report, files) {
  if (
    report === null ||
    typeof report !== "object" ||
    JSON.stringify(Object.keys(report).sort()) !==
      JSON.stringify(
        [
          "expectedSignerCertificateSha256",
          "files",
          "mode",
          "schemaVersion",
          "signingState",
        ].sort(),
      ) ||
    report?.schemaVersion !== 1 ||
    report.mode !== "Unsigned" ||
    report.signingState !== "unsigned" ||
    report.expectedSignerCertificateSha256 !== null ||
    !Array.isArray(report.files)
  ) {
    fail(
      "Managed-browser release must carry an explicit unsigned Authenticode report.",
    );
  }
  const expected = files
    .filter(
      (file) =>
        !file.path.startsWith("engine-package/") &&
        path.extname(file.path).toLowerCase() === ".exe",
    )
    .map((file) => file.path)
    .sort();
  const actual = report.files.map((entry) => entry.path).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(
      "Authenticode report does not cover the managed-browser executables exactly once.",
    );
  }
  if (
    report.files.some(
      (entry) =>
        entry.status !== "NotSigned" ||
        Object.keys(entry).some((key) => !new Set(["path", "status"]).has(key)),
    )
  ) {
    fail(
      "Managed-browser Authenticode report contains a signed or malformed entry.",
    );
  }
}

function verifyAcceptanceReport(report, packageInfo, installerSha256) {
  const expectedKeys = new Set([
    "schema",
    "schemaVersion",
    "profile",
    "release",
    "status",
    "verified",
    "basis",
    "runtimeAcceptance",
    "desktopExecutable",
    "installer",
    "outerAuthenticode",
    "enginePackageRoot",
    "enginePackage",
    "dataRoot",
    "uninstaller",
  ]);
  if (
    report === null ||
    typeof report !== "object" ||
    JSON.stringify(Object.keys(report).sort()) !==
      JSON.stringify([...expectedKeys].sort()) ||
    report.schema !== "urn:verisilo:managed-browser-windows-acceptance:1" ||
    report.schemaVersion !== 1 ||
    report.profile !== profile ||
    report.release !== releaseVersion ||
    !["Pending", "Passed", "Failed", "Inconclusive"].includes(report.status) ||
    typeof report.verified !== "boolean" ||
    typeof report.basis !== "string" ||
    report.basis.length === 0 ||
    report.desktopExecutable !== "verisilo.exe" ||
    report.installer !== installerName ||
    report.outerAuthenticode !== "unsigned" ||
    report.enginePackageRoot !== "engine-package" ||
    report.dataRoot !== "%LOCALAPPDATA%\\io.verisilo.app" ||
    report.uninstaller === null ||
    typeof report.uninstaller !== "object" ||
    JSON.stringify(Object.keys(report.uninstaller).sort()) !==
      JSON.stringify(["dataPolicy"]) ||
    report.uninstaller?.dataPolicy !== "preserve"
  ) {
    fail("Managed-browser acceptance report is not the bounded RC1 contract.");
  }
  if (report.status === "Pending") {
    if (report.verified !== false || report.runtimeAcceptance !== null) {
      fail(
        "Pending managed-browser acceptance must not include runtime evidence.",
      );
    }
  } else {
    const runtime = report.runtimeAcceptance;
    const expectedRuntimeKeys = new Set([
      "os",
      "installerSha256",
      "packageManifestSha256",
      "executedAt",
      "steps",
      "lifecycle",
      "verdict",
    ]);
    if (
      runtime === null ||
      typeof runtime !== "object" ||
      JSON.stringify(Object.keys(runtime).sort()) !==
        JSON.stringify([...expectedRuntimeKeys].sort()) ||
      runtime.os === null ||
      typeof runtime.os !== "object" ||
      JSON.stringify(Object.keys(runtime.os).sort()) !==
        JSON.stringify(["build", "name"].sort()) ||
      runtime.os.name !== "Windows 11" ||
      typeof runtime.os.build !== "string" ||
      runtime.os.build.length === 0 ||
      !/^[0-9a-f]{64}$/u.test(runtime.installerSha256 ?? "") ||
      /^0{64}$/u.test(runtime.installerSha256) ||
      runtime.installerSha256 !== installerSha256 ||
      runtime.packageManifestSha256 !== packageInfo.manifestSha256 ||
      typeof runtime.executedAt !== "string" ||
      !runtime.executedAt.endsWith("Z") ||
      Number.isNaN(Date.parse(runtime.executedAt)) ||
      runtime.steps === null ||
      typeof runtime.steps !== "object" ||
      JSON.stringify(Object.keys(runtime.steps).sort()) !==
        JSON.stringify([...runtimeStepNames].sort()) ||
      runtimeStepNames.some(
        (name) =>
          !["PASS", "FAIL", "INCONCLUSIVE"].includes(runtime.steps[name]),
      ) ||
      runtime.lifecycle === null ||
      typeof runtime.lifecycle !== "object" ||
      JSON.stringify(Object.keys(runtime.lifecycle).sort()) !==
        JSON.stringify([...lifecycleNames].sort()) ||
      lifecycleNames.some(
        (name) => typeof runtime.lifecycle[name] !== "boolean",
      ) ||
      runtime.verdict !== report.status
    ) {
      fail(
        "Managed-browser runtime acceptance evidence is incomplete or unbound.",
      );
    }
    const allPassed =
      runtimeStepNames.every((name) => runtime.steps[name] === "PASS") &&
      lifecycleNames.every((name) => runtime.lifecycle[name] === true);
    if (
      report.status === "Passed"
        ? report.verified !== true || !allPassed
        : report.verified !== false
    ) {
      fail(
        "Managed-browser acceptance verification does not match its runtime verdict.",
      );
    }
  }
  const engine = report.enginePackage;
  if (
    engine === null ||
    typeof engine !== "object" ||
    JSON.stringify(Object.keys(engine).sort()) !==
      JSON.stringify(
        [
          "browserTreeSha256",
          "manifestSha256",
          "packageTreeSha256",
          "signatureAlgorithm",
          "signatureKeyId",
          "signed",
        ].sort(),
      ) ||
    engine.signed !== true ||
    engine.signatureAlgorithm !== "cms-detached-sha256" ||
    engine.manifestSha256 !== packageInfo.manifestSha256 ||
    engine.packageTreeSha256 !== packageInfo.packageTreeSha256 ||
    engine.browserTreeSha256 !== packageInfo.browserTreeSha256 ||
    engine.signatureKeyId !== packageInfo.signatureKeyId
  ) {
    fail(
      "Managed-browser acceptance report does not bind the signed engine package.",
    );
  }
}

function verifyAcceptanceMarkdown(markdown, status) {
  const normalized =
    typeof markdown === "string" ? markdown.replaceAll("\r\n", "\n") : "";
  if (
    typeof markdown !== "string" ||
    !normalized.startsWith("# Managed Browser Windows acceptance report\n") ||
    !normalized.split("\n").includes(`Status: ${status}`)
  ) {
    fail("Managed-browser acceptance markdown is missing the JSON verdict.");
  }
}

function verifyInventory(report, inventory, cyclonedx, spdx) {
  const requiredPythonBuildPurl = "pkg:pypi/pyinstaller@6.22.2";
  const requiredRuntimeNames = new Set([
    "Mozilla Firefox source",
    "CPython embedded runtime",
    "VeriSilo Camoufox Formal-v3 runtime",
  ]);
  if (
    report?.schema !== "urn:verisilo:dependency-license-evidence:1" ||
    report.schemaVersion !== 1 ||
    report.target !== "x86_64-pc-windows-msvc" ||
    report.legalConclusion !== false ||
    !Array.isArray(report.components) ||
    report.components.length === 0 ||
    report.components.some(
      (component) => component.requiresHumanReview !== true,
    ) ||
    report.coverage?.lockedComponents !== report.components.length ||
    !report.components.some(
      (component) => component.purl === requiredPythonBuildPurl,
    )
  ) {
    fail(
      "Managed-browser dependency license evidence is malformed or overclaims review.",
    );
  }
  if (
    inventory?.schema !== "urn:verisilo:dependency-inventory:1" ||
    inventory.artifactProfile !== profile ||
    inventory.componentCount !== inventory.components?.length ||
    !inventory.components.some(
      (component) => component.purl === requiredPythonBuildPurl,
    ) ||
    [...requiredRuntimeNames].some(
      (name) =>
        !inventory.components.some((component) => component.name === name),
    )
  ) {
    fail("Managed-browser SBOM inventory is missing the locked Python graph.");
  }
  if (
    cyclonedx?.bomFormat !== "CycloneDX" ||
    cyclonedx.specVersion !== "1.6" ||
    !cyclonedx.metadata?.properties?.some(
      (property) =>
        property.name === "verisilo:artifact-profile" &&
        property.value === profile,
    ) ||
    spdx?.spdxVersion !== "SPDX-2.3"
  ) {
    fail("Managed-browser SBOM documents are not profile-bound.");
  }
}

function verifyProvenance(provenance) {
  if (
    provenance?.schema !== "urn:verisilo:release-provenance:1" ||
    provenance.schemaVersion !== 1 ||
    provenance.build?.artifactProfile !== profile ||
    provenance.build?.signingState !== "unsigned" ||
    provenance.build?.promotionState !== "LOCAL_RC1_ONLY" ||
    provenance.build?.hyperVImageSource !== null ||
    provenance.versions?.managedBrowser !== releaseVersion
  ) {
    fail(
      "Managed-browser provenance is stale or includes an unrelated release profile.",
    );
  }
}

async function verifyEnginePackage(packageDirectory, pythonCommand) {
  const manifestPath = path.join(packageDirectory, "engine-package.json");
  const manifestRaw = await readFile(manifestPath);
  const manifest = parseJson(manifestRaw, "engine-package.json");
  if (
    manifest?.schemaVersion !== 3 ||
    manifest.engineId !== "camoufox" ||
    manifest.platform !== "windows-x64" ||
    manifest.signature?.algorithm !== "cms-detached-sha256" ||
    typeof manifest.signature.value !== "string" ||
    manifest.signature.value.length === 0 ||
    !/^[0-9a-f]{64}$/u.test(manifest.signature.keyId ?? "") ||
    /^0{64}$/u.test(manifest.signature.keyId)
  ) {
    fail(
      "Managed-browser engine package must carry a non-empty CMS signature and signer pin.",
    );
  }
  const check = spawnSync(
    pythonCommand,
    [
      path.join(root, "scripts", "build-camoufox-host-package.py"),
      "--check",
      packageDirectory,
      "--require-signed",
    ],
    {
      cwd: root,
      encoding: "utf8",
      shell: false,
      windowsHide: true,
      maxBuffer: 4 * 1024 * 1024,
    },
  );
  if (check.error !== undefined || check.status !== 0) {
    fail(
      `Managed-browser engine package verification failed${
        check.stderr?.trim() ? `: ${check.stderr.trim().slice(0, 500)}` : "."
      }`,
    );
  }
  const result = parseJson(
    Buffer.from(check.stdout),
    "engine package verification",
  );
  return {
    manifestSha256: sha256(manifestRaw),
    packageTreeSha256: result.packageTreeSha256,
    browserTreeSha256: result.browserTreeSha256,
    signatureKeyId: manifest.signature.keyId,
    signed: result.signed === true,
  };
}

export async function verifyRelease(
  directory,
  packageDirectory = path.join(directory, "engine-package"),
  pythonCommand = "python",
) {
  const files = await collectFiles(directory);
  assertReleaseShape(files);
  const resolvedDirectory = path.resolve(directory);
  const resolvedPackageDirectory = path.resolve(packageDirectory);
  if (
    resolvedPackageDirectory !== path.join(resolvedDirectory, "engine-package")
  ) {
    fail(
      "Managed-browser verifier requires the packaged engine tree in engine-package/.",
    );
  }
  const packageInfo = await verifyEnginePackage(
    resolvedPackageDirectory,
    pythonCommand,
  );
  const report = parseJson(
    await readFile(path.join(directory, "windows-acceptance-report.json")),
    "windows-acceptance-report.json",
  );
  verifyAcceptanceReport(
    report,
    packageInfo,
    sha256(await readFile(path.join(directory, installerName))),
  );
  verifyAcceptanceMarkdown(
    await readFile(
      path.join(directory, "windows-acceptance-report.md"),
      "utf8",
    ),
    report.status,
  );
  verifyAuthenticodeReport(
    parseJson(
      await readFile(path.join(directory, "authenticode-status.json")),
      "authenticode-status.json",
    ),
    files,
  );
  verifyInventory(
    parseJson(
      await readFile(path.join(directory, "dependency-licenses.json")),
      "dependency-licenses.json",
    ),
    parseJson(
      await readFile(path.join(directory, "sbom/dependency-inventory.json")),
      "dependency-inventory.json",
    ),
    parseJson(
      await readFile(path.join(directory, "sbom/bom.cyclonedx.json")),
      "bom.cyclonedx.json",
    ),
    parseJson(
      await readFile(path.join(directory, "sbom/bom.spdx.json")),
      "bom.spdx.json",
    ),
  );
  verifyProvenance(
    parseJson(
      await readFile(path.join(directory, "provenance.json")),
      "provenance.json",
    ),
  );
  await verifyChecksums(directory, files);
  process.stdout.write(
    `Managed-browser release verification passed for ${files.length} files.\n`,
  );
}

function selfTest() {
  if (
    checksumLinePattern.exec(
      `${"a".repeat(64)}  engine-package/browser/fonts/Academy Engraved LET Fonts.ttf`,
    ) === null ||
    checksumLinePattern.exec(
      `${"b".repeat(64)}  engine-package/host/_internal/tzdata/zoneinfo/Etc/GMT+1`,
    ) === null
  ) {
    fail("Managed-browser verifier self-test rejected valid package paths.");
  }
  if (!forbiddenPath.test("environment/images/base.vhdx")) {
    fail("Managed-browser verifier self-test did not reject a VHDX path.");
  }
  if (forbiddenPath.test("sbom/bom.spdx.json")) {
    fail("Managed-browser verifier self-test rejected a valid SBOM path.");
  }
  assertReleaseShape([
    ...[...requiredFiles].map((filePath) => ({ path: filePath })),
    { path: "engine-package/camoufox.exe" },
    { path: "engine-package/hooks/fixture.ps1" },
  ]);
  const report = {
    schemaVersion: 1,
    mode: "Unsigned",
    signingState: "unsigned",
    expectedSignerCertificateSha256: null,
    files: [
      { path: "verisilo.exe", status: "NotSigned" },
      { path: installerName, status: "NotSigned" },
    ],
  };
  verifyAuthenticodeReport(report, [
    { path: "verisilo.exe" },
    { path: installerName },
  ]);
  const packageInfo = {
    manifestSha256: "a".repeat(64),
    packageTreeSha256: "b".repeat(64),
    browserTreeSha256: "c".repeat(64),
    signatureKeyId: "d".repeat(64),
  };
  const acceptance = {
    schema: "urn:verisilo:managed-browser-windows-acceptance:1",
    schemaVersion: 1,
    profile,
    release: releaseVersion,
    status: "Pending",
    verified: false,
    basis: "fixture",
    runtimeAcceptance: null,
    desktopExecutable: "verisilo.exe",
    installer: installerName,
    outerAuthenticode: "unsigned",
    enginePackageRoot: "engine-package",
    enginePackage: {
      signed: true,
      signatureAlgorithm: "cms-detached-sha256",
      signatureKeyId: packageInfo.signatureKeyId,
      manifestSha256: packageInfo.manifestSha256,
      packageTreeSha256: packageInfo.packageTreeSha256,
      browserTreeSha256: packageInfo.browserTreeSha256,
    },
    dataRoot: "%LOCALAPPDATA%\\io.verisilo.app",
    uninstaller: { dataPolicy: "preserve" },
  };
  verifyAcceptanceReport(acceptance, packageInfo, "e".repeat(64));
  verifyAcceptanceMarkdown(
    "# Managed Browser Windows acceptance report\n\nStatus: Pending\n",
    "Pending",
  );
  const runtimeAcceptance = {
    os: { name: "Windows 11", build: "fixture" },
    installerSha256: "e".repeat(64),
    packageManifestSha256: packageInfo.manifestSha256,
    executedAt: "2026-08-28T00:00:00Z",
    steps: Object.fromEntries(runtimeStepNames.map((name) => [name, "PASS"])),
    lifecycle: Object.fromEntries(lifecycleNames.map((name) => [name, true])),
    verdict: "Passed",
  };
  verifyAcceptanceReport(
    { ...acceptance, status: "Passed", verified: true, runtimeAcceptance },
    packageInfo,
    "e".repeat(64),
  );
  let acceptedPassedStatus = false;
  try {
    verifyAcceptanceReport(
      { ...acceptance, status: "Passed", verified: false, runtimeAcceptance },
      packageInfo,
      "e".repeat(64),
    );
  } catch {
    acceptedPassedStatus = true;
  }
  if (!acceptedPassedStatus) {
    fail("Managed-browser verifier self-test accepted a completed status.");
  }
  let rejected = false;
  try {
    assertReleaseShape([
      { path: "verisilo.exe" },
      { path: "environment/probe.txt" },
    ]);
  } catch {
    rejected = true;
  }
  if (!rejected) {
    fail(
      "Managed-browser verifier self-test accepted an excluded environment path.",
    );
  }
  process.stdout.write("Managed-browser release verifier self-test passed.\n");
}

const isMain =
  process.argv[1] !== undefined &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);

if (isMain) {
  if (process.argv.includes("--self-test")) {
    selfTest();
    process.exit(0);
  }
  if (!process.argv.includes("--check")) {
    throw new Error(
      "Usage: node scripts/verify-managed-browser-release.mjs --check --release <directory> [--engine-package <directory>] [--python <command>] | --self-test",
    );
  }
  const value = (name, required = true) => {
    const index = process.argv.indexOf(name);
    const result = index === -1 ? undefined : process.argv[index + 1];
    if (required && (result === undefined || result.startsWith("--"))) {
      throw new Error(`Missing ${name} argument.`);
    }
    return result;
  };
  await verifyRelease(
    path.resolve(root, value("--release")),
    (() => {
      const packageArgument = value("--engine-package", false);
      return packageArgument === undefined
        ? undefined
        : path.resolve(root, packageArgument);
    })(),
    value("--python", false) ?? "python",
  );
}
