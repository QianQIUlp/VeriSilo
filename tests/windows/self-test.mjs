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
];

for (const guard of requiredRunnerGuards) {
  assert.match(
    runner,
    new RegExp(guard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
  );
}

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

assert.match(workflow, /node tests\/windows\/self-test\.mjs/u);
assert.match(workflow, /Invoke-VeriSiloWindowsE2E\.ps1 -SelfTest/u);
assert.doesNotMatch(workflow, /-Browser\s+(Chrome|Edge|Both)/u);

process.stdout.write(
  "Windows E2E harness static self-test passed (desktop cases are driver-backed, non-temporary roots are refused, and no Windows behavior was claimed).\n",
);
