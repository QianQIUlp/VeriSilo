# FP2-R1 Voices Diagnostic Execution Readiness Contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical diagnostic execution record**: the v1/v2 claims are consumed and the Browser Gate is closed.
> The current task does not inherit this document's diagnostic instrumentation, one-shot, or recomputation matrix. Read it only when auditing the corresponding run. For the current Gate, see the [status page](camoufox-program-status.md).

- Status: **Accepted: v2-executor-recovery-evidence-captured-and-adjudicated**
- Date: 2026-08-24
- Starting code checkpoint: `0efe8aa57bf2e83fc5f3552c1ecb0d1f8e645b72`
- Browser Gate: **CLOSED: v1/v2 one-shot claims consumed; no retry authorized**

## 1. Goal and boundaries

This task only turns the R1-diag Windows binary, whose provenance closure is already complete, into one executable, bounded, fail-closed Voices causal diagnostic package. It does not run a browser, does not accept FP2-R1, does not prove that Voices is fixed, does not verify GPC runtime, and does not create a Formal R1 candidate or `0005`.

The lead agent must separately authorize the next browser execution. Before that authorization, the default command can only verify readiness, or materialize and re-read the browser tree. `--execute-browser-diagnostic` is the only browser entry point.

## 2. Exact input bindings

| Object                      | Binding                                                                                                                                                                  |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| diagnostic build run        | `r1diag-engine-20260823t1542z`                                                                                                                                           |
| closure source lock         | SHA-256 `6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b`; `50158` bytes                                                                                |
| build-input source lock     | SHA-256 `02b7de1a0e6d87cd4a08be1c7bffe3b5979be3f4f9ffcf85c951ca80720441a7`                                                                                               |
| Windows ZIP                 | SHA-256 `241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d`; `493471385` bytes                                                                            |
| `camoufox.exe`              | SHA-256 `9fef022fea062f22e4916e4c125c913931eefe8afe522d3930089ed3393dbfd5`                                                                                               |
| bundle metadata             | BuildID `20260811045234`; SourceStamp `e39c605adc0fc049a165d7fe4a3f6517b761edf7`                                                                                         |
| extraction tree             | SHA-256 `d65b168849b4df8f1fde52e8627e834e3d0b85b4c4e7befb5b179a8440211e06`; 514 entries / 503 files / `982403785` bytes                                                  |
| historical Artifact bridge  | `identity-win-canvas-v1-a.json` SHA-256 `e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb`                                                               |
| historical native reference | Gen5 `raw-realms.json` SHA-256 `ebf10af98b0074b1a48ba0da1bc45788e7cb410bbf9e94714529c84cfc13d9c8`                                                                        |
| native supervisor           | observed host-local support asset SHA-256 `d12204d76ecebed681f601a95e47f29a75bc67a879b6106fb3c1f38579054a98`; `185856` bytes; no independent build provenance is claimed |
| capture runtime             | CPython `3.12.13`; Playwright `1.60.0`; `pw:browser` bridge receipts are bound file by file in the readiness report and the claim                                        |

The 9000 SHA-256 stays
`1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`. Its first line is still
`# VERISILO-DIAGNOSTIC-MARKER: v1`, with `diagnosticOnly = true` and
`formalCarryForward = never`. This task does not modify any patch bytes or source lock.

## 3. Lead agent's fail-closed ruling on the old discrimination table

Implementation Contract §2.2/§2.4 describe the ideal E1–E8 design. The actual bytes of the frozen 9000 implement only E1–E7. A fixed-source audit further proves that the old table has unreachable or overly strong positive signatures. Therefore, the execution interpretation for `1542z` is superseded by `actual-9000-amendment-v1`. The historical text is not rewritten.

The frozen 9000 has only these actual events:

