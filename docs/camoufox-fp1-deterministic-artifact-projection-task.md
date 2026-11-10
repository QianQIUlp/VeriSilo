# Camoufox Managed Fingerprint Core FP1 — Deterministic Artifact Projection

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical contract and evidence record**: for the current FP1 conclusion and the next Gate, the
> [status page](camoufox-program-status.md) governs. Unless you are investigating an immutable FP1 run, do not read or inherit this document's
> old one-shot, matrix, and checkpoint requirements.

- Status: **Frozen task contract / Canvas focused passed / Full FP1 Failed at A1 clean close / not Accepted**
- Freeze date: 2026-08-10
- Current Gate: **FP1**
- Predecessor checkpoint: M3-0 Accepted at
  `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`

This document freezes the single goal, the implementation boundary, and the acceptance method of FP1. The long-term product intent is governed by the
[identity platform north star](identity-platform-north-star.md). The Camoufox-first route and the
FP1–FP4 ordering are governed by the
[Managed Engine architecture decision](camoufox-managed-engine-decision.md). The current Gate
and checkpoint are governed by the [status page](camoufox-program-status.md).

FP1 is not a rerun or repair of M3-WI. The second-Host lifecycle investigation was closed out as `inconclusive`,
and M3-WI stays `failed` / `experimental`. FP1 uses the already established standalone
Host to study and verify the fingerprint projection. FP1 results must not be written as M3-WI Accepted, desktop product available, or
Managed Identity released.

## 1. Fixed baseline and authoritative boundaries

This contract is formed on the following read-only implementation snapshot:

| Object                 | Fixed value                                     |
| ---------------------- | ----------------------------------------------- |
| source HEAD            | `186484feb935076766beab09595a9270f86f78ef`      |
| source tree            | `e33d6d68586a79796ffb9bcc668392e369dc97c6`      |
| Accepted ancestor      | `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`      |
| Artifact schema        | `verisilo-camoufox-resolved-identity/v3`        |
| Projection schema      | `verisilo-camoufox-stable-signal-projection/v3` |
| Observed digest schema | `verisilo-camoufox-observed-website/v2`         |
| Host protocol          | `verisilo-camoufox-host/v1` JSON Lines          |

The starting commit for FP1 execution may be a later docs-only commit that contains this contract and the status sync, but its
receipt-producing implementation must satisfy
`git merge-base --is-ancestor 186484feb935076766beab09595a9270f86f78ef <execution-HEAD>`.
`e33d6d68586a79796ffb9bcc668392e369dc97c6` is used only to check the expected
tree of that baseline commit. The tree is not treated as an object whose ancestry can be judged. Before execution, the exact HEAD/tree must be recorded, and it must be confirmed that
the `e96ef3f` Accepted lineage and the `186484f` implementation baseline are both ancestors and that the tracked worktree is
clean. After any implementation change is complete, the real run must bind the new full commit/tree. It must not still claim to have run on
`186484f`.

The Windows inputs are fixed as:

| Artifact | Repository path                               | raw SHA-256                                                        |
| -------- | --------------------------------------------- | ------------------------------------------------------------------ |
| A        | `tests/fixtures/camoufox/identity-win-a.json` | `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f` |
| B        | `tests/fixtures/camoufox/identity-win-b.json` | `ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9` |

The Artifact raw bytes, sidecar, the pinned Camoufox/Playwright/BrowserForge, the browser archive,
and the browser tree keep the current pins. FP1 does not upgrade upstream, does not download `latest`, and does not replace the
Artifact already materialized in the repository with an unmerged upstream PR.

The table above is the original official-binding baseline of FP1. The later Canvas Engine Patch contract explicitly requires an independent
self-built binding and rebound fixtures. When running focused and full FP1 against the new engine, the run inputs are governed by
the 2026-08-15 checkpoint at the end of this document. The old `identity-win-a/b/c`, the official archive lock, and the old
Windows tree manifest stay protected byte for byte as history and are not rebound at their original paths.

## 2. Single goal and definition of success

FP1 answers only this question:

> Can the same raw Resolved Identity Artifact replay the
> same final Camoufox config in two independent Host cold starts, and make the local first-party probe get
> stable results for the fields visible to the target browser? Does another Artifact separate reasonably only by the differences declared in advance?

FP1 succeeds only when all of the following hold:

1. Every randomness source that may affect the browser-visible identity has a clear owner. There is no gray area of the form "Camoufox or
   BrowserForge randomizes on its own but VeriSilo does not know about it";
2. A1 and A2 run from exactly the same Artifact raw bytes/SHA. The normalized complete
   `CAMOU_CONFIG` and the on-disk `resolvedConfig` are identical key by key and value by value;
3. The target observation surfaces of A1 and A2 are identical field by field. It is not enough to compare only
   `ObservedWebsiteDigest v2`;
4. The differences in B1 agree with the static diff of the A/B Artifacts computed in advance. A field not declared to separate must not be randomized just to
   "look different", and it is not required that every A/B field differs;
5. Profile A's state continues across A1→A2. Profile B does not inherit A's Cookie, LocalStorage,
   or boot count;
6. All three runs complete the fail-closed lifecycle checks, with no residual Host, supervisor,
   Camoufox process, Windows Job active process, Profile lock, or wrong ownership;
7. The result keeps using honest evidence semantics, `verified:false`, and produces no claims of cross-host, undetectable,
   Chrome emulation, or complete font isolation.

Here "the same Artifact" means **replaying the same raw file**. It does not mean calling the resolver twice with the same fixture RNG seed.
Artifact metadata such as `artifactId` and `generatedAtUtc` may by nature make
two regenerated raw Artifacts differ, and FP1 does not take byte equality of resolver output as a goal.

## 3. Output determinism does not mean the execution path has no RNG

The current implementation already executes:

```text
disk resolvedConfig
→ deepcopy
→ launch_options(...)
→ normalize_camou_config_env(...)
→ full diff and configured digest Gate
→ CAMOU_CONFIG
```

But `launch_options()`, BrowserForge, Python `random`, or NumPy may still consume random numbers when computing candidate defaults.
As long as those candidates do not overwrite the final values already materialized in the Artifact, consuming RNG
does not by itself mean browser identity drift. FP1 must therefore prove separately:

- **output determinism**: under different environment RNG states, the normalized complete
  `CAMOU_CONFIG` equals the same on-disk `resolvedConfig` key by key;
- **observed determinism**: A1/A2 of the same Artifact are identical field by field on the surfaces the browser actually shows;
- **claims not made**: it must not be written that the execution path of `launch_options`, BrowserForge, Python, NumPy, or
  Camoufox "uses no random numbers".

The static regression should replay the same Artifact under different Python/NumPy RNG states and assert that the complete sent config
and its digest do not change. This regression can only prove that the final projection is not affected by these RNG states. It cannot replace the real
browser A1/A2, and it cannot upgrade an unobserved kernel randomness surface to stable.

## 4. Complete randomness source ledger

Before the real browser run, a static review must cover the following producers: the Artifact generator,
BrowserForge, Camoufox `launch_options`, the fixed Camoufox internal default/seed use, the Host,
the probe, and the browser runtime. Each identity-relevant source must be assigned to exactly one of the following lifecycles:

1. **Artifact explicit value**: the final value exists directly in the v3 `resolvedConfig`;
2. **materialized replay seed**: the seed already exists in the Artifact, and the fixed engine deterministically derives the website-visible
   result from it, for example the current `canvas:seed`, `audio:seed`, `fonts:spacing_seed`;
3. **Silo/Vault secret**: if such a source is found, only its lifecycle and non-leak boundary may be recorded. FP1
   must not write the secret into argv, logs, the probe, results, or the Artifact;
4. **run/session non-identity entropy**: run-time values such as session ID, PID, temp directory, probe port,
   and timestamp. They must not enter the identity stability comparison;
5. **currently uncontrollable / unavailable**: the current fixed engine cannot clearly control or verify it.
   The reason must be recorded publicly, and it must not be hidden by defaults, guesses, or omission from the digest.

Before the first real launch, the ledger must have the table below filled in. A1 must not run while any candidate source is unclassified:

