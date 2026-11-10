# Managed Artifact P1 handoff document

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

Updated: 2026-09-08. Status: **product acceptance complete, fix in effect** (see “Product acceptance results” at the end). The first half is the snapshot taken at handoff time and is kept unchanged.

## Worktree and authorization boundaries

- Primary checkout: `C:/Users/qiu/src/VeriSilo`
- This task's worktree: `C:/Users/qiu/src/VeriSilo/.verisilo-worktrees/host-managed-artifact-p1-1ba11b`
- Branch: `agent/host/managed-artifact-p1-1ba11b`; lane: `host`
- Baseline at start and at handoff: `4b3163de471b064a266fac31e79b949b10edd36d`, from `refs/heads/baseline/dev`.
- To continue this task, enter the worktree above directly. Keep the existing fix and evidence. Do not run start again for the same task.
- User authorization: locate the exact cause of Managed create → ArtifactIntegrityError, make the smallest safe fix, accept it on a real isolated Vault, and commit. Do not push, do not advance the baseline, do not start the next QA round, and do not build an installer/RC.
- Do not skip the integrity check, loosen the schema/hash, fake the Artifact/Host, change the FP contract, rebuild the architecture, or re-solve package discovery.
- `AGENTS.md`, `docs/agent-task-routing.md`, and `docs/development-worktrees.md` have been read. Ponytail full is in effect.

## Exact root cause located, and the fix

`apps/camoufox-host/provision_artifact.py::apply_identity_overrides` changes the generated config's screen width/height and availWidth/availHeight to the preset size, but keeps the non-zero availTop/availLeft that BrowserForge generated originally.

The actual failing sample:

| Field/constraint                 | Expected                                     | Actual before the fix |
| -------------------------------- | -------------------------------------------- | --------------------- |
| screen.height                    | 800                                          | 800                   |
| screen.availHeight               | 800                                          | 800                   |
| screen.availTop                  | 0 (origin of the full-screen available area) | 4                     |
| availTop + availHeight <= height | <= 800                                       | 804 > 800             |

The first inconsistency occurs **after complete_resolved_config finishes, when apply_identity_overrides rewrites the size**. The earlier complete_resolved_config had clamped the offset to the size at generation time. Changing the size afterward invalidated that constraint.

`validate_artifact_strict` then rejected it and raised:

`ArtifactIntegrityError: strict validation failed: screen avail geometry inconsistent`

At this point no JSON/sidecar had been published, let alone returned to Rust or written to the Vault. So this sample is not a mismatch of the raw SHA, canonical digest, Artifact ID, schema, or Formal-v3 binding.

The two-line fix: after setting availWidth/availHeight, also set `screen.availLeft = 0` and `screen.availTop = 0`. The two existing callers (initial provisioning and the overrides after a v6 rebind) are covered together. The verifier, the format, the hash algorithm, the ID algorithm, the signature pin, and the FP contract were not changed. Fail-closed is kept.

Files changed:

- `apps/camoufox-host/provision_artifact.py`: the two-line fix.
- `apps/camoufox-host/test_package_contract.py`: a geometry regression assertion for non-zero top/left + a full-screen size, added to the existing dependency-free checks.
- This handoff document.

Historical evidence: `git show c21db17 -- apps/camoufox-host/provision_artifact.py` shows the post-hoc size override was added in that baseline WIP checkpoint. Before it, the generation chain clamped the offset only to the original generated size. The current canonical CLI runs fresh provisioning after discovery truly passes, and the randomly generated non-zero offset is what exposed this regression. The FP fixed Artifact / old generation path passing cannot cover this post-hoc override. Do not claim that all old Gates have been verified again.

## Actual reproduction and local verification

Python runtime (read only as an installed dependency; its code/dependencies were not changed):

`C:/Users/qiu/src/VeriSilo/apps/camoufox-host/.venv/Scripts/python.exe`

The old development package (an existing resource; do not overwrite or delete it on handoff):

`C:/Users/qiu/src/VeriSilo/.verisilo-worktrees/core-managed-cli-development-d1ebfa/apps/desktop/src-tauri/target/verisilo-managed-browser-resources/engine-package`