| Event          | Actual payload                      |
| -------------- | ----------------------------------- |
| E1             | `proc=P, seq, n\|null`              |
| E2a/E2b        | `proc=P, seq`                       |
| E3a/E3b        | `proc=P, seq, n` / `proc=P, seq`    |
| E4             | `proc=P, seq, n`                    |
| E5             | `proc=P, seq, h`                    |
| E6 initial/add | `proc=C, seq, n` / `proc=C, seq, h` |
| E7             | `proc=C, seq, ctx, n, cache, first` |

The actual events use a per-process consecutive `seq`. They have no timestamp/tid, no E4 actorTag/inventory hash, and no E8. The `seq` values of `P` and `C` are not comparable with each other. `proc=C` has no PID. Execution must therefore keep a single top content producer and judge by the consecutive sequence inside C. It must not fabricate a cross-process total order.

The fixed source and the exact pre-seam bytes give the following rulings:

- The original V1 signature `E4 < E3b` is `source-refuted-as-written`. E4 calls `GetInstance()->GetVoiceCount()` before it is recorded, and the managed batch has already completed before `GetInstance()` returns.
- The original V2 signature is `source-refuted-as-written`. The fixed SAPI path completes enumeration and registration synchronously, records E2b, and only then enters E1/E3. A normal `E2b < E3b` is not evidence of asynchronous interleaving.
- V3 has no E8, and the fixed `GetVoices` has already formed its return result before it rebuilds `mVoiceCache`. A count change without E6 can only be ruled `inconclusive / unexplained content-local transition`. It must not be called cache causal.
- For V4, when config delivery holds independently, an E1 that is null or missing, or a missing E3, can only form `source-seam suspicion`, followed by a source diff audit. It must not be called a confirmed hunk drift.

## 4. Actually executable discrimination: T1

T1 is a compensating signature for the actual 9000. It is not renamed V1:

```text
same top window, same speechSynthesis object
E7 first: 5 exact known native URI hashes
  → E6 initial or exact managed add delivery appears within the same C seq
E7 second: same 5 native + exact 53 managed URI hashes
and E4 parent snapshot = 58
```

`T1_contentMirrorIncrementalDelivery = supported` may be output only when the set, the counts, E4, and the delivery closure above all hold. A `5 → 58` count alone, an unknown voice, a parent-snapshot contradiction, or cross-producer inference must not form a positive T1 conclusion.

One execution may stay `inconclusive`. `not-observed` does not mean refuted, and no exhaustive exclusion claim is allowed.

## 5. Bounded execution shape

The only plan is top-only. Load a local `127.0.0.1` static page, query the same native `speechSynthesis` object once immediately, wait a fixed 3 seconds, query once more, and close. Do not add an iframe, Worker, Playwright smoke, FP1-R1, FP2-R1, or FP3 flow.

Playwright 1.60 swallows browser stderr by default. The execution package reuses its existing `pw:browser` debug channel. It forces `DEBUG=pw:browser`, `DEBUG_COLORS=0`, and `DEBUG_HIDE_DATE=1`, removes `DEBUG_FILE`, and accepts only:

```text
pw:browser [pid=<positive integer>][err] VSIDIAG <strict JSON>
```

The fixed Python transport, Node driver, and Windows supervisor sources prove that this wrapper connects to the stderr that Camoufox inherits. The fixed FF152 Windows process/sandbox source also passes the parent's stdout/stderr handles to the content process. All actual VSIDIAG lines must come from the same wrapper PID. A raw/fake wrapper, an unknown event, field drift, a P/C sequence gap/duplicate, or OVERFLOW all fail closed. The Python and Node/browser logs have no usable cross-producer total order. The classifier therefore uses only the respective consecutive P/C seq, and does not bound or order E7 by log line position.

## 6. Assets, claim, and lifecycle isolation