| Surface / randomness source     | producer                                        | Lifecycle class                     | Identity-relevant | `resolvedConfig` key          | Fixing/derivation mechanism                                                                            | First-party observed fields                                    | In digest v2?                 | A1/A2 expectation                        | A/B expectation                                                                | Evidence status | Exclusion / to-do reason                                                               |
| ------------------------------- | ----------------------------------------------- | ----------------------------------- | ----------------- | ----------------------------- | ------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- | ----------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------ | --------------- | -------------------------------------------------------------------------------------- |
| UA / Navigator                  | Artifact generator; Camoufox MaskConfig         | Artifact explicit value             | Yes               | `navigator.*`                 | 10 explicit navigator values override the BrowserForge candidates                                      | UA, app/name/version/product, platform, oscpu, hardware        | Yes                           | Explicit fields must be identical        | Only `hardwareConcurrency` should separate per the static diff                 | configured      | `maxTouchPoints` is listed separately and must not be disguised as Artifact-controlled |
| Headers                         | Artifact generator; Camoufox request layer      | Artifact explicit value             | Yes               | `headers.Accept-Encoding`     | Full sent-config replay                                                                                | The current JS probe has no reliable header channel            | No                            | Config must be identical                 | A/B config identical                                                           | configured      | Observation of actual request headers is `unavailable`                                 |
| Locale / Timezone               | Artifact generator; Camoufox MaskConfig         | Artifact explicit value             | Yes               | `locale:*`, `timezone`        | Language, region, script, and IANA timezone are replayed explicitly                                    | `language(s)`, timezone, UTC offset                            | Yes                           | Must be identical                        | A/B config identical                                                           | configured      | Direct-only; no claim of coherence with the network exit                               |
| Screen / Window / DPR           | Artifact generator; Camoufox; Firefox UI        | explicit + host-bound               | Yes               | `screen.*`, `window.*`        | screen/outer/screen offset are explicit; inner/DPR are derived by the fixed engine                     | `screen`, DPR, full `windowGeometry`, session screen X/Y       | Partial                       | Artifact-backed fields must be identical | Only the fields corresponding to the static screen/offset diff should separate | configured      | inner geometry and DPR cannot be inferred from config as `applied`                     |
| History                         | Artifact generator; Camoufox MaskConfig         | Artifact explicit value             | Yes               | `window.history.length`       | The explicit value blocks the random history candidate on each launch                                  | `historyLength`                                                | Yes                           | Must be identical                        | Should separate per the static diff                                            | configured      | —                                                                                      |
| Canvas raw/export               | Artifact generator; Camoufox canvas patch       | materialized replay seed            | Yes               | `canvas:seed`                 | The same materialized seed is handed to the fixed engine                                               | `canvas.rawHash` / `canvas.exportHash`                         | No                            | Both hashes must be identical            | The seeds differ; the observed difference is decided by the real result        | configured      | export drifted historically; real A1/A2 is a hard Gate                                 |
| Audio                           | Artifact generator; Camoufox audio patch        | materialized replay seed            | Yes               | `audio:seed`                  | The same materialized seed is handed to the fixed engine                                               | `audioHash`                                                    | Yes                           | Must be identical                        | The seeds differ; the observed difference is decided by the real result        | configured      | —                                                                                      |
| WebGL / WebGL2                  | Artifact generator; fixed Camoufox database     | Artifact explicit value             | Yes               | `webGl:*`, `webGl2:*`         | vendor/renderer/parameters/extensions/precision are materialized; candidates only fill what is missing | WebGL1/WebGL2 vendor, renderer, summary, availability          | WebGL1 partial                | Each field must be identical             | A/B config identical                                                           | configured      | The probe is a summary and does not cover all shader precision                         |
| Fonts / metrics                 | Artifact generator; Camoufox font patch         | explicit + materialized replay seed | Yes               | `fonts`, `fonts:spacing_seed` | The font list and the spacing seed are both materialized                                               | injected availability, fixed-universe widths, negative control | Partly excluded under inherit | Each field must be identical             | Fields corresponding to the fonts/seed diff are decided by the real result     | configured      | `fontMode=inherit`; the full host font set remains host-bound                          |
| Voices                          | Artifact generator; Camoufox voice patch        | Artifact explicit value             | Yes               | `voices`                      | The full voice list is materialized                                                                    | name/lang/URI/local/default                                    | Yes (keeps the old v2 shape)  | Must be identical                        | A/B config identical                                                           | configured      | The new FP1 default stays only in `observedFull`                                       |
| Media devices                   | Artifact generator; Camoufox/Firefox fake media | Artifact explicit value             | Yes               | `mediaDevices:*`              | enabled and the counts of the three device classes are explicit; Windows deterministic prefs           | `mediaDevices`, readiness/counts                               | Yes                           | Must be identical                        | A/B config identical                                                           | configured      | Proves only the current fixed engine / this machine                                    |
| `navigator.maxTouchPoints`      | BrowserForge candidate; Firefox native          | currently unavailable / host-bound  | Yes               | none                          | The candidate is deleted after a closed-allowlist audit, and the final value is the native one         | `maxTouchPoints`                                               | No                            | Observe on the same machine and classify | A/B need not differ                                                            | unavailable     | Not in v3; unknown extra fields fail closed before spawn                               |
| Browser/process/session entropy | Host / OS / Playwright                          | run/session non-identity entropy    | No                | none                          | Does not enter the final `CAMOU_CONFIG`                                                                | Explicitly labeled session/process/Job/lock receipts           | No                            | May differ                               | May differ                                                                     | observed        | UUID, PID, port, temp path, timestamp, and RPC correlation are excluded                |

The implementation audit also confirmed the following RNG producers. Here we distinguish "the execution path consumes RNG" from "the final identity is rewritten by RNG":

| producer                                         | When consumed                                               | Candidate-affected surfaces                               | FP1 handling                                                                                                                                                                          |
| ------------------------------------------------ | ----------------------------------------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Python `random`                                  | Candidates such as history/seeds in each `launch_options()` | history, Canvas/Audio/font seeds, fonts, voices           | The explicit values/seeds of the 47-key Artifact occupy their slots first, and `merge_into`/`set_into` do not overwrite them; the focused regression perturbs and compares key by key |
| NumPy / BrowserForge RNG                         | Each BrowserForge fingerprint and WebGL candidate           | Navigator, screen, WebGL, and occasionally maxTouchPoints | The Artifact covers the declared fields; the only classified extra field is maxTouchPoints; an unknown extra fails closed                                                             |
| Camoufox random subset selection of fonts/voices | When the candidate lacks `fonts` / `voices`                 | font list, voices                                         | v3 already materializes them explicitly, so the two random branches do not change the final projection                                                                                |
| Camoufox noise seed generation                   | When the candidate lacks the three seeds                    | Canvas, Audio, font spacing                               | v3 has materialized the three seeds, and candidate random seeds do not overwrite them                                                                                                 |
| UUID / PID / port / path / time                  | Host/session/probe/process lifecycle                        | No identity config; run receipts only                     | Classified as non-identity entropy; must not exclude any Artifact-backed browser observation field                                                                                    |

The focused static regression runs 100 groups of Python/NumPy/permitted environment entropy perturbations on the same raw A Artifact. Every group must equal the on-disk 47-key config key by key and type by type. This regression does not claim that BrowserForge did not run, and does not replace A1/A2 browser observation.

"To be filled in / to be proven" is pre-execution work. It is not a final state that may be left until after the Gate. The final table must classify
every row as one of `configured`, `applied`, `observed`, or `unavailable`. Only when the overall FP1 acceptance meets all the binding conditions of this contract
may it be written as Accepted at the task level, but the browser capability stays
`verified:false`.

## 5. Schema and seed decisions

By default, FP1 keeps the v3 Artifact, the 47-key `resolvedConfig`, Policy v3, Projection v3,
ObservedWebsiteDigest v2, and the existing fixture raw bytes unchanged. It adds no user-facing master
seed, and does not interpret the fixture-only `--rng-seed` as a Silo seed or a browser process input.

If the audit finds that an important identity randomness source cannot be expressed deterministically by existing explicit values or a materialized replay seed:

1. Stop the related implementation and real runs immediately;
2. Mark that surface `unavailable`, and output the minimal source trace;
3. Submit a separate schema/ADR proposal that explains migration, compatibility, key lifecycle, and upstream dependencies;
4. Freeze the task separately after the lead agent makes an explicit decision.

Do not silently add Artifact fields, regenerate the A/B fixtures, write runtime random values back into the Profile,
or obtain stability "by chance" through an unpinned upstream version.

## 6. Allowed scope

A future execution agent may modify only the following for FP1:

- the Artifact→`launch_options`→browser projection inside the standalone Host, redacted stage diagnostics,
  and necessary fail-closed cleanup;
- the existing first-party probe, to add observation fields for current surfaces that are missing but do not involve FP2 cross-realm;
- focused deterministic projection, diagnostics, secret scanning, and lifecycle regressions in the existing Host/Artifact/Windows tests;
- this task contract's Gate result/coverage table and the status page.

The implementation should reuse the existing Host, probe, Windows supervisor, fixtures, and test entry points. For
FP1 it must not create a new runner, freezer, evidence manifest, manifest schema, or large evidence system,
and must not add production dependencies.

## 7. Explicitly prohibited scope

FP1 does not do:

- RuntimeManager, Tauri, EngineAdapter production package, M3-WI Accepted, or desktop UI;
- the Standard Silo or Managed Silo creation flow, installer, signing, upgrade, or distribution;
- changes to proxy, GeoIP, region, DNS, WebRTC, network fallback, or Direct-only policy;
- iframe, cross-origin iframe, Dedicated/Shared/Service Worker, multi-context, and other FP2 items;
- detection sites, ordinary website matrices, per-site fallback, and other FP4 items;
- Controlled Chromium, WSL, Hyper-V, VMware, Remote, or whole-machine environment isolation;
- ten cycles, 5+5, R1/R2/R2H, or other matrices expanded to chase the occasional second-Host hang;
- rewriting the Host v1 protocol, changing the stdout JSONL wire shape, or writing progress frames to stdout;
- modifying, deleting, or regenerating any M0–M3-WI historical accepted/rejected evidence.

## 8. Minimal lifecycle observability hardening

FP1 only splits the existing coarse stages into the following persistent, bounded, protocol-level boundaries:

```text
launch_options
→ launch_persistent_context
→ supervisor_job_bind
→ new_page
→ goto
→ observed.fonts
→ observed.media
→ observed.identity
→ cookie
→ observed.write
→ response_write
```

The boundary definitions are fixed as follows:

| stage                       | start                                             | success boundary                                                                                                     |
| --------------------------- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `launch_options`            | Before calling the fixed `launch_options()`       | The config is normalized and has passed the full disk/sent diff                                                      |
| `launch_persistent_context` | Before calling persistent `AsyncNewBrowser`       | The context object has returned and been written into the session                                                    |
| `supervisor_job_bind`       | After the context returns                         | The supervisor metadata identity is validated; the Windows Job is opened successfully and its ownership is confirmed |
| `new_page`                  | Before `ctx.new_page()`                           | The explicit page object has returned                                                                                |
| `goto`                      | Before local probe navigation                     | `domcontentloaded` has completed                                                                                     |
| `observed.fonts`            | Before probe font inputs / `document.fonts.ready` | The font inputs, ready, and the preparation needed for the negative control are complete                             |
| `observed.media`            | Before the media readiness RPC                    | media readiness has returned an explicit result                                                                      |
| `observed.identity`         | Before the `readIdentity()` RPC                   | The full first-party identity object has returned                                                                    |
| `cookie`                    | Before boot count / Cookie operations             | boot count and API/page Cookie evidence are collected                                                                |
| `observed.write`            | Before signals/digest/projection assembly         | `observed.json` is completely written                                                                                |
| `response_write`            | Before the final protocol response is serialized  | A single stdout line is written and flushed                                                                          |

Within each command, each stage records at most one `start` and one terminal event. The terminal event can only
be `success`, `error`, `timeout`, or `cancelled`. Every record is flushed immediately to stderr and to the
controlled persistent diagnostic file. The old coarse stages must not duplicate the new stages and produce an ambiguous "success". The diagnostic budget must
have explicit event/byte limits and reserve space for close/failure. When a limit is reached, later ordinary records are omitted in a fail-closed way, but the last
stage and the cleanup conclusion must be kept on a best-effort basis.

