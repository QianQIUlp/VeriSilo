# Windows browser / Native Host / NSIS E2E harness

`Invoke-VeriSiloWindowsE2E.ps1` is a real-device harness for the Windows
surface. It does not start Chrome or Edge without an explicit temporary
`--user-data-dir`, does not load an extension, and does not rewrite or delete a
default browser profile. The local fixture is a Node HTTP server that binds
only to `127.0.0.1`.

The runner records `PASS`, `FAIL`, `SKIP`, and `BLOCKED` in `summary.json`.
`SKIP`/`BLOCKED` means that a required real prerequisite was absent; it is never
replaced with a mock. A normal run does not turn `BLOCKED` into a non-zero exit
status; add `-RequireAll` when a CI gate should reject an incomplete real-device
run.

## Cross-platform static safety check

This checks the harness's path/argument safeguards and fixture binding without
claiming that any Windows behavior ran:

```bash
node tests/windows/self-test.mjs
```

On a host with PowerShell 7, the runner also has a no-browser input self-test:

```powershell
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloWindowsE2E.ps1 -SelfTest
```

Neither command is a Windows E2E result.

The manual/reusable `Windows candidate promotion gate (real E2E)` workflow
targets separately administered self-hosted standard-user runners labeled
`verisilo-win10` and `verisilo-win11`. Its only release inputs are the exact
same-repository candidate artifact ID, upload-artifact SHA-256 digest, and full
provenance revision. It safely extracts and verifies that candidate's
`SHA256SUMS` and `provenance.json`, then runs fixed Windows 10/11 x64 ×
Chrome/Edge matrix cells with `-RequireAll`. From that exact revision it builds
the `acceptance-tests`-only `verisilo-acceptance-driver`, registers the exact
candidate Native Host in an isolated current-user manifest root, and passes the
verified candidate descriptor to the harness. Each cell uploads a
machine-readable v2 attestation binding the candidate ID/digest/revision,
descriptor hash, driver receipt hash, driver build revision, browser, OS, and
canonical result set. Any `SKIP`, `BLOCKED`, missing browser, wrong OS, missing
receipt, or missing summary is `FAIL`. A queued workflow with no matching runner
is not evidence.

The local runner parameters documented below remain useful for direct lab
diagnosis, but runner-local executable paths are intentionally not accepted by
the promotion workflow because they cannot bind evidence to an uploaded
candidate.

## Real browser cases

Run this on a physical or virtual Windows 10/11 machine as the intended test
user with PowerShell 7.4+, Node.js, and Chrome and/or Edge installed. Close the
selected browser completely first if the default-profile integrity assertion is
required; the runner will otherwise mark only that assertion `BLOCKED` instead
of racing the user profile.

```powershell
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloWindowsE2E.ps1 `
  -Browser Both `
  -ExpectedWindowsVersion 'Windows 10' `
  -KeepArtifacts
```

Use `-ChromePath` or `-EdgePath` if the browser is not in a conventional
Program Files location. One unavailable browser is `SKIP`; the other can still
run. Repeat the same command on a Windows 11 machine with
`-ExpectedWindowsVersion 'Windows 11'`; the runner records the product name,
display version, and build and will not label a mismatched OS as the requested
matrix target.

For each available browser, the runner:

- starts a loopback-only fixture and launches only temporary A and B
  `--user-data-dir` directories using `ProcessStartInfo.ArgumentList`;
- writes and reads `localStorage`, `sessionStorage`, IndexedDB, and a cookie;
  B must be empty, and a fresh A process must retain A's values;
- snapshots every default-profile file path, SHA-256, size, and UTC mtime before
  and after the temporary-profile cases, refusing to claim a result if another
  browser process could mutate it;
- holds Chromium's real Windows `lockfile` and checks that a second process does
  not expose another DevTools session for that profile;
- configures an unreachable `127.0.0.1` proxy with
  `--proxy-bypass-list=<-loopback>`, requiring both
  `ERR_PROXY_CONNECTION_FAILED` and zero fixture requests; and
- runs a fresh `--disable-extensions` browser baseline. This proves the fixture
  itself does not rely on an extension, but deliberately does **not** claim the
  desktop's optional-Companion UX passed.

The browser-level profile-lock assertion remains separate from
`verisilo_profile_lock_safe_refusal`. The latter can pass only when the
feature-gated Rust driver invokes the real desktop `VaultRuntime` and
`RuntimeManager` against the same live Chromium lock. A raw browser refusal is
never promoted to desktop evidence.

## Native Messaging Host and current-user registration

Use an actual Host built with the actual published Chrome and Edge extension
IDs, an actual release configuration, and an existing HKCU registration. The
runner invokes the repository's registration verifier, sends a length-prefixed
allowlisted `handshake` for each formal browser ID, requires a syntactically
valid but non-allowlisted origin to exit nonzero without emitting stdout, and
sends an unknown message that the real Host must reject with `invalid_message`.

```powershell
pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloWindowsE2E.ps1 `
  -Browser Both `
  -NativeHostPath 'C:\Users\<user>\AppData\Local\VeriSilo\verisilo-native-host.exe' `
  -ReleaseConfigPath 'C:\Users\<user>\AppData\Local\VeriSilo\native-host\native-host-release-config.json' `
  -DesktopExe 'C:\Users\<user>\AppData\Local\VeriSilo\verisilo.exe' `
  -KeepArtifacts
```

