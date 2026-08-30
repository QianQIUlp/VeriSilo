# Managed Browser RC1 acceptance

Status: Pending. Runtime acceptance: Not Run. This runbook describes the
bounded clean Windows 11 procedure; it is not runtime evidence and does not
claim `Passed`.

## Frozen release artifact

Run the procedure only against the exact release directory
`artifacts/release/managed-browser/v0.1.0-rc1` and its exact installer
`VeriSilo-Managed-Browser-v0.1.0-rc1-x64-setup.exe`. Do not substitute a build
directory, loose engine files, another version, or source/runtime dependencies.

Before installing, confirm that the release directory contains exactly the
verifier's required release members and the `engine-package/**` tree:

```text
verisilo.exe
VeriSilo-Managed-Browser-v0.1.0-rc1-x64-setup.exe
README.txt
LICENSE
THIRD_PARTY_NOTICES.md
windows-acceptance-report.json
windows-acceptance-report.md
authenticode-status.json
dependency-licenses.json
sbom/dependency-inventory.json
sbom/bom.cyclonedx.json
sbom/bom.spdx.json
SHA256SUMS
provenance.json
engine-package/**
```

There must be no Hyper-V/VHDX, environment, extension, Native Host,
installer-hook, WSL, sandbox, portable, or updater files. Run the verifier
against this artifact:

```text
node scripts/verify-managed-browser-release.mjs --check --release artifacts/release/managed-browser/v0.1.0-rc1
```

The verifier requires the `engine-package/**` detached CMS SHA-256 signature,
the pinned signer, and complete manifest, Host, and browser tree hashes. This
internal CMS requirement is separate from the outer executable boundary. The
outer `verisilo.exe` and installer remain unsigned: `authenticode-status.json`
must use `mode: "Unsigned"`, `signingState: "unsigned"`,
`expectedSignerCertificateSha256: null`, and `NotSigned` for both executables;
the acceptance JSON must use
`outerAuthenticode: "unsigned"`. Neither boundary is a runtime or public
release approval.

The generated report must remain unchanged for this closeout:

```text
windows-acceptance-report.json: "status": "Pending", "verified": false,
"runtimeAcceptance": null
windows-acceptance-report.md: Status: Pending
```

Do not fill runtime evidence or change the verdict to `Passed`, `Failed`, or
`Inconclusive` here. `Not Run` describes the current runtime state; `Pending`
is the verifier-valid JSON status.

## Secrets and evidence boundary

Enter Vault and proxy credentials only in the local application UI when the
test requires them. Never request or paste a Vault password, proxy password,
token, seed, cookie, full command line, environment dump, or raw log into chat,
the acceptance report, screenshots, or shared evidence. Record only redacted
statuses, hashes, the Windows build, UTC time, and the required booleans/enums.

## Clean Windows 11 procedure

Use a clean Windows 11 x64 machine with no VeriSilo process running. Run every
step as the current standard user; do not elevate the installer. Use a
reachable fixed HTTP/SOCKS5 proxy for the proxy smoke, with credentials entered
locally if needed. A required-proxy failure test must use an unavailable
endpoint and must not fall back to Direct.

Record each runtime step under the exact key shown below, with only `PASS`,
`FAIL`, or `INCONCLUSIVE` as its value:

| Output key                     | Direct acceptance action                                                                                                                                  |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `installCurrentUser`           | Run the exact installer without elevation and confirm the installed app opens.                                                                            |
| `vaultInitializeUnlock`        | Create or unlock the local Vault; keep all secret material local.                                                                                         |
| `enginePackageVerified`        | Start a Managed Browser Silo and observe verification of `engine-package/**` before browser launch.                                                       |
| `siloAProxySiteSmoke`          | Create Silo A with a reachable fixed HTTP/SOCKS5 proxy in required mode, start it, and open a normal page; confirm the observed route matches that proxy. |
| `siloAStatePersisted`          | Stop A, start the same A again, and confirm its private Profile and ordinary site state remain available.                                                 |
| `siloBIsolation`               | Create/start Silo B after A stops and confirm B uses a different Profile and identity seed without exposing the seed.                                     |
| `singleActiveLimit`            | While A is active, attempt to start B; confirm the second browser launch is refused.                                                                      |
| `siloAReplayStable`            | Stop B, replay A, and confirm the saved Identity Artifact/Profile binding is stable without claiming undetectability.                                     |
| `requiredProxyFailClosed`      | Make A's required proxy unavailable, attempt launch, and confirm launch fails without a direct fallback.                                                  |
| `applicationRestart`           | Exit and relaunch the application, unlock the Vault, and confirm the saved Silos remain available.                                                        |
| `repairReinstallPreservesData` | Run the same RC installer again as the current user and confirm Vault/Silo data remains present.                                                          |
| `uninstallPreservesData`       | Uninstall from Windows Apps as the same user; confirm the application is removed while the data root remains.                                             |
| `reinstallReopensVault`        | Reinstall the same RC, unlock the Vault, and confirm the prior Silos/Profile state reopens.                                                               |

At each user-close, **Stop Silo**, application-exit, failed-launch, uninstall,
and reinstall boundary, inspect the owned process/lifecycle state before moving
on. Record the following `lifecycle` object with exactly these keys and boolean
values; do not replace them with renamed fields or prose:

| Output key                 | Direct observation                                                                                                            |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `hostExitZero`             | The Host exits with code 0 on the expected clean shutdown paths.                                                              |
| `browserProcessTreeEmpty`  | No owned browser process or descendant remains.                                                                               |
| `jobActiveCountZero`       | The owned job has zero active members.                                                                                        |
| `relayClosed`              | The network/proxy relay is closed.                                                                                            |
| `profileOwnershipReleased` | Profile ownership/lock is released.                                                                                           |
| `residualPidEmpty`         | No owned residual PID remains.                                                                                                |
| `userClosePassed`          | The user-close path completes with the lifecycle checks above satisfied.                                                      |
| `stopPassed`               | **Stop Silo** completes with the lifecycle checks above satisfied.                                                            |
| `applicationExitPassed`    | Tray **退出 VeriSilo** completes only after the lifecycle checks above; window X alone is the documented hide-to-tray action. |
| `failedLaunchPassed`       | The expected required-proxy launch failure completes cleanly.                                                                 |
| `applicationRemoved`       | Uninstall removes the application.                                                                                            |
| `dataPreserved`            | `%LOCALAPPDATA%\io.verisilo.app` remains intact after uninstall.                                                              |

If a future authorized run is performed, its `runtimeAcceptance` object must
contain only these verifier-bound fields: `os` (`name: "Windows 11"` and the
build), lower-case `installerSha256` and `packageManifestSha256`, UTC `executedAt`
ending in `Z`, the exact `steps` and `lifecycle` objects above, and `verdict`
matching the top-level status. The installer hash and package manifest hash
must be taken from this frozen artifact. That future report is outside this
Not Run closeout.

## Preservation boundary

The expected uninstall result is application removal with data preservation:
`%LOCALAPPDATA%\io.verisilo.app` remains intact so the same RC can reopen the
Vault and Silo data. Permanently delete a Silo from the application's archived-
Silo view before uninstalling only when deletion is the intended test outcome.

Do not report a successful local run as universal browser compatibility,
undetectability, CAPTCHA/payment bypass, network-path control, or a signed
outer installer.