- With the current source unfixed, calling the real provision_artifact with preset balanced-zh-cn, window [1280,800], direct, and test seed `[5]*32`, one actual sample recorded availTop=4 and printed the exact error above.
- The old packaged Host also failed through the real length-prefixed provisioning protocol: `artifacts/p1-packaged-failing-seed/response.txt`, `state/host-stderr.log`. Input seed `[5]*32`, preset balanced-zh-cn, window [1280,800], followNetwork=false. exit 1, and the response contains the specific error above.
- A seed cannot be taken as a guarantee that a new process reproduces the same BrowserForge sample: another new process with the same seed generated a different screen. The regression test pins the non-zero offset directly, so it stably covers the confirmed owning seam.
- After the fix, real source provisioning succeeded. Calling it repeatedly with the same artifact root confirmed that the stored Artifact is identical. `verify_artifact_raw` and `verify_browser_binding` passed.
- The actual successful Artifact is in `artifacts/p1-source-fixed/identity/`, and the result is in `artifacts/p1-source-fixed/result.json`:
  - ID: `identity-c1ba5bbd1f443edbd41ee4fa`
  - schema: `verisilo-camoufox-resolved-identity/v5`
  - raw SHA: `20e8498291d7eaa2ddc0e49000d622b022cd362125abab40d81fc0100b7f7eb7`
  - configured digest: `sha256:211389bfa41cce6483b1cb96228ebdc79a0d4224af6b8792d36680dfb685d801`
- An object with corrupted geometry is still rejected by the strict verifier. Appending a space to the end of the real Artifact's raw file, while keeping the old expected SHA/sidecar, is also rejected by the raw SHA verifier. The damaged sample: `artifacts/p1-source-fixed/damaged.json`.
- The above is local evidence from the actual Host/Artifact. It **cannot replace the Managed CLI create / Vault persistence / start acceptance**.

## Tests / build / check status

Passed:

- `node C:/Users/qiu/src/VeriSilo/scripts/agent-task.mjs verify`: the host lane's package contract + page command tests all passed.
- `cargo build --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --bin verisilo --bin verisilo-cli`: an independent debug build in this worktree succeeded; log `artifacts/p1-desktop-build.log`.
- The new Host PyInstaller 6.22.2 build succeeded. The new package contents and the Formal-v3 binding passed `recheck_formal_package`; see the next section.
- `git diff --check`, the final diff inspection, the host scope check, and the primary checkout contamination check for the two product changes passed. A document diff/scope check will be done again after this document is added.

Additionally ran `test_identity_artifact.py`: 57/59. The full pass/fail output is in `artifacts/p1-identity-tests.log`.

Two unresolved items:

1. `test_fp1_fake_stage_timeout_crosses_protocol_and_fail_closed_cleanup`: an empty AssertionError. Not located further, and the old FP Gate was not rerun.
2. `test_self_built_launch_rechecks_the_pinned_tree_before_profile_use`: the test expects `verify_tree_contents=True`, but the current implementation calls it with `False`.

Neither the matching test nor the owning runtime file was changed in this task, so it cannot be claimed that this suite is all green, and the failures were not removed by changing a test or a security boundary. These two items have not been run separately on an unmodified baseline. "Unmodified" must not be written as "independently proven that the baseline fails the same way".

## New development package and the signing blocker

The new package:

`apps/desktop/src-tauri/target/verisilo-managed-browser-resources/engine-package`

- Only the Host built from this task's source was swapped in. The Formal-v3 browser, supervisor, probe, and asset/tree bindings come from the existing development package above.
- Local build script: `artifacts/p1-build-package.py`; log: `artifacts/p1-package-build.log`; PyInstaller output: `artifacts/p1-pyinstaller/`.
- The first attempt with the formal builder stopped because the required raw runtime-asset input files are not on this machine, and no package was produced. Afterward, the frozen binding of the existing package was used to rebuild the Host/package tree, and `recheck_formal_package` was used to check the final new package.
- The existing old package has an extra `browser/version.json`; the other package-tree members have nothing missing or modified. browser_tree.py explicitly lists version.json as a runtime extra. The new package was copied without this generated file, and neither the verifier nor the old package was changed.
- Note that the Host cache uses a junction, and at runtime seed_camoufox_cache may generate version.json in the browser root. Do not widen checking or cleanup to other worktrees, and do not delete existing evidence by mistake. The state of the old package's version.json before the call was not recorded, so its creator cannot be asserted.
- **At the time of the handoff check the new package was still unsigned**: keyId is all zeros. It cannot pass the production adapter. This is a blocker that must be cleared before the Managed product acceptance can be completed.
- production pin: `57f3b44cf572571e8b133c6b605b061e0d1c4d9dd75a490b14f658c292bebd93`.
- PFX: `C:/Users/qiu/.verisilo-signing/engine-package-rc1.pfx` exists. The CurrentUser/My enumeration is empty. `VERISILO_CAMOUFOX_PFX_PASSWORD` was not provided at Process, User, or Machine scope. The passphrase was not requested, read, or printed. The pin was not changed, and no new signing identity was created.

