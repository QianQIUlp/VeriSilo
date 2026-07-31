import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const directory = path.dirname(fileURLToPath(import.meta.url));
const runner = await readFile(
  path.join(directory, "Invoke-VeriSiloWindowsE2E.ps1"),
  "utf8",
);
const fixture = await readFile(
  path.join(directory, "fixtures", "loopback-server.mjs"),
  "utf8",
);
const workflow = await readFile(
  path.join(
    directory,
    "..",
    "..",
    ".github",
    "workflows",
    "windows-e2e-harness-static.yml",
  ),
  "utf8",
);
const promotionWorkflow = await readFile(
  path.join(
    directory,
    "..",
    "..",
    ".github",
    "workflows",
    "windows-e2e-real.yml",
  ),
  "utf8",
);
const driver = await readFile(
  path.join(
    directory,
    "..",
    "..",
    "apps",
    "desktop",
    "src-tauri",
    "acceptance",
    "windows_acceptance_driver.rs",
  ),
  "utf8",
);
const cargoManifest = await readFile(
  path.join(
    directory,
    "..",
    "..",
    "apps",
    "desktop",
    "src-tauri",
    "Cargo.toml",
  ),
  "utf8",
);
const contractModels = await readFile(
  path.join(directory, "..", "..", "packages", "contracts", "src", "models.ts"),
  "utf8",
);
const nativeHost = await readFile(
  path.join(
    directory,
    "..",
    "..",
    "apps",
    "desktop",
    "src-tauri",
    "src",
    "native_host.rs",
  ),
  "utf8",
);
const nativeProtocol = await readFile(
  path.join(
    directory,
    "..",
    "..",
    "packages",
    "contracts",
    "src",
    "protocol.ts",
  ),
  "utf8",
);
const nativeHostInstaller = await readFile(
  path.join(directory, "..", "..", "scripts", "install-native-host.ps1"),
  "utf8",
);
const nativeHostVerifier = await readFile(
  path.join(directory, "..", "..", "scripts", "verify-native-host-install.ps1"),
  "utf8",
);

const requiredRunnerGuards = [
  "Assert-TemporaryUserDataDirectory",
  "--user-data-dir=$UserDataDirectory",
  "--proxy-bypass-list=<-loopback>",
  "ERR_PROXY_CONNECTION_FAILED",
  "verify-native-host-install.ps1",
  "Native Host release configuration",
  "ArgumentList.Add",
  "verisilo_profile_lock_safe_refusal",
  "extension_absent_desktop_degradation",
  "desktop_recovery_after_exception",
  "nsis_silent_install_upgrade_uninstall_data_retention",
  "windows_matrix_target",
  "$skipped -gt 0",
  "$script:ActiveFixturePort = if ($FixturePort",
  "$requestedArtifactDirectory = if ($ArtifactDirectory",
  "Win32_OperatingSystem",
  "Get-WindowsFamilyFromBuild -Build 22000",
  "return ,(ConvertFrom-JsonArray -Json $json)",
  "[void]$Connection.Socket.SendAsync",
  "$message.PSObject.Properties['id']",
  "$Message.PSObject.Properties['error']",
  "[void]($Connection.NextId++)",
  "read-lifecycle&expectedPersistent=A&expectedEphemeral=",
  "no Unix-only Singleton marker was assumed",
  "Test-VeriSiloExtensionTarget",
  "Unrelated extension targets were ignored",
  "New-TemporaryArtifactDirectory",
  "Remove-TemporaryArtifactDirectory",
  ".verisilo-e2e-sentinel",
  "Browser.close",
  "ExpectedToken",
  "operationToken",
  "harnessToken",
  "Wait-CdpEndpointStable",
  "$secondEndpoint = @()",
  "Native Host did not return a complete frame within fifteen seconds.",
  "Assert-NativeHostRejectsUnauthorizedOrigin",
  "Native Host accepted a syntactically valid non-allowlisted origin.",
  "stdout bytes before rejecting a non-allowlisted origin.",
  "unsupported_protocol",
  "$runExitCode = Complete-Run",
];

