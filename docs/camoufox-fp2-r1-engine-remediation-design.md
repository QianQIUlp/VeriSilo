# FP2-R1 Engine Remediation Design Freeze

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical design record**: the current execution entry point is the
> [status page, "Current next task"](camoufox-program-status.md#current-next-task). Read
> this document only when tracing the design rationale for GPC/Voices or the rulings on the
> source seam. Do not copy early Gate or candidate wording into new tasks.

- Status: **Accepted** (checkpoint `ff4f8c960d5cf63ff242da990b904fca8743be54`; includes the `38ff360c` policy-state amendment)
- Date formed: 2026-08-22
- Authorizing task: `FP2-R1 Engine Remediation Design Freeze` (offline/static only)
- Browser execution: this task never launched a browser. The browser stays CLOSED.

This document freezes the exact source seam for GPC and voices inside the current v152
candidate, the single-source-of-truth design direction, the versioned contract for DNT Policy
V-next, and the qualification rules for the R1 fresh lineage and the FP1 carry-forward.
It approves no implementation patch. Patch implementation belongs to a later, separate
contract.

## Binding anchors

```text
source lock:
apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json
(lockSha256 42b14bfe7331e6c023a3a6fa49da614852b6fd0a28c3225cc84bfc515d4036d5)

upstream:
https://github.com/daijro/camoufox
tag v152.0.4-beta.28
commit 0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d
tree   1435d544d9b61dee7fcf74cf92462952ca43d38e
(This analysis obtained the commit by sparse checkout. The git root tree hash matches the lock byte for byte.)

Firefox baseline comparison:
archive.mozilla.org firefox-152.0.4.source (sha512 is in the lock)
The file-by-file comparison used hg.mozilla.org releases/mozilla-release @ FIREFOX_152_0_4_RELEASE

patch application record:
artifacts/camoufox-fp1/windows-candidate-20260818T061456Z-e571f6c/
provenance/container.log (all 50 upstream patches applied, no rejects;
the individual hunks for speech-voices/voice/navigator/fingerprint are recorded below)
```

Terminology constraint (lead agent ruling, followed by this document):
`settings/properties.json` is the **engine configuration contract's declaration that it
recognizes a property**. It is not proof that the runtime implementation is correct. The Gen5
evidence shows exactly that the latter was not delivered.

---

## A. GPC source seam (fully decided)

### A.1 Seam table (inside the v152 candidate build)

| Exposure surface                        | File / function                                                             | Mechanism in this build                                                                                                                                                                                                                                                                                                                             |
| --------------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Window `navigator.globalPrivacyControl` | `dom/base/Navigator.cpp` `Navigator::GlobalPrivacyControl()`                | **Not modified by any patch.** Native path: `privacy.globalprivacycontrol.enabled`. When false, it falls back to the private-browsing branch (`UsePrivateBrowsing() && privacy.globalprivacycontrol.pbmode.enabled`). The result is finally gated by `privacy.globalprivacycontrol.functionality_enabled`. No consumer sets any of the three prefs. |
| Worker `navigator.globalPrivacyControl` | `dom/workers/WorkerNavigator.cpp` `WorkerNavigator::GlobalPrivacyControl()` | `patches/fingerprint-injection.patch` hunk (near container.log L503): it first reads `MaskConfig::GetBool("navigator.globalPrivacyControl")` and returns the value if one exists. Otherwise it takes the same native pref fallback.                                                                                                                 |
| `Sec-GPC` request header                | netwerk native path                                                         | Not patched. Driven by the same group of prefs as above.                                                                                                                                                                                                                                                                                            |

Key exclusion evidence:

- In `patches/navigator-spoofing.patch`, `bool Navigator::GlobalPrivacyControl()` appears
  only as hunk-header context. The actual change in that hunk is the `HardwareConcurrency()`
  that follows it.
- A search of all 50 patches, `additions/**`, `settings/camoufox.cfg`, and `scripts/**`
  found no `globalPrivacyControl` / `Sec-GPC` / `privacy.globalprivacycontrol` reference
  other than the two in the table above. camoufox.cfg does not set these prefs.
- `properties.json` / `camoucfg.jvv` declare `navigator.globalPrivacyControl: bool`. That
  is recognition at the contract layer.

### A.2 Structural explanation of the Gen5 observation

With the configuration set to `true`, the observation was worker=`true`, window=`false`,
header absent. A single mechanism reproduces this triple completely: **the worker side is
patched to read MaskConfig directly, while the window and HTTP sides stay on the native pref
path and nobody writes the configuration into the pref.** This is a realm-specific consumer
divergence (a single consumer covers only the Worker and everything else falls back to
native). It is not "two writers". The earlier wording "the engine has two inconsistent
writers" is withdrawn.

### A.3 Frozen single-source-of-truth design

```text
Artifact GPC bool
        ↓
one canonical engine state (single MaskConfig key, parsed once)
      ↙        ↓        ↘
 Window    Worker    Sec-GPC HTTP
```

Freeze requirements (acceptance boundary for the R1 patch design):

1. `Navigator::GlobalPrivacyControl()` and `WorkerNavigator::GlobalPrivacyControl()` read
   the same MaskConfig key. It is forbidden for the two sides to keep separate state or to
   obtain the value through different intermediate layers.
2. `Sec-GPC` emission and the two getters are driven by the same state. The allowed
   implementation shapes are "write the Gecko pref once at a single point when the
   configuration loads" or "patch the header emission point directly to read the same
   state". Pick one of the two. Do not introduce both paths.
3. It is forbidden to use an independent pref switch as a spoofing channel. A pref may only
   be a projection of the canonical state.
4. R1 verification invariant: `window == worker == Sec-GPC-present == configured value`.
   The GPC row in the relation matrix stays managed-required.

---

## B. Voices source seam (seam located exactly; root cause narrowed to initialization/synchronization timing)

### B.1 Verified full chain

```text
resolvedConfig["voices"] (53 entries, five fields {name,lang,voiceUri,isDefault,isLocalService},
Artifact A measured shape is valid, exactly 1 default)
→ CAMOU_CONFIG_N (on win, sequential slices of 2047 characters; host_v1
  normalize_camou_config_env + a digest round-trip check show the env and the on-disk
  resolvedConfig are equal key by key)
→ MaskConfig::GetJson() (additions/camoucfg/MaskConfig.hpp: sequential concatenation,
  validated by json::accept; if truncated, the whole result degrades to {}. The Gen5 worker
  GPC=true rules out truncation.)
→ MaskConfig::MVoices() (checks that the five fields exist with contains; a missing field
  logs ERROR to stderr and skips that entry)
→ nsSynthVoiceRegistry::GetInstance() (parent-process branch: after
  NS_CreateServicesFromCategory, it injects MVoices → AddVoiceImpl(nullptr, uri, name, lang,
  isLocal, false) + SetDefaultVoice)
→ SpeechSynthesisParent::SendInit() snapshot + incremental SendVoiceAdded inside AddVoiceImpl
  (FF152 adds the InitialVoicesAndState snapshot path; neither the snapshot nor the
   increments filter null-service entries;
   GetVoiceCount/GetVoice are bare mVoices enumeration and likewise do no filtering)
→ content-process mirror (RecvInitialVoicesAndState / RecvAddVoice)
→ SpeechSynthesis::GetVoices()
```

Confirmed bypass mechanisms:

- `patches/speech-voices-spoofing.patch` adds dom/base/SpeechVoicesManager and the
  `window.setSpeechVoices(DOMString)` WebIDL. This is an **allowlist filter** switch keyed
  by userContextId, and it takes effect only if it is called. There are zero callers in the
  whole repository (patches + additions). VeriSilo never activated it ⇒ the filter never
  participates.
- On FF152 the Windows backend is `dom/media/webspeech/synth/windows/SapiService.cpp` (the
  old `win/` directory no longer exists). Its enumeration URI scheme is
  `urn:moz-tts:sapi:<raw name>?<lang>` (L307) and it registers through
  `registry->AddVoice(this,...)` (L315). This is exactly the mandatory function guarded by
  `voices:blockIfNotDefined`. The "SAPI bypasses the guard" branch is rejected.
- container.log: all 4 hunks of `voice-spoofing.patch` applied (offset -2/-4, no fuzz).
  `speech-voices-spoofing.patch` applied (moz.build fuzz 1, nsGlobalWindowInner.cpp hunk#2
  fuzz 1 offset 444, and so on). At the mechanical application level, everything landed.

### B.2 Ruling corrections from the Gen5 raw evidence (supersedes part of the wording of the first offline reading)

A1 `raw-realms.json`, measured per realm:

```text
top-window:          5 entries: all native raw-name URIs (isDefault all false)
same-origin-iframe: 58 entries: 5 native + all 53 configured slug URIs
                              (slug David has isDefault:true, consistent with Artifact A)
cross-origin-iframe: 58 entries: same as above
```

New facts established by this:

1. **The injection chain works end to end**: MVoices parsing, AddVoiceImpl registration,
   and IPC synchronization all work. The first-round inference "voices projection failed
   completely / zero injection" is corrected here to "injection works, but enumeration
   consistency fails". The first-round ruling (FP2 Failed, a real capability gap) is
   unchanged. The comparator compares per realm for full equality with the configured
   list, so the 58-entry state also does not pass, and the verdict is unchanged.
2. **The reason native voices were not blocked is located as a wiring decision, not a
   defect**: VeriSilo v3 Artifacts (measured A) contain neither the
   `voices:blockIfNotDefined` nor the `voices:fakeCompletion` key. The guard condition is
   always false, so the native SAPI list is necessarily merged in.
3. **The real engine defect (newly and precisely defined) = enumeration consistency and
   default-value semantics**:
   - Within the same session, top-window and iframe observe different counts (5 vs 58).
   - `isDefault` is inconsistent across entries and realms (native entries are always
     false; the configured default is visible in only some realms).

### B.3 Root-cause candidates (after narrowing, all in the timing/initialization domain of voice propagation between the parent process and content processes)

| ID  | Candidate                                                                                                                                                                                                        | Discriminating experiment (R1 instrumented build; preparation is offline, running it needs future authorization) |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| V1  | The parent's first `GetInstance()` is lazily triggered later than the first content-process binding: the snapshot holds only the entries present at that time, and the injected batch then arrives as increments | Browser-process stderr timestamps (printf_stderr marks on the GetInstance/injection/AddVoice/SendInit sequence)  |
| V2  | Async SAPI enumeration interleaves with synchronous injection: different interleavings (managed first or native first) produce different intermediate states                                                     | Same marks plus a controlled-delay query sequence                                                                |
| V3  | Interaction between `mVoiceCache` / per-window state and the patched GetVoices                                                                                                                                   | Compare two queries (does a spaced query in the same window converge?)                                           |
| V4  | Hunk placement drifts semantically under fuzz=2 (mechanically successful but at the wrong position)                                                                                                              | Source-level diff audit of the build output (the new R1 lock must include per-file post-patch digests)           |

Until V1–V4 converge, no implementation patch is approved (the lead agent's instruction is kept).

### B.4 Frozen single-source-of-truth design

```text
MVoices (parsed eagerly once at parent-process startup)
        ↓
one canonical voice inventory (managed inventory exists ⇒ native inventory blocked)
      ↙         ↓         ↘
 snapshot   incremental broadcast   SpeakImpl fake-completion
(the three channels must present the same ordered inventory and the same default semantics)
```

Freeze requirements:

1. The inventory must be built before the moment the first content process can bind
   (eager), which removes the lazy-initialization window.
2. The semantics of `voices:blockIfNotDefined` change to "declaring a managed inventory
   implies blocking". In addition, R1 Artifacts explicitly carry
   `voices:blockIfNotDefined=true`, `voices:fakeCompletion=true`, and `charsPerSecond`, and
   the validator forces a check of these three keys whenever `voices` exists (this removes
   the silent defaults seen here).
3. The only source of `isDefault` is the managed inventory. Native entries must not enter
   the exposed set.
4. `window.setSpeechVoices` stays inert and is documented as a non-managed experimental
   interface.
5. R1 verification invariant: in any realm and at any query time, getVoices equals the
   configured inventory (URI, order-insensitive comparison; the default is unique and
   points to a configured entry), and the worker, iframe, and top-window agree.

---

## C. DNT Policy V-next (versioned contract frozen)

```text
policyVersion: verisilo.identity-policy.v4-dnt-native
selection:     fail-closed by binding (engineRevision + ffVersion)
```

| Dimension                              | FF < 135 (legacy branch)                                 | FF >= 135 (native branch)                                                                                                                                   |
| -------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `navigator.doNotTrack`                 | managed-required (current rule kept, target value `"1"`) | **native/unavailable**: no longer declares a managed target                                                                                                 |
| requiredConfigKeys                     | includes `navigator.doNotTrack: STR`                     | **does not include** that key; the presence of the key is a policy-schema violation                                                                         |
| stableWebsiteFields                    | includes the dnt observation field                       | **does not include** it; moved to the nativeSignals observation section                                                                                     |
| configured/applied/observed expression | current three states                                     | this signal has only observed (native capture), with no configured/applied declaration; comparator rule "no configuration + any observation = PASS(native)" |
| header relation matrix                 | DNT row managed                                          | DNT row marked `not-applicable/native`, takes no part in failure decisions                                                                                  |
| Artifact schema                        | verisilo-camoufox-resolved-identity/v3                   | **v4 variant** (selected by policyVersion), which removes the internal contradiction "policy says unavailable while the schema forces STR"                  |

Constraints:

- Historical A/B Artifacts are immutable. Gen5 stays Failed under v3 semantics and must
  not be reinterpreted.
- Future R1 uses brand-new `Artifact A-R1/B-R1` (new policy generation). It is forbidden to
  rewrite the DNT of the old A/B as unspecified and reuse them.
- The validator applies a strict either/or on `policyVersion + ffVersion`. Fuzzy
  compatibility is forbidden.

---

## D. R1 fresh lineage and FP1 carry-forward qualification

### D.1 Fresh lineage structure

```text
Historical lineage (immutable)            Remediation lineage (new)
  artifacts/camoufox-fp2/                   artifacts/camoufox-fp2-r1/
  claims: fp2-vN-one-shot-claim.json        claims: fp2-r1-*-claim namespace
  Gen1–Gen5                                 R1 execution contract to be drafted separately
  Gen6: permanently closed                  Generation naming permanently retired
```

From here on, the authoritative question is: "Which engine revision passes FP2 for the first
time under which identity policy version?" It is not a continuation of the Gen count.

### D.2 FP1 carry-forward qualification (mandatory clauses after the engine is recompiled)

The historical FP1 Accepted is bound to the old archive/executable/tree/engine revision. A
new R1 binary **does not inherit it automatically**. The new candidate must pass:

1. The Canvas deterministic patch/seam exists in the new tree (check pre-image/post-image
   digests of the target seam of 0001-verisilo-canvas-export-key.patch).
2. The bounded-close patch/seam exists (the same check for
   0002-verisilo-juggler-bounded-close.patch).
3. The source/build binding is complete (upstream revision, FF source digest, patch order
   and per-file digests, toolchain, archive SHA/size, tree manifest, BuildID/SourceStamp).
4. Whether deterministic replay needs a complete rerun of the FP1 one-shot: the lead agent
   decides after the R1 patch design is frozen. The proposition "no re-qualification is
   needed" does not hold.

### D.3 Subsequent Gate sequence

```text
this design freeze → lead agent review
  → patch implementation contract (new downstream patch files, with seam pre/post digests and discriminating-experiment marks)
  → new candidate build + FP1 carry-forward qualification
  → A-R1/B-R1 generation (Policy v4-dnt-native)
  → FP2-R1 execution contract (browser authorization returns to the lead agent)
```

## Appendix: evidence index for this analysis

- Upstream source: temp sparse checkout @ 0583c3ec… (root tree hash matches the lock);
  hg.mozilla.org FIREFOX_152_0_4_RELEASE Navigator.cpp / nsSynthVoiceRegistry.{cpp,h} /
  SpeechSynthesis.cpp / synth/moz.build / windows/{moz.build,SapiService.cpp} /
  ipc/SpeechSynthesisParent.cpp.
- Build record: candidate provenance/container.log (per-hunk patch record).
- Runtime evidence: fp2-20260822T065118Z-18fdbee7a8/A1/raw-realms.json (per-realm voices
  arrays), Artifact A (tests/fixtures/camoufox/identity-win-canvas-v1-a.json, 47-key,
  voices[53] valid, no blockIfNotDefined/fakeCompletion).

## 2026-08-22 Conditional-gate amendment: GPC policy-state model

The implementation contract review returned Conditional. The verification invariant in §A.3
of this document and the bool wording in the style of §1.2 hold under a managed-opt-out
declaration. But "an explicit false is equivalent to the default" is no longer legal
semantics: the managed identity declaration for GPC is opt-out only. The authoritative
correction is in implementation contract §1.2:

```text
gpcPolicy ∈ { native, managed-opt-out }
managed-opt-out ⇔ engine key present and true ⇒ all three projections true/present
native          ⇔ key absent ⇒ no pref write, observation is diagnostic only
explicit false  ⇒ illegal shape in v4, rejected by the validator on both sides
```

The historical v3 mandatory BOOL and the Gen5 verdict are unchanged. fakeCompletion is
frozen at the same time as policy-derived semantics. It must not become an independent
random fingerprint dimension.

## 2026-08-24 Voices publication-phase amendment

New evidence does not reopen this design's Managed Identity goal. It only narrows the Voices
root cause and the next Gate. Together, the immutable Gen5 `top=5 / iframe=58 / iframe=58`,
the actual-9000 exact native5 prefix with the full 58 delivery, and the fixed FF152 source
order support the following: the historical difference is most likely a temporal
publication race of `A0 empty → A1 native-only first notification → A2 settled managed+native`.
It is not a persistent realm filter or a divergence of the `mVoiceCache` inventory.

Gen5 itself has no same-run E6/E7/event anchor, so the current evidence is still not enough
to author `0005`. First freeze and pass `voices-phase-anchor-v1`: listener-before-query, a
synchronous query inside the first trusted event callback, a 3-second final, top-only, the
same synth object, and a continuous in-C seq that anchors the E7 between native5 and managed1
exactly. A missed window can only be Inconclusive. It must not disprove the race or trigger an
automatic rerun.

Only after the lead agent separately opens a remediation design Gate on new direct phase
evidence may the minimal future source seam be restricted to: "form the canonical managed
inventory and apply the policy-derived native suppression before the first
content-visible readiness notification". Do not move to realm special cases, a cache
workaround, probe delays that mask the problem, or a generic initialization framework. This
amendment does not modify frozen patch bytes, does not create `0005`, and does not accept
FP2-R1 or Formal R1.

## 2026-08-24 Voices final remediation design freeze

Status: **Accepted (design-only)**. The only v2 run,
`fp2-r1-phase-anchor-recovery-v2-20260824T112146Z-fee3df2667`, directly observed the
following on the same `speechSynthesis` object:

```text
S0 = 0
→ native VoiceAdded ×5
→ first trusted voiceschanged: exact native 5
→ managed VoiceAdded ×53
→ initial snapshot / settled: exact native 5 + managed 53
```

This matches the parent-process order in the fixed FF152 source exactly. The first content
actor's `SendInit()` lazily triggers `nsSynthVoiceRegistry::GetInstance()`. SAPI synchronously
registers and notifies the native voices, and only then does `MaskConfig::MVoices()` run the
managed injection and default setting. The root cause is therefore frozen as: **when the first
actor can already receive increments, the parent canonical managed state has not yet been
established**. It is not a realm filter, a content cache divergence, or async SAPI
interleaving.

### Minimal source seam

The reserved future name `0005-verisilo-voices-final.patch` may modify only:

```text
dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp
preimage SHA-256:
c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f
```

In the `SpeechSynthesisParent` constructor, only when `MaskConfig::MVoices()` exists and
`MaskConfig::GetBool("voices:blockIfNotDefined").value_or(false)` is `true`, call the
existing `nsSynthVoiceRegistry::GetInstance()` early. This call happens before
`AllocPSpeechSynthesisParent()` returns the new actor and before IPDL adds it to the
`ManagedPSpeechSynthesisParent` set. The parent therefore completes SAPI suppression, the
managed inventory, and the default first, and the existing `SendInit()` then sends only the
complete snapshot to the new actor. The authoring Gate must lock "the actor is not yet
registered during construction" with an exact source/generated-IPDL lifecycle check. If that
condition does not hold, fail closed. Do not substitute a speculative implementation.

The earlier candidate "move the managed block inside `GetInstance()` ahead of native service
startup" is not adopted. That candidate would fix the first `voiceschanged`, but an actor that
is already registered would still receive each managed `VoiceAdded` one by one. That cannot
rule out a script that polls and observes a managed prefix. The managed-only constructor seam
is smaller. When the managed/suppression condition does not hold, it does not initialize the
registry early, so the native FF152 path and the historical v3 notification shape stay
unchanged.

### Policy and behavior invariants

| Artifact mode | Must satisfy                                                                                                                                                            | Source/runtime result                                                                                                   |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| managed       | `voices` is a non-empty complete array; `voices:blockIfNotDefined=true`; `voices:fakeCompletion=true`; `voices:fakeCompletion:charsPerSecond=12.5`; exactly one default | native `AddVoice()` is blocked by the existing guard; the first content snapshot is the exact managed inventory/default |
| native        | `voices` and all three derived keys are absent                                                                                                                          | the constructor does not init eagerly; the native FF152 service, increments, and notification path are kept             |
| invalid       | empty/malformed managed list, missing or inexact derived keys, non-unique default                                                                                       | the Artifact generator/validator rejects it before startup; it must not rely on lenient C++ parsing as a backstop       |

`MaskConfig::GetBool()` returns `optional<bool>`, and existing call sites branch on key
presence. The managed policy therefore allows only an exact `true`, and an explicit `false` is
also an illegal shape. The suppression and the source seam are both required. Eager snapshot
alone yields a native+managed union. Policy suppression alone may leave the first notification
corresponding to an empty inventory.

The visible semantics for R1 are frozen as follows. The initial synchronous query `S0=0` is a
legal "not ready yet". The publication trace of each managed content registry may only be
`empty* → exact canonical snapshot*`. Native, union, or a non-empty managed prefix must not
appear. The first trusted, target-matched `voiceschanged` must carry a managed inventory that
equals the Artifact **in input order**, with the same unique default URI. After that, queries
from top, same-origin iframe, and cross-origin iframe must all keep that complete projection.
In the same future FP2-R1 probe, polling in the bounded window from listener-before-query to
the first event records count/hash/default and accepts only `0* → N*`. Do not create a
separate diagnostic run. The current executable comparator actually performs ordered list
equality, so this section supersedes the "order-insensitive comparison" wording of §B.4 with
"keep the Artifact input order". Do not add sorting or randomize the order. Repeated complete
notifications are allowed, and the event count is not frozen.

### Explicit prohibitions and Gate boundaries

- Do not change the managed/native blocks inside `nsSynthVoiceRegistry::GetInstance()`,
  `AddVoiceImpl`, `SetDefaultVoice`, SAPI, `SendInit`/`Recv*`, `GetVoices`/cache, or
  `SpeakImpl`.
- Do not add a ready flag, a state machine, a delay, a retry, an event coalescer, a realm
  branch, a cache workaround, or a generic initialization framework.
- Authoring source review/compile must prove that a constructor failure does not publish
  partial content state, and that the process registry is only ever absent or fully
  initialized. Add bounded fault injection only if that proof does not close, and do not use
  it to widen the patch.
- The Formal R1 series permanently excludes 9000/VsiDiag. This section does not author `0005`,
  does not modify any frozen patch bytes, does not build or launch Camoufox, and does not mean
  Voices is fixed or that FP2-R1 or Formal R1 has passed.