The command for the user's local terminal has already been given to the user. The user has not yet replied “已签名” ("signed"). **Do not ask for the passphrase through the chat tool.** Have the user run it only in their own PowerShell terminal:

```powershell
& 'C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\host-managed-artifact-p1-1ba11b\artifacts\p1-sign-package.ps1'
```

This wrapper calls the repository's existing sign-camoufox-host-manifest.ps1 and takes the passphrase as hidden local input with Read-Host -AsSecureString. Do not run an interactive prompt in the agent background and expect the user to be able to type into it. After signing, the user only needs to reply “已签名” ("signed"). Then check the actual manifest/signature. Do not infer success from the reply.

`artifacts/` is a locally ignored directory and is not copied with commits. On the current machine, just reuse the worktree. Do not delete these artifacts awaiting acceptance.

## Current real QA Vault / process state

- Vault: `host-managed-artifact-p1-1ba11b`, initialized from an empty Vault in this task.
- Data root: `C:/Users/qiu/AppData/Local/io.verisilo.app/vaults/host-managed-artifact-p1-1ba11b`
- CLI: this worktree's `apps/desktop/src-tauri/target/debug/verisilo-cli.exe`
- desktop: `verisilo.exe` in the same directory. Background mode is running; at handoff the PID was 8888 (reconfirm before use; a PID is not a permanent identity).
- This is the real CLI/background product path of an independent debug/Tauri backend. There is no installer and no mock Host.
- The QA-only throwaway test passphrase is the `password` in `artifacts/p1-standard-cli.py`. It is not the user's passphrase. It was submitted only through CLI stdin and was not printed. Do not use it for other Vaults.
- Standard A was really created, started, and read back successfully:
  - ID `9ebfa89a-5ad8-4c44-a9a2-aa3b02117d20`
  - Edge `152.0.4191.66`, adapter stock-edge, Network Direct, activation running.
  - The separate profile is under this Vault's silo/browser-data.
  - Still active at handoff. Root Edge PID 18540, parent PID 8888, and the command line is bound to the silo profile above. Reconfirm before use.
- `artifacts/p1-standard-cli.json` keeps the commands that were run and the raw output.
- Its last step, `stop Standard A`, returned an explicit error: the local stock browser requires its window to be closed, and the product will not terminate unrelated browser processes. The script's assertion stopped there, and **the later service restart / unlock / readback did not run**.
- Standard A was left active so that Managed create can be verified while it is running in the next step. Standard must not be closed first, because that would mask the trigger condition of the create.
- Initially, Python subprocess capture_output hung because the pipe of the background child process was never closed. The Vault was actually initialized successfully. The task's own Python driver was stopped by exact process, and it then ran normally with stdout/stderr redirected to files. Do not rerun the whole standard driver again (it would create again). Reuse its call helper or use the CLI directly.
- There is currently no unfinished code build or verification script to wait for. The product processes kept are the isolated desktop/Standard above.

## Conclusions on Managed create and single-active

The static call chain has been checked:

`CLI/local_api → application/identity.rs::create_managed_silo_with → provision_managed_artifact → production adapter → Host provision_artifact → Vault::create_managed_silo`

Neither end of create has a condition that forbids other Silos from running, so create should be allowed. The temporary provisioning root uses a new UUID and is cleaned up on success or failure. On failure there is no partial Managed record.

The single-active error for Managed **start** is `managed_another_silo_running` (the mapping of LauncherError::AnotherSiloRunning). It cannot be treated as a legitimate integrity error for create. The static conclusion awaits proof from a real Managed create with an active Standard in the next step.

## Shortest remaining path after handoff

1. Enter this task's worktree and read this document, the current diff, and the task metadata. Keep the two-line fix.
2. If the user has not yet signed locally, give only the terminal command above. Never ask for the passphrase in chat. After signing, confirm the existing pin, the signature, and the final tree/Host bytes. Do not change production trust.
3. Keep/confirm Standard A running. In the current brand-new isolated QA Vault, run:

   ```powershell
   & './apps/desktop/src-tauri/target/debug/verisilo-cli.exe' --vault host-managed-artifact-p1-1ba11b create --name 'Managed B' --network direct --preset balanced-zh-cn --json
   ```

   If the Vault auto-locks, unlock it through stdin first. Do not put the passphrase in command arguments or output.