Records may contain only the fixed stage, event, a monotonic elapsed bucket, a redacted correlation value of the protocol request ID,
and a non-sensitive error category. They must not contain URLs, Cookies, LocalStorage, Artifact seeds, Vault
values, tokens, proxy credentials, argv, environment values, or user absolute paths. The runner must not clean up the diagnostic file before that command/run has been
reviewed, and stdout continues to contain only the final Host v1 JSONL response.

Timeout only fixes the following strict relationships:

```text
specific Playwright / Host operation deadline
< Host launch command deadline (must leave time for cleanup and the error response)
< current parent-side stdout watchdog
```

This contract does **not freeze new second counts**. The second-Host investigation is not enough to support writing a new
`60/100/120` combination by guesswork. The parent watchdog must not be enlarged merely to hide a hang. The implementation result must list the existing
parent watchdog, the actual deadline at each level, their sources, and the inequality checks. If an auditable budget cannot be obtained from the current implementation,
the existing bounded cleanup, and real clean-run data, stop and return
`blocked: timeout_budget_unfrozen`. The execution agent must not choose convenient second counts on its own.

This hardening only improves error attribution and the fail-closed return. It must not be packaged as the underlying second-Host hang having been fixed.

The FP1 implementation audit did not yield a new strict timeout hierarchy that could be frozen: the direct Python harness and the
test-only Rust M3-WI watchdog are both 120 seconds, but the initial receipt window of the production adapter
is a different 5-second contract, so neither of them can be generalized into the sole parent side of the standalone Host. The persistent launch of the fixed
Playwright 1.60 defaults to 180 seconds, the existing `goto` is 60 seconds, supervisor
metadata is 5 seconds, and media readiness is 8 seconds, while `new_page`, the fonts/identity evaluate, and the
cookie RPC have no separate channel deadline. The existing cleanup also includes server
shutdown and file I/O with no total deadline, so the total cleanup budget cannot be proven either. On this basis, this FP1 round implements only stage diagnostics.
It adds no operation/command deadline, does not enlarge any parent watchdog, and does not write the timeout hierarchy as a
closed Gate.

The semantic ruling of the follow-up execution on `observed.media` is: the media readiness inside launch is only an optional piece of Runtime
Evidence, not a hard barrier that the Artifact has been applied. The later full probe is the authoritative media
observation for FP1. A browser-side enumerate timeout or API unavailable may continue to the full probe with a fixed reason,
but after a channel timeout / Playwright exception the page cannot be safely reused and must enter the existing fail-closed
cleanup. The readiness result itself does not upgrade the Artifact's configured media count to `observed`;
the actual count returned by the later full probe is `observed`, whether or not it matches. Only a match can pass FP1;
when the probe cannot return reliably, it is marked `unavailable`.

This focused closure reuses the probe's existing browser-side enumerate bound of at most 3 seconds, the Host's existing 8-second
media readiness bound, and the parent 120-second watchdog. The existing 250 ms poll cadence also serves as the channel
response/cancel-settle margin, so the first Python channel await takes at most 7.75 seconds, and the actual local relation is
`3 < 7.75 < 8 < 120`. When the margin is under 500 ms, no new RPC is issued. The 8 seconds here is the media stage
deadline, not a new global Host launch command deadline. Although the failure path already has an explicit budget of 5 seconds for Playwright close
plus 6 seconds for process-tree closure, server shutdown / file I/O still has no total deadline, and the recorded stages of the failed
A1 total 35.837 seconds and do not include all launch work, so a strict global
cleanup margin cannot be claimed on this basis. This patch does not disguise a global command/cleanup budget that has not been frozen as established.

## 9. The only real verification matrix

Real verification must run, on the same native Windows interactive desktop, with the same fixed engine / Host build, and under the same local
first-party probe origin, strictly only three sequential sessions:

| Run | Host / Profile                    | Artifact                             | Initial state                         | Must prove                                                                         |
| --- | --------------------------------- | ------------------------------------ | ------------------------------------- | ---------------------------------------------------------------------------------- |
| A1  | independent Host H1 / `fp1-a`     | A raw SHA                            | run-owned empty Profile               | First identity observation; boot `0→1`; write A state                              |
| A2  | new Host H2 / same `fp1-a`        | exactly the same raw bytes/SHA as A1 | The same Profile after A1 clean close | Target identity equals A1 field by field; boot `1→2`; Cookie/LocalStorage continue |
| B1  | new Host H3 / independent `fp1-b` | B raw SHA                            | run-owned empty Profile               | Identity separation corresponding to the static A/B diff; boot `0→1`; no A state   |

A2 may start only after the A1 close/shutdown, Job active count `0`, all processes exited, and both Profile lock bytes can be reacquired.
B1 may start only after A2 is likewise clean. The three runs use the same fixed probe origin, so that
Profile B not inheriting A's Cookie/LocalStorage is a valid counterexample. B1 must not hide
state cross-talk by changing the origin.

Before the run, a canonical field-by-field diff of the A/B `resolvedConfig` must be generated, and each difference must be mapped to the coverage table.
The acceptance comparison rules are:

- A1/A2: only a field that is both "truly run/session non-identity entropy" and "not backed by an
  explicit `resolvedConfig` value or a materialized replay seed" may be excluded. The target fields
  must be deeply equal. The `session-variable` class in the old policy cannot override FP1's Artifact
  ownership. `windowScreenX`, `windowScreenY`, Canvas `rawHash` / `exportHash`, Audio, the
  full WebGL summary, font availability/width, voices, and media device results must each be
  compared separately. When they cannot be observed reliably they must be written as `unavailable`, and must not be silently excluded;
- A/B: expected differences are required only for the target fields that the Artifact static diff can affect. Common fields should still be coherent and
  stable, and "the summaries differ" does not replace the field mapping;
- `ObservedWebsiteDigest v2` may serve as an auxiliary check, but it excludes Canvas, internal seeds, and Artifact
  font inputs, and under inherit font mode it excludes font widths, so it cannot be the sole evidence for FP1;
- A configured field that the probe cannot observe stays at `configured` by default. It may be upgraded to `applied` only when audited
  per-key engine consumption/application evidence exists. When the current engine cannot control it reliably, mark it
  `unavailable`. In neither case may it be inferred as `observed` from the config value.

## 10. Focused automated regressions

Besides A1/A2/B1, only focused regressions that do not expand the real matrix are allowed:

1. After the Python/NumPy RNG state is perturbed, the complete normalized sent config for the same raw Artifact
   still equals the disk config key by key;
2. Any added/removed/changed key still yields `config_mutation` before the browser spawns;
3. Each new stage produces only start + terminal, flushed immediately, and stdout stays pure JSONL;
4. Secret/path sentinels do not appear in the newly added stage stderr, the persistent diagnostics, or the tracked/sanitized Gate
   summary. The controlled absolute root in the existing Host hello may remain only in the gitignored local raw bundle
   and must not be copied into the committed summary;
5. A fake delayed/hung call can return the timeout/error classification of the corresponding stage and enter the existing
   fail-closed cleanup, without launching the real browser matrix;
6. The existing regressions for the Profile lock, the Windows Job, and exact process ownership continue to pass.

No new real loop is added for the occasional second-Host problem. If the problem arises naturally in A1/A2/B1, handle it per the next section.

## 11. Stop, failure, and limited repair rules

- **Randomness source unclassified**: stop before A1. Complete the ledger, and do not run the browser hoping for luck.
- **v3 cannot express an important identity source**: stop and submit a schema/ADR. Do not change fixtures or persist anything secretly.
- **A1/A2 identity field drift**: FP1 failed. Save the two raw observations and the minimal field diff, and fix only the located
  projection/seed root cause. Do not mask it by excluding fields or modifying the digest.
- **Undeclared differences appear in A/B, or an expected difference is missing**: FP1 failed. Fix the static projection mapping first,
  and do not satisfy separation by expanding randomization.
- **A lifecycle hang occurs naturally and the last stage is clear**: fix only that specific call and add one focused
  regression. After cleanup is confirmed, rerun only the affected A1/A2/B1 sequence, and do not restore the R2/R2H matrix.
- **Hang again but no reliable last stage**: stop real runs immediately and rule the diagnostics insufficient. Fix observability first,
  and do not add counts, a runner, or a manifest.
- **Proven to be a fixed Camoufox/Playwright upstream problem**: save the redacted minimal reproduction and version binding, stop, and
  return to the lead agent to choose a pinned upgrade or a workaround. The execution agent must not upgrade on its own.
- **close/Job/lock cannot be confirmed**: keep failed/quarantined and ownership, do not start the next run,
  and do not manually delete the lock and pass the run off as clean.
- **Requires RuntimeManager, UI, proxy, FP2–FP4, or a public protocol/schema change**: stop and
  report the out-of-scope dependency. Do not expand FP1.

## 12. Evidence semantics

FP1 follows these throughout:

| State         | Meaning in FP1                                                                                                                                                                                                  |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `configured`  | The value exists in an Artifact that passed raw SHA/schema validation. Full disk/sent equality still only proves that the projection is ready                                                                   |
| `applied`     | The exact config was handed to the fixed engine, the persistent context returned successfully, no config mutation/rejection occurred, and the field has audited per-key engine consumption/application evidence |
| `observed`    | The designated observation channel returned the value: identity fields use the first-party probe, and process/lifecycle uses a clearly labeled runtime receipt                                                  |
| `verified`    | FP1 does not produce this. All results keep `verified:false`                                                                                                                                                    |
| `unavailable` | The current fixed engine or probe cannot reliably control/observe it. The reason must be kept                                                                                                                   |

A1/A2 agreement can only be written as `observed stable on this Windows host`. It does not prove cross-physical-host,
cross-browser-version, iframe/Worker, TLS/QUIC, network identity, complete font isolation, or undetectability. B1
separation does not mean that every Silo has a unique, unlinkable device identity.

## 13. Result package and Gate output

Raw output is written to the sole gitignored `artifacts/camoufox-fp1/<run-id>/` and must contain no secrets.
If the existing Host hello contains a controlled absolute root, it may stay only in this local raw bundle. The tracked Gate
result must use repository-relative paths and be redacted. FP1 does not create a tracked evidence manifest, schema,
runner, or freezer. The final result must append a Gate result at the end of this contract and include at least:

1. source, receipt-producing, and evidence-only (if any) commit/tree and clean/ancestry;
2. the fixed Host/browser/dependency/Artifact raw SHA bindings;
3. the complete randomness source ledger and the final coverage table;
4. the A/B static config diff;
5. for each of A1/A2/B1: the raw result relative path, SHA-256, Host/session identity, stage
   endpoint, clean close, Job, lock, and residual-process results;
6. the normalized sent config full-equality result, and the field-by-field observation diff;
7. the isolation results for Cookie, LocalStorage, boot `0→1→2`, and B `0→1`;
8. the exact commands, exit codes, and counts of the focused tests;
9. the actual values, sources, and strict inequalities of the timeout hierarchy, or the explicit blocked reason;
10. each `configured/applied/observed/unavailable` conclusion, and the capabilities that still cannot be claimed;
11. integration extraction: the minimal production diff that can enter a future product patch, the focused
    regressions, and the research infrastructure that must be excluded;
12. the final Gate: `Accepted`, `Failed`, or `Blocked`. Do not write only that the overall tests are "green".

The coverage table uses the following fixed format, and difficult surfaces must not be deleted:

| Fingerprint surface       | Artifact control                           | Fixing mechanism                             | A1/A2              | A/B                     | Observed fields                                 | digest v2 coverage  | Evidence status | Limits/exclusions                                                  |
| ------------------------- | ------------------------------------------ | -------------------------------------------- | ------------------ | ----------------------- | ----------------------------------------------- | ------------------- | --------------- | ------------------------------------------------------------------ |
| UA / Navigator / platform | 10 explicit values; except touch           | v3 key-by-key replay                         | No result obtained | Not executed            | all navigator fields and touch                  | Yes                 | `configured`    | A1 did not reach `observed.identity`                               |
| Headers                   | `Accept-Encoding` explicit                 | v3 key-by-key replay                         | No result obtained | Not executed            | The current probe has no request header channel | No                  | `configured`    | Actual header observation `unavailable`                            |
| Locale / timezone         | Explicit                                   | v3 key-by-key replay                         | No result obtained | Not executed            | language(s), timezone, UTC offset               | Yes                 | `configured`    | Direct-only; not observed                                          |
| Screen / window / DPR     | screen/outer/offset explicit               | v3 explicit values; inner/DPR are host-bound | No result obtained | Not executed            | screen, DPR, full geometry                      | Partial             | `configured`    | A1 wrote no observed result                                        |
| History                   | Explicit                                   | `window.history.length`                      | No result obtained | Not executed            | `historyLength`                                 | Yes                 | `configured`    | Not observed                                                       |
| Canvas raw                | `canvas:seed`                              | materialized replay seed                     | No result obtained | Not executed            | `observedFull.canvas.rawHash`                   | No                  | `configured`    | Restart stability cannot be verified                               |
| Canvas export             | `canvas:seed`                              | materialized replay seed                     | No result obtained | Not executed            | `observedFull.canvas.exportHash`                | No                  | `configured`    | The historical risk is still not closed                            |
| Audio                     | `audio:seed`                               | materialized replay seed                     | No result obtained | Not executed            | `audioHash`                                     | Yes                 | `configured`    | Not observed                                                       |
| WebGL / WebGL2            | parameters, extensions, precision explicit | v3 key-by-key replay                         | No result obtained | Not executed            | vendor/renderer/summary of both                 | WebGL1 partial      | `configured`    | The new WebGL2 probe obtained no real result                       |
| Fonts / metrics           | fonts + spacing seed                       | explicit list + materialized seed            | No result obtained | Not executed            | availability/widths                             | Partly excluded     | `configured`    | fonts preparation complete does not equal probe observation        |
| Voices                    | Full list explicit                         | v3 key-by-key replay                         | No result obtained | Not executed            | full voice list including default               | Old shape           | `configured`    | Not observed                                                       |
| Media devices             | Counts of three classes explicit           | Windows fake-media prefs                     | A1 hung            | Not executed            | `mediaDevices`                                  | Yes                 | `unavailable`   | Stuck in the Playwright RPC inside readiness                       |
| `maxTouchPoints`          | None                                       | native value used after candidate deletion   | No result obtained | Difference not required | `maxTouchPoints`                                | No                  | `unavailable`   | closed allowlist; host-bound                                       |
| Cookie / LocalStorage     | Owned by the Profile                       | Profile persistence                          | No result obtained | Not executed            | API/page/SQLite/boot                            | Non-identity digest | `unavailable`   | A1 did not return, so state writing or isolation cannot be claimed |

## 14. Gate result

### 2026-08-10 execution conclusion

**Failed.** This is the failure result given by the execution owner per the frozen contract. It is not the lead agent's Accepted judgment,
and it does not change the `failed` / `experimental` status of M3-WI.

Implementation binding:

| Object                              | commit / tree                                                                                            |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Documentation checkpoint            | `d4fd8993b6e51a47e9bdd9c84ceaa5cb1b328f35` / `2d65e0efde241b239109532465a5891c4720b8be`                  |
| receipt-producing implementation    | `6362e91a05413fce981f8e738d11ac21a169da48` / `06e3a129febb8e3bd9781cc46c80781aa2730fd2`                  |
| Fixed browser archive               | Camoufox `v152.0.4-beta.28` / SHA-256 `386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d` |
| Fixed Python execution dependencies | Camoufox `0.5.4`, BrowserForge `1.2.4`, Playwright `1.60.0`                                              |
| Artifact A raw SHA-256              | `a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`                                       |
| Artifact B raw SHA-256              | `ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9`                                       |

The pre-implementation focused regression holds: under 100 groups of Python/NumPy/permitted environment entropy perturbations of the same raw A Artifact,
the normalized sent config equals the on-disk 47-key config key by key and type by type every time. The only candidate
extra that appeared is `navigator.maxTouchPoints` from the closed policy, and unknown extras and invalid types all fail closed.
This only proves deterministic projection prepared, and does not replace browser observation.

The canonical 47-key static diff of A/B is exactly: `audio:seed`, `canvas:seed`, `fonts`,
`fonts:spacing_seed`, `navigator.hardwareConcurrency`, `screen.availHeight`,
`screen.availTop`, `screen.availWidth`, `screen.height`, `screen.width`,
`window.history.length`, `window.screenX`, `window.screenY`. B1 was not executed, so these
configured differences were not upgraded to applied or observed differences.

The real run stopped strictly at A1:

| Run | Launched | Result                                                                                                |
| --- | -------- | ----------------------------------------------------------------------------------------------------- |
| A1  | Yes      | The parent timed out after waiting 120 seconds for the first launch response; stdout frame count is 0 |
| A2  | No       | Stopped per the contract after A1 failed; the second Host was not launched                            |
| B1  | No       | Stopped per the contract after A1 failed; the B browser identity was not created                      |

The persistent stage log of A1 gave a reliable boundary:

```text
launch_options:success                 285 ms
launch_persistent_context:success   30,660 ms
supervisor_job_bind:success              0 ms
new_page:success                      3,722 ms
goto:success                            105 ms
observed.fonts:success                1,065 ms
observed.media:start
<no terminal before the 120-second parent watchdog>
```

This is therefore not the indistinguishable set of persistent context / new page / later RPC from the old report. The natural reproduction has
narrowed to the `wait_for_configured_media_devices()` stage. The nominal 8-second
deadline of this helper cannot bound the unbounded Playwright RPC inside it. The existing evidence cannot yet attribute uniquely between its `page.evaluate()` and the
later `page.wait_for_timeout()`. This round did not modify the timeout, rerun A1,
or launch A2/B1 on this basis.

raw bundle (gitignored, not a manifest, not Accepted evidence):

- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/fp1-run-summary.json`,
  SHA-256 `e42613bea6b53dc6bd770f5b3ccae352f723a113ea31fece615726e05572df8a`;
- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/state/host-stderr.log`,
  SHA-256 `51865285975b1c85d45e3bf5ca1f84b56f59135730c7e7dff3db17c4a5bce910`;
- `artifacts/camoufox-fp1/run-20260810T145548Z-0f5dddff/a1/failed-attempt.json`,
  SHA-256 `82d68080d2c07707cc526fbb3ec660e67b2a62cc8da0b7a2d1ffd680c55406aa`.

The ownership closure after the failure holds, but it cannot be packaged as a clean protocol close: the Host was ended by the harness after the
watchdog; the exact PID/creation-time identities of the supervisor and Camoufox have both exited,
the supervisor `exit.json` is `0`, the named Job is closed with no active processes, Profile bytes 0/1 can both be reacquired,
and no task child processes remain. The stage log contains no URL, Cookie name/value, Artifact seed, lease token,
proxy sentinel, or user absolute path. The native browser lease has been released by the matching token.

Focused verification:

- `uv run --frozen --offline python test_identity_artifact.py`: 32/32 passed;
- `uv run --frozen --offline python test_windows_host.py --close-context-regression`:
  the four cases normal/timeout/exception/job-not-exited passed;
- `py_compile` of the modified Python files, probe JavaScript syntax, Prettier, Markdown relative links,
  `git diff --check`: passed.

Integration extraction stays limited: a future product patch may extract the candidate-extra closed policy,
deterministic normalization, the per-launch 11-stage diagnostics, the Job fail-closed ownership with launch-before-context, the minimal probe fields, and the focused regressions.
It must not extract the R1/R2/R2H runner, the freezer/finalizer, the manifest/schema, historical receipts, or this run's Profile/cache/log.

Before entering FP2 the following is still missing: first establish a bounded failure attribution and a single
focused regression for the specific Playwright RPC inside `observed.media`, and then run the full A1→A2→B1 under a new authorization. There are currently no A1/A2 fingerprint field-by-field results,
so it cannot be claimed that the same Artifact replayed stably, that Canvas raw/export is stable, that A/B separated, or that FP1 passed.

### 2026-08-11 follow-up execution conclusion

**Blocked (`blocked-upstream`).** This is not FP1 Accepted, and it does not change the
`failed` / `experimental` status of M3-WI. The implementation started from `68861a37923f7fcd44f68f1435cc808c87fcc496`
/ tree `dfe3c5cf9d4d66b23aac1af7886e5426b6266b67` and was frozen at
`bc153f13c18af9f7404e3cb6674e3b29a18de800` / tree
`e3a788f6a06de9a8860b9c3bfe6ad0583f7f534d`. The worktree was clean before the real browser launch, and the source code
was not modified during the run.

