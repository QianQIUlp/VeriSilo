import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
);
const read = (relativePath) => readFile(path.join(root, relativePath), "utf8");
const [
  rust,
  environment,
  build,
  probe,
  hyperv,
  sandboxController,
  sandbox,
  guest,
] = await Promise.all([
  read("apps/desktop/src-tauri/src/environment_backend.rs"),
  read("apps/desktop/src-tauri/src/environment.rs"),
  read("apps/desktop/src-tauri/build.rs"),
  read("scripts/verisilo-environment-probe.ps1"),
  read("scripts/verisilo-hyperv.ps1"),
  read("scripts/verisilo-sandbox.ps1"),
  read("scripts/verisilo-sandbox-bootstrap.ps1"),
  read("scripts/verisilo-wsl-guest-agent.sh"),
]);

for (const operation of [
  "Create",
  "Start",
  "Stop",
  "Pause",
  "Snapshot",
  "Destroy",
  "ConfigureNetwork",
  "Health",
  "Logs",
]) {
  assert.match(rust, new RegExp(`EnvironmentOperation::${operation}`, "u"));
}

for (const guard of [
  "CommandCompletion::WaitForExit",
  "CommandCompletion::ConfirmSpawned",
  "Fixed provider process exceeded its",
  "Persistent Silo binding does not match",
  "expected_agent_sha256",
  "WSL_GUEST_AGENT_VERSION",
  "required proxy lacks fresh guest-observed proxy DNS and exit evidence",
  "Windows Sandbox V0.8 does not configure fixed proxies",
  "release_scripts_trusted",
  "manifest_schema_version",
]) {
  assert.match(
    rust,
    new RegExp(guard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}

assert.match(environment, /EMBEDDED_WSL_GUEST_AGENT/u);
assert.match(
  environment,
  /symlink_metadata[\s\S]*backend::metadata_is_reparse_point/u,
);
assert.match(environment, /identity\.mode == "755"/u);
assert.match(
  environment,
  /option_env!\("VERISILO_AUTHENTICODE_SIGNER_SHA256"\)/u,
);
assert.match(environment, /valid_lowercase_sha256/u);
assert.match(environment, /"-ExpectedSignerCertificateSha256"/u);
assert.match(
  build,
  /cargo:rerun-if-env-changed=VERISILO_AUTHENTICODE_SIGNER_SHA256/u,
);
assert.match(probe, /Get-AuthenticodeSignature/u);
assert.match(probe, /ValidatePattern\('\^\[0-9a-f\]\{64\}\$'\)/u);
assert.match(
  probe,
  /GetCertHashString\([\s\S]*HashAlgorithmName\]::SHA256[\s\S]*\)\.ToLowerInvariant\(\)/u,
);
assert.match(
  probe,
  /\$actualSignerCertificateSha256 -cne \$ExpectedSignerCertificateSha256/u,
);
assert.doesNotMatch(probe, /\$thumbprints|\.Count -eq 1/u);

for (const denied of [
  "<VGpu>Disable</VGpu>",
  "<AudioInput>Disable</AudioInput>",
  "<VideoInput>Disable</VideoInput>",
  "<PrinterRedirection>Disable</PrinterRedirection>",
  "<ClipboardRedirection>Disable</ClipboardRedirection>",
  "<ProtectedClient>Enable</ProtectedClient>",
  "<ReadOnly>true</ReadOnly>",
]) {
  assert.match(
    rust,
    new RegExp(denied.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}

assert.match(hyperv, /\$vmName = "VeriSilo-\$environmentId"/u);
assert.match(hyperv, /\$switchName = \$vmName/u);
assert.match(hyperv, /Read-SiloBinding/u);
assert.match(hyperv, /manifestTrusted/u);
assert.match(hyperv, /hyperv-receipt\.json/u);
assert.match(hyperv, /baseImageSha256/u);
assert.match(hyperv, /guestAgentVersion = \$null/u);
assert.match(hyperv, /ExpectedVmId/u);
assert.match(hyperv, /active VM adapters/u);
assert.match(hyperv, /success = \$false/u);
assert.match(
  hyperv,
  /Disable-VMIntegrationService[^\n]+'Guest Service Interface'/u,
);
assert.match(hyperv, /Get-VMAssignableDevice/u);
assert.match(hyperv, /DhcpGuard On -RouterGuard On -MacAddressSpoofing Off/u);
assert.match(hyperv, /Get-VMSnapshot[^\n]+checkpointName/u);
assert.doesNotMatch(hyperv, /New-VMSwitch[^\n]+SwitchType\s+External/u);
assert.doesNotMatch(hyperv, /Stop-VM[^\n]+-TurnOff/u);
assert.doesNotMatch(
  hyperv,
  /\b(?:Invoke-Expression|iex)\b|\s-(?:Command|EncodedCommand)\b/iu,
);

assert.match(guest, /\[\[ "\$AGENT_MODE" == '755' \]\]/u);
assert.match(guest, /concurrent multi-Silo Chromium is gated in V0\.8/u);
assert.match(guest, /multi-Silo WSL profiles are gated in V0\.8/u);
assert.match(guest, /readonly BROWSER_USER='verisilo-browser'/u);
assert.match(guest, /setpriv[\s\S]+--no-new-privs[\s\S]+--bounding-set=-all/u);
assert.match(guest, /DIRECT fallback is forbidden/u);
assert.match(guest, /--proxy-bypass-list=<-loopback>/u);
assert.match(guest, /--host-resolver-rules=MAP \* ~NOTFOUND/u);
assert.match(guest, /readonly LOOPBACK_PROXY_HOST='127\.0\.0\.1'/u);
assert.match(guest, /proxyDns/u);
assert.match(guest, /guestResolver/u);
assert.match(guest, /validUntil/u);
assert.match(guest, /authorization was revoked without terminating Chromium/u);
assert.doesNotMatch(guest, /emit_evidence 'configured' 'failed'/u);
assert.doesNotMatch(guest, /\beval\b|\b(?:bash|sh)\s+-c\b|\brm\s+-rf\b/u);

assert.match(sandbox, /first Sandbox slice only creates a local profile/u);
assert.doesNotMatch(sandbox, /param\([^)]*(?:Command|Path)/isu);
assert.match(sandboxController, /CloseMainWindow\(\)/u);
assert.match(sandboxController, /WaitForExit\(20000\)/u);
assert.match(sandboxController, /sandbox-process\.json/u);
assert.match(
  sandboxController,
  /Start-Process -FilePath \$actualExecutable -ArgumentList @\(\$descriptorPath\) -PassThru/u,
);
assert.match(sandboxController, /browserReady = 'unavailable'/u);
assert.doesNotMatch(sandboxController, /\bStop-Process\b|\btaskkill\b/iu);

process.stdout.write(
  "V0.8 environment backend static invariants passed; no Windows or virtualization runtime was exercised.\n",
);
