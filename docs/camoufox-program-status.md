# Camoufox Managed Engine status and evidence index

> Product-wide current status, phase and next task live in [program-status.md](program-status.md). This page keeps the Camoufox engine Gate history and evidence index.

- Status: **Camoufox engine Gate history and evidence index**. Since 2026-11-10 it is no longer the product routing page; the current phase and next task are on [program-status.md](program-status.md).
- Updated: 2026-09-28 (Camoufox facts); header updated 2026-11-10
- Current stable product branch: `codex/camoufox-m3-engine-adapter`
- Current canonical development source: `origin/baseline/dev` (the local working ref is `baseline/dev`; when everything is normal the two are exactly equal)
- Current public version: **v0.1.0-rc5** (`PUBLIC_GITHUB_PRERELEASE`; tag `v0.1.0-rc5`, release gate closed)
- `RC5_SOURCE_SHA`: `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`; this is the fixed release source, not the development baseline
- Current engineering phase: **post-rc5 normal product development**; new tasks branch from the latest `origin/baseline/dev`
- Historical rc2 candidate: source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`, installer SHA-256
  `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`; document changes do not change its source binding

This page keeps only the current facts, the next task and the key evidence index. Old checkpoints, failed runs, full hash tables and historical
wording are preserved by Git, lock/result files, evidence and the corresponding historical contracts. They are no longer appended permanently to this default must-read page.

For an ordinary Camoufox task, after reading this page, work from the "Current next task" section and the owning code/test. Read a separate active contract only when that section
explicitly names one. Read the
[north star](identity-platform-north-star.md) and the
[Camoufox-first decision](camoufox-managed-engine-decision.md) only when changing the product or architecture.

## Current product direction

VeriSilo delivers a persistent, replayable, verifiable browser identity environment:

```text
Silo = Persistent Profile
     + Resolved Identity Artifact
     + Engine Binding
     + Network Policy
     + Runtime Evidence
