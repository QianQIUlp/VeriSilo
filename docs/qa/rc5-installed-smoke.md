# RC5 installed verification boundary

Source: `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`.
Installer SHA-256: `b1459b2305fe58e5d1370d591434d0e67ff14f7cadc0f9c56eacf414ca99aaca`.

The required automated integration matrix and release verification passed. All 14 package/audit attachments have matching local and GitHub SHA-256 digests. The installer completed in Windows Sandbox. Installed Desktop, CLI and Engine manifest bytes matched the actual installer payload.

**Installed Managed runtime acceptance remains inconclusive.** Attempt 1 stopped at an incorrect comparison between the loose Desktop and its NSIS-marked variant. Actual extraction and installed-byte checks proved only the expected three-byte Tauri bundle marker difference. Attempt 2 passed installation/hash checks but the harness waited after initial CLI bootstrap; a separate read-only CLI status returned success with an uninitialized Vault and no Managed browsers. No A/B runtime, repair or uninstall/reinstall pass is claimed for this RC. After the user requested that this remain a bounded artifact publication, further acceptance work stopped. Both test Sandbox instances were cleaned up.

Previously committed native Windows cold-backup/concurrency and UI evidence is inherited only for unchanged product code. The complete historical `windows-acceptance-report` remains `Pending`, `verified:false`, `runtimeAcceptance:null`. This pre-release does not prove strict unelevated standard-user installation, arbitrary website compatibility, or complete leak prevention. The engine is internally CMS-signed; Desktop/CLI/installer remain unsigned for outer Authenticode.