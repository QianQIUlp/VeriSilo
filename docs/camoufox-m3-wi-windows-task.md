# M3-WI native Windows Desktop / Real Host integration task contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical contract notice (2026-08-10)**: the frozen contract in this document was
> executed and entered a failure investigation. The "Frozen, awaiting execution agent" status
> below and its 1–16 acceptance matrix are kept only as the task contract of that time. They
> are not the current Gate. The current status is governed by this section and the
> [Camoufox status page](camoufox-program-status.md). The "awaiting execution" of the original
> contract must not be used to override the Failed/Inconclusive conclusions below.

## 2026-08-10 lead agent Gate and closure of the second-Host lifecycle investigation

### Gate conclusion

- M3-WI Gate: **Failed**.
- This root-cause investigation: **Inconclusive**.
- The last Accepted checkpoint is still M3-0
  `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`. It proves only the fake Host contract.
- There is no production fix, no focused regression, and no post-fix final real verification.
- Camoufox Windows Managed stays `experimental`. Do not claim M3-WI Accepted, Windows Managed
  verified, or Managed Identity shipped.

At the end of the investigation HEAD was `186484feb935076766beab09595a9270f86f78ef`, the tree
was `e33d6d68586a79796ffb9bcc668392e369dc97c6`, `e96ef3f` is its ancestor, and the tracked
working tree was clean. The investigation made no tracked modification and no new commit. All
temporary diagnostics lie in the gitignored `artifacts/cfx-life/run-0cdd1ed6/` of the
execution checkout of that time.

### Restricted investigation receipt

This local bundle is not manifest-bound evidence, did not enter Git, and cannot support
Accepted on its own. This document binds only three digest byte strings that cannot be
reproduced once away from the execution checkout:

| Local relative path                                       | SHA-256                                                            | Evidence limitation                                                     |
| --------------------------------------------------------- | ------------------------------------------------------------------ | ----------------------------------------------------------------------- |
| `artifacts/cfx-life/run-0cdd1ed6/experiment1-result.json` | `5ec698262a9438cc89f011c7e13ca79fb14f38ea06efac719b8f4d7b9984b51c` | A successful control before the fix, not a post-fix verification        |
| `artifacts/cfx-life/run-0cdd1ed6/experiment2-result.json` | `3fbc6c090d31630c1eab50598ea362be30969b1886a5633037fbce4ed9ec561a` | Bypasses the Host/supervisor, and the digest itself has no `sourceHead` |
| `artifacts/cfx-life/run-0cdd1ed6/experiment3-result.json` | `e49b1bbc0bfd0f4183f48c1e6d873feb8d7fc17bef92c323f3d6009e40da8c8d` | Completed only the first Host, cut off by an independent close failure  |

The historical raw stage log was cleaned up during the R2H preparation, so the last protocol
stage of the old 120-second failure cannot be rebuilt from the repository. The Security 4688
chain and timestamps below are only **historical OS observations reported by the execution
agent**. The corresponding raw records can no longer be recovered, and they are not written
up as evidence that the repository independently verified:

- venv launcher PID `4632` → actual Host PID `8220` → Playwright Node PID `8724` →
  supervisor PID `8676` → Camoufox PID `2996` and its complete child tree;
- the last reported event is the creation of a low-integrity Camoufox PID `2052` at
  `2026-08-10T04:19:58.8647846Z`.