```

Standard Silo is kept long term. The Camoufox Managed Engine standalone, the qualification chain, the production package/
adapter, the Managed Silo product path, the current-user NSIS installer, rc2's packaged runtime and Windows
Sandbox installed acceptance, and the v0.1.0-rc5 public prerelease have each reached the boundary of its own documents. Profile,
Artifact, Engine, Network and Evidence are not merged, and `configured`, `applied`, `observed`,
`verified` and `unavailable` are not mixed.

The historical qualification and public release sequence from rc2 to rc4, and the source-bound release of rc5, are complete.
The rc5 installed evidence is classified only by its own separate bounded report:

```text
source stabilization → rc2 build → package verification → packaged runtime
→ pristine Windows Sandbox installed lifecycle → v0.1.0-rc3 → v0.1.0-rc4 public prerelease
→ rc4 release gate closed → post-rc4 product development → v0.1.0-rc5 public prerelease
→ rc5 release gate closed
```

The current phase is **post-rc5 normal product development**. The canonical development source is
`origin/baseline/dev`, and `main` is not advanced automatically because it is behind. The recent performance and responsiveness work is closed out, and it obtained bounded lifecycle evidence in a native
Windows Managed development instance. The Camoufox launch difference in the worktree was traced to an inherited-DACL development limit of the current
Codex sandbox/worktree, and the product path in a separate LocalAppData directory in the normal user context passed.
See the [performance task disposition](qa/performance-architecture-2026-09-27.md) and the
[directory ACL causal comparison](qa/managed-engine-directory-acl-2026-09-27.md). These results are not a general performance guarantee or an installed acceptance.
There is currently no product task branch waiting to be integrated, and no active fingerprint, QA or release Gate.

## v0.1.0-rc2 current-source installed acceptance (historical record)

The fixed identity of rc2 remains:

```text
releaseVersion = v0.1.0-rc2
RC_SOURCE_SHA = c1688d5a392ffa69ae77c246bcb4bb78b083e26f
installerSha256 = 3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3
```

The candidate package already has the current dual-boot Formal-v3 exact bindings, the current Host, a detached
CMS signature, an approved signer / public pin match, package-tree verification, release
provenance and SBOM/license evidence. The existing packaged Managed runtime completed:

```text
provision → hello → launch → page snapshot → page windows → status → close → shutdown
```

Response-ID correlation passed. The coverage boundary that the existing stale development package did not support `page` is closed.

Installed acceptance evidence:

- QA branch: `origin/agent/qa/v0-1-0-rc2-installed-69d6d6`
- evidence tip: `df1b82fe3ca2f071a4b6676adc92db046558cdd3`
- Environment: pristine/disposable Windows Sandbox, Microsoft Windows 11 Enterprise `10.0.26100`, AMD64
- Candidate transfer integrity, install, Desktop GUI, Vault initialize/lock/unlock, Standard
  create/first start/second start, Managed provision/package verification/first start/second
  start, Desktop restart persistence, same-version reinstall, Vault/Standard/Managed preservation,
  Managed post-reinstall start, uninstall, application binary removal and test-data preservation: **PASS**;
  `reinstallAfterUninstall=NOT_RUN_OPTIONAL`
- Final classification: `CURRENT_SOURCE_INSTALLED_PRODUCT_ACCEPTED_IN_WINDOWS_SANDBOX` / `CLEAN_DISPOSABLE_WINDOWS_SANDBOX_ACCEPTANCE_PASS`

The `WDAGUtilityAccount` used by the Sandbox has `IsAdministrator = true`. Strict standard-user
install/reinstall/uninstall semantics therefore remain **NOT_PROVEN**. This is not an installer lifecycle failure
and not rc2 acceptance pending. It is a future public-release/promotion boundary. The outer Desktop/NSIS
Authenticode is still unsigned. The internal engine CMS signature and the outer Authenticode are different boundaries.

During rc2 acceptance there was a known observation, `MANAGED_STOP_TRANSIENT_NETWORK_POLICY_MESSAGE`
(on the first Managed close / Direct stop, a proxy/network mismatch was briefly shown, classified
`LOW / NON_BLOCKING_UX_INCONSISTENCY`). It was fixed in the owning source afterwards and closed,
and it is no longer an open observation.

## M3 research conclusions

- M3-0 closed the fake Host package, transport, failure matrix, RuntimeManager lifecycle and evidence semantics at `e96ef3f`.
  It has no real browser run-id.
- The R2 ten-cycle real soak of M3-WI showed that the same Profile, Cookie and observed digest can stay stable in
  one controlled sequence, but the later Host matrix of the same Host/test source passed only once in six runs.
- The last R2H test-only candidate is `186484f` / tree `e33d6d6`. In the pre-declared sequence, persistence
  and lock-crash each passed once. The third item, persistence, failed after the second Host `launch` waited 120 seconds for the stdout
  response. There was no retry, no evidence manifest and no Accepted commit.
- The lead agent's final verdict for that historical investigation at the time was **M3-WI failed**. Camoufox Windows Managed
  productionization was paused at that checkpoint, and no R3/R4 or new test-only sub-Gate would be created. Later
  Formal-v3, FP1–FP4 and clean M3-WI Attempt 4 changed the current state. The current state at the top of this page governs.

## Standard Silo Windows preview first execution

- The lead agent's contract commit is `944dff9`. The product candidate is `b93259a` / tree `76eb5f3`, and it changed only the creation page,
  the UI contract test, styles and the manual Windows runbook.
- The candidate makes Edge-only/Chrome-only machines automatically choose the first valid browser, and converges the default path to
  Local + Direct. WSL, manual paths and network configuration move into advanced settings. The local stock Silo checks its state
  at short intervals while running and honestly displays `native`, `inherit` and `unavailable`.
- `pnpm` check/test/build, desktop Rust fmt/test and the unsigned desktop-only build passed.
  The two WSL Clippy warnings that already existed at the starting point were not fixed out of scope in this task. The existing Windows acceptance
  driver lacked `execution_target`; `5b09f04` only added `SiloExecutionTarget::Local` and it then compiled.
- The formal Edge/desktop-core acceptance did not start. The execution agent wrongly probed the version with a bare
  `msedge.exe --version`, and Edge actually started with the default Profile. Although it started from zero Edge
  processes before launch and reclaimed the whole new process tree by the exact root PID, there was no pre-launch Profile
  fingerprint, so whether the default Profile was modified is **unknown**.
- Lead agent Gate: **failed**. There is no desktop-core receipt, browser E2E summary or real preview
  smoke. The unsigned build must not be used as Accepted, a formal installer or a shipped product. There is no automatic
  retry, the user's Edge Profile is not read, and the environment as it was left is not deleted or rewritten.

## Standard/Profile Isolation Windows local Preview closure

- The final product checkpoint is `aa72eadaf8300d1cd33a2c32173c06e3e677ca89` / tree
  `cd126770be02a33c6bb698853813512748b894c8`. On native Windows Server, the source-bound
  desktop-core acceptance, the Edge A/B Profile and cold-start persistence, the before/after default Profile metadata
  check and the real Preview smoke passed. The historical default Profile impact recorded in the previous section stays
  **unknown**. The consistent metadata this time proves only that this run added no new impact.
- The deliverable is an **unsigned local Preview**, not a signed installer, shipped release or formal release.
  The verified platform is Windows Server. The normal Windows 10/11 client release matrix is still to be added.
- Standard/Profile Isolation contains only the independent Profile, website state persistence, single-active ownership and the local
  Chrome/Edge lifecycle. It does not contain Managed Identity, device or browser fingerprint virtualization, proxy isolation,
  WSL/Remote/Hyper-V or whole-machine virtualization.
- At that historical checkpoint, Camoufox was still under investigation in a separate Managed Engine worktree and did not enter this product integration chain. The Profile
  isolation layer is frozen from this checkpoint on and is not expanded except for real regression defects.

## Closed Gates and current release-readiness status

| Capability                                              | Current conclusion                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Standard Silo Windows Profile Isolation                 | **Closed**; the Windows Profile isolation, lifecycle and regression boundaries are closed, and Standard Silo is kept long term                                                                                                                                                                                                                                                                                                        |
| Linux M0–M2 standalone Host / Artifact                  | **Accepted**; only for the recorded platform, `verified:false`                                                                                                                                                                                                                                                                                                                                                                        |
| Native Windows M2-W                                     | **Accepted**; the Profile, Artifact replay and Job/process ownership Gates are closed                                                                                                                                                                                                                                                                                                                                                 |
| M3-0 EngineAdapter contract slice                       | **Accepted** at `e96ef3f`; a fake Host contract, not a shipped desktop                                                                                                                                                                                                                                                                                                                                                                |
| Historical M3-WI real Windows desktop/Host              | **Failed / Inconclusive**; the old contract is not revived, and the old test-only path stays historical/experimental and does not represent the current production path                                                                                                                                                                                                                                                               |
| FP1 deterministic Artifact projection                   | **Accepted by corrected adjudication of immutable A1/A2/B1 evidence**; the original runner verdict is still Failed, `verified:false`                                                                                                                                                                                                                                                                                                  |
| Historical FP2 candidate                                | **Failed / retired**; Generation 6 is permanently closed                                                                                                                                                                                                                                                                                                                                                                              |
| R1-diag Windows build/provenance                        | **Passed diagnostic-only closure**; not a Formal or runtime pass                                                                                                                                                                                                                                                                                                                                                                      |
| actual-9000 diagnostic run                              | Original runner Failed, offline adjudication Inconclusive                                                                                                                                                                                                                                                                                                                                                                             |
| Voices phase-anchor                                     | v1 Failed/no observation; the only v2 run directly supports A0→A1→A2                                                                                                                                                                                                                                                                                                                                                                  |
| Voices `0005` + Artifact v4 policy                      | **Static authoring closed**; only an input to the Formal source candidate, not a runtime pass                                                                                                                                                                                                                                                                                                                                         |
| Formal source + Windows-target build/provenance         | Formal-v3 **Passed build/provenance closure**; the exact runtime tree is bound and was used for native Windows qualification                                                                                                                                                                                                                                                                                                          |
| Formal R1 runtime / FP1-R1                              | **Formal R1 Passed on this native Windows host**; the FP1 carry-forward and Formal-v3 FP2 are both closed, `verified:false`                                                                                                                                                                                                                                                                                                           |
| FP2 / FP3                                               | **FP2 and FP3 both Passed on this native Windows host**; FP3 covers the exact required route, network exit, timezone/locale, Geo, ICE and clean lifecycle, `verified:false`                                                                                                                                                                                                                                                           |
| FP4 ordinary-site compatibility                         | **Passed on this native Windows host**; the exact V5 six tasks, Profile replay and clean lifecycle all passed, `verified:false`                                                                                                                                                                                                                                                                                                       |
| clean M3-WI                                             | **Passed on this native Windows host** at Attempt 4; the real Desktop RuntimeManager / test-only adapter / Host / Browser two cycles closed, `verified:false`                                                                                                                                                                                                                                                                         |
| production package/signing/UI/S1 stabilization          | **Implemented/build closed** in the current source; the internally CMS-signed package, the Desktop public pin, the production adapter, the Managed Silo UI/network/run path, the outer-unsigned current-user NSIS, and S1's Create Silo UX, reconcile-stopped lifecycle, BUG-01 and acceptance-driver fixes are in the current source                                                                                                 |
| Managed Font Isolation                                  | **Closed on this native Windows host**; production presets uniformly use `managed` mode, and the undeclared host font masking on Windows is verified twice, by a rendering oracle and by fail-closed static/runtime checks (commit `1b41b81`); cross-host glyph rendering consistency (cross-host hermeticity) is not claimed                                                                                                         |
| Window Geometry Coherence                               | **ACCEPTED_IN_WINDOWS_SANDBOX**; the source clamp and the real-browser four-scenario acceptance are complete, see [Packaged-Host acceptance](qa/packaged-host-sandbox-runtime-acceptance/STATUS.md); the old environment-blocked record is only the historical state of that time                                                                                                                                                     |
| Evidence Coverage Closure II (WebGL2 + Accept-Encoding) | **ACCEPTED_IN_WINDOWS_SANDBOX**; the real-browser WebGL2, request-header and Fresh Recheck reconciliation passed, see [Packaged-Host acceptance](qa/packaged-host-sandbox-runtime-acceptance/STATUS.md); the old harness-pending record has been superseded                                                                                                                                                                           |
| Transport Coherence                                     | **Observed Coherent**; the QA diagnosis (commit `94a3641`) directly observed on a real Managed session that the TLS ClientHello and HTTP/2 behavior are self-consistent with Firefox 152 NSS, and observed a QUIC v1 Initial handshake attempt; the formal verifier is still unavailable, and honestly no claim is made                                                                                                               |
| WebGPU Boundary                                         | **Policy Boundary Open / No Leak Observed** (classification `WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`); the pinned Camoufox 152 has no WebGPU spoofing primitive, `requestAdapter()` in a secure context returns `null` in this virtual / no-discrete-GPU environment, and no real hardware leak was observed; disabling it would produce the anomaly of a missing `navigator.gpu` and needs an underlying Gecko patch |
| Historical local `v0.1.0-rc1` artifact                  | **Historical candidate / superseded / never runtime-accepted**; source `6497828aa0643f94fed3ae708734eef6b85f8305`, dirty `true`, verifier passed for 1403 files, acceptance `Pending`, `verified:false`, `runtimeAcceptance:null`                                                                                                                                                                                                     |
| v0.1.0-rc2 historical candidate                         | **PUBLIC_GITHUB_PRERELEASE (superseded by rc5 as the current public version)**; this fixed candidate completed the post-install lifecycle acceptance in a pristine Windows Sandbox; source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`, installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`                                                                                                           |
| v0.1.0-rc3 public prerelease                            | **PUBLIC_GITHUB_PRERELEASE; release gate closed**; tag `v0.1.0-rc3`, source `407c501741c1be3444bfa607c7e693ecc7324409`                                                                                                                                                                                                                                                                                                                |
| v0.1.0-rc4 historical public prerelease                 | **PUBLIC_GITHUB_PRERELEASE; release gate closed**; tag `v0.1.0-rc4`, source `68e21c3d601e1df3699f1b21431cc23da873e546`                                                                                                                                                                                                                                                                                                                |
| v0.1.0-rc5 current public prerelease                    | **PUBLIC_GITHUB_PRERELEASE; release gate closed**; tag `v0.1.0-rc5`, source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`; separate bounded installed report: `INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`; the full old-contract `windows-acceptance-report.json` is still `Pending` / `verified:false` / `runtimeAcceptance:null`                                                     |
| Current source release readiness                        | **post-rc5 normal product development**; there is currently no active QA/release Gate; strict standard-user install/reinstall/uninstall semantics are still `NOT_PROVEN`, and the bounded RC5 install check must not be extrapolated to the full old-contract matrix or to all environments                                                                                                                                           |

