# VeriSilo Managed Fingerprint FP2 — Cross-Realm Consistency

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical generation record**: this candidate is retired and Generation 6 is permanently closed. Current work does not inherit this document's one-shot/claim/matrix. Read it only when investigating Gen1–Gen5 evidence. For the current Gate, see the [status page](camoufox-program-status.md).

- Status: **FP2 generation 1 blocked / generation 2 failed on HTTP harness / generation 3 failed on insufficient historical failure evidence / generation 4 failed formal execution with confirmed probe lifecycle semantic defect / ServiceWorker semantic closure accepted / generation 5 execution package frozen / claim not created**
- task version: generation 1 `fp2-v1`; generation 2 execution package `fp2-v2`; generation 3 execution package `fp2-v3`; generation 4 execution package `fp2-v4`; generation 5 execution package `fp2-v5`
- execution boundary: this file, the separate `tests/fingerprint-probe/fp2/` bundle, the pure comparator/runner, and gitignored FP2 evidence
- `verified`: always `false`

This document only freezes the FP2 execution contract and the actual inputs of this run. The long-term product intent, the Camoufox-first route, the division of work between the lead agent and the execution agent, and the current Gate are each governed by the four authoritative sources of truth. The FP1 original runner `failed`, immutable evidence, offline adjudication, Accepted checkpoint, and `verified:false` all stay unchanged. This document does not reinterpret FP1 and does not open FP3.

## Fixed inputs

A real run is allowed only on the current close-bound Windows candidate, and only once as `A1 → A2 → B1`:

| Input                             | Fixed value                                                                           |
| --------------------------------- | ------------------------------------------------------------------------------------- |
| engine revision                   | `verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1`                   |
| source commit                     | `e571f6c0b2cea90955b929a4ff04ad54007778fa`                                            |
| archive SHA-256 / size            | `148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94` / `493100709`      |
| `camoufox.exe` SHA-256            | `172f51387bc61e331446883e5499c67611aea5fd81091f68df26b166c9687bf1`                    |
| asset lock SHA-256                | `ce05302d317ec562b096eba52e806ed20302d99d472229640c5eea840d7f98ac`                    |
| browser tree manifest raw SHA-256 | `3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9`                    |
| browser tree canonical digest     | `42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1`                    |
| browser root                      | `artifacts/camoufox-fp1/windows-candidate-20260818T061456Z-e571f6c/extracted-browser` |

FP2 recomputes the Artifact SHA from the current files and sidecars. It does not use the old prompt values:

| Artifact | Path                                                    | Current raw SHA-256                                                |
| -------- | ------------------------------------------------------- | ------------------------------------------------------------------ |
| A        | `tests/fixtures/camoufox/identity-win-canvas-v1-a.json` | `e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb` |
| B        | `tests/fixtures/camoufox/identity-win-canvas-v1-b.json` | `a35b40d7023f93313167e4e9cbd07fe090cad8d20421cf80c78c3ac48b7c7821` |

Both `resolvedConfig` objects have 47 keys, and the static difference is exactly:

```text
audio:seed
canvas:seed
fonts
fonts:spacing_seed
navigator.hardwareConcurrency
screen.availHeight
screen.availTop
screen.availWidth
screen.height
screen.width
window.history.length
window.screenX
window.screenY
```

If the actual diff does not equal these 13 keys, the runner returns `blocked: artifact_baseline_drift` before the claim. It does not modify the Artifact and does not consume the claim.

## Realms and observation relations

The fixed bundle covers: Top Window, same-origin iframe, cross-origin iframe, DedicatedWorker, SharedWorker, and ServiceWorker. The two explicit origins use the same controlled server:

```text
primary:   http://127.0.0.1:<selected-run-port>
secondary: http://localhost:<selected-run-port>
```

The cross-origin iframe accepts only the exact parent origin, source window, and session nonce. All timeouts, error messages, and nonce/source/origin mismatches fail closed. Each realm issues the same kind of `GET`/`no-store`/`credentials=omit` fetch, and the server returns and records the identity headers. Origin, Referer, Sec-Fetch-\*, Accept, and Cookie are recorded only as request context and do not take part in the cross-realm equality judgment.

The frozen mapping of identity headers is:

```text
HTTP User-Agent        == navigator.userAgent
Accept-Language token  == frozen-order mapping of navigator.language / navigator.languages
Accept-Encoding        == resolvedConfig["headers.Accept-Encoding"]
DNT / Sec-GPC          == the corresponding navigator value that is configured and actually supported by the engine
```