- The ZIP is materialized only into a Git-ignored diagnostic runtime root. The temporary tree is renamed atomically only after a complete re-read passes.
- Before each launch, recompute the closure lock, the tree manifest, and the 503-file live tree. Reject every reparse, extra, missing, or changed file.
- The browser child can consume only a single-use token authorization created by the parent. The authorization binds the run-id, the claim bytes, the runner, the runtime interpreter, the Playwright bridge, the native supervisor bytes, and all child paths. The consumption record uses exclusive create.
- The claim is written before launch into the separate `artifacts/camoufox-fp2-r1-diag/` namespace. It must not reuse `artifacts/camoufox-fp2/` or a Formal claim.
- The parent watchdog is 150 seconds, and launch and close are 60 seconds each. A clean close, Job active=0, exit 0, and an empty target process list are required conditions for evidence capture.
- The report fixes `diagnosticOnly=true`, `formalEligible=false`, and `verified=false`, and explicitly writes `fp2R1Accepted`, `formalR1`, `voicesFixed`, `gpcRuntimeVerified`, and `remediationSuccess` all as false.

## 7. Acceptance conditions of this Gate and next step

The readiness closure accepts only the following. The exact inputs, tree, runtime, and log bridge can be re-read. The actual-9000 parser, classifier, single-use authorization, claim isolation, and bounded lifecycle are all covered by no-browser regressions. `browserLaunches=0` in default and materialize modes. The tracked worktree is finally clean and pushed.

When this Gate passes, only this statement is allowed:

> FP2-R1 V1–V4 diagnostic execution readiness closure passed.

It does not mean that FP2-R1, V1–V4, Voices remediation, GPC runtime, or Formal R1 passed. That authorized run and its result are now recorded in the next section. The run result does not rewrite this readiness Gate itself.

## 8. The sole authorized run on 2026-08-24 and offline re-adjudication

The sole authorized run is `fp2-r1-diag-20260824T055549Z-56f7c5fced`, and the browser was launched only once. The child completed, the browser and child exited 0, the close was clean, the process tree exited, and the parent reported `processClean=true`. A single transport PID captured 125 events. P seq `0..63` and C seq `0..60` are consecutive, with no OVERFLOW. The original claim SHA-256 is
`2b9151e9032ecfda4e1ea29e3dd1d38b368ab734b61239753c4a1c6ff582b34c`.

The original runner report must stay `Failed / config_delivery_unproven`, SHA-256
`40ed7905f3eb313452b55238b9bd515b514f7a7dd4107ac404bc765a4ac94728`. The failure happened in post-processing. The runner wrongly treated the canonical `sha256:<64hex>` configured digest as a bare 64 hex string. The browser, 9000, config delivery, and lifecycle did not fail. The minimal fix commit is
`1e889bb8aed23852fbe8582462edf38e59e862b6`, tree
`9cb1265f8f4fd69e89a01ca6b63955ccb906a20a`. The regression now accepts the canonical prefix and rejects bare values.

No second browser was launched. The post-fix clean/pushed classifier re-read the 8 fixed raw evidence receipts and sidecars, and wrote a separate
`verisilo-fp2-r1-diag-offline-readjudication/v1` that does not overwrite the original files. The receipt SHA-256 is
`2ffe6b55952c1d27704044f384ce6d09dc7663e96a86f6e6e53e8a0178a390ef`, size `6435`.
The sidecar raw SHA-256 is
`f734d03e21082b3d741d3cbc2be0edf964efddb1fac7e44864612fbd5c0e0f73`, size `98`.

The actual observation, within the same C context, was `E7(0) → 58 exact E6 add + E6 initial(58) → E7(58)`. The second inventory was exactly 5 known native + 53 managed, with E4=58. It does not satisfy the first=5 condition of the frozen T1. The offline conclusion is therefore `inconclusive`, with `supported=[]`, and T1/V3/V4 are all `not-observed`. V1/V2 remain `source-refuted-as-written`. The 0→58 pattern may stand as a new observed fact. It must not be used to extend T1 after the fact, to pass as FP2-R1/Voices remediation success, or to author `0005`.

## 9. Voices phase-anchor readiness amendment

The Gen5 immutable raw evidence is `top=5 / same-origin iframe=58 / cross-origin iframe=58`. The 5 URI hashes of top are exactly the same as the first 5 native E6 events of the actual-9000 C sequence. The 58 hashes of both iframes are exactly the same as the full `5 native + 53 managed` set. The Gen5 frozen probe also ran serially in the order top → same-origin iframe → cross-origin iframe. Its `voiceSnapshot()` waits for the first `voiceschanged` when the initial result is empty, and then queries only once more.