## Currently unproven boundaries

- Production presets use `fontMode=managed`. The undeclared host font masking on Windows has been shown to take effect by the rendering oracle and fail-closed verification (commit `1b41b81`), but full cross-host glyph rendering consistency (cross-host hermeticity) is still not claimed (it still depends on the fonts available on the host and on the font fallback rules);
- Transport layer and anti-detection: the self-consistency of the TLS ClientHello, HTTP/2 characteristics and QUIC v1 has been directly observed in controlled tests (QA commit `94a3641`), but the formal transport verifier is still unavailable, and absolute "undetectable" is not claimed;
- WebGPU boundary: classified as `WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`. A secure context exposes `navigator.gpu`, and `requestAdapter()` returns `null` in the current environment, with no direct hardware leak. Disabling WebGPU would cause an anomaly of a missing API, and there is no spoofing primitive yet for WebGPU adapter info;
- FP3 does not prove Camoufox's native Geolocation provider or an exhaustive native address inventory;
- FP4 covers only the frozen V5 live-site matrix and does not claim universal compatibility. Login, payment and CAPTCHA were not tested;
- The existing Attempt 4 of clean M3-WI still proves only the test-only adapter path of that time. rc2 separately accepted the current-source package/adapter, the exact runtime bindings and the post-install lifecycle directly in a pristine
  Windows Sandbox,
  but this is not extrapolated to all Windows hardware/editions or to a strict non-admin user path;