For the Window family, Navigator, locale/timezone/UTC offset, screen/DPR, Canvas, Audio, WebGL/WebGL2, fonts, voices, media, and identity headers must be coherent across the same Artifact. Geometry/history are checked, per the relation matrix, only for stability within the same realm type, so that legitimate iframe context differences are not misjudged as identity drift. Workers compare at least WorkerNavigator, Intl, UTC offset, and request headers. OffscreenCanvas, PNG blob, Worker WebGL/WebGL2, Worker fonts, DNT/GPC, and maxTouchPoints are classified by the applicability ledger fixed before the run.

The Canvas window family uses the same drawing scene and records separately raw, raw RGBA, decoded PNG pixels, PNG bytes, dataURL, export hash, and PNG signature/decode/size/mime. If applicable, the Worker records raw pixels, blob PNG bytes, decode, and PNG validity. The relation between window `toBlob` and Worker `convertToBlob` is handled per the relation matrix, and byte equality is not assumed without evidence. A raw change in the full B is allowed only when `fonts` / `fonts:spacing_seed` and the corresponding font/width observation change in sync. The seed-only export relation must not be rewritten.

The ServiceWorker must register for the first time and be activated in A1, and must record the script path/SHA/scope/controller or controlled page, plus MessageChannel evidence. In A2, a new Host with the same `fp2-a` must reuse the registration, and the evidence must be stable. In B1, the fresh `fp2-b` must not inherit A's registration, Cookie, or LocalStorage.

### ServiceWorker activation lifecycle semantic closure

`navigator.serviceWorker.ready` only establishes a barrier that the registration has a
non-null active worker. The registration's `active` worker may first be in `activating`,
and then reach `activated` through `statechange`. The probe therefore must not treat the
resolve of `ready` directly as proof that `active.state === "activated"`.

FP2 ServiceWorker evidence now keeps the same registration, script, scope, controller/
controlled-page, and MessageChannel contract, and runs the following state machine
within the existing activation/stage budget of `REALM_STAGE_DEADLINE_SECONDS = 15`:

```text
ready returned
→ active missing: service_worker_active_missing
→ active.state == activated: continue
→ active.state == activating: install statechange listener, re-read state, await activated
→ activating → redundant: service_worker_redundant
→ deadline exhausted: service_worker_activation_timeout
→ other state: service_worker_unexpected_state:<state>
```

The wait listens only to a `statechange` transition that has already begun, and re-reads
the state after the listener is installed to close the event race. It does not re-register,
sleep, retry, reload, or enlarge the existing timeout. `activated` still does not mean the
current page is controlled; the controller or controlled-page check remains a separate
later step.

The semantic audit of this closure is:

```json
{
  "probeSemanticChange": true,
  "measurementContractCorrection": "ready is not an activated barrier",
  "observabilityChange": true,
  "unchanged": [
    "realm surfaces",
    "collector order outside ServiceWorker activation barrier",
    "Canvas scene",
    "HTTP request shape",
    "Artifact mapping",
    "applicability",
    "relation matrix",
    "frozen timeout value",
    "controller/controlled-page contract"
  ]
}
```

## Frozen ledgers and timeout

Fixed and hashed before the run:

- `tests/fingerprint-probe/fp2/probe-bundle-manifest.json`; each bundle file has its own SHA-256;
- `tests/fingerprint-probe/fp2/applicability-ledger.json`;
- `tests/fingerprint-probe/fp2/relation-matrix.json`;
- `apps/camoufox-host/fp2_cross_realm.py`; the pure comparator and the runner use the same exact bytes;
- `apps/camoufox-host/test_fp2_cross_realm.py`;
- the current A/B raw Artifacts, sidecars, and the candidate lock/tree/archive/executable.

This run may use only the following frozen event-driven margins, and does not modify the existing Host close timeout or parent watchdog:

```text
browser-side operation deadline: 3 s
< realm stage deadline:           15 s
< session watchdog:               60 s
< existing parent watchdog:       120 s
```

The existing Host close/context/process-tree budget (10 s / 8 s) keeps its original value. If this margin cannot be established, return `blocked: timeout_budget_unfrozen` before the claim.

## Output boundary