This observation can narrow the historical stall to: the request entered the second Host, and
completed `_prepare`, the stdin read/dispatch, the Artifact/browser-tree verification, the
acquisition of Profile byte `0` / `1`, `launch_options`, and the Playwright driver startup, and
actually spawned the Camoufox process tree. It cannot prove that `launch_persistent_context`
returned. A persistent Firefox first initializes the default context / `about:blank`, while an
explicit page uses a separate new-page/target path. An OS PID carries no target ID, so a PID
cannot tell the two apart. For the corresponding upstream paths, see the
[Playwright v1.60 Firefox source](https://github.com/microsoft/playwright/blob/v1.60.0/packages/playwright-core/src/server/firefox/firefox.ts#L84-L105)
and the [Camoufox TargetRegistry](https://github.com/daijro/camoufox/blob/v152.0.4-beta.28/additions/juggler/TargetRegistry.js#L304-L357).

The remaining minimal candidate set is:

1. the final handshake of the default context / initial page of `launch_persistent_context`;
2. `ctx.new_page()` waiting for an independent page/target to be ready;
3. an unbounded Playwright RPC for fonts, media, identity, or cookies after the page returns.

After the persistent context returns and before `ctx.new_page()`, there is also a wait of up to
about 5 seconds for the supervisor metadata / Job binding. This is an already-bounded
independent seam, not a candidate for the historical 120 seconds.

The exact 120-second symptom under investigation came from the `stdout.readline()` watchdog of
the direct Python harness `apps/camoufox-host/test_windows_host.py`. It was not that run's
RuntimeManager receipt, and it was not a 120 seconds built into Camoufox. The Host protocol
calls `_send` only after the whole launch completes and sends no intermediate progress frames.
When any call above does not return, the parent can only observe 120 seconds without a final
stdout line. The Rust launcher also has a test-only `M3_WI_REAL_HOST_TIMEOUT = 120s`. It can
therefore only be said that the 120 seconds of this investigated failure came from the direct
harness. This must not be generalized into "Rust has no 120 seconds". What is established as
test-only is the 120-second symptom and the loss of raw diagnostics, not the underlying hang
itself.

### Qualified results of the three real-browser sequences

1. **Direct two-Host / same-Profile**: both launches passed, the second taking `12.047s`. The
   boot count `1 → 2`, Cookie/digest, clean close, Job active count `0`, process tree exit,
   and the release of the two lock bytes all hold. This is a control before the fix and cannot
   pass for a post-fix verification.
2. **Direct fixed Camoufox / Playwright reopen**: it bypasses the Host/supervisor and starts
   and stops the same Profile twice. Both passed and all processes exited. It only rules out
   "a deterministic Profile reopen upstream failure in the current environment". The result
   itself has no `sourceHead`, so its evidence level is lower.
3. **Deep persistent/new-page/goto/evaluate/cookie/stdout enclosure**: the first Host
   completed the launch and the final response, and `noViewport=true` was observed. After
   `page.close` succeeded, `ctx.close` timed out after about 10 seconds, the Job was forcibly
   emptied, `exit.json` was missing, and the session state was `failed`, so the second Host
   was never entered. The top-level `closeOk=true` of the digest only means that the close
   command returned a protocol response. It does not mean that the session closed cleanly.

After the experiments, the number of task-related processes was `0`. Although experiment 3's
close failed, the Job active count was `0`, the tree had exited, the Host/Profile lock bytes
could be acquired again, and the matching lease token was released afterward. This cannot
restore the lock holder and the process liveness state of the historically failed second
session at the instant of the timeout: Security 4689 was not enabled at that time, and the raw
directory has also been deleted.

The three discriminating sequences are used up. The first and second did not reproduce, and
the third was cut off by an independent close defect. The old 120 seconds therefore cannot be
attributed to the production lifecycle, a fixed upstream defect, or a test-only race, and no
speculative fix is implemented.

### Lead agent extraction and the next task

- The production diff that a future product branch must extract: **none**.
- The focused regression that must be extracted: **none**. The current evidence is not enough
  to encode a single root cause.
- Do not merge this research branch wholesale. The already tracked R1/R2/R2H runners, freezer,
  and manifest/schema/receipt/debug facilities are kept as research history but are explicitly
  excluded from the future product patch extraction. This run's gitignored temporary scripts,
  timeline, runtime, Profile, and cache must likewise not enter.
- Do not run the old R2/R2H matrix again or do selective retries. Only if it naturally recurs
  in the future and real protocol-stage/target evidence is obtained, rebuild a minimal fix and
  a single regression on the product baseline of that time.
- The current task turns to the
  [FP1 Deterministic Artifact Projection](camoufox-fp1-deterministic-artifact-projection-task.md)
  on the standalone Host. Only after FP1→FP2→FP3→FP4 will a brand-new clean M3-WI contract be
  frozen using the final Managed Engine of that time. This document's old contract is not
  revived.

## The original text of the 2026-08-09 frozen contract follows (historical)

- Status: **Frozen, awaiting execution agent**
- Freeze date: 2026-08-09
- Execution branch: `codex/camoufox-m3-engine-adapter`
- M3-0 Accepted checkpoint: `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- M3-WI implementation code baseline: `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- Previous Gate: M3-0 **Accepted**; it proves only the fake Host contract and contains no
  real-browser run ID

This document is both the frozen task card for M3-WI and the prompt handed to the execution
agent. The documentation-only Git commit that first fully contains this document and the
corresponding status update is the execution starting checkpoint. The lead agent fixes its
full SHA in the dispatch prompt. The execution agent must confirm that HEAD equals that SHA
exactly and that the worktree is clean. If they do not match, stop. Do not reset, rebase,
cherry-pick, or rewrite history on your own.

Before execution, read the following completely and in order:

1. The root `AGENTS.md`;
2. [Identity platform north star](identity-platform-north-star.md);
3. [Camoufox-first decision](camoufox-managed-engine-decision.md);
4. Agent operating model (removed on 2026-11-10; see Git history);
5. [Camoufox current status](camoufox-program-status.md);
6. [M3-0 task and Gate result](camoufox-m3-engine-adapter-task.md);
7. This document;
8. [EngineAdapter current state](engine-adapters.md), [Host v1](camoufox-host-v1.md), and the
   [M2-W Accepted Gate](camoufox-m2w-windows.md);
9. Read the related implementation and tests only to understand the seams. Do not expand into
   UI, the installer, or other engines.

## Role and Gate meaning

You are the **execution agent** for this phase, not the lead agent who re-decides the product
direction. Your task is, in a native Windows interactive desktop, to use the real standalone
Python Host and the fixed Camoufox browser to close the runtime risks that remain after M3-0,
and to return auditable evidence. You cannot declare M3-WI Accepted on your own, declare the
product shipped, or promote an observation on this one host to cross-host verification.

What M3-WI must prove is that the existing desktop-backend `RuntimeManager`, the
EngineAdapter launch plan, the `camoufox-host-jsonl-v1` transport, the Artifact/Profile
binding, and the stop/refresh lifecycle can truly drive `apps/camoufox-host/host_v1.py`, and
that the Host in turn drives the fixed Windows Camoufox tree. It is not a UI Gate, a signed
release Gate, an installer Gate, or a production package Gate.

## Current real gap

M3-0 closed the package, transport, failure matrix, lifecycle, and evidence contract of the
fake Host, but it still did not execute a real Python Host or a real browser. Meanwhile:

- The production external-engine verifier has no trusted signer pin and must keep failing
  closed.
- The repository has no signed Host package, published runtime, or installer.
- The current v3 package contract defines the package entrypoint as a Host executable, while
  the real Host in the development tree is still the `host_v1.py` in a locked Python
  environment.
- So this Gate cannot be completed by loosening the production verifier, forging a signature,
  or treating the system Python as a member of a signed package.

M3-WI must honestly split the evidence into two kinds:

1. The production-shaped package/transport **contract evidence** accepted in M3-0;
2. The native-Windows integration-only **real Host/browser execution evidence** newly produced
   by this Gate.

Both holding at the same time is still not equal to having a releasable trusted Host package.

## Frozen goal

In one native Windows interactive desktop session, starting from the exact clean checkpoint,
complete the following minimal real vertical slice:

1. `RuntimeManager.launch_with_identity_deriver` forms a Camoufox Host plan through a
   compile-time-isolated integration-only adapter/entrypoint;
2. The real `spawn_engine_child` launches the locked `uv/Python + host_v1.py`, not a fake Host;
3. The same production Host transport completes `hello → launch → status → close → shutdown`;
4. The Host launches the real Windows Camoufox of release `v152.0.4-beta.28`, using the
   Windows Artifact v3, the browser tree, and a Persistent Profile;
5. Two different Host children / two new `RuntimeManager` cycles reuse the same Profile,
   proving that the Cookie, LocalStorage, cookies.sqlite, and boot count carry over;
6. Normal stop, Host EOF/crash, and desktop parent shutdown all keep exact process/profile
   ownership, leave no Camoufox/supervisor subtree behind, and do not kill unrelated
   processes;
7. `RuntimeActivation`, the persisted `RuntimeRecord`, and the new tracked receipt record only
   evidence-backed `configured/applied/observed/unavailable`, keeping `verified: false` and
   `verifiedAdapter = null`.

All writable app/artifact/profile/state/cache/report roots must lie in the
`artifacts/camoufox-m3-wi-windows-gate/` subtree owned by this one unique run-id. Do not reuse
or clean an ordinary user Profile, and do not write into the accepted fixture directories.

## Native Windows and fixed inputs

All Gate evidence must come from native Windows. Linux, WSL, Wine, containers, virtual
browsers, or a fake Host are not accepted as substitutes. The execution session must have a
real interactive desktop (RDP is acceptable), and must record the Windows edition/version/build,
session, architecture, and the Python, uv, Rust, Camoufox, Playwright, and BrowserForge
versions.

The fixed inputs are:

- Camoufox Python package: `0.5.4`;
- Playwright: `1.60.0`;
- BrowserForge: `1.2.4`;
- browser release: `v152.0.4-beta.28`;
- Windows asset lock:
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-windows-x86_64.json`;
- archive SHA-256:
  `386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d`;
- browser tree: `tests/fixtures/camoufox/browser-tree-manifest-windows.json`, canonical
  manifest SHA-256
  `1c749534d139b7efcb425faf03de9cfe1d59004034a1fe1c5ba423b86239c37b`;
- Artifact: `tests/fixtures/camoufox/identity-win-a.json`, raw SHA-256
  `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`;
- dependency lock: `apps/camoufox-host/uv.lock`; runs must use
  `uv run --frozen --offline` and must not download or update dependencies, addons, or browser
  assets.

Before execution, recompute the actual tracked bytes, the sidecars, the tree, and the local
archive/cache bindings. Any mismatch fails closed before the browser launches. Do not
regenerate the accepted Artifact, the M2-W manifest, or the browser tree to make the test
pass.

## Integration-only launch seam

Because the production signer/package does not yet exist, this Gate allows one minimal,
explicit, non-releasable real-Host launch seam. It must satisfy all of these constraints:

- It may exist only in `#[cfg(test)]`, a Windows-only ignored integration test, or an
  equivalent non-shipping test target. The production build must not reference or select it.
- It may combine the locked `uv`/Python interpreter and `host_v1.py` into a test command, but
  it must record the interpreter path and version, the raw SHA of `host_v1.py`, the raw SHA of
  `uv.lock`, and all the fixed input SHAs.
- It must reuse the real `RuntimeManager` launch/refresh/stop, `spawn_engine_child`,
  `CamoufoxHostTransport`, the Artifact/Profile roots, the runtime record, and the evidence
  mapping.
- It must not use another direct Python driver to bypass the desktop seam and pass for M3-WI.
  The existing standalone driver can serve only as regression or counterexample evidence.
- It must not write the result of the test verifier up as production signature verification.
  The tracked receipt must explicitly record `integrationPath=test-only-real-host`,
  `productionPackageVerified=false`, and `shipped=false`.
- `ExternalPackageEngineAdapter::production_prototype` must keep failing closed when the signer
  pin is missing, and must keep an independent regression test.
- Do not add a production dependency, private key, certificate, signer pin, hidden feature
  flag, environment backdoor, or a verifier bypass that a release build can trigger.

If the real Host cannot be launched within these constraints, stop and report the
package/runtime delivery gap. Do not silently expand M3-WI into a signing and release project.

## Allowed modifications

- `apps/desktop/src-tauri/src/engine.rs`, `launcher.rs`, and the adjacent Windows-only/test-only
  integration harness;
- minimal, backward-compatible tests or evidence support in `apps/camoufox-host/` that is
  necessary for the real desktop seam;
- a new M3-WI Windows runner/freezer, JSON schema, and
  `tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.json`;
- `docs/engine-adapters.md`, this document, and the status page, kept in sync with the actual
  behavior;
- necessary test target/Cargo configuration, but no new production dependency.

The allowed scope is not a list of things that must be modified. Prefer reusing the existing
implementations of M3-0 and M2-W. Do not rewrite the Host, the Artifact generator, the Windows
supervisor, or the generic desktop launcher.

## Forbidden modifications and forbidden claims

- Do not change the existing semantics of Artifact/Policy/Projection v3, ObservedWebsiteDigest
  v2, or Host v1. If the real response lacks an irreplaceable binding field, stop and hand it
  back to the lead agent for a ruling.
- Do not rewrite, delete, or regenerate the M0–M2-W accepted evidence, run IDs, fixtures, or
  protected hashes.
- Do not add UI, a Managed Identity menu, an installer, auto-download/update, proxy injection,
  network geo coupling, site fallback, Controlled Chromium, JS stealth, WSL, VMware, Hyper-V,
  or a Remote backend.
- Do not add a real signer, a signed package, a published runtime, or a production verifier
  bypass.
- Do not promote Camoufox as a Chrome emulation. It is a Firefox identity engine.
- Do not claim font isolation, Canvas stable identity, TLS/QUIC identity, cross-host
  reproduction, undetectability, or `verified: true`.
- Do not write an integration-only run up as a shipped Tauri product, production package
  evidence, or a capability real users can use.

## Acceptance matrix

Items 1–16 below must each return `passed`, `failed`, or `blocked`. An overall test pass must
not be used instead.

### Baseline and trust boundary

1. **Native baseline**: confirm the original Windows checkout, the exact starting commit, a
   clean worktree, and a native Windows/interactive session. Before and after execution there
   is no Host/Camoufox/supervisor left over from this task.
2. **Fixed-input closure**: the dependency lock, asset lock/archive, browser tree, and Artifact
   raw bytes/sidecar match the fixed values above. The whole run is offline, and DownloadGuard
   observed no webdl.
3. **Test-only isolation**: the real-Host launch seam cannot enter the production build. The
   production adapter without a signer pin still fails closed before spawn, and the receipt
   does not claim package verified.

### Real Host / browser vertical slice

4. **Real path**: the evidence proves that the call chain actually went through
   `RuntimeManager → adapter.launch_plan → spawn_engine_child → host_v1.py → Camoufox`. The
   Host PID, the browser/supervisor ownership, and the run-id can be cross-bound, and no fake
   Host took part.
5. **hello/launch binding**: the protocol, Host version, platform, release, asset SHA, browser
   tree path/SHA, Artifact ID/raw SHA, and Profile ID all match exactly. Any mismatch is
   rejected before Running.
6. **Real observation**: the real browser launch returns the Artifact/config binding,
   ObservedWebsiteDigest v2, the media-device match, and `evidenceClass`. Record only observed
   on this machine, and do not upgrade any fingerprint, Canvas, font, TLS/QUIC, or cross-host
   conclusion from it.

### Persistent Profile and lifecycle

7. **Cross-desktop-cycle persistence**: starting from an empty Profile root newly created by
   this Gate and owned by the run, two different Host PIDs and two new `RuntimeManager`
   instances use the same Silo/Profile/Artifact in sequence. The boot count goes `1 → 2`. The
   Cookie API, document cookie, cookies.sqlite, and LocalStorage carry over, and both runs
   bind the same Artifact raw SHA.
8. **Normal stop**: `Stopped` is published and the Profile lease released only after all of
   `close.processTreeExit.exited=true`, Windows Job active process count `0`, `shutdown`, and
   the exit of the exact Host child are confirmed.
9. **EOF/crash/desktop close**: within an active session, cover separately a Host EOF/abnormal
   exit and a desktop parent shutdown. Reclaim only the child/tree that this RuntimeManager
   owns exactly. When a full-tree exit cannot be confirmed, the state must be
   `VerificationFailed` or `RecoveryRequired`, and the Profile lease must not be released
   wrongly.
10. **Concurrency and unrelated processes**: a second launch of the same Profile is rejected.
    A pre-created unrelated process and unrelated browser are not waited for, terminated,
    taken over, or written into evidence.

### Fail-closed and secret boundary

11. **Cross-seam counterexamples**: cover at least the Artifact raw SHA, the browser tree SHA,
    the Host hello binding, profile-in-use, active-tree exit unconfirmed, a malformed/oversize
    frame, timeout, and early-exit. The failures are bounded and cannot wrongly enter Running.
12. **Network policy**: the real successful run may use only `Direct { proxy_required: false }`.
    A required proxy, FixedProxy, PAC, and non-empty fallback are rejected before the Host
    spawn, and the run cannot fall back or quietly pass a Chromium proxy argv to the Host.
13. **Secret scan**: the Vault deriver sentinel, the Artifact seed, the token value, and the
    proxy username/password sentinels must not appear in the Host/browser argv, the
    environment snapshot, the plan, the JSONL wire, errors, `RuntimeActivation`, the persisted
    `RuntimeRecord`, or tracked/raw evidence.

### Evidence and regression

14. **Honest semantics**: a successful real Host records only `hostLaunch` as `Observed`. The
    generic bootstrap / receipt fields stay `NotApplicable`, and `verifiedAdapter=null`,
    `verified:false`. After a failure, the old observed evidence must not remain valid.
15. **Regression**: the M3-0 package/transport tests, Stock Chrome/Edge, Controlled Chromium
    bootstrap/receipt/fallback, Artifact 25/25, the full Desktop Rust check/test/clippy, and
    the JS contracts/desktop/extension tests keep passing.
16. **Auditable receipt**: add a separate M3-WI tracked manifest that binds the
    receipt-producing commit/tree, the native Windows host, the fixed input SHAs, each
    run-id/report/sidecar SHA, the lifecycle/secret-scan results, and an explicit
    test-only/unpublished boundary. The raw profiles/reports stay gitignored, and the manifest
    contains no secrets or sensitive data beyond the user path.

## Real-run and evidence freeze order

1. On the clean documentation checkpoint, implement and run the fast tests that do not launch
   a browser;
2. Commit the complete receipt-producing code and record the full commit and tree hash;
3. Confirm clean again, and run the M3-WI real suite on the native Windows interactive desktop
   at that exact commit;
4. Write the original report and sidecar into the new gitignored
   `artifacts/camoufox-m3-wi-windows-gate/runs/`, with a unique run-id for each run;
5. The freezer derives the new tracked manifest only from reports that are complete, whose
   sidecar verification passed, and whose `codeGitRevision` matches exactly;
6. Freeze the manifest with a subsequent evidence-only commit. If any receipt-producing
   runtime, harness, or schema is modified afterward, the affected Gate must be rerun. Do not
   amend/rebase old evidence to pass it off as the same revision.

Do not reuse an M2-W run-id to pass for this Gate, and do not name an M3-0 fake Host test a
real-browser run. The new manifest records at least:

- schema `verisilo-camoufox-m3-wi-windows-evidence-manifest/v1`;
- `codeGitRevision`, `codeTreeHash`, branch, and the host/session/tool versions;
- `integrationPath=test-only-real-host`, `productionPackageVerified=false`, `shipped=false`,
  `verified=false`, `evidenceClass=observed-on-this-windows-host`;
- the exact SHAs of the Host source/lock, asset/archive, browser tree, and Artifact;
- the run-id, report path, and SHA-256 of the real launch, persistence, clean stop,
  EOF/crash/desktop-close, and negative matrix;
- the results for `productionVerifierFailClosed=true`, DownloadGuard, the secret scan,
  residual processes, and the protected accepted-file hashes.

## Required verification

Run and report at least the actual commands, exit codes, and counts:

```powershell
pnpm check
pnpm test
pnpm build
pnpm engine:verify
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
Set-Location apps/camoufox-host
uv run --frozen --offline python test_identity_artifact.py
```

You must also run one clearly named, single-threaded, bounded-timeout, Windows-only M3-WI real
integration command that outputs a new run-id and a summary sidecar. If implemented as an
ignored Rust test, the command must have the form:

```powershell
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml <exact test name> -- --ignored --nocapture --test-threads=1
```

If `apps/camoufox-host/host_v1.py`, `host_platform.py`, `browser_tree.py`, the Windows
supervisor, or the accepted Windows fixtures are modified, then in addition to the M3-WI suite
the complete `test_windows_host.py` must be rerun, and the impact must be handed back to the
lead agent for a new judgment. If these files were not modified, you need not rerun the whole
long M2-W matrix for form's sake, but the cross-desktop-seam evidence of items 7–11 still
cannot be omitted.

## Stop conditions

On any of the following, stop immediately and return `blocked` or `failed`. Do not expand on
your own:

- The actual permissions, cwd, branch, starting commit, worktree, or the native
  Windows/interactive desktop do not match.
- The fixed archive/cache/uv environment does not exist or does not match, and continuing
  would need a network download or a lock update.
- The real Host can start only through a production verifier bypass, a forged signature, a
  real signer, or a published package.
- It would be necessary to change Artifact v3, ObservedWebsiteDigest v2, the Host v1 semantics,
  or the accepted M0–M2-W evidence.
- It would be necessary to add a production dependency, UI, an installer, a proxy, Controlled
  Chromium, or another deferred backend.
- The exit of the browser/Job tree cannot be confirmed, the Profile ownership is uncertain, an
  unrelated process was killed by mistake, or a secret leaked.
- The real run was not generated by the final receipt-producing commit and cannot be safely
  rerun.

## Execution agent result package

The final return must follow this order:

1. The execution conclusion: `passed`, `failed`, or `blocked`. Do not write Accepted on your
   own;
2. The native Windows host/session, the exact starting commit, the receipt-producing commit,
   the evidence commit, the branch/upstream, the worktree, and the push/PR status;
3. The item-by-item 1–16 Gate matrix, with the run-id, test name, or code evidence for each;
4. The real call chain and the integration-only isolation notes, explicitly stating that the
   production verifier still fails closed;
5. The fixed Artifact/asset/browser tree/Host/lock SHAs and the real Host/browser PID/Job
   ownership;
6. The summary/report/sidecar paths and SHAs of the real launch, two-cycle persistence, clean
   stop, EOF/crash/desktop-close, and negative matrix;
7. The complete test commands, counts, the post-failure correction record, and the items not
   run;
8. The conclusions on the secret scan, residual processes, temporary directories, network
   downloads, and external side effects;
9. The modified files, the diff against the forbidden scope, the unverified boundaries, and
   the inputs for the next Gate.

After execution ends, wait for the lead agent to review under the cost-controlled protocol. An
all-green fake Host, a single real browser open, an old standalone M2-W receipt, or a
`passed` self-reported by the execution agent cannot close M3-WI on its own.