- The `WDAGUtilityAccount` used by the rc2 Sandbox has `IsAdministrator = true`. Strict unelevated
  standard-user install/reinstall/uninstall semantics are still `NOT_PROVEN`. This is a future public-release /
  promotion boundary, not an installer lifecycle failure;
- The outer Desktop/NSIS of both rc2 and rc3 is `authenticode=false` / unsigned. The internal engine detached CMS
  signature, the approved signer / public pin and the outer Windows Authenticode are different boundaries. The signing status of these historical candidates
  does not automatically prove the signing status of rc4, rc5 or future candidates;
- The Formal-v3 runtime observation covers only the candidate/Artifacts that rc2 bound in this Sandbox.
  Voices covers only the three-second top-window traces of A1, A2 and B1, and is not an exhaustive exclusion. FP4 and the existing installed acceptance
  still do not claim universal site compatibility, undetectability, login/payment/CAPTCHA, the complete TLS ClientHello,
  the complete QUIC or the exhaustive browser DNS path.

## Current next task

This round of normal product development completed three capabilities: unified interpretation and difference localization of the current identity / network evidence, confirmation of the resolved result after Managed creation, and one encrypted recent-run record for each local Standard/Managed Silo. A recent record always means history / the last known state. It does not provide current runtime verification, and the match, unable-to-verify and expired boundaries stay independent. For the implementation scope and the composed verification results, see the [product integrity slices record](product-integrity-slices-2026-09-28.md).