The one-shot claim is created atomically with `O_EXCL` before the first A1. It binds the current HEAD/tree, the candidate, the A/B raw bytes, the bundle/ledger/relation/comparator/runner/no-browser hashes, the selected port, and the run ID. Once the claim exists, it is consumed. A failure or interruption does not retry, does not change the port, does not change the Artifact, does not change the probe, and does not delete the claim.

Raw evidence is written only to `artifacts/camoufox-fp2/`. The report and sidecar, the applicability/relation/static diff, the three realm observations, the request-header captures, the ServiceWorker registration, the lifecycle/protocol/stderr, the referenced hashes, the final offline adjudication, and the byte closure are all bound by SHA. Sanitized evidence does not write Cookie/LocalStorage values, Artifact seeds, Vault/token/proxy secrets, user absolute paths, or the full argv/environment. It keeps only the continuity/isolation boolean conclusions and hash bindings.

The execution agent's success string can only be `execution-passed-awaiting-main-brain-gate`. A failure or block can only be `failed` or `blocked`. This file must not be written by an execution agent as FP2 Accepted, Managed Identity verified, or FP3 Open.

## Historical Runtime Preflight Closure: generation 2 pre-claim path after generation 1 blocked

### Generation 1 frozen outcome

The old claim of generation 1 is kept permanently and must not be overwritten:

```text
claim: artifacts/camoufox-fp2/fp2-v1-one-shot-claim.json
claim SHA-256: e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402
run: fp2-20260820T121344Z-470b08fdb9
browser observations: 0
classification: pre-browser-runtime-dependency-block
```

The blocked state of this claim came from the bare `python` child lacking `camoufox`. Neither the Host nor the browser was launched. It is not browser semantic evidence, and it is not a claim that can be deleted or reused. Generation 2 is a new execution package that fixes the claim-before-runtime-preflight contract gap. It is not a retry with a changed run-id/port.

### Hard pre-claim Gate (generation 2 historical package)

Before any generation 2 claim is created, the runner must complete the following in order:

```text
candidate / Artifact / probe byte closure
→ no-browser static tests
→ exact interpreter closure
→ exact child-path runtime preflight
→ synthetic success/failed/blocked report finalization
→ process / port / preflight-lock clean
→ only then O_EXCL create fp2-v2 claim
→ explicit browser execution A1 → A2 → B1
```

The fixed child runtime is:

```text
relative interpreter: apps/camoufox-host/.venv/Scripts/python.exe
implementation: CPython
version: 3.12.13
camoufox: 0.5.4
playwright: 1.60.0
browserforge: 1.2.4
```

The preflight uses the same interpreter, repository cwd, `PYTHONPATH`, `PYTHONUNBUFFERED`
environment construction, and runner script as the real session child. It only imports the
Host/FP2/runtime dependencies, resolves `AsyncNewBrowser` and its actual launch helpers, and
stops before `AsyncNewBrowser(playwright, from_options=opts, persistent_context=True)`. It does
not open the candidate, does not create a Profile, does not create a lock, does not listen on a
loopback port, and does not consume the claim.

The preflight receipt must bind the interpreter SHA, the dependency closure SHA, the child
invocation SHA, the runner SHA, the selected port, and the previous blocked claim, and must
explicitly state `browserLaunchCalled:false`, `browserProcessCreated:false`,
`profileCreated:false`, and `lockFilesCreated:false`.

The runtime preflight is a deterministic pre-claim environment integrity check. It may be run
repeatedly before claim creation, including one independent closure and one pre-claim closure
inside the formal runner. It does not create a browser process, does not produce a realm
observation, and does not consume the one-shot. It is therefore not browser evidence selection,
and it does not constitute generation 3 or a retry. A preflight failure must return `blocked`
before the claim is created.

For the target-process clean check, the existing Windows `tasklist.exe` backend is preferred.
If that backend returns Access Denied or another unavailable result, the runner uses a separate
PowerShell `Get-Process` backend to query `camoufox.exe` and
`verisilo-camoufox-supervisor.exe`. An unavailable backend must not be read as empty. Only when
none of the allowed backends can prove the target process state does the runner return
`blocked: process_cleanliness_unverifiable`.

### Generation 2 claim boundary (historical)

The generation 2 claim uses a separate path:

```text
artifacts/camoufox-fp2/fp2-v2-one-shot-claim.json
```

