# Managed Browser RC5 release scope

The user explicitly opened a new RC and public release task on 2026-09-28.
The integration task starts at canonical `origin/baseline/dev`
`86ee698e5d7d7838b29478625be71a0d905885c6`. The candidate is
`v0.1.0-rc5`; its immutable source revision is recorded in the generated
provenance and release tag after the source is frozen.

## Product and package scope

RC5 includes the completed cold backup and bounded local Managed concurrency
work. At most two local Camoufox Managed Silos may run in one desktop/Vault,
using Direct or the supported fixed proxies. Other engine/location combinations
and Clash/Mihomo retain their single-session boundary. Profile, network, runtime
evidence and lifecycle ownership remain per Silo.

The release directory additionally exposes the existing `verisilo-cli.exe` as
an audited standalone asset. Tauri's existing Cargo binary bundling installs it
next to the desktop executable; the installed smoke must verify those bytes and
the documented local API workflow without a source checkout. No new API or
runtime dependency is introduced. Companion and Native Host are outside this
installer profile.

Use the fixed Formal-v3 Camoufox `152.0.4-beta.28` browser tree. Rebuild only the
locked Host one-folder package to bind its provenance to the frozen candidate;
do not rebuild the browser kernel or relabel an old Host receipt. Sign with the
existing release certificate SHA-256
`57f3b44cf572571e8b133c6b605b061e0d1c4d9dd75a490b14f658c292bebd93`.
The encrypted PFX stays outside the repository and its password is entered only
in a local hidden-input prompt. Desktop, CLI and installer remain unsigned for
outer Authenticode; the internal CMS signature is a separate boundary.

## Verification and evidence

Run the required integration full matrix once for this RC/configuration change.
After a fix, repeat only the affected checks. Verify the exact signed package,
release shape, checksums, provenance, license evidence and installer contents.

Inherit unaffected product evidence from
[cold backup](../qa/managed-cold-backup-2026-09-28.md),
[native concurrency](../qa/managed-concurrency-2026-09-28.md) and
[UI concurrency](../qa/managed-concurrency-ui-2026-09-28.md). These remain
development/runtime evidence, not installed RC5 evidence. No new fingerprint
qualification or complete proxy/recovery matrix is required without a relevant
change or contradictory result.

Use a fresh disposable Windows Sandbox for the bounded installed smoke. Do not
install over the user's real VeriSilo, stop unrelated Sandbox sessions, change
global ACLs, or overwrite historical evidence. Bind its report to source,
installer hash and engine manifest hash. Check the installed adjacent CLI and
real backend, two Managed sessions with isolated same-site storage, third-session
rejection, independent stop/restart, same-version repair, uninstall preserving
data and reinstall/reopen. Clean up only test-owned processes and data. Record
failures as failures; changes to the harness require a new identified attempt.

The historical `windows-acceptance-report` schema contains a single-active-Silo
check incompatible with the accepted two-session product. Leave that complete
historical report `Pending`, `verified:false`, `runtimeAcceptance:null`; do not
reinterpret it as passed. Publish the separate bounded RC5 installed report and
its actual coverage. Sandbox administrator execution does not prove strict
unelevated standard-user semantics. No result implies public proxy exit identity,
complete leak prevention, arbitrary website compatibility or universal login
survival.

## Delivery

Publish an immutable source-bound `v0.1.0-rc5` GitHub prerelease with verified
assets, checksums, provenance and explicit evidence limits. Do not replace rc4,
force-push, advance `main`, or create a stable product release. Publish the task
branch and advance canonical baseline through the existing integration workflow.
Update the public download references and release status after publication.