The fixed FF152 source and the actual-9000 consecutive sequence together support the following three-phase interpretation:

```text
A0  content registry created, inventory not yet arrived: 0
A1  SAPI native 5 arrived incrementally, first voiceschanged published: 5
A2  managed 53 and initial snapshot arrived: 58
```

This interpretation rules out a stable realm inventory and a persistent stale cache as the preferred root cause. However, Gen5 has no E6/E7, monotonic time, or object anchor from the same run. Cross-run hash prefix alignment can only form a high-confidence source-supported inference. It cannot retroactively confirm the old T1, and it cannot directly author `0005`. The next Gate therefore narrows to `voices-phase-anchor-v1`, without modifying 9000 and without recompiling the binary:

1. The top-only JS holds one `speechSynthesis` object and registers a `voiceschanged` listener before the first `getVoices()`.
2. It records S0 initial. The event handler only increments a count, and records S1 synchronously only inside the first event callback. It records S2 final after a fixed 3 seconds. It must not query again after a resolve, must not poll, and must not create an iframe or Worker.
3. The observer inventory corresponds one-to-one with each E7. It uses only the same consecutive C seq, and does not splice P/C seq or stderr line order into a total order. `ctx` is used only as a single-producer consistency constraint and does not pass as an object ID.
4. The exact positive signature is
   `E7(0) → native E6×5 → E7 inside the event (exact native 5) → managed E6×53 →
initial(58) → final E7(58)`, and the P side keeps
   `SAPI native 5 → managed 53 → E4(58)`. Later notifications are only counted and no longer produce E7.
5. When the first event fires after settled, or when no event occurs, rule only `not-observed / Inconclusive`. Do not use that to refute the historical race. If an event is not trusted or its target does not match, a voice is unknown, an E7 cannot be mapped, or the sequence/transport/config/lifecycle does not close, rule `Failed`.

The new claim uses the separate
`artifacts/camoufox-fp2-r1-diag/fp2-r1-voices-phase-anchor-v1-one-shot-claim.json`
namespace. The old claim and the original run bytes are kept read-only for offline re-adjudication. Even when the classifier outputs `supported`, it keeps `0005-remains-closed`. It only returns the evidence to the lead agent for separate adjudication, and it does not mean, even less so, that FP2-R1, Voices remediation, GPC runtime, or Formal R1 passed. This amendment and the runner regression are themselves no-browser readiness. Browser execution still needs a separate Gate.

## 10. Voices phase-anchor v1 single-run failure closure

The sole v1 run authorized by the lead agent is
`fp2-r1-phase-anchor-20260824T102701Z-ee4a02f604`, bound to clean/pushed HEAD
`2db42c00f296006210daaca69ff21a17ec9546e5` / tree
`ff7684d07d85898d592e661a64311e9314db8607`. The one-shot claim SHA-256 is
`c384c41d57c5018297c097dcfb7da62a57505067f51a3905e9107c272aa341f4`, size
`3482` bytes. The global claim and the run copy are byte-identical.

This run launched the browser once, but failed before the first realm observation. The supervisor and the Camoufox parent/Juggler started. The log then recorded, in succession, 8 occurrences of
`Failed to launch tab subprocess @SB::LA::SpawnTarget (Error:0)` and 2 occurrences of `gBrowser never populated`. `host.launch()` returned no launch receipt, and the 60-second watchdog finally output `diagnostic_session_watchdog_timeout`. The failing command actually ran under the identity
`telecaster\codexsandboxonline`. The lead agent's read-only comparison at the same checkpoint confirmed that the native non-sandbox execution identity is `telecaster\qiu`. Therefore **an executor-context mismatch at Gecko content-process creation is the strongest current bounded diagnosis and the next item to discriminate**. The raw evidence itself formed no engine/9000, Voices observation, or classifier failure evidence. Whether the executor identity causally triggers the failure must still be discriminated directly in a future frozen new lineage, by running natively without the sandbox.