[Managed Silo cold backup and original-identity restore v1](managed-silo-cold-backup-v1.md) was completed next. After a stop, the Profile, the original Artifact, the identity/engine bindings and the necessary network configuration are saved together in encrypted form, and the original identity is restored through an inspected digest and an explicit overwrite confirmation. The scope is limited to the same Windows, the same system user, the same Vault path and a compatible engine. The original Silo metadata must exist, and if it is missing, the Vault configuration backup is restored first. The [native development acceptance of the cold backup](qa/managed-cold-backup-2026-09-28.md) directly shows that A's persistent Cookie, LocalStorage and IndexedDB read back their original values after being overwritten and restored, that the Artifact binding is kept and that B is not affected. A wrong passphrase / digest is rejected. After a restore, current evidence is obtained again, and an old record cannot be treated as current verification.

The [local Managed Silo bounded concurrency v1](managed-silo-concurrency-v1.md) authorized by the user afterwards is now implemented. One instance and one Vault allow at most two local Camoufox Managed sessions, and each session manages its own process, Profile, proxy relay, health, runtimeId, evidence and recovery record. Direct and the existing fixed-proxy combination are supported. Other engines, run locations and Clash/Mihomo keep the single-session boundary. The [native evidence](qa/managed-concurrency-2026-09-28.md) covers same-site storage isolation of A/B, independent re-observation / stop / restart, cold restore of A while B is running, local fixed-proxy routing and fail-closed on one-sided failure, locking, and attribution after quit / restart. UI Preview covers targets, independent busy states, late responses and errors. A real window was used to check the P/Q selection and the dual-running display. The 90-second automatic runtimeId after-the-fact comparison timed out and is still recorded as unconfirmed, see the [UI record](qa/managed-concurrency-ui-2026-09-28.md). The local proxy experiment does not support deriving the public network exit or complete leak-freedom. These development changes did not enter the rc4 installer. The fixed rc5 candidate contains these capabilities.