for (const guard of requiredRunnerGuards) {
  assert.match(
    runner,
    new RegExp(guard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
  );
}

assert.doesNotMatch(runner, /\$script:(?:FixturePort|ArtifactDirectory)\s*=/u);
assert.doesNotMatch(runner, /Singleton(?:Lock|Cookie)/u);
assert.doesNotMatch(
  runner,
  /Where-Object\s*\{[^}]*\.url\s+-like\s+['"]chrome-extension:\/\/\*['"]/u,
);
const contractProtocolVersion = Number(
  contractModels.match(/export const PROTOCOL_VERSION = (\d+) as const;/u)?.[1],
);
const nativeHostProtocolVersion = Number(
  nativeHost.match(/pub const PROTOCOL_VERSION: u32 = (\d+);/u)?.[1],
);
const harnessProtocolVersion = Number(
  runner.match(/\$script:NativeHostProtocolVersion = (\d+)/u)?.[1],
);
assert.ok(
  Number.isInteger(contractProtocolVersion),
  "contracts must declare an exact Native Host protocol version",
);
assert.equal(nativeHostProtocolVersion, contractProtocolVersion);
assert.equal(harnessProtocolVersion, contractProtocolVersion);
assert.match(
  nativeProtocol,
  /product:\s*z\.literal\("VeriSilo"\)/u,
  "the harness's strict handshake assertion must match the shared response schema",
);
assert.match(
  runner,
  /Assert-ExactObjectProperties[\s\S]*'type', 'protocolVersion', 'requestId', 'product'[\s\S]*\$positive\.product -cne 'VeriSilo'/u,
);
assert.match(
  runner,
  /native_host_current_user_registration_and_messages'\s+-Status 'FAIL'/u,
  "real Native Host protocol or behavior regressions must be FAIL, not BLOCKED",
);
assert.match(
  runner,
  /function Assert-NativeHostRejectsUnauthorizedOrigin[\s\S]*?RedirectStandardInput = \$true[\s\S]*?RedirectStandardOutput = \$true[\s\S]*?CopyToAsync\([\s\S]*?WaitForExit\(15000\)[\s\S]*?\$process\.ExitCode -eq 0[\s\S]*?\$stdout\.Length -ne 0/u,
  "the formal Native Host gate must keep stdin available while requiring bounded nonzero-exit, zero-byte-stdout rejection",
);
const unauthorizedOriginGate =
  runner.match(
    /function Assert-NativeHostRejectsUnauthorizedOrigin[\s\S]*?(?=\r?\nfunction Assert-ExactObjectProperties)/u,
  )?.[0] ?? "";
assert.ok(
  unauthorizedOriginGate.length > 0,
  "the Native Host unauthorized-origin gate must remain present",
);
assert.doesNotMatch(
  unauthorizedOriginGate.slice(
    0,
    unauthorizedOriginGate.indexOf("WaitForExit(15000)"),
  ),
  /StandardInput\.Close\(\)/u,
  "the unauthorized-origin gate must not manufacture an EOF before the Host rejects the origin",
);
assert.match(
  runner,
  /Get-NonAllowlistedNativeHostExtensionId -AllowedExtensionIds[\s\S]*?Assert-NativeHostRejectsUnauthorizedOrigin[\s\S]*?-Origin "chrome-extension:\/\/\$unauthorizedExtensionId\/"/u,
  "the formal Native Host gate must exercise an extension ID outside both release allowlist entries",
);
const temporaryBrowserShutdown =
  runner.match(
    /function Stop-TemporaryBrowser[\s\S]*?(?=\r?\nfunction Wait-CdpEndpointStable)/u,
  )?.[0] ?? "";
assert.ok(
  temporaryBrowserShutdown.length > 0,
  "the temporary-browser shutdown gate must remain present",
);
assert.match(
  temporaryBrowserShutdown,
  /\$deadline = \[DateTime\]::UtcNow\.AddSeconds\(15\)[\s\S]*?do \{[\s\S]*?\$processExited[\s\S]*?Get-CdpEndpoint[\s\S]*?\$processExited -and \$endpointClosed[\s\S]*?Start-Sleep -Milliseconds 125[\s\S]*?\} while \(\[DateTime\]::UtcNow -lt \$deadline\)/u,
  "temporary-browser shutdown must poll process and DevTools closure within a bounded deadline",
);
assert.doesNotMatch(
  temporaryBrowserShutdown,
  /WaitForExit\(15000\)[\s\S]*?Get-CdpEndpoint/u,
  "temporary-browser shutdown must not make a one-shot post-exit endpoint decision",
);
assert.match(
  runner,
  /function New-ShortAcceptanceLeaf[\s\S]*?Get-RandomHex -Bytes 16\)\.Substring\(0, 16\)[\s\S]*?return "vda-\$token"/u,
  "the short acceptance leaf must retain 64 random bits and a separate sentinel boundary",
);
assert.match(
  runner,
  /\$acceptanceRoot = Join-Path \(\[IO\.Path\]::GetTempPath\(\)\) \(New-ShortAcceptanceLeaf\)/u,
  "the desktop acceptance root must leave enough Windows path budget for the managed Chromium Profile",
);
assert.match(
  runner,
  /\$shortAcceptanceLeaf = New-ShortAcceptanceLeaf[\s\S]*?\^vda-\[0-9a-f\]\{16\}\$/u,
  "the runtime self-test must exercise the short acceptance-root generator",
);
assert.doesNotMatch(
  runner,
  /\$acceptanceRoot\s*=.*verisilo-desktop-acceptance-/u,
  "the desktop acceptance root must not reintroduce the legacy overlong leaf",
);

for (const desktopEvidence of [
  "verisilo_profile_lock_safe_refusal",
  "extension_absent_desktop_degradation",
  "desktop_recovery_after_exception",
]) {
  assert.doesNotMatch(
    runner,
    new RegExp(
      `Add-Result\\s+-Name\\s+'${desktopEvidence}'\\s+-Status\\s+'BLOCKED'`,
      "u",
    ),
    `${desktopEvidence} must be produced by the real driver, not an unconditional BLOCKED result`,
  );
}

for (const driverGuard of [
  '#![cfg(feature = "acceptance-tests")]',
  'option_env!("VERISILO_ACCEPTANCE_SOURCE_REVISION")',
  "read_request_from_anonymous_stdin",
  "refusing non-temporary acceptance root",
  "refusing production Vault or default browser Profile root",
  "acceptance root must contain only its random sentinel",
  ".verisilo-acceptance-sentinel",
  "is_strict_descendant(&canonical_root, &canonical_temp)",
  "anonymous-stdin-pipe",
  ".create_new(true)",
  'const RECEIPT_FILE: &str = "acceptance-receipt.json"',
  "LauncherError::ProfileInUse",
  "Holding this kernel handle prevents the recorded PID from being reused",
  "ExactProcessHandle::open(runtime_record.pid)",
  "not_connected_no_extension_evidence",
  "unrelated_process_survived: true",
]) {
  assert.match(
    driver,
    new RegExp(driverGuard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}
assert.doesNotMatch(driver, /std::env::args|env::args/u);
assert.match(cargoManifest, /acceptance-tests\s*=\s*\[\]/u);
assert.match(
  cargoManifest,
  /name\s*=\s*"verisilo-acceptance-driver"[\s\S]*required-features\s*=\s*\["acceptance-tests"\]/u,
);
for (const provenanceGuard of [
  "ref: ${{ inputs.sourceRevision }}",
  "VERISILO_ACCEPTANCE_SOURCE_REVISION",
  "--features acceptance-tests --bin verisilo-acceptance-driver",
  "-CandidateDescriptorPath $env:VERISILO_PROMOTION_DESCRIPTOR",
  "--acceptance-receipt $env:VERISILO_PROMOTION_ACCEPTANCE_RECEIPT",
]) {
  assert.match(
    promotionWorkflow,
    new RegExp(provenanceGuard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}

assert.match(fixture, /host !== "127\.0\.0\.1"/u);
assert.match(fixture, /server\.listen\(port, host/u);
assert.doesNotMatch(fixture, /0\.0\.0\.0/u);
assert.match(fixture, /localStorage\.setItem/u);
assert.match(fixture, /document\.cookie/u);
assert.match(fixture, /indexedDB\.open/u);
assert.match(fixture, /operation === "read-lifecycle"/u);
assert.match(fixture, /--token/u);
assert.match(fixture, /urn:verisilo:windows-e2e-fixture-health:1/u);
assert.match(fixture, /operationToken/u);
assert.match(
  fixture,
  /url\.searchParams\.get\("harnessToken"\) !== harnessToken/u,
);
assert.match(
  fixture,
  /persistent\.every[\s\S]*expectedPersistent[\s\S]*ephemeral\.every[\s\S]*expectedEphemeral/u,
);

for (const installerGuard of [
  "Refusing to overwrite a pre-existing",
  "$registrySnapshots",
  "$fileSnapshots",
  "Rollback also failed",
  "Remove-Item -LiteralPath $temporaryPath",
  "verify-native-host-install.ps1",
]) {
  assert.match(
    nativeHostInstaller,
    new RegExp(installerGuard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}
assert.match(
  nativeHostInstaller,
  /Write-Utf8JsonAtomically -Path \$installRecordPath[\s\S]*?& \$verifyScript[\s\S]*?\} catch \{/u,
  "post-install verification must run inside the snapshot transaction",
);
for (const verifierGuard of [
  "$expectedConfigProperties",
  "Unsupported Native Host release configuration version.",
  "$expectedRecordProperties",
  "Native Host install record",
  "ConvertFrom-JsonPreservingDateStrings",
  "-DateKind String",
]) {
  assert.match(
    nativeHostVerifier,
    new RegExp(verifierGuard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}
assert.doesNotMatch(
  runner,
  /function Complete-Run[\s\S]*?\{\s*exit 1\s*\}/u,
  "Complete-Run must return an exit code so outer finally cleanup still runs",
);

assert.match(workflow, /node tests\/windows\/self-test\.mjs/u);
assert.match(workflow, /Invoke-VeriSiloWindowsE2E\.ps1 -SelfTest/u);
assert.doesNotMatch(workflow, /-Browser\s+(Chrome|Edge|Both)/u);

process.stdout.write(
  "Windows E2E harness static self-test passed (desktop cases are driver-backed, non-temporary roots are refused, and no Windows behavior was claimed).\n",
);