The media readiness is optional Runtime Evidence before the full probe. It is not a Host
launch hard barrier for the Artifact having been applied successfully. The actual count returned by the full probe is what is `observed`. Even if it differs from configured,
it is still an observed mismatch, and only an inability to return reliably is unavailable. The old helper's local 8-second
deadline was checked only after the unbounded Playwright `evaluate` / wait RPC returned, so it could not
produce a typed terminal before the parent watchdog. This fix adds a browser enumerate of at most 3 seconds, a Python channel of at most
7.75 seconds, a 250 ms response/cancel margin, and a total 8-second media-stage budget. The fixed terminal reasons are
`success`, `enumerate_timeout`, `readiness_timeout`, `count_mismatch`,
`playwright_exception`, or `unavailable`. The local hierarchy is
`browser JS ≤ 3s < channel ≤ 7.75s < media stage 8s < parent 120s`. It does not falsely claim that a global
Host command deadline exists.

All focused verification passed:

- `uv run --frozen --offline python test_identity_artifact.py`: 39/39;
- `uv run --frozen --offline python test_windows_host.py --close-context-regression`:
  the four cases normal/timeout/exception/job-not-exited passed, `secretFree=true`;
- `py_compile` of the modified Python files, the embedded media JavaScript syntax, Prettier, Markdown links,
  `git diff --check`: passed.

The only real sequence launched A1 once and A2 once. After the A1/A2 comparison failed it stopped immediately, and B1 was not launched:

| Run | Host PID / session                          | boot  | media                           | close / ownership                                                          |
| --- | ------------------------------------------- | ----- | ------------------------------- | -------------------------------------------------------------------------- |
| A1  | `7252` / `024a543c1ddb4d91904498555cc88f20` | `0→1` | `success`, 234 ms, counts 1/1/0 | clean close; Job 0; both Profile locks can be reacquired                   |
| A2  | `8552` / `df49c2779d9248b2b9cca0b13cb21a7e` | `1→2` | `success`, 203 ms, counts 1/1/0 | clean close; Job 0; Cookie/LocalStorage continued; locks can be reacquired |
| B1  | Not launched                                | —     | —                               | Stopped per the contract after the A1/A2 Canvas export mismatch            |

A1/A2 used the same raw Artifact SHA
`a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f`, the same
47-key normalized config, configured digest, Profile, and probe origin, but two independent Hosts.
Navigator, locale/timezone, screen, full window geometry, DPR, Canvas raw, Audio,
WebGL/WebGL2, fonts/widths, voices/default, history, media, and the auxiliary ObservedWebsiteDigest
were all deeply identical. `maxTouchPoints=0` stays host-bound / unavailable. The value in the Cookie SQLite was confirmed to have been
written by A1 and continued by A2, but the value itself was not written to logs or tracked results.

The only hard comparison item that differs is Canvas export:

| Field               | A1                                                                        | A2                                                                        | Result |
| ------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ------ |
| `canvas.rawHash`    | `sha256:acd1515152354d8bb340e2cf4d9516f48762504eb370ce5d0b3339801f9cfe10` | Same as A1                                                                | Same   |
| `canvas.exportHash` | `sha256:8a362ea76f7e46a7ce130b372eee1b59c618f71ecc4bf0ab158904a69cb090af` | `sha256:06d110cba2cd32c852359be9ef5227085254722bdf55a823bd2dbf59aad0c87e` | Drift  |

For a deterministic scene, the probe first hashes the `getImageData()` RGBA, and then independently hashes the complete
`toDataURL("image/png")` string of the same canvas. There is no probe RNG. The identical raw hash rules out drift of the drawing input and the raw
pixels. The identical raw Artifact, normalized config, and configured digest rule out projection drift.
The source tree of the fixed Camoufox `v152.0.4-beta.28` has deleted `canvas-spoofing.patch` and the
per-context `SetCanvasSeed` path, and the existing `fingerprint-injection.patch` has no Canvas hook either.
Therefore, although `canvas:seed` is still accepted by the config format, it has no auditable per-key application path, so it can only stay
`configured` and cannot be written as `applied`. The actual failure boundary lies in the fixed Camoufox/Firefox Canvas export
path, not in the Host, the Playwright transport, the static projection, or the probe hash implementation.

The A/B static 47-key diff is still exactly the frozen 13 keys: `audio:seed`, `canvas:seed`, `fonts`,
`fonts:spacing_seed`, `navigator.hardwareConcurrency`, `screen.availHeight`,
`screen.availTop`, `screen.availWidth`, `screen.height`, `screen.width`,
`window.history.length`, `window.screenX`, `window.screenY`. The Artifact B raw SHA is
`ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9`. Since B1 was not launched,
these configured differences were not upgraded to applied/observed, and no browser evidence was obtained for Profile B isolation.

The raw bundle is still gitignored, not a manifest, and not Accepted evidence:

- `artifacts/camoufox-fp1/run-20260811T023336Z-03647283/fp1-run-summary.json`,
  SHA-256 `a355b68cf27cdfae5991016fb7e0761a1f09c044a117ac268179f0c22669a325`;
- the A1/A2 `attempt.json` SHA-256 values are respectively
  `6c74d645b133ddea4cae9ca05bbd1f2b65b91b8a1a7e60bdfcbf8ed5406fa3ae` and
  `56b39070a8329b5d587f42dfd304219de442c74e441079d68163db13dab87e6f`;
- `a1-a2-comparison.json` SHA-256
  `5e8d11e7cc52d44f5c5a02068cb04dc713de9bc6d6b786452e6f045d00c77ccf`.

All of this task's Python Host, Playwright Node, supervisor, Camoufox, Job, and probe server processes have
exited. Both Job Objects are closed with 0 active processes, the probe port can be rebound, and Profile bytes
0/1 can be reacquired. The atomic browser lease with the matching token has been released. There was no retry, B1, FP2, dependency upgrade,
public schema/API change, push, or PR.

The minimal upstream proposal to continue closing FP1 is: pin and rebind a Camoufox build that has an explicit, auditable Canvas export application path
that is deterministic across independent Hosts (while updating the archive/tree/binding and making
the existing focused tests pass again). Another option is to explicitly modify the Artifact/FP1 contract and reclassify
export as unavailable/session-variable. This would change shared semantics, and this round must not adopt it on its own just to get green.
Until the lead agent authorizes one of them, A1/A2/B1 must not be rerun, and FP2 must not be entered.

### 2026-08-11 Canvas Engine Patch contract freeze

The lead agent has chosen the minimal engine decision above: keep the Artifact/Silo-scoped Canvas identity, pin FF152
and maintain a VeriSilo downstream patch. Export must not be downgraded to optional, and site or
browsing-session entropy must not be mixed into the Managed Canvas identity.

#### Browser contract

For a Managed config where `canvas:seed` exists (including the value `0`):

```text
same seed + same Canvas operation + independent browser process
→ exact same PNG/dataURL observable

different seed + same Canvas operation
→ deterministic different PNG/dataURL observable
```

The key is fixed as
`SHA-256(ASCII("verisilo-canvas-export-v1\0") || uint32_be(seed))`. Do not add
site/origin, Profile, PID, time, session UUID, or any other run entropy. The patch only replaces the randomization key of the Canvas export
resolver. Firefox's original PNG encoder/`deBG` structure continues to be used, and raw pixels
get no noise. When `canvas:seed` is missing, it must go through Firefox's CookieJar fallback unchanged.

The hard observation surface of FP1 v1 is `toDataURL("image/png")`. The PNG blob path of the shared resolver needs
source/test coverage, but this round does not write iframe, Worker, Service Worker, or cross-realm consistency as an FP1
pass condition. Other encodings such as JPEG/WebP must not be extrapolated as controlled from the PNG result.

#### Binding and compatibility

- The upstream Camoufox tag/commit is pinned to `v152.0.4-beta.28` /
  `0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d`; Firefox is pinned to `152.0.4`;
- The new source lock, patch, archive lock, tree manifest, and rebound Artifact fixtures use new names.
  They do not overwrite the official lock, the old Windows tree manifest, `identity-win-a/b/c`, or historical evidence;
- The provenance of the self-built archive must not be written as GitHub official digest agreement, and stays
  `verified:false`;
- The `canvas:seed` field and the top-level v3 Artifact schema are unchanged. The old binding continues to require the legacy
  session-variable Canvas policy; only the new binding allows the deterministic Canvas Policy v3
  variant. The validator must select the unique variant by the archive binding, failing closed.

#### Focused verification

After the browser source, patch set, archive, tree, and focused harness are all frozen and the worktree is clean,
first take the atomic browser lease, and execute the following sequence exactly once:

```text
canvas-A1: seed A / new Profile A / independent process
canvas-A2: same seed A / same Profile A / second independent process
canvas-B1: A config with only canvas:seed changed to seed B / Profile B /
           third independent process
```

The three runs must use the same run-owned loopback HTTP origin and the same probe bytes. `about:blank`,
`page.set_content()`, `data:`, and `file:` are prohibited. Requirements:

- The A1/A2 raw RGBA, decoded PNG pixels, PNG bytes, and dataURL hashes are all identical;
- B1's final config equals A key by key and type by type except for the seed, the PNG is valid, and the export hash differs from A;
- Each run has a clean close, Job active 0, both Profile lock bytes reacquirable, and no residual processes;
- The stock fallback when `canvas:seed` is missing has at least a source-level focused regression. If no pre-declared
  browser control is added, it must not be written as runtime-observed.

These three runs are not the complete FP1, and they do not use the other 12 differing keys of the full Artifact B. Only after the focused
sequence passes once may the complete A1→A2→B1 be executed exactly once from the re-frozen final code and the new binding.
Any failure stops immediately, with no rerun after modification, no sample selection, and no entry into FP2.

### 2026-08-11 Canvas source patch implementation result

The Canvas source patch was frozen at
`eea8606b6512922cfcf071e977a9e0cf2958deaf` / tree
`5693e8879348970078aeab79c51eab2ecfc333c0`. The build-route provenance and the exact-test
no-skip closure were additionally frozen at `dbad62d24c6e8f6d57bb29ba5864a308584e9ca4` / tree
`df761be583b0171335a0955a7af46cb683f6e0bf`. The implementation adds:

- a narrow downstream patch for the fixed FF152 `nsRFPService::GetFingerprintingRandomizationKeyAsString()` seam.
  Only when `canvas:seed` exists does it derive an Artifact/Silo-scoped key; when it is missing, the original CookieJar fallback is
  kept;
- a `verisilo-camoufox-source-binding/v1` source lock that binds the upstream tag/commit/tree,
  the Firefox source archive, the 50 upstream patches, the recipe/input trees, the downstream patch, and the
  seam pre/post hashes. Its status is explicitly `source-patch-only`, `verified:false`,
  `buildBinding.status=not-built`, `binaryBinding=null`;
- strict pre-spawn validation of `canvas:seed` as an unsigned 32-bit value: `0` and `2^32-1` are valid, and negatives and
  `2^32` fail closed;
- focused source-binding tests. The default mode must provide all exact source inputs, and any skip fails.
  `--tracked-only` is only an explicitly downgraded tracked-text self-consistency check, not source/build evidence.

Exact static verification results: 9/9 source-binding tests, 40/40 existing Artifact/Host focused tests, `py_compile` of the modified
Python files, Prettier on the source lock, forward/reverse patch dry-run, and
`git diff --check` passed. The 9/9 only proves the exact archive digest, the upstream input closure, that the patch applies,
the seam hashes, and the current caller-file closure. It does not prove the full source derivation, compilation, the Windows
binary, or runtime behavior.

The current execution conclusion is **Blocked (`blocked-build-environment`)**. The Windows
build of the pinned Camoufox tag is completed by cross-compilation in a Linux execution environment. Upstream also supports a Linux Docker container on a Windows physical host, but does not support
a direct native-Windows or WSL build. The current machine has no
Docker/container engine, and this round had no authorized external Linux build host. Installing a machine-level
container runtime or triggering external CI is beyond the existing local implementation authorization. Therefore no candidate
archive/tree/binding was generated, no browser lease was taken, no browser was launched, and neither the Canvas focused run
nor the full FP1 A1→A2→B1 was executed. `canvas:seed` still cannot be upgraded from configured to applied/observed, neither FP1 nor
M3-WI is Accepted, and FP2 is not open.

### 2026-08-15 Windows candidate build/finalization checkpoint

The `blocked-build-environment` of the previous section was the execution conclusion at that time. That environment block has now been lifted. A supported
Linux container build completed the Windows-target compilation from source commit
`4f1f01f00844e1888139b4236424550c94a6e10f` / tree
`35da4d372bf8f468062c6eb5ea64187be1c6d595`. The original source lock
keeps its raw bytes unchanged, and the binary closure is added by a new browser asset lock. The historical fields of the source lock must not be
rewritten into a post-build state.

| Object                  | Exact binding                                                                                                                                                                                                                                                                        |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| build run               | `canvas-engine-20260815t041500z-4f1f01f`                                                                                                                                                                                                                                             |
| source lock SHA-256     | `1b6271855474ac6d264eaa7f5830b4125457e488747f9c53f9b9bbae321e8b82`                                                                                                                                                                                                                   |
| Windows archive         | SHA-256 `8221486f42f547603339da7442e4c412671afc66d6742d01f99918f12f85be1d`; 493,054,882 bytes                                                                                                                                                                                        |
| build result            | SHA-256 `55b4a5f7f62742ab9eebb59cd953ab9ce2dc0c7a38e519c179c75c8c37989a61`; `compiled-not-runtime-verified`                                                                                                                                                                          |
| binary metadata         | BuildID `20260811045234`; SourceStamp `e39c605adc0fc049a165d7fe4a3f6517b761edf7`                                                                                                                                                                                                     |
| `camoufox.exe`          | SHA-256 `bb247f1c21a8e8aa4c590bec01b499ced307cd9fdf6974e24309fa7e52963a16`                                                                                                                                                                                                           |
| asset lock              | `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-canvas-v1-windows-x86_64.json`; raw SHA-256 `0ce34b8a44c90e6c313aad66030a800359bbca78b97ca565cbdefa4e4eb95cfe`                                                                                                           |
| tree manifest           | `tests/fixtures/camoufox/browser-tree-manifest-verisilo-canvas-v1-windows.json`; raw SHA-256 `68ae52e3d11bba5b2868b68ea90af962840c6890a4418fc24199ba9a96138bf3`; canonical SHA-256 `ebc35ddbdc59c32b9856b56a9dcfc6e375d5a090abc6b498238e6cd874c09dfa`; 503 files / 981,198,096 bytes |
| runtime seam checkpoint | `970b357d8e2cf0ffb619d2e478519a12242abc5a` / tree `6119c15f18aebe49cae488cd0f9870ba37ad42e4`                                                                                                                                                                                         |

The new lock explicitly uses `verisilo-camoufox-browser-asset/v3`, `assetKind=self-built`,
`verified:false`, and `evidenceClass=compiled-not-runtime-verified`. It does not claim GitHub
official digest agreement, and does not overwrite the historical official lock/tree/evidence.

The rebound runtime inputs corresponding to the new binding are:

| Purpose            | Path                                                         | raw SHA-256                                                        |
| ------------------ | ------------------------------------------------------------ | ------------------------------------------------------------------ |
| focused A / full A | `tests/fixtures/camoufox/identity-win-canvas-v1-a.json`      | `240feeddd7b3e36de48f1c2c89f1b4a1397e4f94dcbbc369099bd75738c86ee2` |
| full B             | `tests/fixtures/camoufox/identity-win-canvas-v1-b.json`      | `765260741bb9d428a9b6a453c6d9f955b7219191654b6d8ce4e1c33bf2d4a68e` |
| focused seed-B     | `tests/fixtures/camoufox/identity-win-canvas-v1-seed-b.json` | `18fe872d40d948f149f0b517cd1d49f353f9edfec53e637c58e6fc054946b1df` |

The validator selects the deterministic Canvas Policy v3 only under the complete, exact new browser binding. The old official fixtures continue to select the legacy Canvas classification, and unknown, crossed, or partial bindings
all fail closed. The Host's self-built path requires an explicit asset lock/browser root, and checks the full tree against the lock-bound raw/canonical manifest before the cache copy
and before every launch. The Host v1 stdout
JSONL wire shape stays unchanged. The probe keeps the old raw/export fields and adds independent hashes for PNG bytes, a valid decode,
decoded pixels, and the dataURL.

Exact verification results before the static freeze: source/input closure 39/39 (0 skip), Artifact/Host 49/49,
candidate finalizer 6/6, focused harness 10/10, the four Windows close regressions, byte-for-byte verification of 503/503 files of the real extracted
tree, Python `py_compile`, probe JavaScript syntax, and
`git diff --check` all passed. Prettier's local dependencies had not been restored after the machine-level fault recovery, so this round obtained only
JavaScript syntax evidence and does not write Prettier as having been run.

This checkpoint took no browser lease, did not launch this candidate, and did not execute the Canvas focused run
or the full FP1 A1→A2→B1. Archive/lock/tree finalization therefore cannot promote `canvas:seed` from
configured to applied/observed and cannot claim that the Canvas contract has passed at runtime. FP1 stays open,
M3-WI stays `failed` / `experimental`, and FP2 is not open. The next real run is still strictly one
Canvas focused A1→A2→seed-B1; only after it passes and is frozen again is one complete FP1 A1→A2→B1 allowed.

### 2026-08-15 Canvas focused runtime checkpoint

The one-shot focused path above was executed from clean HEAD
`96546259574f73ec50f4c3715c3d978641155d8d` / tree
`4a3248f4564ddb5f8983b93db74faca9a77f77d8`. The candidate archive, asset lock, tree
manifest, rebound A/seed-B fixtures, and probe are all exactly identical to the previous checkpoint. The local
gitignored harness raw SHA-256 is
`c9016d747b72cecbce64495f5bd27761520c518480084468b9595e5f33157d73`. The candidate-level atomic
one-shot claim has been consumed, with no retry or sample selection.

| Object               | Result                                                                                                                                                                                     |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| run                  | `canvas-focused-20260815T081637990579Z`                                                                                                                                                    |
| report               | SHA-256 `1329506655594e5a65b73ffbad6edb0f7f45f6ddeecd92ecc6d7f1f3e8263460`; 18,864 bytes; sidecar matches                                                                                  |
| A1/A2 Artifact       | raw SHA-256 `240feeddd7b3e36de48f1c2c89f1b4a1397e4f94dcbbc369099bd75738c86ee2`                                                                                                             |
| seed-B Artifact      | raw SHA-256 `18fe872d40d948f149f0b517cd1d49f353f9edfec53e637c58e6fc054946b1df`; relative to A, the 47-key config changes only `canvas:seed`                                                |
| A raw RGBA           | `sha256:acd1515152354d8bb340e2cf4d9516f48762504eb370ce5d0b3339801f9cfe10`; A1=A2                                                                                                           |
| A decoded PNG pixels | `sha256:ba28621cd5cfb8ef73dc52ffbb1882fbbaa8bdc1c52f37929894f66e191e1f71`; A1=A2                                                                                                           |
| A PNG bytes          | `sha256:2f9d2d223c4eed37e334569db0447daa3cb2ac7e30a4f96a046dac6464d14c8d`; A1=A2                                                                                                           |
| A dataURL            | `sha256:72962259071308c39c0ed11bb90976d0fce49e765de49d39ee9fa179134d9d3e`; A1=A2                                                                                                           |
| seed-B pixels        | raw RGBA and decoded PNG pixels both equal A                                                                                                                                               |
| seed-B export        | PNG bytes `sha256:72f0ae10f0a7ee3ac60529311bd6806eb535e15cd5970a5570994dabd988e2b3`; dataURL `sha256:ef41ec1f2b3fbc31b75b917debe99c9aa0c51a8c4fdc35ce81a1ba1d99114d4e`; both differ from A |

A1, A2, and seed-B1 used three different Host, session, and supervisor/browser process identities. The A
Profile boot was `0→1→2`, and the seed-B Profile boot was `0→1`. All three PNG signature/decode/240×120/mime
checks hold. Each close was `exited` / exit 0, Job active 0, forced cleanup
`not_needed`, and managed residual empty, and both the Profile and supervisor lock bytes could be reacquired.
After the terminal state there were 0 target processes, no probe port listener, and the tracked worktree was clean. The independent review also
recomputed the report, the sidecar, the one-shot claim, and the three Host protocol/stderr hashes. Each protocol
has exactly four successful responses: hello/launch/close/shutdown.