It must explicitly reference the generation 1 blocked claim, and bind the new implementation
commit/tree, the runner SHA, the no-browser test SHA, the runtime preflight receipt SHA, the
runtime dependency closure SHA, and the child invocation SHA. The candidate archive, executable,
asset lock, browser tree, Artifact A/B, probe bundle, applicability ledger, and relation matrix
keep using exactly the same frozen bytes.

The default runner invocation only runs the Runtime Preflight Closure. Only when
`--execute-browser-matrix` is explicitly provided and all pre-claim Gates have passed may the
generation 2 claim be created and one `A1 → A2 → B1` run be entered. Any child runtime
dependency, interpreter, spawn-boundary, finalization, process, port, or lock failure must
return `blocked` before the claim. If the runtime binding changes after claim creation, the run
fails closed. It must not silently switch the interpreter.

`ensure_sanitized(report, label)` is the fixed call form for finalization. The success, failed,
and blocked synthetic reports must each be able to write the report sidecar, the offline
adjudication sidecar, and the byte closure. This closure produces no browser observation and
does not change FP1/FP3/M3-WI/Standard status.

### Runtime Preflight Closure result (generation 2 historical checkpoint)

The previous Runtime Preflight Closure passed on a clean implementation checkpoint. This
paragraph records the generation 2 pre-claim checkpoint, not the current Gate. Afterward the
generation 2 formal claim was consumed. After the browser launched, the run stopped because the
HTTP evidence handler failed. See the frozen record below.

## Generation-2 frozen formal failure

Generation 2 is a real formal execution. The claim and evidence are kept permanently and must not be overwritten or rerun:

```text
run: fp2-20260821T053550Z-9f98de991d
claim: artifacts/camoufox-fp2/fp2-v2-one-shot-claim.json
claim SHA-256: bcf9170cb26e46a35664ebad3cd8b39a2ec93928e597b21b84037e8cc6f22b67
implementation: 359aa14be6c2d3bf5ef912ce1655f089fb7d7b85
browser launched: true
valid realm observations: 0
header capture count: 0
classification: harness-http-capture-failure
```

The failure happened when the actual loopback HTTP request handler called the evidence capture
API. The handler's `self.command` was wrongly read from `FP2HTTPServer`, which triggered
`AttributeError`, and the A1 realm probe then ended with `ProtocolError`. This evidence proves
that the generation-2 measurement harness could not complete collection. It is not a
cross-realm capability verdict on the Managed Engine. The FP2 capability is still unadjudicated,
and all results stay `verified:false`.

## Historical Generation-3 Harness Closure (pre-run checkpoint)

Generation 3 only fixes the harness seam located above, and adds a real no-browser loopback regression:

```text
request handler owns: method / path / request headers
FP2HTTPServer owns: evidence capture storage
```

The regression must go through the real `FP2HTTPServer`, a standard-library loopback HTTP
request, and the actual request-handler class. It must verify method/path/realm/header capture,
malformed metadata fail-closed, sanitization, and port release after shutdown. It must not
substitute a direct call to the capture method for this seam.

The generation-3 claim must explicitly reference the generation 1 blocked claim and the
generation 2 failed harness claim. The two old claims, reports, and referenced evidence stay
byte-identical. Generation 3 may change only the new implementation commit/tree, the runner/test
hash, the fresh runtime-preflight closure, and the new claim. The candidate, Artifact A/B, probe
bundle, applicability ledger, relation matrix, and static 13-key mapping must stay frozen. No
browser may be started before the new claim is created. If Gen3 again produces an unauthorized
harness failure, do not create generation 4 on your own.

This section records the historical checkpoint before the generation-3 formal run. The harness
closure implementation and the no-browser regression were complete. The new immutable
implementation checkpoint and a fresh runtime closure were the next Gate. The generation-3 claim
had not been created yet, and A1/A2/B1 had not been started yet. FP1 was still
`Accepted / verified:false`, and FP3 was still `Closed`.

## Generation-3 Failure Observability Closure

The immutable generation-3 formal run is preserved as a failed execution:

```text
run: fp2-20260821T055448Z-8f6b69c851
claim SHA-256: 0cf358045f86257af3126eb27b8f21f8df254984ee8c1fe91f6f79bbe44e09c7
implementation: daf42584ad1bae670aa9c25fcb67650bafd20590
browser launched: true
valid realm observations: 0
header captures: 0
execution classification: failed
root-cause adjudication: blocked / gen3_failure_evidence_insufficient
```

