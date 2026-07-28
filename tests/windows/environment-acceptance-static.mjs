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
]) {
  assert.match(
    harness,
    new RegExp(guard.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"),
  );
}

assert.match(
  harness,
  /@\('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs'\)/u,
);
assert.doesNotMatch(
  harness,
  /\b(?:Invoke-Expression|iex)\b|\s-(?:Command|EncodedCommand)\b/iu,
);
assert.doesNotMatch(harness, /Remove-Item[^\n]+-Recurse/iu);

process.stdout.write(
  "Environment acceptance harness static self-test passed; no Windows virtualization was exercised.\n",
);
