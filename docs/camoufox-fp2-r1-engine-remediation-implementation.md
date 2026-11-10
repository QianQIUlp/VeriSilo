# FP2-R1 Engine Remediation Implementation Contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical implementation record**: the current execution entry point is the
> [status page, "Current next task"](camoufox-program-status.md#current-next-task). §1–§8
> record the closed GPC/diagnostic lineage, and §9 preserves the Voices authoring reasoning.
> Ordinary implementation work does not need to read this document from the start.

- Status: **Accepted** (checkpoint `38ff360c65f2fb5e28fe3ebaed7931727c0d6b68`)
- Date formed: 2026-08-22
- Prerequisite: the [design freeze](camoufox-fp2-r1-engine-remediation-design.md) is Accepted
  (checkpoint `ff4f8c960d5cf63ff242da990b904fca8743be54` /
  tree `e000f6ad8ebf2bf1ac2c06be8ebdf6fb2c0a4004`). It includes two tightenings:
  mandatory canonical-owner, and no ruling on the causal axis.
- Browser execution: **CLOSED**. This contract freezes the specification only. The build and
  the diagnostic experiment are later steps that need separate authorization.

This contract freezes two sets of changes of different natures:

```text
GPC    → remediation patch (product level, enters the R1 Engine Binding)
Voices → diagnostic instrumentation (investigation only, excluded from the R1 Engine Binding by default)
```

It is forbidden to bundle a "speculative voices fix" with the GPC fix in the same formal
candidate.

---

## 1. GPC remediation patch contract

### 1.1 Canonical owner (mandatory)

```text
Canonical identity state:
  artifact.policy.navigator.gpcPolicy ∈ { native, managed-opt-out }
  (the only engine-side projection key MaskConfig["navigator.globalPrivacyControl"]:
    managed-opt-out ⇒ true; native ⇒ key absent)

Native Firefox prefs:
  privacy.globalprivacycontrol.enabled
  privacy.globalprivacycontrol.functionality_enabled
  — only a derived projection of the canonical policy, not a second source of state

Window / Worker / HTTP:
  all consume the same native pref machinery; no spoofing getter that reads MaskConfig directly exists
```

Selected architecture (strongest form): **all three projections go through the native
machinery, and the Worker-specific override is deleted.** Rationale: in FF152 the three
already have the same semantics:

| Exposure surface | FF152 anchor                                                                         | Shared predicate                                                               |
| ---------------- | ------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------ |
| Window           | `dom/base/Navigator.cpp` `Navigator::GlobalPrivacyControl()`                         | `functionality_enabled && (enabled \|\| (pbmode_enabled && private browsing))` |
| Worker           | `dom/workers/WorkerNavigator.cpp` getter of the same name (native body)              | Identical (the principal determines private browsing)                          |
| HTTP             | `netwerk/protocol/http/nsHttpChannel.cpp` `SetGlobalPrivacyControl()` (L11751–11760) | Identical → `SetHeader("Sec-GPC","1")`                                         |

So it is enough for the parent process to project the canonical bool into the two prefs, and
the three exposure surfaces become consistent automatically. The Worker override (the direct
`MaskConfig::GetBool` read introduced by fingerprint-injection.patch) is reverted, so no
second state remains that could drift independently.

### 1.2 GPC policy-state model (Conditional Gate amendment, frozen)

The data model is **not a free bool**. The true identity semantics of GPC have only one
managed direction, the opt-out declaration (`true` = the user explicitly expressed a
do-not-sell/share request; `Sec-GPC` carries only the opt-out signal; `false` = "nothing
expressed", which is not a manageable website-visible state). Therefore:

```text
GPC policy states:  managed-opt-out | native
```

| Policy state (artifact.policy.navigator.gpcPolicy) | resolvedConfig engine key                                                     | Pref writes                                     | Window / Worker  | Sec-GPC |
| -------------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------- | ---------------- | ------- |
| `managed-opt-out`                                  | `navigator.globalPrivacyControl = true` (must be present, value exactly true) | `enabled=true` and `functionality_enabled=true` | `true`           | `1`     |
| `native`                                           | key **absent** (must be missing)                                              | no writes (prefs keep their native defaults)    | `false` (native) | absent  |

Rules:

1. **An explicit `false` is an illegal shape in v4**: the validator rejects it on both the
   generation side and the load side. The path "configured=false ⇒ not managed ⇒ expect
   native to happen to be false" is removed. This restores the configured ≠ native-fallback
   boundary. The `false == missing` ambiguity is resolved by making "false not
   representable".
2. The two `pbmode_*` prefs are outside the managed scope and are never written.
3. Defensive runtime behavior: if a boolean false appears in MaskConfig (unreachable in the
   normal flow), the projection point treats it as native and writes no pref.
4. Historical v3 Artifacts are unchanged: REQUIRED_CONFIG_KEYS keeps forcing BOOL
   (immutable). The `gpc=true` of Gen5 A is one failed application of managed-opt-out, and
   the verdict is unchanged.
5. v4 validator two-way rule: `gpcPolicy="managed-opt-out"` ⇔ the engine key is present and
   true; `gpcPolicy="native"` ⇔ the key is absent. The policy field lives in the
   artifact.policy namespace and **does not enter resolvedConfig/CAMOU_CONFIG** (the engine
   sees only the boolean key itself, which is compatible with the properties.json contract).
6. The R1 verification invariant is refined accordingly: under managed-opt-out,
   `Window == Worker == Sec-GPC present == true`. Under native there is no managed
   assertion, and the observation is only a diagnostic record that takes no part in failure
   decisions.

Executable reference model: `apps/camoufox-host/test_gpc_policy_contract.py`
(the pure-Python mirror of T3, landed with this amendment; the later C++ seam and patch
authoring must be consistent with it).

### 1.3 Patch series (file names and order)

The new engine revision uses a separate downstream series directory (reusing the seam
verification mechanism of the canvas precedent):

```text
diagnostic candidate revision: verisilo-camoufox-152.0.4-beta.28-r1-diag-v1
  apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/
    0000..0002  ← the current midl/canvas/bounded-close three patches carried as-is (digests unchanged)
    0003-verisilo-gpc-canonical-pref-projection.patch
    0003a-verisilo-gpc-preferences-namespace-compile-repair.patch
    0004-verisilo-remove-worker-gpc-mask-override.patch
    9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch

R1 formal candidate revision: verisilo-camoufox-152.0.4-beta.28-r1-v1
  apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1/
    0000..0004  same as above (no 9000)
    0005-verisilo-voices-final.patch   ← drafted and approved separately, only after V1–V4 converge
```

Constraints:

- The `strict_build` driver refuses to include a patch carrying the `DIAGNOSTIC-ONLY` marker
  in a non-diag recipe (the first line of the patch header is a machine-readable marker; the
  driver validates it statically).
- Diagnostic logging **must not enter the R1 Engine Binding**, unless it is explicitly
  promoted in the future to product-level bounded runtime evidence and passes the Gate again.

Authoring status (2026-08-22): the three patches were generated per this contract and passed
round-trip verification (fresh-tree apply rc=0, and all post-image hashes equal the seam
records one by one):

```text
0003  3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b
0004  5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185
9000  1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8
```

The derived record (sections SHA, seam pre/post digests, verification results) is in
`apps/camoufox-host/build/r1-diag-v1/authoring-record.json`, which stays as the original
0003/0004/9000 authoring record. 0003a is bound separately by the current v2 source lock. The
static regression `test_engine_remediation_patches.py` locks the original patch fingerprints,
the exact repair boundary of 0003a, and the behavior invariants.

#### 2026-08-23 compile-failure closure and 0003a additive repair

The exact run `r1diag-engine-20260823t1206z` completed the diagnostic gate, the 50 upstream
patches, and the `--fuzz=0` application of the original downstream
`0000 → 0001 → 0002 → 0003 → 0004 → 9000`. It entered the real Windows `mach build` and then
failed with container exit `1`. The classification is a
**source/patch compile error**. It is not a recipe/driver, toolchain, resource, or transport
failure. Lines 38/39 of `camoucfg/GpcProjection.h`, which `toolkit/xre/nsAppRunner.cpp`
consumes, use an unqualified `Preferences`, while Firefox declares `mozilla::Preferences`.
The exact six-patch chain above is therefore proven **non-compiling** by a real build. No
archive or binary was produced.

The durable copy of the failure evidence is at:

```text
/var/lib/verisilo/camoufox-build-evidence/r1diag-engine-20260823t1206z
```

| Artifact                      | SHA-256                                                            | Size (bytes) |
| ----------------------------- | ------------------------------------------------------------------ | -----------: |
| `host-provenance.json`        | `5888c637859b22fbc6b5320e145f7c727e5e45afcbf4f01f30e7cfd0926e138a` |        4,616 |
| `build-failure.json`          | `b90f6c2b2cdbe03c96c3e36a93c420f89b90e6c44724b62eb66ee56bd4a03bda` |          376 |
| `container.log`               | `ed728a71eb1d8a80990cbc24d47e0ad7a0c73361639f86f447b1201e9843fde0` |    1,422,435 |
| `build.log`                   | `4ebaff48a277475e40c36cb2ca3971bc6e3255eb01c56f5b31b824cac174db2b` |    1,420,797 |
| `diagnostic-gate-result.json` | `931a11c64381aae83d823226ae07e2a3dc90c0db6a5a45c9f639318ea66c70d3` |          523 |

The bytes of the original six patches all stay unchanged:

```text
0000  8d407bdc4010f7b2989f206a70909bfa9ad89046ddb9e17fa76092c864433600
0001  4fa6d3bbf203e2385e29a72ec2669ee17a571281be7ee2a73598e38918069b02
0002  efb006d5b2b05756fc310b52eb48e0bdab5e8b23e780fa08534a7fc099c22ce7
0003  3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b
0004  5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185
9000  1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8
```

The minimal repair is only appended after 0003:

```text
0003a-verisilo-gpc-preferences-namespace-compile-repair.patch
SHA-256: c2f9a9f88ba8aeb610eb1cb29f2515f1d79fcf582393397a571bc3206889588c
size: 500 bytes
seam: camoucfg/GpcProjection.h
pre:  ab0b4c26e628a74d0ef4bac66d35bc6b0e9aee45cd67ad6bd5e5da91b609cf3f
post: 364655669418c106f80f030a7a48797dbdbca1030c0d29e4e91c841129999bda
```

0003a only lexically qualifies the two `Preferences::SetBool` calls as
`mozilla::Preferences::SetBool`. The pref keys, conditions, call timing, the canonical GPC
owner, and the three-projection semantics are all unchanged. It is not a voices fix and does
not create a Formal R1 candidate. Its `formalCarryForward` is only
`allowed-after-qualification`. Because the patch series, the strict driver, the gate, and the
source-lock recipe binding change with it, the Phase B-7 image / Phase C-7 binding are
superseded, and the current lock returns to unbound, waiting for a brand-new Phase B-8 /
C-8. `binaryBinding = null`, `diagnosticOnly = true`, `formalEligible = false`,
`browserLaunches = 0`, `verified = false`.

### 1.4 Seams and verification steps (reusing the canvas recipe pattern)

| Seam file                                                                                                                                                                                                                                                   | Pre-image                                                               | Post-image semantics                                                                                                                                                                  |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Parent-process initialization anchor (pinned at authoring time: prefer extending the same initialization path already touched by the current `browser-init.patch`; the alternative is a new projection unit under additions/camoucfg called from that path) | build driver `verify-gpc-seam-preimage`                                 | a single-point, once-only, parent-process-guarded pref projection block                                                                                                               |
| `camoucfg/GpcProjection.h` (after 0003)                                                                                                                                                                                                                     | `ab0b4c26e628a74d0ef4bac66d35bc6b0e9aee45cd67ad6bd5e5da91b609cf3f`      | after 0003a it is `364655669418c106f80f030a7a48797dbdbca1030c0d29e4e91c841129999bda`; the two calls are qualified to `mozilla::Preferences`, and the behavior semantics are unchanged |
| `dom/workers/WorkerNavigator.cpp`                                                                                                                                                                                                                           | `verify-worker-seam-preimage` (= the state after fingerprint-injection) | restores the FF152 release native getter body exactly (golden fragment in Appendix A), and the whole file no longer contains a `MaskConfig` GPC reference                             |

Driver step order (new, after the upstream patches and before compilation):
`verify-gpc-seam-preimage → apply-0003 → verify-gpc-seam-postimage →
verify-0003a-seam-preimage → apply-0003a → verify-0003a-seam-postimage →
apply-0004 → verify-worker-native-restored → (diag recipe: apply-9000)`.

### 1.5 Browserless regression (host side, landed with this contract)

- **T1 golden restoration assertion**: the text of 0004 contains the line-by-line fragments
  of the native getter body in Appendix A. In the patch text after the series is applied,
  the count of `MaskConfig::Get*` references for `navigator.globalPrivacyControl` is 0.
- **T2 single-writer assertion**: across the whole series, exactly 1 place writes
  `privacy.globalprivacycontrol.*`.
- **T3 projection policy model test**: `test_gpc_policy_contract.py` is a pure-Python mirror
  of the §1.2 policy-state predicate. It covers the projection of the two states
  managed-opt-out/native, the rejection of illegal shapes (explicit false, policy
  inconsistent with the engine key), and the invariance of the pbmode combination.
- **T4 driver order test**: the recipe step list contains the §1.4 0003a pre/apply/post
  closure in the correct order.

---

## 2. Voices diagnostic patch contract (only to discriminate V1–V4)

### 2.1 Causal boundary (frozen, permanent wording constraint)

```text
observed:      cross-realm voice-inventory inconsistency (top 5 vs iframes 58)
causal axis:   realm-specific vs temporal-initialization — UNRESOLVED
```

Until the discriminating experiment converges, evidence, documents, and commit messages must
all avoid "realm bug / realm-specific defect" as a causal conclusion. Only the two lines
above may be cited.

### 2.2 Instrumentation event table

Output channel: single-line JSON on stderr, prefix `VSIDIAG`, ≤240 characters. `uri` is
always the first 12 hex characters of its sha256. The timestamp is in-process monotonic
milliseconds. Each line carries `proc:PARENT|CONTENT` and the tid.

| Event   | Injection point (file :: function)                                  | Payload                  | Discrimination target                                           |
| ------- | ------------------------------------------------------------------- | ------------------------ | --------------------------------------------------------------- |
| E1      | additions MaskConfig consumer side: the GetInstance injection entry | `{engaged, n}`           | whether the configuration shape/parsing takes part (refutes V4) |
| E2a/E2b | `windows/SapiService.cpp :: Init` entry/exit (with a thread marker) | `{}` / `{}`              | SAPI sync or async (V2)                                         |
| E3a/E3b | `nsSynthVoiceRegistry.cpp :: GetInstance` managed batch entry/exit  | `{n}`                    | injection completion time (V1/V2 time axis)                     |
| E4      | `SpeechSynthesisParent.cpp :: SendInit`                             | `{actorTag, n, invHash}` | snapshot content and timing (core of V1)                        |
| E5      | `AddVoiceImpl` broadcast site                                       | `{seq, uriHash}`         | incremental delivery time axis                                  |
| E6      | content side `RecvInitialVoicesAndState` / `RecvAddVoice`           | `{n}` / `{uriHash}`      | mirror arrival time                                             |
| E7      | `SpeechSynthesis.cpp :: GetVoices` first call per object            | `{ctxId, n, t}`          | observation time vs delivery time axis (V1/V2/V3)               |
| E8      | `mVoiceCache` insert/clear counts                                   | `{ins, clr}`             | V3 publication/cache hypothesis                                 |

Each event is bounded. No raw voice names are printed. No profile/private paths are included.

### 2.3 Hard behavior-invariance constraints (the review checklist ticks each item)

Must not change: the voice order, the initialization timing, the native suppression behavior,
the managed inventory content. Must not introduce: sleep/retry/yield, an active
`getVoices()` trigger, unbounded logging (a cap of 512 events per process in total, with the
excess silently dropped and counted as one overflow event). The patch review uses a static
forbidden-word check: the number of call sites matching `Sleep|Wait|delay|getVoices\(\)` is
zero.

### 2.4 Discrimination decision table (frozen)

| Observed signature (VSIDIAG timeline)                                                               | Conclusion                                                                            |
| --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| E4(n<managed count) earlier than E3b                                                                | **V1**: the parent's lazy initialization being later than the first binding holds     |
| E2b later than E3b (native arrives later) or the reverse, with a significant offset                 | **V2**: async enumeration interleaving holds                                          |
| A second query on the same E7 object changes the count while E5/E6 delivered nothing new in between | **V3**: a cache/publication artifact holds                                            |
| E1 engaged=false or E3a never appears (the configuration is already proven delivered)               | **V4**: suspicion of hunk placement drift holds → switch to a source-level diff audit |

Multiple signatures may coexist (for example a V1+V2 composite). The 0005 final patch is
drafted only after the experiment convergence report is submitted to the lead agent.

---

## 3. Enforcement control ≠ identity declaration (frozen model)

```text
Identity declaration (website-visible identity state):
  the voices[] inventory itself
  fakeCompletion outward behavior (affects the SpeechSynthesis event timeline the website can observe)
    → policy-controlled semantics, enters identity policy/stableWebsiteFields

Engine enforcement control (execution semantics, not an identity attribute):
  native inventory suppression
    → [derived] as REQUIRED from the existence of the managed inventory, not a free switch
  voices:blockIfNotDefined
    → may enter resolvedConfig, but only as an engine config key deterministically derived
      from policy, always written as true by the artifact generator when voices is non-empty;
      not an identity property a user can decide separately
  voices:fakeCompletion:charsPerSecond
    → enforcement parameter, derived from policy
```

Validator freeze rule (implemented by the A-R1/B-R1 generation contract): an Artifact shape
with non-empty `voices` and suppression off is a **violation**, rejected directly at
generation time.

fakeCompletion additional freeze (Conditional Gate):

1. **It must not become an independent random Artifact fingerprint dimension**: the
   generator is forbidden to randomize `fakeCompletion` / `charsPerSecond` per artifact.
2. Both are derived deterministically from `voicesMode` / engine policy (or declared by an
   explicit policy variant).
3. The same voices identity must derive the same completion semantics in different
   Artifacts. Two Artifacts with different `fakeCompletion` are treated as **different
   identity contracts** and must not be compared under the same identity name.

## 4. FP1-R1 Carry-Forward Qualification (answer frozen, no TBD left)

A new R1 binary does not automatically inherit the historical FP1 Accepted. It also does not
redo the historical deBG source archaeology. There are two qualification layers, and only
after all pass may work enter FP2-R1:

**Layer 1 · static binding closure**

```text
canvas seam pre/post digest        byte-for-byte equal to the previous build
bounded-close seam pre/post digest same
patch application order            upstream 50 + downstream numbering order unchanged
new binding complete               new source revision / archive SHA / executable SHA /
                                   tree manifest / BuildID / SourceStamp all land in the new lock
```

**Layer 2 · Browser replay qualification**

Use brand-new `A-R1 / B-R1` on the new binary to re-prove deterministic replay (canvas
export determinism and core-surface replay). This step needs a browser and requires separate
lead agent authorization. It happens after the diag discrimination and the 0005 finalization.

## 5. Candidate lineage tree (formal historical structure)

```text
Candidate C0 (retired)
└─ FP2 Gen1–Gen5 ………………… FAILED
Diagnostic remediation candidate(s)
└─ capability investigation only (9000 series; never carries a product claim)
Candidate R1
├─ Policy verisilo.identity-policy.v4-dnt-native
├─ Artifact A-R1 / B-R1 (newly generated, validator per the §3 rules)
├─ FP1-R1 Carry-Forward Qualification (§4)
└─ FP2-R1 fresh one-shot lineage (artifacts/camoufox-fp2-r1/)
```

Generation numbering is permanently retired.

## 6. Gate sequence and authorization boundaries

```text
this contract Accepted
  → patch chain frozen (0003/0003a/0004/9000; seam digests computed into the new lock)
  → diagnostic build (Linux-bound route)
  → bounded voices discrimination experiment (needs separate lead agent authorization for the browser)
  → V1–V4 convergence report → 0005 drafting approval
  → clean R1 build → FP1-R1 layer 1 → layer 2 (browser authorization)
  → FP2-R1 execution contract (browser authorization)
```

---

## Appendix A · Worker getter target state (FF152 release native body, which 0004 must restore exactly)

```cpp
bool WorkerNavigator::GlobalPrivacyControl() const {
  bool gpcStatus = StaticPrefs::privacy_globalprivacycontrol_enabled();
  if (!gpcStatus) {
    JSObject* jso = GetWrapper();
    if (const nsCOMPtr<nsIGlobalObject> global = xpc::NativeGlobal(jso)) {
      if (const nsCOMPtr<nsIPrincipal> principal = global->PrincipalOrNull()) {
        gpcStatus = principal->GetIsInPrivateBrowsing() &&
                    StaticPrefs::privacy_globalprivacycontrol_pbmode_enabled();
      }
    }
  }
  return StaticPrefs::privacy_globalprivacycontrol_functionality_enabled() &&
         gpcStatus;
}
```

Source: hg.mozilla.org releases/mozilla-release @ `FIREFOX_152_0_4_RELEASE`
`dom/workers/WorkerNavigator.cpp` (actually fetched for this analysis). 0004 also removes the
part of the `#include "MaskConfig.hpp"` that fingerprint-injection.patch added to this file
only because of GPC. If that include is still needed by other hunks in the same file
(appVersion/platform/UA/hardwareConcurrency override), keep it. The criterion is that
MaskConfig GPC references in the file drop to zero.

## Appendix B · Sec-GPC emission semantic anchor

`nsHttpChannel::SetGlobalPrivacyControl()` (FIREFOX_152_0_4_RELEASE L11751–11760):
`functionality_enabled && (enabled || (pbmode_enabled && private browsing))` ⇒
`SetHeader(nsHttp::GlobalPrivacyControl, "1")`. It has the same structure as the Window/Worker
predicate, which proves that the single-point projection in §1.1 is enough to drive all three
exposure surfaces.

---

## 7. 2026-08-24 actual-9000 discriminator amendment

This section corrects the execution interpretation for `r1diag-engine-20260823t1542z` and its
frozen 9000. It supersedes the idealized E1–E8 table of §2.2/§2.4 as the runtime criterion
for that binary, but it does not retroactively edit the historical text.

The frozen 9000 actually has only E1–E7, `proc=P|C`, and a per-process `seq`. It has no
timestamp/tid, no E4 actorTag/inventory hash, and no E8. The fixed source also proves that E4
calls `GetInstance()` before its mark, and the managed batch completes before that call
returns. SAPI initialization enumerates and registers synchronously and returns before
E1/E3. Therefore:

- The `E4 < E3b` signature of V1 and the async interleaving signature of V2 are both
  `source-refuted-as-written`. Unreachable signatures must not be used to explain a future
  null result.
- A separate T1 is added. Only if the C sequence of the same top object shows exactly
  `E7(5 known native) → E6 delivery → E7(5 native + 53 managed)`, and the E4 parent
  snapshot is 58, does it support `content-mirror incremental delivery`. T1 is not renamed
  to V1.
- V3 has no E8, and its return value is formed before the cache rebuild. A count change
  without E6 can therefore only be `inconclusive / unexplained content-local transition`.
  It must not be called cache-causal.
- A null/missing E1/E3 for V4, when config delivery is already proven, is only a
  `source-seam suspicion`. It must not be called confirmed hunk drift.

P/C sequences cannot be compared across processes, and the log lines provide no reliable
total order across producers. The execution contract is therefore limited to top-only and a
single content producer, and it judges T1 only with the continuous seq/ctx/first inside C. A
single run may stay inconclusive and does not produce an exhaustive exclusion.

For the complete inputs, stderr capture, claim/lifecycle, and forbidden wording, see the
[FP2-R1 Voices Diagnostic Execution Readiness Contract](camoufox-fp2-r1-diagnostic-execution-task.md).
The 9000 SHA-256 is still
`1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8`,
`diagnosticOnly=true`, `formalCarryForward=never`. The patch, the source lock, and the
`1542z` build/provenance closure are all unchanged.

The Gate sequence is corrected to:

```text
R1-diag build/provenance closure (passed)
  → actual-9000 execution readiness (no browser)
  → one separately authorized bounded top-only diagnostic run (executed once)
  → immutable evidence offline readjudication (Inconclusive)
  → phase-anchor/source-level follow-up questions not yet frozen
```

The original report of the actual run stays `Failed / config_delivery_unproven`. The
offline verdict after the fix does not overwrite the original bytes, and the browser was not
rerun. For the full hashes and the 0→58 observation, see execution contract §8. This
amendment does not create a Formal R1 candidate, does not author `0005`, and does not accept
FP2-R1.

## 8. 2026-08-24 Voices phase-anchor implementation freeze

The lead agent ruled that the historical `top=5 / iframe=58` is a high-confidence
explanation of a temporal publication phase, not a stable realm split. The Gen5 top-5 hash
is the exact native prefix of the actual-9000 C-side 58-item delivery. The fixed source also
executes the SAPI native register/notify exactly, then adds the managed 53, and finally sends
the 58-item snapshot. Because Gen5 lacks a same-run event/sequence anchor, this conclusion is
not yet direct runtime proof, and `0005` stays closed.

`apps/camoufox-host/fp2_r1_diag.py` reuses the existing binary, the 9000 stderr transport, the
native supervisor, the bounded lifecycle, and strict JSON, and adds no instrumentation or
generic framework. The new `voices-phase-anchor-v1` makes only these minimal changes:

- The listener is registered before the initial query. Each event only increments a counter,
  and `getVoices()` is called synchronously only inside the first callback. Its evidence must
  then prove it was trusted and target-matched. A final query runs after a fixed 3 seconds.
  The same top JS holds the same synth object.
- The new claim and report schema/namespace are isolated from the already consumed
  actual-9000 claim. The old offline readjudication explicitly reads the legacy claim and
  does not overwrite historical bytes.
- The classifier requires, on the P side, exactly `5 native → 53 managed → E4(58)`, and on
  the C side, exactly `5 native E6 → 53 managed E6 → initial(58)`. Each observer query maps
  one-to-one to an E7.
- A phase positive holds only when initial=0, the first event is exact native5 with its
  corresponding E7 between native5 and managed1, and final=exact 58. Later events do not
  query again. If the first event is already settled, or there is no event, the output is
  only `not-observed / Inconclusive` and does not disprove the historical race. A structural,
  trust, object, or evidence misalignment fails closed.
- Even a positive fixedly outputs `0005-remains-closed` and
  `mainBrainAdjudicationRequired=true`, and all product/Formal/remediation claims remain
  false.

The exact SHAs of the Gen5 `raw-realms.json`, the probe bundle manifest, `realm-common.js`,
and `top.js` enter the readiness re-read. The tests also lock the first-event wait and the
top→iframe serial semantics of the historical probe. This section only freezes no-browser
measurement readiness. It does not launch Camoufox, does not modify frozen patch bytes, does
not accept FP2-R1, and does not author `0005`.

The Gate sequence is updated to:

```text
immutable Gen5 + actual-9000 + fixed source causal alignment
  → voices-phase-anchor-v1 readiness (this section; no browser)
  → one separately authorized phase-anchor run (not yet executed)
  → supported evidence returns to the lead agent; 0005 still needs a separate design Gate ruling
```

## 9. 2026-08-24 Voices final remediation authoring contract

Status: **Accepted (design/authoring contract only)**. The only v2 phase-anchor direct
evidence already supports `A0 empty → A1 native-only first notification → A2 settled union`.
On that basis this section opens the authoring boundary for a future, separately scoped
`0005`, but that patch, a Formal candidate, and a new source lock are still not created.

### 9.1 Formal series and exact seam

The only patch order allowed for a future Formal R1 is:

```text
0000 → 0001 → 0002 → 0003 → 0003a → 0004 → 0005
```

9000/VsiDiag/diagnostic markers must not enter this series. `0000`–`0004` (including
`0003a`) keep their frozen bytes. `0005-verisilo-voices-final.patch` may touch only:

```text
dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp
preimage SHA-256:
c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f
```

The only allowed source changes are: include the existing `MaskConfig.hpp`, and in
`SpeechSynthesisParent::SpeechSynthesisParent()` guard a single call to
`nsSynthVoiceRegistry::GetInstance()` with both `MaskConfig::MVoices()` existing and
`MaskConfig::GetBool("voices:blockIfNotDefined").value_or(false) == true`. Do not move the
call onto the unconditional native path. Do not move the existing block inside
`nsSynthVoiceRegistry::GetInstance()` or modify a second file. The postimage, the patch SHA,
and the full source-lock binding may only be computed from the actual bytes at future
authoring time. This section does not prefill them.

The static authoring Gate must prove:

1. The only production construction point of `SpeechSynthesisParent` is still
   `ContentParent::AllocPSpeechSynthesisParent()`. During construction the new actor is not
   yet in the manager's `ManagedPSpeechSynthesisParent` set, and
   `RecvPSpeechSynthesisConstructor()` still calls the existing `SendInit()` after it.
2. The managed eager init runs on the parent main thread, which satisfies the `GetInstance()`
   assertion.
3. The synth target keeps using the existing `/camoucfg` include path, with no new target,
   library, or dependency.
4. `nsSynthVoiceRegistry.cpp`, `SapiService.cpp`, `SpeechSynthesisChild.cpp`,
   `ContentParent.cpp`, and their seam bytes are unchanged.
5. A fresh exact tree passes `--fuzz=0` apply/reverse/apply, pre/post digests, patch order,
   diagnostic rejection, and a clean compile.

### 9.2 Artifact v4 policy plumbing

This is a separate, necessary prerequisite for the same Formal R1 outcome. It is not part of
the `0005` source bytes:

| mode    | generator output and validator conditions                                                                                                                                                                                                     |
| ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| managed | `voices` must be non-empty, every entry must have all five fields, and there must be exactly one default; deterministically derive `voices:blockIfNotDefined=true`, `voices:fakeCompletion=true`, `voices:fakeCompletion:charsPerSecond=12.5` |
| native  | `voices` and all three derived keys are absent                                                                                                                                                                                                |
| invalid | both sides fail closed when the managed list is empty/malformed, the default is non-unique, a derived key is missing, an explicit `false` appears, or the numeric value is not equal to `12.5`                                                |

The current v3 47-key schema/generator has none of the derived keys above, so it cannot be
used for Formal R1 and must not be silently accepted as compatible. The implementation only
extends the existing `identity_policy.py` / `generate_identity.py` / strict Artifact tests.
It adds no policy framework. `fakeCompletion` remains policy-derived semantics, not a random
identity dimension.

### 9.3 Behavior invariants and the future acceptance matrix

| Layer                          | Must pass                                                                                                                                                                                                                               |
| ------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| source/patch                   | exact FF152/upstream/preimage; single-file managed-only constructor guard; the rest of the voice source unchanged; no 9000 in the Formal series                                                                                         |
| actor lifecycle                | when managed init runs the new actor is not registered, so no initialization `VoiceAdded`/default/notify increments are sent to that actor; one `InitialVoicesAndState` handler then forms the complete inventory/default synchronously |
| native/historical preservation | when `MVoices()` or the exact-true suppression condition does not hold, the constructor does not call the registry, and the native and historical v3 startup/incremental/notification shape keep the current path                       |
| managed readiness              | the bounded listener-before-query/poll/event trace allows only `0* → exact managed*`; the first trusted, target-matched `voiceschanged` must be exact; any native, union, managed prefix, or wrong default fails                        |
| realms                         | top, same-origin iframe, and cross-origin iframe equal the Artifact in input order; the unique default URI is the same; a fixed notification count is not required                                                                      |
| replay                         | A-R1 projects identically across two cold starts, B-R1 projects B's inventory/default exactly; host native voices disappear from the managed observations                                                                               |
| lifecycle                      | compile/source review locks constructor main-thread, pre-registration, and reentrancy; the formal browser Gate continues to require clean close, an empty process tree, and evidence binding                                            |

The current comparator's `normalize_voice_projection(...) == expected_voice_projection(config)`
is an ordered list equality. The implementation and future fixtures must keep the Artifact
input order and must not sort. The initial `S0=0` does not count as a failure, but any
non-empty partial/native/union observation fails. The old §B.4 wording "non-empty at any
query time, including before publication" and "order-insensitive" is no longer a criterion
for R1 stronger than this section.

### 9.4 Gate order and stop conditions

```text
this design/authoring contract (no browser, currently complete)
  → separate bounded 0005 authoring + v4 Artifact/policy static implementation
  → Formal source lock + clean Windows build/provenance closure
  → FP1-R1 static binding + browser carry-forward qualification
  → one fresh FP2-R1 execution (phase/readiness + cross-realm + replay)
```

There is no need to build or run a diagnostic 9000 candidate again. Future
authoring/build/FP1-R1/FP2-R1 are each opened by their own Gate. This section by itself does
not mean `0005` is implemented, Voices is fixed, or FP2-R1, GPC runtime, or Formal R1 has
passed.