The preserved evidence establishes Python bootstrap, Host hello, browser context,
page creation, navigation and FP2 top-script start. It does not identify the first
operation inside the old `collectWindowRealm()` because the probe saved only
`error.name`; the old claim, report, sidecar, adjudication and byte closure remain
byte-preserved and are not reinterpreted.

This closure changes only measurement observability. The new probe failure object is
versioned and carries `realm`, `stage`, `operation`, `errorName`, bounded sanitized
`errorMessage`, `lastSuccessfulStage` and `probeCompleted:false`. Window and Worker
collectors preserve their existing operation order; frame/Worker messages and the
Python child/parent/report/offline-adjudication path retain the same object. No
Canvas scene, HTTP request shape, Artifact mapping, applicability, relation,
timeout, Worker behavior or ServiceWorker behavior is changed.

The machine-readable semantic audit for this closure is:

```json
{
  "probeSemanticChange": false,
  "observabilityChange": true,
  "changed": [
    "stage markers",
    "error name/message preservation",
    "sanitized failure metadata"
  ],
  "unchanged": [
    "realm surfaces",
    "operation order",
    "Canvas scene",
    "HTTP request shape",
    "Artifact mapping",
    "applicability",
    "comparison rules",
    "timeouts"
  ]
}
```

The old probe bundle manifest SHA-256 was
`95e27ceb55e687841dd13398b869bc8709d2edef845ca4584cfadd5b3c5370cc`.
The observability-closure manifest SHA-256 is
`b4be8f80d56621b817b351ccb12d51d8b04eeafe6d9bc26d6e03c144799e621c`.
Generation-1/2/3 evidence remains bound to its own historical probe hashes; none is
rewritten. The new manifest is only a future execution-package binding.

No browser was started by this closure and no generation-4 claim was created.
At that historical closure checkpoint, Generation 4 remained closed pending a separate
main-brain decision; FP2 remained `Failed / Not Accepted`, its capability verdict
remained unresolved, and FP3 remained closed.

## Generation-4 execution package freeze

The main brain has authorized generation 4 after accepting the generation-3 failure
observability closure. This no-browser closure aligns the executable runner with that
authorization without changing FP2 measurement semantics:

```text
executionGeneration: 4
taskVersion: fp2-v4
claimPath: artifacts/camoufox-fp2/fp2-v4-one-shot-claim.json
claimSchema: verisilo-camoufox-fp2-one-shot-claim/v4
```

The generation-1, generation-2 and generation-3 claims remain separate immutable
history. Generation-4 claim creation verifies each historical claim's exact SHA-256,
identity and preserved failed/blocked classification before any new claim can be
created. The generation-3 claim and report are also checked as the formal failed run
with zero valid realm observations and zero header captures.

The probe bundle, applicability ledger, relation matrix, candidate, Artifact A/B,
13-key mapping, timeouts and browser engine are unchanged. This package freeze creates
no claim, browser process, profile or realm observation; a fresh runtime preflight
receipt bound to the new runner checkpoint is required before the generation-4 claim
Gate can proceed.

## Generation-4 ServiceWorker lifecycle semantic closure

The immutable generation-4 failure is preserved as a failed formal execution. Its first
structured probe failure was `service_worker_not_activated` in the top-window
`serviceWorkerEvidence` stage. Offline review established that the old probe treated
`navigator.serviceWorker.ready` as an `active.state === "activated"` barrier. That is
too strong: `ready` can resolve once a non-null active worker is assigned while the
worker is still `activating`.

The probe now waits for the existing worker's `statechange` within the unchanged
15-second realm-stage deadline. The wait is event-driven, installs the listener before
a second state read to close the listener-installation race, accepts `activated`, and
fails explicitly for `redundant`, unexpected states, or deadline exhaustion. It does
not re-register, sleep, retry, reload, or change the controller/controlled-page check;
an activated worker and a controlled current page remain separate predicates.

The Host mapping also preserves a structured probe failure without reading an
undeclared `ProtocolError.detail` attribute. This closure changes the ServiceWorker
lifecycle measurement semantics to match the required state machine and preserves the
failure metadata path; it does not change the other realm surfaces, HTTP request shape,
Artifact mapping, applicability ledger, relation matrix or timeout value. The semantic
audit is:

```json
{
  "probeSemanticChange": true,
  "observabilityChange": true,
  "measurementContractCorrection": "ready is not an activated barrier"
}
```