This run has no `voice-observation.json`, `vsidiag-timeline.json`, or `phase-anchor-decision.json`, and the VSIDIAG count in stderr is 0. It is therefore neither supported nor valid-but-Inconclusive. The child exit is confirmed. The parent's final recheck reports `processClean=true`, and there is currently no target process or 18193 listener. However, the clean launch/close receipt is missing, so the execution verdict remains
**Failed / no observation**. Key raw SHA-256 values:

- `child-stderr.log`: `7147755d6d774b24eb7c4379acc001a800e2d20f22bee7e3577d1e24685a5b51`
  / 8959 bytes;
- `child-result.json`: `e69ab1936f1700baeda65f9fb2e98dcf83a94d38685df499b2dc88da2c863c43`
  / 304 bytes;
- `run-report.json`: `3d6d5b3733e2730d6c41f7d5dfdb1bbff1e14bc7b1920f665bd6e01e9fe605b3`
  / 5814 bytes.

All existing JSON/log sidecars were recomputed independently and match. The original claim and run bytes are unchanged. v1 is not retried. `0005`, FP2-R1, Formal R1, FP3, and other browser experiments stay closed. If work continues, it may only first freeze a separate executor-recovery claim lineage at a new clean checkpoint, and run from a native non-sandbox identity. This v1 claim must not be deleted or reused.

## 11. Executor-recovery v2 no-browser freeze

The v1 failure closure confirmed that the command at that time ran under `telecaster\codexsandboxonline`. The link between executor identity and the Gecko `SpawnTarget` failure is still only the strongest current bounded diagnosis. To discriminate it directly, v2 changes only the execution lineage. It changes no measurement, classifier, binary, 9000, or timeout:

- The v1 global claim and the run copy must be byte-identical. The run's 7 primary JSON/log files and all existing sidecars are re-read against the fixed SHA/size values in §10. The semantics must still be browser launch 1, watchdog failure, and no observation / timeline / decision / VSIDIAG.
- The new claim is
  `fp2-r1-voices-phase-anchor-v2-executor-recovery-one-shot-claim.json`, with schema
  `verisilo-fp2-r1-voices-phase-anchor-executor-recovery-one-shot-claim/v2`, and it embeds the full
  `priorAttempt` receipt. The v1 claim is not deleted, not rewritten, and not reused.
- Before readiness, run directory, and claim creation, the parent reads the real token identity with Windows
  `GetUserNameExW(NameSamCompatible)`. Only `telecaster\qiu` is accepted. The child recomputes the same receipt before it consumes the authorization and launches the browser. Inheritable values such as environment variables and `getpass.getuser()` do not take part in the Gate.
- `PHASE_CONTRACT`, the observation/decision schema, the top-only JS, listener-before-S0, synchronous S1 inside the first event callback, the fixed 3-second S2, and the exact 5/53/58 classifier all stay as in v1. The JS and classifier source fingerprints are fixed as SHA-256
  `93af1e2e68c5fbe1c568abf7435ff8494b19eef054440c9b00b114ce7685a708` and
  `11286655ca82fdcd3c5f4c81bb5badbefa5796b624b3f6644b3faae702112388` respectively.

v2 may be consumed once only after this implementation and its direct regressions form a clean/pushed checkpoint and the user explicitly opens this Browser Gate. Lead agent authorization cannot override the earlier user-level scope "do not launch Camoufox". If the native executor Gate does not pass, execution must stop before the claim and run are created, with `browserLaunches=0`. If it passes, then whether the result is supported, Inconclusive, or Failed, the raw bytes are preserved, automatic retry is forbidden, and the result returns to the lead agent for adjudication. The classifier continues to output `0005-remains-closed`. FP2-R1, Formal R1, and FP3 are not opened.

## 12. Executor-recovery v2 single run and lead agent adjudication

