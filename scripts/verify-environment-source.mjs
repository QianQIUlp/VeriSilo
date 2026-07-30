import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sourcePaths = [
  "apps/desktop/src-tauri/src/environment_backend.rs",
  "scripts/verisilo-environment-probe.ps1",
  "scripts/verisilo-hyperv.ps1",
  "scripts/verisilo-sandbox.ps1",
  "scripts/verisilo-sandbox-bootstrap.ps1",
  "scripts/verisilo-wsl-guest-agent.sh",
];
const files = Object.fromEntries(
  await Promise.all(
    sourcePaths.map(async (relativePath) => [
      relativePath,
      await readFile(path.join(root, relativePath), "utf8"),
    ]),
  ),
);

function requirePattern(relativePath, pattern, explanation) {
  if (!pattern.test(files[relativePath])) {
    throw new Error(`${relativePath}: ${explanation}`);
  }
}

function rejectPattern(relativePath, pattern, explanation) {
  if (pattern.test(files[relativePath])) {
    throw new Error(`${relativePath}: ${explanation}`);
  }
}

const rustPath = "apps/desktop/src-tauri/src/environment_backend.rs";
requirePattern(
  rustPath,
  /WSL_GUEST_AGENT_PATH:\s*&str\s*=\s*"\/opt\/verisilo\/bin\/verisilo-guest-agent"/u,
  "the WSL adapter must retain a fixed in-guest executable path.",
);
requirePattern(
  rustPath,
  /assert!\(!spec\.args\.iter\(\)\.any\(\|argument\| argument == "-Command"\)\)/u,
  "the Hyper-V adapter test must reject command-text execution.",
);

const guestPath = "scripts/verisilo-wsl-guest-agent.sh";
for (const [pattern, explanation] of [
  [/^set -euo pipefail$/mu, "strict Bash error handling is missing."],
  [
    /readonly MAX_REQUEST_BYTES=16384/u,
    "the fixed request-size ceiling is missing.",
  ],
  [
    /case "\$ACTION" in[\s\S]*configure-network\)[\s\S]*start\)[\s\S]*stop\)[\s\S]*health\)[\s\S]*logs\)/u,
    "the guest command allowlist is missing or incomplete.",
  ],
  [
    /--proto '=https' --proto-redir '=https' --tlsv1\.2/u,
    "the guest exit probe must remain HTTPS-only.",
  ],
  [
    /guest probe configuration mode must be exactly 0600/u,
    "the guest probe configuration must remain root-only.",
  ],
  [
    /\["dnsEchoUrl", "dnsProbeHostname", "expectedDnsAnswer", "ipEchoUrl"\]/u,
    "the complete strict self-hosted DNS probe schema is missing.",
  ],
  [
    /dnsEchoUrl hostname must exactly match dnsProbeHostname/u,
    "the controlled DNS endpoint must be the SOCKS5H-resolved probe hostname.",
  ],
  [
    /readonly LOOPBACK_PROXY_HOST='127\.0\.0\.1'[\s\S]*socks5h:\/\/%s:%s/u,
    "guest probes must remain on the loopback SOCKS5H path.",
  ],
  [
    /--noproxy '' --proxy "\$proxy"/u,
    "guest probes must disable curl bypass and name the fixed proxy.",
  ],
  [
    /proxyDns[\s\S]*guestResolver[\s\S]*validUntil/u,
    "proxy DNS, guest resolver, and evidence validity must remain distinct.",
  ],
  [
    /stored proxy evidence is stale; authorization was revoked without terminating Chromium/u,
    "stale evidence must revoke launch authorization without killing the browser.",
  ],
  [
    /proxy DNS probe failed or changed answer; authorization was revoked without terminating Chromium/u,
    "proxy failure must revoke authorization without a DIRECT or automatic-kill fallback.",
  ],
]) {
  requirePattern(guestPath, pattern, explanation);
}
const revokeBody = files[guestPath].match(
  /revoke_network_authorization\(\) \{([\s\S]*?)\n\}/u,
)?.[1];
if (
  revokeBody === undefined ||
  !/rm -f -- "\$READY_FILE"/u.test(revokeBody) ||
  /terminate_owned_browser|\bkill\b/u.test(revokeBody)
) {
  throw new Error(
    `${guestPath}: evidence revocation must remove only readiness and never terminate Chromium.`,
  );
}
const stopBody = files[guestPath].match(
  /stop_browser\(\) \{([\s\S]*?)\n\}/u,
)?.[1];
if (stopBody === undefined || !/terminate_owned_browser/u.test(stopBody)) {
  throw new Error(
    `${guestPath}: only the explicit stop operation may terminate exact owned Chromium.`,
  );
}
for (const [pattern, explanation] of [
  [/\beval\b/u, "eval must not be introduced."],
  [/\b(?:bash|sh)\s+-c\b/u, "generic shell command execution is forbidden."],
  [/\brm\s+-rf\b/u, "recursive force deletion is forbidden."],
  [
    /\b(?:curl|jq)\b[^\n]*\+\s+(?:--|')/u,
    "a stray patch marker would change command semantics.",
  ],
]) {
  rejectPattern(guestPath, pattern, explanation);
}

for (const relativePath of sourcePaths.filter((value) =>
  value.endsWith(".ps1"),
)) {
  rejectPattern(
    relativePath,
    /\b(?:Invoke-Expression|iex)\b|(?:^|\s)-(?:EncodedCommand|Command)\b/imu,
    "environment scripts must not expose command-text execution.",
  );
  requirePattern(
    relativePath,
    /\$ErrorActionPreference\s*=\s*'Stop'/u,
    "PowerShell errors must fail closed.",
  );
}

requirePattern(
  "scripts/verisilo-hyperv.ps1",
  /\$request\.action\s+-notin\s+@\('create', 'start', 'stop', 'pause', 'checkpoint', 'remove', 'health', 'logs'\)/u,
  "the Hyper-V request action allowlist is missing.",
);
requirePattern(
  "scripts/verisilo-sandbox.ps1",
  /CloseMainWindow\(\)[\s\S]*WaitForExit\(20000\)[\s\S]*not force-killed/u,
  "the Sandbox controller must retain graceful exact-process stop without force kill.",
);
rejectPattern(
  "scripts/verisilo-sandbox.ps1",
  /\bStop-Process\b|\btaskkill\b/iu,
  "the Sandbox controller must never force-kill a process.",
);
requirePattern(
  "scripts/verisilo-sandbox-bootstrap.ps1",
  /first Sandbox slice only creates a local profile/u,
  "the Sandbox bootstrap must retain its explicit capability ceiling.",
);

if (process.argv.includes("--self-test")) {
  const fixture = "curl --fail + --proto '=https'";
  if (!/\bcurl\b[^\n]*\+\s+(?:--|')/u.test(fixture)) {
    throw new Error("Environment source verifier self-test failed.");
  }
  process.stdout.write("Environment source verifier self-test passed.\n");
} else {
  process.stdout.write(
    "Environment sources passed fixed-command, fail-closed, and guest-agent policy checks.\n",
  );
}