The [RC5 release](acceptance/managed-browser-rc5.md) authorized by the user on 2026-09-28 is complete. Starting from `86ee698e5d7d7838b29478625be71a0d905885c6`, it froze the candidate source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027` and published a public prerelease. The separate bounded installed report records `INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`. For its scope see the [RC5 smoke procedure](qa/rc5-installed-smoke.md). The full old-contract `windows-acceptance-report.json` stays `Pending`. There is currently no active RC Gate, fingerprint Gate or known product code branch waiting to be integrated. The next product task has not been specified.

This RC5 inherits unaffected product evidence and adds only a bounded install check for the actual candidate. By default, do not rerun FP1–FP4, open FP5, continue the ACL deep dive on the current Codex worktree, or rerun the completed performance / Managed lifecycle acceptance. If a similar launch failure appears in the future in a normal product directory and a normal user context, collect native evidence for that exact environment. The old worktree failure is not a current product blocker.

The recent performance and responsiveness work is complete. The [disposition and native Windows development lifecycle evidence](qa/performance-architecture-2026-09-27.md) records Managed creation at 2.952/1.136 seconds, a Vault read of 25.9 ms during creation, two starts at 25.171/13.920 seconds, reobserve at 1.873 seconds, stop at about 1 second, plus page ready, 122-second survival and 0 final owned processes. These are bounded development run evidence, not a general performance guarantee or an rc5 installed acceptance. The [directory ACL comparison](qa/managed-engine-directory-acl-2026-09-27.md) shows the causal role of the inherited DACL in the launch failure of the current worktree. The specific ACE, Windows API error or deeper mechanism has not been identified.

rc2/rc3/rc4/rc5 each have a fixed source binding. They are not refs that move with development. The rc5 public prerelease is published and its release gate is closed. A new release Gate is opened only by an explicit user instruction.

## Historical qualification chain and subsequent release path

```text
Formal-v3 build/provenance
→ FP1 deterministic Artifact projection
→ FP2 cross-realm consistency/replay
→ FP3 network/geo/timezone/locale coherence
→ FP4 bounded ordinary-site compatibility
→ clean M3-WI Attempt 4 native qualification
→ production package/signing/adapter + Managed Silo UI + current-user NSIS
→ v0.1.0-rc2 source-bound freeze
→ package verification + packaged runtime
→ pristine Windows Sandbox installed acceptance
→ v0.1.0-rc3 → v0.1.0-rc4 public prerelease (release gate closed)
→ post-rc4 product development → v0.1.0-rc5 public prerelease (release gate closed)
→ post-rc5 normal product development (canonical: origin/baseline/dev)
```

The first part is the closed qualification chain. The middle part records the source-bound candidate of rc2, its package/runtime and the
pristine Windows Sandbox installed acceptance. After that come the rc3, rc4 and rc5 public prereleases and the current post-rc5 development phase.
rc2, rc3, rc4 and rc5 are all `PUBLIC_GITHUB_PRERELEASE`, and the existing acceptance did not prove strict standard-user semantics.
This record is not a new research Gate, and it does not turn the historical RC1 into the current candidate.

## Key evidence index

| Object                                               | Current anchor                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| upstream Camoufox                                    | tag `v152.0.4-beta.28`; commit `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`; tree `1435d544d9b61dee7fcf74cf92462952ca43d38e`                                                                                                                                                                                                                                                                                                  |
| Firefox source                                       | `799102676` bytes; SHA-512 `0c5662aba8fb897902af95dbb2fd988b196d9cf9ae8b987ae89e0a6492ac753b8d4b8bb7b3274909c2eb200ab098df356e23cd6084556467f55e69127317f39a`                                                                                                                                                                                                                                                               |
| R1-diag closure lock                                 | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json`; SHA-256 `6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b`                                                                                                                                                                                                                                                             |
| diagnostic ZIP                                       | SHA-256 `241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d`; diagnostic only                                                                                                                                                                                                                                                                                                                                 |
| frozen 9000                                          | SHA-256 `1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`; `formalCarryForward=never`                                                                                                                                                                                                                                                                                                                      |
| Formal `0005` static candidate                       | patch SHA-256 `998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec`; parent pre/post `c6171e…` / `c43447…`                                                                                                                                                                                                                                                                                                     |
| Formal-v3 source/recipe lock                         | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json`; SHA-256 `a32cf21852909be6ed4a3a4b10dec9310533908996dd73e465535e262f61bc53`; static candidate                                                                                                                                                                                                                                         |
| Latest Windows-target build result                   | Formal-v3 result lock SHA-256 `4eeffbf1dc505c743871a90510f81854243f48fc9abffc4fd1459079cab3b631`; ZIP SHA-256 `032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5`; bound to the FP2 runtime evidence                                                                                                                                                                                                         |
| FP2 Attempt 8                                        | run `fp2-20260827T082048Z-9a7821e264`; report SHA-256 `86f0ae525925809757456c11fec33b5c7a20a4d6fa00d686bda903f75ca1cc53`; immutable Failed at native-DNT harness mapping; launch/Search/MediaDevices/Voices discriminator passed                                                                                                                                                                                            |
| FP2 Attempt 9                                        | run `fp2-20260827T084257Z-c9d6dcc498`; report SHA-256 `590e90cb20a7c9a1341fb36a03c9a04bf7a0c36b034717fadd830d952d4339a3`; A1/A2/B1 phases passed, immutable Failed at post-sequence storage harness semantics                                                                                                                                                                                                               |
| FP2 Attempt 10                                       | run `fp2-20260827T090954Z-7a85050695`; report SHA-256 `d14bf5f2881ce1c48ec49cf0ba1184b940d61013a462f56218fb1569d873455b`; A1/A2/B1 execution passed, the runner keeps the awaiting-main-brain boundary                                                                                                                                                                                                                      |
| FP2 Formal-v3 aggregate result                       | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp2-result.json`; SHA-256 `caa5ed4005c3e9c392c76a5d264d3d7d4d30cb741ac675fd27803c7f5fa06fa6`; **Passed on this native Windows host**; `verified:false`                                                                                                                                                                                             |
| FP3 Attempt 7                                        | run `fp3-20260828T024057905465Z`; report SHA-256 `697a190ff485814a3f310cf3977792698e9ac2aaa2bcbae625bbbf7797acc25d`; required route, network exit, Geo, ICE and lifecycle all passed                                                                                                                                                                                                                                        |
| FP3 Formal-v3 aggregate result                       | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp3-result.json`; SHA-256 `8a821eca7b9e11716668d6742ac356743b7438ab2b9a7ca8b0d604264be86e62`; **Passed on this native Windows host**; `verified:false`                                                                                                                                                                                             |
| FP4 Attempt 13                                       | run `fp4-853f5fe2c6ad4238ac76776f3668f163`; report SHA-256 `de69f0083f7babfdfae5d3d1887fbf18e22e711981e2ca26e9f79e89bcc9e6a7`; the V5 six tasks, Profile replay, bindings and lifecycle all passed                                                                                                                                                                                                                          |
| FP4 Formal-v3 aggregate result                       | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp4-result.json`; SHA-256 `14c7de3a8a14b8037cf0e16ec7b5dc213294b68050665a57513dea79efd8f2de`; **Passed on this native Windows host**; `verified:false`                                                                                                                                                                                             |
| clean M3-WI input contract                           | `docs/camoufox-m3-wi-clean-contract.md`; SHA-256 `acdc725dbbb1ccb0c39571cea43f6eb7ef3137429f4f8b256ec764f3be20af74`; Attempts 1–3 immutable Failed, Attempt 4 Passed                                                                                                                                                                                                                                                        |
| clean M3-WI Attempt 4                                | `artifacts/camoufox-m3-wi-clean-attempt-4/run-report.json`; SHA-256 `edd08b83497e09a73a0a0e29203475f1e9163b20366b2dd7c899aea8634262fe`; native evidence SHA-256 `2f292585a010dbdc3cad35bfcf26b14800bad402ed4a160c5123f41005c972ad`; revision `26ded609bf5bf52882c9ba37496f783ab2b01681`; **Passed on this native Windows host**, `verified:false`                                                                           |
| Historical RC1 release artifact                      | `artifacts/release/managed-browser/v0.1.0-rc1`; source revision `6497828aa0643f94fed3ae708734eef6b85f8305`; source dirty `true`; installer SHA-256 `ea1108e7623118df6b45b7ceb570a4481fc3a030c32b64747395551efd7ce7df`; verifier `Managed-browser release verification passed for 1403 files.`; acceptance `Pending`, `verified:false`, `runtimeAcceptance:null`; historical candidate, superseded, never runtime-accepted   |
| v0.1.0-rc2 source-bound candidate                    | `PUBLIC_GITHUB_PRERELEASE` (superseded by rc5 as the current public version); source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`; installer SHA-256 `3e7c9158c7f41520984c6e16e54725d60c7e5d1e6313a3bbaf39384af0415cb3`; installed acceptance completed within its fixed boundary; outer Authenticode unsigned, the internal engine CMS signature / public pin are separate                                                   |
| v0.1.0-rc2 Windows Sandbox installed acceptance      | QA branch `origin/agent/qa/v0-1-0-rc2-installed-69d6d6`; evidence tip `df1b82fe3ca2f071a4b6676adc92db046558cdd3`; Windows 11 Enterprise `10.0.26100` / AMD64 / pristine disposable Sandbox; `CURRENT_SOURCE_INSTALLED_PRODUCT_ACCEPTED_IN_WINDOWS_SANDBOX`; strict standard-user semantics `NOT_PROVEN`; the `MANAGED_STOP_TRANSIENT_NETWORK_POLICY_MESSAGE` observation during acceptance was closed by a later source fix |
| v0.1.0-rc3 public prerelease                         | tag `v0.1.0-rc3` (commit `407c501741c1be3444bfa607c7e693ecc7324409`); `PUBLIC_GITHUB_PRERELEASE`; rc3 release gate closed; the strict standard-user and outer Authenticode boundaries are unchanged                                                                                                                                                                                                                         |
| v0.1.0-rc4 historical public prerelease              | tag `v0.1.0-rc4` (commit `68e21c3d601e1df3699f1b21431cc23da873e546`); `PUBLIC_GITHUB_PRERELEASE`; release gate closed                                                                                                                                                                                                                                                                                                       |
| v0.1.0-rc5 current public prerelease                 | tag `v0.1.0-rc5` (commit `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`); `PUBLIC_GITHUB_PRERELEASE`; bounded installed report `INCONCLUSIVE — installed files matched; Managed runtime/repair/uninstall flow not completed`, the full old-contract report is still `Pending`; release gate closed                                                                                                                              |
| Recent performance and Managed development lifecycle | [performance task disposition](qa/performance-architecture-2026-09-27.md); the bounded lifecycle in a separate LocalAppData development path on native Windows passed; not a universal performance / installed-candidate claim                                                                                                                                                                                              |
| Codex worktree inherited-DACL limitation             | [directory ACL causal comparison](qa/managed-engine-directory-acl-2026-09-27.md); 504 files with identical hashes, the same executable/owner, and after changing only the DACL a 60-second failure turned into a page created in 5.344 seconds; not a confirmed product blocker                                                                                                                                             |
| Surface Truth Matrix audit                           | QA branch `origin/agent/qa/fingerprint-surface-52ebde`; commit `96543d74e34089c8d3f0b6199aaa974a2cff9922`; four-layer capability audit, baseline `0d74290`, zero CONTROL_GAP                                                                                                                                                                                                                                                |
| Transport coherence diagnosis                        | QA branch `origin/agent/qa/transport-fingerprint-d96e42`; commit `94a364143f68f270597f86baf9ce735573632aff`; TLS ClientHello / HTTP2 / QUIC v1 observed self-consistent, `TRANSPORT_COHERENCE_NO_CONTROL_BLOCKER_FOUND`                                                                                                                                                                                                     |
| Managed Font Isolation                               | commit `1b41b812f3d10bdb2dc6543ea4a740a665f8d7c9`; `fontMode=managed` became the production preset, and undeclared host font masking on Windows was verified by the oracle                                                                                                                                                                                                                                                  |
| Window Geometry Coherence                            | `ACCEPTED_IN_WINDOWS_SANDBOX`; `provision_artifact.py`, `generate_identity.py` and `host_runtime.py` enforce the virtual screen bounds clamp; the real-browser four-scenario acceptance passed, see `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md`                                                                                                                                                            |
| WebGPU Boundary conclusion                           | Ruled in this task; `WEBGPU_NO_LEAK_OBSERVED_BUT_POLICY_BOUNDARY_STILL_OPEN`; no hardware leak observed, the policy boundary stays open                                                                                                                                                                                                                                                                                     |
| Evidence Coverage Closure II                         | `ACCEPTED_IN_WINDOWS_SANDBOX`; the real runs of WebGL2, Accept-Encoding, Managed font masking and the Fresh Recheck Sentinel reconciliation all passed, see `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md`                                                                                                                                                                                                    |
| Packaged-Host Windows Sandbox Acceptance             | `CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`; based on the current baseline's self-contained Dev Engine Package, the 4-stage real-browser acceptance matrix and process cleanup were completed in a healthy Windows Sandbox, see `docs/qa/packaged-host-sandbox-runtime-acceptance/STATUS.md`                                                                                                         |
| FP1-R1 carry-forward result                          | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp1-r1-result.json`; SHA-256 `a4f0ef539ee09925d7715e6bfea1cbd74dde74ff62dac26f619ab56dbae5b197`; report `f05f2fd…`; claim `b1a37e60…`; this native Windows host only                                                                                                                                                                               |
| FP2 attempt 1 result                                 | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-r1-result.json`; SHA-256 `bd91dff1a324cfdd3e6241aa5a61a59e0b64597e8ca173ff8d6a64374d309a24`; immutable Inconclusive                                                                                                                                                                                                                            |
| retired Formal-v1 FP2 aggregate                      | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-fp2-result.json`; SHA-256 `540472a6f33f2426fc66a6a1d0ea722356b259a8e315b19b10b445d813f045db`; attempt 2 immutable Failed                                                                                                                                                                                                                           |
| final Voices design checkpoint                       | `594d16700c7d8f5d169eaac6cf6fd62d5a12df49`                                                                                                                                                                                                                                                                                                                                                                                  |