The release configuration must have two non-placeholder 32-character IDs. A
missing Host, config, or formal ID is `SKIP`/`BLOCKED`; an unpacked extension ID
or fabricated manifest is not accepted as release evidence. Current-user Native
Messaging registration does not require administrator rights. The NSIS case
below intentionally requires an unelevated user because the bundle is
`currentUser` scoped.

## NSIS silent lifecycle and retained real data

This case is destructive to the installed application, so use a disposable
standard-user Windows account and two real V1/V2 NSIS artifacts. It never makes
a fake Vault, report, or Silo directory. Seed data in the real V1 application
first, then provide paths to actual data that must survive the upgrade and
uninstall. Close the desktop and managed browser before starting the lifecycle
so their own writes cannot invalidate the retained-data fingerprint.

```powershell
# First real V1 silent installation, then use the V1 desktop UI to create the
# actual data paths supplied below. Do not use an elevated prompt.
& 'C:\E2E\VeriSilo_0.1.0_x64-setup.exe' /S

pwsh -NoProfile -File .\tests\windows\Invoke-VeriSiloWindowsE2E.ps1 `
  -RunNsis `
  -NsisInstallerV1 'C:\E2E\VeriSilo_0.1.0_x64-setup.exe' `
  -NsisInstallerV2 'C:\E2E\VeriSilo_0.1.1_x64-setup.exe' `
  -InstallDirectory 'C:\Users\<user>\AppData\Local\VeriSilo' `
  -RetainedDataPath 'C:\Users\<user>\AppData\Local\io.verisilo.app\vault', 'D:\E2E\real-silo-profile' `
  -KeepArtifacts `
  -RequireAll
```

The runner executes V1 `/S` again (an idempotent silent-install check), V2
`/S`, then the actual `uninstall.exe /S`, and compares the complete SHA-256 and
mtime fingerprints of every supplied real data path. It blocks before those
actions if an artifact, install directory, data path, or standard-user context
is missing. It does not infer an install location or manufacture data-retention
evidence. When `-RunNsis` is absent this optional destructive matrix is not
selected and does not emit a synthetic `SKIP`; the promotion attestation instead
requires an explicit canonical browser/desktop/Native Host result set. The V1/V2
installer lifecycle remains a separate public-release prerequisite.

## Acceptance-only desktop core driver

The repository intentionally exposes no production desktop automation API. The
only automated entry is the Cargo target
`verisilo-acceptance-driver`, guarded by the empty `acceptance-tests` feature and
`required-features`; normal desktop, Native Host, and release builds do not
include it. The promotion workflow builds it from the clean checkout whose HEAD
exactly equals the candidate descriptor's source revision, in a fresh target
directory under `RUNNER_TEMP`.

The driver accepts one strict JSON request through redirected stdin and accepts
no command-line arguments. The random Vault passphrase therefore stays in the
harness/driver process and anonymous pipe; it is never placed in argv, a file,
the receipt, or a log. Before initializing a Vault, the driver requires all of
the following:

- a newly created strict descendant of the OS temporary directory;
- a matching random 256-bit `.verisilo-acceptance-sentinel` supplied through
  that same anonymous request;
- no reparse-point root or sentinel; and
- a root outside the production `LOCALAPPDATA\VeriSilo` Vault and Chrome/Edge
  default User Data roots.

For each real Chrome/Edge matrix cell it initializes, locks, and unlocks a real
encrypted Vault; creates a real stock-browser Silo; confirms the managed
user-data-dir remains under the sentinel root; verifies locked-Vault sensitive
operations fail; launches the real browser through `RuntimeManager`; verifies a
second desktop-core launch safely refuses both the real Windows Chromium
`lockfile` and VeriSilo's `.verisilo-runtime.lock` cross-process lease; and proves
the extension-absent run stays usable while its Companion evidence is explicitly
empty/not requested. It then terminates only the exact PID tree recorded for
that runtime, requires the core to recover to `stopped`, checks the Profile and
Silo binding remain intact, and proves a separately spawned unrelated process
survived.

The receipt contains no password or Profile bytes. It binds the formal browser,
candidate repository/artifact ID/artifact digest/source revision, compile-time
driver revision, safety assertions, and the six canonical desktop results.
Missing driver/candidate/real-browser conditions are `SKIP` or `BLOCKED`; a
driver error or incomplete receipt is `FAIL`. With `-RequireAll`, every one of
those states fails the run. Static tests only verify this wiring and refusal
logic; they are never Windows acceptance evidence.