This checkpoint may only be written as "the Canvas focused contract of this self-built candidate has observed passed on this native
Windows host". It still keeps `verified:false`. The missing-seed fallback has only the
frozen source-level regression and no runtime control. The full 47-key A1→A2→full-B1 has not been executed yet,
so FP1 is not Accepted, M3-WI is unchanged, and FP2 is not open. The next step is only to execute one complete FP1 from a re-frozen clean
checkpoint. B1 must use
`identity-win-canvas-v1-b.json` and must not be replaced by the focused seed-B.

### 2026-08-15 Full FP1 one-shot lifecycle failure checkpoint

The one-shot path of the complete FP1 was executed from clean HEAD
`7a5465e8f971a81ef067be4735c764ce3cb0ea29` / tree
`460a386f12fc46b004a2d8121d31cb0208d07b05`. Before the run, the 503-file browser tree, the two 47-key Artifacts A/B, the frozen 13-key static diff, the focused
pass report, and the port and process terminal state were verified again byte for byte. The raw SHA-256 of the gitignored full harness and of the no-browser tests are respectively
`409cebe8ef3ff6ea7da072f585a6b62762b87a811dda5820f7f25a4496464dce` and
`4b2243fc5f56230ea5ae1e00113249aeb68e6322ad3146bf2cb326f193f0a2d5`, and 14/14 passed.
The media readiness is kept only as auxiliary evidence, and the authoritative hard Gate is still the actual device counts returned by the full probe.

| Object               | Result                                                                                                                                          |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| run                  | `fp1-full-20260815T090107649930Z`                                                                                                               |
| report               | SHA-256 `9c90b95d3f4932535d6d3fb46be5232429ae5587dc66d7e3ee565386566a49a7`; 5,741 bytes; sidecar content matches                                |
| one-shot claim       | SHA-256 `50573440ad2cfec3b9621319d71032ff0dc7914f026f44f7398125014557d95a`; the candidate-level sole claim has been consumed                    |
| Actual sequence      | Only `A1 hello → launch → close → shutdown`; A2/B1 not launched                                                                                 |
| A1 session           | `f5ba8345c4334164bf99a8260ec82685`; boot `0→1`; supervisor PID 9452 / browser PID 3912                                                          |
| A1 raw observation   | `state/f5ba8345c4334164bf99a8260ec82685/observed.json`; SHA-256 `69f64f2dd7632b42da42120feb58ec817220ba9ca88bc075d2b1f04aad98d6d3`              |
| A1 protocol / stderr | SHA-256 `37122c60a71642b5ae1e9317822d68a5cbb136fd3b00b32849ded7b1c9d5c141` / `09724bcf939ffc86949c49f5795f881b4d7176d091122ff1ff8786c2aa38b1f3` |
| A1 session receipt   | SHA-256 `c518e598b7b68035e1d0aa9b09bb9de186d4ddd421a1cd8b66d99acdc647a49b`                                                                      |
| Gate                 | **Failed: `unclean_close` / `state='failed'`**                                                                                                  |

The identity observation of A1 itself returned completely: the normalized 47-key sent config passed the Host's
disk/sent/digest Gate; the Canvas raw, decoded pixels, PNG bytes, and dataURL match
focused A; the media readiness first matched at 343 ms, and the full probe also actually observed 1
`audioinput`, 1 `videoinput`, and 0 `audiooutput`; the Cookie, LocalStorage boot, and
SQLite evidence were all written to disk. This failure is therefore not a media, Canvas, or config projection failure.

The hard failure occurred at A1 close: `page.close()` succeeded within 16 ms, and then `ctx.close()` did not return within the existing
10-second lifecycle budget (a 9,992 ms timeout was recorded). There was no natural exit file, and
`exitStatus=null`. The Host then performed named Job forced cleanup under fail-closed ownership, finally
`activeProcessCount=0` and `remaining=[]`, but forced cleanup cannot pass as a clean close.
The independent terminal review confirmed that the supervisor/browser/Host/full runner were all not alive, 18191 had no listener and could be
rebound, both lock bytes of the A Profile and of the global lease could be reacquired, and the tracked worktree was clean.

`observations=[]` and `windowsRuntimeObserved=false` in the report are conservative short-circuit semantics: the helper did not
promote A1 to a public observation before the clean-close validation threw. The raw `observed.json` and the
protocol prove that A1 really ran and obtained a partial Windows runtime observation. They must not be interpreted as
A1 not having launched, and they must not bypass the lifecycle Gate to be upgraded to complete FP1 evidence.

Where `ctx.close()` stopped internally, whether at the Playwright channel acknowledgement, the closed future, the API
request dispose, or the Firefox shutdown, the existing raw evidence is insufficient, and the root cause must stay
`inconclusive`. No evidence supports modifying media/probe/digest, relaxing forced cleanup, adding a retry,
speculatively extending a timeout, or restoring R2/R2H. The one-shot full claim of this candidate with the same implementation must not be deleted or reused.
If work continues later, it may only first add secret-free focused sub-stage diagnostics for `ctx.close()` and a fake
regression, and after locating the specific subcall and freezing a new evidence-backed fix, the lead agent decides a new limited
verification. The current final Gate is **FP1 Failed / not Accepted**; M3-WI keeps its status, and FP2 stays closed.

### 2026-08-16 close root-cause closure and bounded-close fix decision

The lead agent authorized "exhaust every means to solve FP1, and do not enter FP2 for now". Diagnosis continued along the allowed paths above:

1. **Experiment C (corrected marker timestamps)**: the new harness replaces batch draining with a line-by-line real-time stderr timeline,
   reuses the Experiment A Host hooks, and adds process CPU sampling during close and a thread snapshot at the timeout point.
   `close-media-baseline-20260816T070221012301Z` closed cleanly on its first run (`ctx.close` 784 ms
   success, exit 0, no Job cleanup; the `TypeError` in the harness recording layer of that run affected only the result.json
   classification and not the Host close receipt), and then `close-hang-hunt-20260816T070700549038Z`
   closed cleanly in all 8 consecutive brand-new A1 sessions. The cumulative total for that day is 11/11 clean.
2. **Race nature confirmed**: the 3 hangs of 08-15 and the 11 clean closes of 08-16 used the same binary, the same
   Artifact, the same Host, and the same-shaped sequence. `mediaDeviceReadiness.matched=true` appeared in both groups,
   so "fake media backend activation" is only an accompanying phenomenon and not a sufficient cause. This pattern matches the intermittent history of 2 `ctx.close` timeouts in 6 runs on the official build in the M3-WI R2 era,
   and is a close race that spans versions and clusters by system state.