After the user explicitly opened the v2 Browser Gate, the sole run
`fp2-r1-phase-anchor-recovery-v2-20260824T112146Z-fee3df2667` executed from clean/pushed HEAD
`651e34ec42ee635d4cf2b1f690b800b5c64c1a88` / tree
`d3b95a947de981126880840382b0010ff8c7d82f`. Both the parent and the child read `telecaster\qiu` from the Windows primary token. The v2 claim SHA-256 is
`47ce7d90230d90654d26e21c9aba5b73634760b154db10d1c3a902e915486e61`, size `6325`
bytes. The global claim and the run copy are byte-identical. The claim exactly binds the v1 claim from §10, the 7 primary evidence files, all existing sidecars, and the Failed/no-observation semantics.

The browser was launched only once. The child completed, exited 0, closed cleanly, and its process tree exited. The parent reported `processClean=true` and `browserLaunches=1`, and there is currently no target process or 18193 listener. The complete run and the global claim are kept locally in `artifacts/camoufox-fp2-r1-diag/`, and all primary and sidecar files were recomputed independently:

| evidence                            | SHA-256                                                            | bytes |
| ----------------------------------- | ------------------------------------------------------------------ | ----: |
| `child-authorization-consumed.json` | `12b5297693549f0c88f34ba28d2f30d54254e60096ad8027ab063500b267596e` |   262 |
| `child-authorization.json`          | `faaa851c8873b7877e7b14345175ee8f2a0f950719c0e7715baeab216366b9d9` |  4177 |
| `child-result.json`                 | `aa6dd845213cf2b1f025997fa1cfb41062d8b17c0de5bc789eee4ed3bf3b3183` |   521 |
| `child-stderr.log`                  | `ad1bbf3b4036aff8fddd3d0fcf1f417af52f5fb5f45eb46260935e398da7180f` | 19541 |
| `child-stdout.log`                  | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |     0 |
| `one-shot-claim.json`               | `47ce7d90230d90654d26e21c9aba5b73634760b154db10d1c3a902e915486e61` |  6325 |
| `phase-anchor-decision.json`        | `2708431234fc14682af509656249d1ebfc090458e85e971e643a517bdc31c31d` |   895 |
| `run-report.json`                   | `8f7108ff9e369d0f0b46d1b558825a81efa2112158b4ce3b5808d5c05f78bdb3` |  9571 |
| `voice-observation.json`            | `3f1da1f5cebfd3f96876031b02f84133f5d406886dd1d20017f1274777d26939` |  2444 |
| `vsidiag-timeline.json`             | `32776169111f664388cacce7a3189eb257211d2a29c1a7c6274b618b63cc4257` | 13729 |

The configured digest and the expected value are both
`sha256:c672afc21d9975b85a4bc032261d462ceae8573cfcf9f9a071f2fdfdca9a381f`.
On the same `speechSynthesis` object, S0 was 0 at 67 ms. Inside the first trusted, target-matched `voiceschanged` callback at 79 ms, S1 was exactly the 5 known native voices. S2 at 3071 ms was exactly the same native 5 + managed 53. The total event count was 2.

A single transport PID captured 126 VSIDIAG events, with no unknown event, OVERFLOW, or seq gap. P was
`E2a → native E5×5 → E2b → E1(53) → E3a(53) → managed E5×53 → E3b → E4(58)`.
C was `E7(0,first=1) → native E6×5 → E7(5) → managed E6×53 → E6 initial(58) → E7(58)`.
All E7 events use the same ctx. The adjudication did not splice P/C seq or stderr line order into a cross-producer total order.

The above directly supports the phase model A0(0) → A1(native 5 / first notification) → A2(native 5 + managed 53). It also narrows the preferred explanation of the historical Gen5 top=5 result to first-notification temporal sampling, rather than a stable realm inventory split. The conclusion is fixed as
`native-only-first-notification-phase-supported`. It is not an exhaustive exclusion, and it does not mean that FP2-R1, Voices remediation, GPC runtime, Formal R1, or FP3 passed. v2 is not retried, and `0005-remains-closed`.
