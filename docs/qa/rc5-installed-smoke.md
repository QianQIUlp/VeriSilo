# RC5 exact-candidate installed smoke

Status: **not run**. This page records the bounded RC5 installer check; it is not the historical complete `windows-acceptance-report.json` contract. That report stays `Pending`.

Run from the frozen RC5 source worktree, after the signed Engine package and NSIS candidate have passed release verification:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/windows/Invoke-VeriSiloRc5Installed.ps1 -SelfTest
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/windows/Invoke-VeriSiloRc5Installed.ps1 `
  -ReleaseDirectory artifacts/release/managed-browser/v0.1.0-rc5 `
  -ExpectedSourceSha <full-frozen-commit-sha>
```

The runner hashes the exact installer, Desktop and CLI executables, and Engine manifest against the candidate checksum list and checks the provenance revision. It stages the installer in a unique temporary directory, maps input read-only and evidence writable into one fresh Windows Sandbox, then writes `artifacts/qa/rc5-installed-<random>/result.json`. It does not stop other Sandbox instances, change host ACLs, or overwrite earlier evidence. If the guest cannot read or write its mapped folders, the run is blocked; do not reuse an unchanged failed attempt.

The guest requires an empty product install/data path and uses a fresh named Vault. It installs the NSIS package, checks the installed Desktop, CLI, and Engine hashes, starts two Managed Direct Silos and rejects a third, then checks separate Profile paths and synthetic same-site Cookie and localStorage state through the installed CLI. After stopping A, it confirms B remains running with its own state, then stops B. It restarts the service, repairs the same version, uninstalls while preserving the encrypted Vault bytes, reinstalls, reopens the Vault and reads both Profiles again. It reports cleanup of owned product processes. No Node, repository source, real account, proxy credential, or personal browser Profile is used in the guest.

Prior native Windows concurrency evidence from source `043821b69699ecff6c739175f25607b37709b49f` remains applicable where frozen RC5 Desktop/Host product code is unchanged: IndexedDB isolation, independent reobservation, network routing/fail-closed behavior, cold recovery, lock, and restart attribution are not rerun here. This installed smoke covers packaging and lifecycle at the exact new candidate. A Windows Sandbox `WDAGUtilityAccount` is an administrator, so a pass does not prove strict unelevated standard-user install, repair, or uninstall semantics. It also does not claim the full historical proxy and lifecycle acceptance matrix or promote the complete report from `Pending`.