3. **Upstream code-level convergence** (evidence chain):
   - The Playwright driver's `doClose` for an FF persistent context returns `"close-browser"`,
     and the Python `ctx.close()` finally enters Juggler `Browser.close`;
   - In camoufox `0583c3e`'s self-maintained Juggler
     `additions/juggler/protocol/BrowserHandler.js`, `['Browser.close']` `await`s in order before
     `Services.startup.quit(eForceQuit)`: the idleTasks/window-close race, `this._startCompletePromise`, and `Promise.all(XPIProvider.startupPromises ++ enabledAddonsStartupPromises)`, all with no timeout. A stall in any one produces "no close response sent +
     the process does not exit", which matches all observations of Experiments A/B;
   - In Experiment A, `request.dispose` succeeded with a 3.6 ms round trip 10 ms before the stuck RPC, which rules out a
     pre-existing dead pipe (the dead-pipe form of issue #719 of the same upstream version);
   - The `connection.disconnected()` disconnect fallback calls the same `['Browser.close']`, so there is no way
     on the Host/transport side to bypass this path. The fix point can only be in the browser patch.
4. **Fix frozen**: adds the downstream patch
   `0002-verisilo-juggler-bounded-close.patch` (SHA-256
   `efb006d5b2b05756fc310b52eb48e0bdab5e8b23e780fa08534a7fc099c22ce7`), which adds a shared 3,000 ms deadline to the whole
   pre-quit sequence: a stage that times out or rejects is dumped to stderr with the
   `[verisilo-juggler-close]` prefix and then skipped, which guarantees that
   `quit(eForceQuit)` is always reached. The patch changes only one file, `juggler/protocol/BrowserHandler.js`, and the
   seam pre/post SHA-256 are `7eadb3dd…61afc0` / `3b5d24b6…3b38e` (the pre is the original upstream
   additions file: none of the 50 upstream patches touch the juggler path). The source-level engine revision
   is raised to `canvas-export-v1-close-bound-v1`; the binary asset binding and
   `SELF_BUILT_ENGINE_REVISION` keep pointing to the existing `8221486f…` candidate until the rebuilt candidate is verified.
   The source closure was frozen at `9ee93e4`, strict_build applies 0002 with `--fuzz=0` and validates the new seam, and
   the tracked-only closure passes with 33 items.

Subsequent bounded chain (FP1 remains Failed until it is complete): rebuild the Windows candidate on a controlled builder → new
archive/tree validation and a new self-built binding (also raising `SELF_BUILT_ENGINE_REVISION` and the
Policy variant binding) → focused A1/A2/seed-B1 and the close regression → execute exactly one new
full FP1 from the new candidate. The diagnostic runs and upstream forensics are kept in the gitignored
`artifacts/camoufox-fp1/close-diagnosis/` (including the `upstream-juggler/` original files).

Backlog (a pre-existing anomaly unrelated to this fix): the `test_windows_host.py` protocol check asserts that
`"stage":"response write"` must appear, but the current Host activates the stage recorder only on the `launch` command, so a
pure hello session cannot satisfy it. This assertion fails the same way on clean `0455dec`; it is a stale test assertion that needs a separate
fix and must not be mixed into this fix chain.

### 2026-08-18 close-bound candidate build/finalization checkpoint

The two steps above, "rebuild the Windows candidate → archive/tree validation and the new self-built binding", are now complete:

- build run: `canvas-close-engine-20260816t144711z-e571f6c`;
- engine revision: `verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1`;
- source commit/tree: `e571f6c0b2cea90955b929a4ff04ad54007778fa` /
  `7d6e41dd892c68aadccb3d177f5e33a6bd974486`;
- archive: SHA-256
  `148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94`,
  493,100,709 bytes;
- executable: `camoufox.exe` SHA-256
  `172f51387bc61e331446883e5499c67611aea5fd81091f68df26b166c9687bf1`;
- extraction tree: 503 files / 981,205,753 bytes; manifest raw/canonical SHA-256
  `3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9` /
  `42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1`;
- self-built lock raw SHA-256:
  `ce05302d317ec562b096eba52e806ed20302d99d472229640c5eea840d7f98ac`.

The lock/tree and the rebound A/B/seed-B fixtures were statically re-verified by the production validator. This result is still
`compiled-not-runtime-verified` / `verified:false`. This checkpoint did not launch a browser, did not consume the focused one-shot claim of the new
archive, and does not change the Failed history of the old full FP1. The next real execution is strictly
limited to one Canvas focused A1→A2→seed-B1, with a hard verification of clean close in the same sequence. Only if the focused run
passes and is frozen again is one new full FP1 allowed. FP2 stays closed.

### 2026-08-18 close-bound Canvas focused runtime acceptance

The sole close-bound focused run has completed and passed:

| Item                 | Frozen result                                                                                                                                                                                                                                                                                                                                               |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| producing checkpoint | commit `91c7f4252126d5286509c79e2ce98c82e44aa1ef`; tree `ec14bcc6e4c2f8a2a669807c29a25e95533e460e`                                                                                                                                                                                                                                                          |
| run                  | `canvas-focused-20260818T095204967986Z`                                                                                                                                                                                                                                                                                                                     |
| report               | SHA-256 `06a9b68876dbeef5a2c34a0fe43f03efb949bfddea1a3dc2eb71d160069e170d`; 18,862 bytes; sidecar matched                                                                                                                                                                                                                                                   |
| one-shot claim       | SHA-256 `799856606b2903697cc97775e0673482e0e4f2d912a0774a4ebac648ab27072e`; 517 bytes                                                                                                                                                                                                                                                                       |
| harness              | SHA-256 `fa63703abb858c3f9843728b97b18b653044545e20bfcde536f00e97ea51cee8`                                                                                                                                                                                                                                                                                  |
| A Artifact           | raw SHA-256 `e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb`; boot `0 -> 1 -> 2`                                                                                                                                                                                                                                                          |
| seed-B Artifact      | raw SHA-256 `b79d370843302e753450ca5e57d44c1c901f03a8ba73c6a4a585c1efd6c6327e`; relative config diff only `canvas:seed`; boot `0 -> 1`                                                                                                                                                                                                                      |
| A Canvas             | raw RGBA `sha256:acd1515152354d8bb340e2cf4d9516f48762504eb370ce5d0b3339801f9cfe10`; decoded pixels `sha256:ba28621cd5cfb8ef73dc52ffbb1882fbbaa8bdc1c52f37929894f66e191e1f71`; PNG bytes `sha256:2f9d2d223c4eed37e334569db0447daa3cb2ac7e30a4f96a046dac6464d14c8d`; dataURL `sha256:72962259071308c39c0ed11bb90976d0fce49e765de49d39ee9fa179134d9d3e`; A1=A2 |
| seed-B Canvas        | raw/decoded pixels equal A; PNG bytes `sha256:72f0ae10f0a7ee3ac60529311bd6806eb535e15cd5970a5570994dabd988e2b3`; dataURL `sha256:ef41ec1f2b3fbc31b75b917debe99c9aa0c51a8c4fdc35ce81a1ba1d99114d4e`; both separate from A                                                                                                                                    |
| close                | A1/A2/seed-B1 `ctx.close=success`, about 0.726/0.372/1.021 s; exit 0; Job active 0; no forced cleanup/remaining process                                                                                                                                                                                                                                     |

The Host/session/supervisor/browser identities of all three runs are independent; the PNG signature, decode, size
240x120, and MIME are all valid; the Profile and supervisor lock bytes are reacquirable; at the terminal state there is no target
process or 18191 listener. What is accepted is therefore only the Canvas focused sub-Gate of this binary binding.
`verified` is still false, the missing-seed case still has only a source regression, the full FP1
has not been executed, FP1 is not yet Accepted, and FP2 stays closed.

The next action is fixed as: bind this report/claim/checkpoint exactly to the full A/B 47-key harness,
and after completing the no-browser counterexamples and a clean checkpoint, run only one new full FP1
`A1 -> A2 -> full-B1`. The focused run must not be rerun, and the evidence of the old `8221486f...` candidate must not
pass as the current close-bound binding.

### 2026-08-20 FP1 Offline Evidence Closure

This phase prohibits a real browser, deleting a claim, modifying original evidence, and entering FP2. The actual execution
obeyed these four restrictions. The full output of the read-only baseline is saved in the gitignored closure bundle, and the check result is:

| Baseline item         | Exact value                                                                |
| --------------------- | -------------------------------------------------------------------------- |
| branch                | `codex/camoufox-m3-engine-adapter`                                         |
| HEAD                  | `b7a615ac39606deb741b3b3ea13d3584a987a39c`                                 |
| tree                  | `9461adcc6924539dc4c2bb80963fab71a2efef49`                                 |
| tracked / cached diff | Both empty                                                                 |
| baseline receipt      | SHA-256 `689fa4c2714cbf4d6a98a7d5bb9d0d0b46a8576d449f56621cbfdbeef4d6b804` |

The current byte closure was completed and fails closed before the comparator was modified: the task contract, the status page, the ignored full
harness/test, the true full report/sidecar/claim, the 12 referenced files of A1/A2/B1,
the A/B/seed-B Artifacts, the probe, the `0002` patch, the source lock, the asset lock, the tree manifest,
the archive, `camoufox.exe`, the complete extracted browser tree, and the focused report/sidecar/claim
all match. The byte-closure receipt SHA-256 is
`2e5f46872f1108b037e55bfe8320e4b950d9c1fc3df2561e01a154d73af3986f`. The production
verifier re-hashed 503 files / 981,205,753 bytes, with canonical tree SHA-256
`42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1`.

#### Comparator contract restoration

Only the gitignored `run_fp1_full_windows.py` and `test_fp1_full_windows.py` were modified. The final raw
SHA-256 values are respectively
`c7c6b90b01a02e46a83b934e50f6569261ab7ac25c037f26b805e8eef074c9d6` and
`c09ede5a2077099e7623aa56b504bf1eaa3ff2d4b279b03ceba7c204bc8e7a57`.
The restored verdict is strictly:

1. All six items `rawHash`, `rawRgbaHash`, `exportHash`, `dataUrlHash`,
   `pngBytesHash`, `decodedPngPixelsHash` are identical for A1/A2;
2. For the seed-only focused B, raw/raw-RGBA/decoded pixels equal A, and PNG bytes,
   dataURL, and export separate from A;
3. Full B no longer unconditionally requires raw to equal A. Only when the Artifact's cross-surface drawing inputs
   `fonts` / `fonts:spacing_seed` change, the corresponding observed injected-font/width evidence
   also changes, and the common families show no drift, are raw/raw-RGBA/decoded pixels allowed to change together;
4. Each observation continues to hard-verify the PNG signature, decode, 240x120, `image/png`, and the existing
   hash alias/internal consistency;
5. Any unexplained common-family drift, or any report/sidecar/claim/referenced-file
   hash mismatch, fails closed.

`py_compile` passed; full harness no-browser tests 21/21, focused harness 10/10, candidate
finalizer 6/6. The newly added regressions explicitly prove: a raw difference between A1/A2 fails; a raw difference in seed-only B fails; a font-driven raw difference in
full B is not misjudged by raw equality; an unexplained common-family
drift fails; an invalid PNG fails; and report, sidecar, claim, and referenced hash mismatches all fail.

#### Corrected adjudication of the immutable original evidence

The sole adjudication object is the original run `fp1-full-20260818T103842543519Z`:

| Immutable object               | SHA-256 / result                                                                             |
| ------------------------------ | -------------------------------------------------------------------------------------------- |
| report                         | `45a40123a1877e1c97d6989f5ba763c32b62a3b1c0e6f74968da13e957abd588`                           |
| report sidecar raw bytes       | `6d98e8df5382b2b2344636f9bcccacb03d765ce61dd2eb6a649d41dfae5e1f43`                           |
| one-shot claim                 | `f609ff5d72353bc820e69b509d5764a509a3f438a1967eddeea5d41bc3ff8e12`                           |
| original status / failure      | `failed` / `ab_mapping_failed: canvas.rawHash`                                               |
| corrected offline adjudication | `passed`; receipt SHA-256 `2494a0da712bf6598c774dc7b33657784d78226b05a92536e4984c211f51106d` |

The adjudication re-read and re-hashed all the original referenced evidence, and did not rewrite the report, the failure code, the claim,
the Artifacts, or the probe. All 14 hard families of A1/A2 are identical, and all six Canvas hashes are identical. The 13-key static diff of full B
maps exactly. The differences in Canvas raw/raw-RGBA/decoded pixels are jointly explained by the changed
`fonts` and `fonts:spacing_seed` and the corresponding injected-font/width evidence; the common
families are stable, and PNG/export separation and internal consistency are valid. A boot is `0 -> 1 -> 2` and B boot
is `0 -> 1`; A's Cookie persists, and B's Cookie/Profile is isolated; the media configured counts match in all three runs.
Each of the three protocols has four successful responses (hello/launch/close/shutdown) and a clean secret scan. All three runs closed cleanly, exit 0, Job active 0, forced cleanup `not_needed`. Lifecycle, storage,
media, and integrity are therefore all closed.

This `verisilo-camoufox-fp1-offline-adjudication/v1` is a separate offline adjudication receipt. It is not a new runner report,
and does not change or impersonate the original runner verdict.

#### Main-brain Gate

- **Original runner verdict:** `failed`;
- **Reason:** false negative caused by non-contractual A/B `canvas.rawHash` equality assertion;
- **Main-brain FP1 Gate:** Accepted based on immutable original A1/A2/B1 evidence plus corrected contract adjudication;
- **verified:** `false`;
- **FP2:** Not entered; it may proceed only as the next separately frozen task.

This Gate only closes the FP1 deterministic Artifact projection execution risk. It does not rewrite M3-WI, does not open
UI/proxy/production packaging, and does not write offline evidence as a verified product capability. After the closure, the original report/claim
re-hash still matches the values above.
