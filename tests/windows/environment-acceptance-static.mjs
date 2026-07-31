import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const harness = await readFile(
  path.join(directory, "Invoke-VeriSiloEnvironmentAcceptance.ps1"),
  "utf8",
);

for (const guard of [
  "ProcessStartInfo.ArgumentList",
  "ArgumentList.Add",
  "MaximumOutputBytes",
  "WaitForExit($MaximumSeconds * 1000)",
  "Get-AuthenticodeSignature",
  "exact path/hash/owner/mode/version",
  "-ConfirmHyperVDestroy",
  "manifestTrusted",
  "cannot strand its test VM",
  "hyperv_confirmed_cleanup",
  "ExpectedSignerCertificateSha256",
  "Initialize-VerifiedProviderStage",
  "provider_signature_digest_stage",
  "Get-VerifiedProviderPath",
  "Open-VerifiedProviderLease",
  "Open-DirectoryChainLease",
  "DirectoryLeaseNative",
  "Open-HyperVImageLease",
  "Locked Hyper-V image handle",
  "[IO.FileShare]::Read",
  "Get-StreamSha256",
  "'-ExpectedSignerCertificateSha256', $ExpectedSignerCertificateSha256",
  "hyperv-create-journal.json",
  "rolled_back_from_journal",
  "cleanupState",
  "requestNonce",
  "-ExpectedEnvironmentId",
  "-ExpectedAction",
  "-ExpectedRequestNonce",
]) {
  assert.match(
    harness,
    new RegExp(guard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}

assert.match(
  harness,
  /Initialize-VerifiedProviderStage[\s\S]*Invoke-ProviderSelfTests/u,
);
assert.match(
  harness,
  /if \(\$SelfTest\)[\s\S]*provider_source_self_tests' 'SKIP'[\s\S]*else \{[\s\S]*Initialize-VerifiedProviderStage/u,
);
assert.doesNotMatch(harness, /Invoke-FixedProcess[^\n]+Get-ProviderPath/u);
assert.match(
  harness,
  /@\('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs'\)/u,
);
assert.match(
  harness,
  /Open-HyperVImageLease[\s\S]*Invoke-HyperVAction \$stateRoot \$environmentId 'create'[\s\S]*Close-HyperVImageLease/u,
);
assert.match(
  harness,
  /CreateFileW[\s\S]*0x00000001 -bor 0x00000002[\s\S]*0x02000000 -bor 0x00200000/u,
);
assert.doesNotMatch(
  harness,
  /\b(?:Invoke-Expression|iex)\b|\s-(?:Command|EncodedCommand)\b/iu,
);
assert.doesNotMatch(harness, /Remove-Item[^\n]+-Recurse/iu);

process.stdout.write(
  "Environment acceptance harness static self-test passed; no Windows virtualization was exercised.\n",
);