The raw machine evidence was once kept in the local `artifacts/` directory. That directory is not in Git
(gitignored) and has been cleaned up locally, so the anchors in the table above that point to `artifacts/…` cannot be resolved from the repository.
The only durable copies are the locks/results under `apps/camoufox-host/lock/` and the
revisions in Git history. This status page no longer copies the SHA/size table of every file.

## Historical index

Read only when investigating the corresponding facts:

- [FP1 historical contract/evidence](camoufox-fp1-deterministic-artifact-projection-task.md)
- [FP2 generation history](camoufox-fp2-cross-realm-consistency-task.md)
- [R1-diag durable builder history](camoufox-r1-diag-durable-builder-evidence-contract.md)
- [actual-9000 / phase-anchor execution history](camoufox-fp2-r1-diagnostic-execution-task.md)
- [M3-WI failed investigation](camoufox-m3-wi-windows-task.md)

## Update rules

When a Gate changes, the status page **replaces** the current Gate, the next task and the necessary evidence index, and no longer appends full historical sections.
Historical accuracy is carried by Git commits, immutable claims/results, locks/manifests and the historical documents above.

An ordinary status update does not require a full regression or recomputing stable artifacts. Check only the changed references and the directly related facts.
Browser / build / product claims are still handled under the L2/L3 rules of the [Agent operating model](agent-operating-model.md).