4. Verify that create succeeds, there is no ArtifactIntegrityError, the record persists, the adapter is Camoufox, the network is Direct, the Artifact ID/raw SHA/schema are self-consistent, and the Formal-v3 engine binding is correct. Then run the same preset/network under a different name to verify that the new generation path no longer fails.
5. While Standard is still running, you can try Managed start to confirm the correct single-active error. Then close only this task's Standard window (identify it by the exact profile/PID, and do not terminate unrelated Edge), wait for the product to reconcile, and then do a Managed start → stop → reopen smoke.
6. service stop → restart (the CLI starts the sibling desktop automatically) → unlock → readback. Confirm that the persisted bindings are the same as the first time. After the minimal reopen, clean up the processes this task is running.
7. If a new error appears, locate it along the direct causal seam. Do not equate the success of the two-line fix with the whole product goal being complete. A new schema, shared contract, or cross-layer change must be handled through lane routing and not forced into the host scope.
8. A corrupted real Artifact being rejected by the verifier has already been verified, and that evidence can be reused. Retest only when the input or code changes. Do not mechanically rerun FP1–FP4.
9. Run the minimum sufficient verification for the new changes, plus verify/check/scope/contamination/final diff, complete the final report, and commit. Do not push, do not advance the baseline, and do not start the next QA round.

Report `MANAGED_ARTIFACT_P1_FIXED` only when the full acceptance is reached. For now the report should be `BLOCKED`. It must not claim that Managed CLI create/start/stop/restart has passed.

## Product acceptance results (2026-09-08 continuation)

Signing and package: after the user ran `p1-sign-package.ps1` locally, the manifest keyId = the production pin `57f3b44c…`, and `recheck_formal_package` passed (1431 members, tree SHA matches the manifest). Byte-level signing evidence: `packageManifestSha256 = 6551e914…` matches the byte SHA of the manifest on disk, and the `signerCertificateSha256` recorded by Managed start is that pin. The production trust chain passed with the signed manifest in practice.

On the real isolated Vault `host-managed-artifact-p1-1ba11b` (CLI driver evidence: `artifacts/p1-managed-accept.json`, `artifacts/p1-managed-accept-phase{1,2,3}.log`):

1. With Standard A (restart of the existing record) running, ran `create --name 'Managed B' --network direct --preset balanced-zh-cn`: succeeded, no ArtifactIntegrityError (id `a49b5887-9009-4906-ada4-c45bf9ea17c7`, adapter camoufox, binding `identity-1bb38820eb726ff8bad3f82b` / v5 / raw SHA `3db7ed90…`).
2. A second run with the same preset/network as `Managed C`: succeeded (`identity-9a4220cf…` / `bd76aec6…`). The fresh generation path is stable.
3. With Standard running, `start Managed B` → the exact error `managed_another_silo_running` (not an integrity error).
4. Closed Standard (the local stock browser requires the user to close the window as a product rule. After taskkill /F, Chromium left a `lockfile`, the product honestly entered `recovery_required` and did not delete the lock. After confirming that no process was bound to it, the user removed the stale lock on their side, and the product reconciled → stopped).
5. Managed start → stop → service restart + unlock → readback: binding/adapter/network match the first time, and `identityLocked` is retained. After reopen, start → stop again succeeded. The real Camoufox profile/engine-state was generated in the silo directory, `camoufox-host/v1 running` observed-on-this-host, engineRevision `…-formal-v3`.
6. The on-disk Artifact is self-consistent: the raw SHA of `silos/<id>/identity/identity-….json` == the binding's `artifactFileSha256`, and `verify_artifact_raw` strict passes (measured on Managed B. Managed C was not started, so it did not materialize an identity file; its binding is inside the encrypted Vault record).
7. Standard regression: a fresh `Standard B` create succeeded. The restart start/readback of Standard A is normal.
8. The evidence for rejecting a corrupted Artifact carries over from above (zero changes to the verifier code).

Wrap-up: this task's QA processes were cleaned up with `service stop`. Nothing was pushed, the baseline was not advanced, and the next QA round was not started.