The current probe bundle manifest SHA-256 is
`d69e61c4da482c8cebaed912a6c24b57b73ac0c465a9fafd6a0be8dc974cfb37`. Historical
generation-1/2/3 claims and evidence remain bound to their original probe hashes and
are not rewritten. This no-browser closure created no generation-5 claim, browser
process, profile or realm observation; generation 4 remains failed/not accepted and
the Managed Engine capability remains unresolved pending main-brain adjudication.

## Generation-5 execution package freeze

The main brain has accepted the generation-4 ServiceWorker lifecycle semantic closure
at checkpoint `700f0a62fd67f59b2ffc5f6047c749988fb09ac3` / tree
`8a775660a08dbdb95e2814d77931927e2668aae6` and authorized only the no-browser
construction of a new executable package:

```text
executionGeneration: 5
taskVersion: fp2-v5
reportSchema: verisilo-camoufox-fp2-cross-realm-run/v5
claimPath: artifacts/camoufox-fp2/fp2-v5-one-shot-claim.json
claimSchema: verisilo-camoufox-fp2-one-shot-claim/v5
```

Generation-4 execution stays permanently `Failed`; the original `service_worker_not_activated`
observation and all generation-1/2/3/4 claims, reports and evidence stay byte-preserved.
What changed is only the main-brain root-cause adjudication recorded in lineage: the probe
incorrectly treated `navigator.serviceWorker.ready` as proof that `active.state` was already
`activated`. Generation 4 therefore did not adjudicate Managed Engine ServiceWorker capability.

### Generation-5 fail-closed lineage bindings

Before any generation-5 claim can be created, the runner must verify each item and embed it
in the claim, report and preflight receipt lineage; any drift fails closed before the claim:

```text
generation-1 claim e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402
generation-2 claim bcf9170cb26e46a35664ebad3cd8b39a2ec93928e597b21b84037e8cc6f22b67
generation-3 claim 0cf358045f86257af3126eb27b8f21f8df254984ee8c1fe91f6f79bbe44e09c7
generation-4 claim 44e4ee032c027f80a2470ecfd3a502ab5176789fc9ae1de052f9262e7c979059
generation-4 run    fp2-20260821T072352Z-c9af4cfd7d (status failed, verified:false)
generation-4 report SHA-256 77ad1771c26449bfbddbe63acd4562c8c1d931bc6819278d811c9aedefefed2d
generation-4 corrected root cause: confirmed ServiceWorker probe lifecycle semantic defect
semantic closure commit/tree 700f0a62fd67f59b2ffc5f6047c749988fb09ac3 / 8a775660a08dbdb95e2814d77931927e2668aae6
semantic audit artifacts/camoufox-fp2/fp2-v4-service-worker-lifecycle-semantic-audit.json
audit SHA-256 8626c37a6952009e6cc07044d2d68866e4f34ad8fe540cc26b9e6812bbb46923
probe bundle manifest pinned exactly to d69e61c4da482c8cebaed912a6c24b57b73ac0c465a9fafd6a0be8dc974cfb37
prior runtime evidence (generation-4 closure): preflight receipt 77e244d7…29580a and byte closure d1ae9872…fc2a37
```

The semantic-closure runtime evidence is bound as prior-generation history only. Because the
runner bytes change with this package, that evidence cannot be reused as the generation-5
runtime closure; a fresh runtime preflight bound to the generation-5 runner checkpoint is
required after this package freeze commit.

### Generation-5 claim Gate and auto-authorization

The generation-5 runner keeps the frozen order: git/baseline/semantic-closure ancestry,
candidate and Artifact byte closure, probe manifest exact pin, ledger/relation hashes,
no-browser tests, generation-1..4 claim verification, semantic-closure bindings,
process/port/lock cleanliness, then one deterministic runtime preflight — and only then the
`O_EXCL` creation of `fp2-v5-one-shot-claim.json`. A generation-4 claim on disk does not
block generation 5; an existing generation-5 claim fails closed with
`one_shot_claim_already_exists`; any historical claim hash drift rejects the package.

Per the main-brain authorization rule, once the generation-5 runner is committed, the
worktree is clean, all frozen hashes match, a fresh generation-5 runtime closure has passed,
port 18192 is free, target processes and locks are absent, and no generation-5 claim exists,
browser execution `A1 → A2 → B1` is auto-authorized without returning to this Gate. After
the claim is created there is no code, probe, timeout or package change, no retry and no
generation 6; the run freezes immutable evidence and returns to the main brain regardless
of outcome. This section changes no measurement semantics: realm surfaces, ServiceWorker
activation state machine, HTTP request shape, Artifact mapping, ledger, relation matrix,
candidate and timeout values remain exactly as frozen by the semantic closure.

## Generation-5 frozen formal failure and capability adjudication

The generation-5 one-shot was consumed and the formal run failed:

```text
run: fp2-20260822T065118Z-18fdbee7a8
claim SHA-256: 3ce470074c09d1f949c19fb63da3064ea7bd4d60f58467d47fbd7e27ea04eb70
first runner failure: dnt_mapping_mismatch / A1.top-window.navigator
valid realm observations: first FP2 evidence obtained (raw-realms 111767 B,
realm-observations 135598 B, 6 header captures); ServiceWorker activation path
observed working (script/scope/sha/activated + topController true)
```

The offline semantic adjudication (`fp2-v5-offline-semantic-adjudication.json`,
SHA-256 `586d37b10a736632147207076c9aa0ae5eb0f9e64c7ebd07d9795f7c9ab74b13`)
established, and the main brain accepted:

1. **Real capability gaps** in this candidate: GPC projection plus cross-realm
   coherence (window `false` / worker `true` / header absent against configured
   `true`), and voices projection (configured 53 -> observed 5 host SAPI voices
   with zero intersection). Delivery of the exact resolvedConfig through
   camoufox `launch_options` into the engine boundary is verified, and the
   candidate's own `properties.json` declares all three keys as supported engine
   properties — so the defects are inside this build's consumption/application.
2. **Comparator category error**: the httpHeaders surface carried contextHeaders
   into cross-realm identity equality although the frozen contract excludes
   Origin/Referer/Sec-Fetch-* from it. All 15 Gen5 pairwise flags reduced to
   referer hashes. The comparator now projects only `identityHeaders` +
   `requestPolicy`; `contextHeaders` stays recorded but excluded — a structural
   three-segment split, not a referer special case.
3. **Lifecycle instrumentation gap**: the Gen5 ctx.close exception (~1 ms,
   bounded-close timeout signature excluded) could not be root-caused because
   close evidence was not persisted on the failure path.

The remediation phase closes (2) and (3) in code: close outcomes persist a
bounded path-redacted message next to `exceptionType`, and failure-path teardown
(contextClose/closeOutcome/exit state/closeSeconds) is written into child-result
lifecycle even when validation fails before `host.close()` returns.

DNT product policy decision (main brain): Firefox >= 135 removed user-facing
DNT, so future identity policy stops declaring managed `DNT=1` for such targets
(native/unavailable under a version-aware policy variant, specified separately).
Historical A/B Artifacts remain byte-unchanged; Gen5 remains Failed.

Generation 6 is closed permanently for this candidate; the current candidate is
not eligible for another FP2 browser attempt. Any engine change creates a new
source revision/archive/executable/tree manifest/engine binding and opens a fresh
candidate-scoped lineage (`FP2-R1`). FP2 remains **Failed / Not Accepted** with
all results `verified:false`, and FP3 remains **Closed**.

### Round-1 acceptance and R1 design freeze

The main brain Accepted the remediation round-1 checkpoint (`94e107bf…`) as a
foundation closure, with binding terminology corrections (contract recognition ≠
runtime implementation proof; consumer divergence ≠ presupposed two writers;
observability closure ≠ ctx.close root-cause fix). The authorized successor task
`FP2-R1 Engine Remediation Design Freeze` (offline/static) is recorded in
[camoufox-fp2-r1-engine-remediation-design.md](camoufox-fp2-r1-engine-remediation-design.md):
GPC seam fully adjudicated (window native pref path unpatched, worker MaskConfig,
Sec-GPC native — single divergence); voices injection chain proven working by
Gen5 raw evidence with the real defect restated as enumeration coherence plus
inactive native blocking (artifacts omit `voices:blockIfNotDefined`), root cause
narrowed to V1–V4 init/sync timing candidates pending discriminators; DNT Policy
`v4-dnt-native` versioned contract frozen with an artifact schema v4 variant;
R1 fresh lineage (`artifacts/camoufox-fp2-r1/`, Generation naming retired) and
mandatory FP1 carry-forward qualification frozen. No implementation patch is
approved until the voices root cause converges; browser execution remains CLOSED.
