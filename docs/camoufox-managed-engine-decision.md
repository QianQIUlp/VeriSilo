# Camoufox-first Managed Engine architecture decision

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

- Status: **Accepted; superseded in part on 2026-11-10** by the [commercial direction decision](product-direction-decision-2026-11.md). Controlled Chromium is no longer deferred: it is the planned primary commercial engine (roadmap E0 and Phase 5), and the rule against two controlled engines became "only one controlled engine in active build-out at a time". The reasoning below about Camoufox, pinned assets, Artifact replay and evidence remains valid.
- Decision made: 2026-08-03
- Current facts and Gates: [Camoufox status page](camoufox-program-status.md)

This document records only stable architecture decisions. Phase checkpoints, failed runs, patch hashes, and implementation details belong to the status page,
the current task contract, lock/result files, and Git. They are not appended to this ADR.

## Problem

VeriSilo already has the Silo, Vault, Profile, network Provider, and EngineAdapter control plane. The biggest product
unknown now is not "whether another control UI can be built". It is whether a persistent, coherent identity can really be applied to the signals websites can see and
replayed across launches. An individual developer's resources are not enough to maintain several browser engines at once or to introduce whole-machine virtualization by default.

## Decision

The first Managed Identity Engine route uses:

```text
VeriSilo
→ EngineAdapter
→ standalone Host
→ pinned and bound Camoufox/Firefox engine
→ Resolved Identity Artifact
→ Persistent Profile
→ first-party runtime evidence
```

Camoufox-first is the stable first Managed Engine route. It does not remove Standard Silo, and it does not equate Camoufox
with the whole product. Its standalone, qualification chain, and productization seams have been completed at their respective evidence layers. The current execution phase is decided by the
[Camoufox status page](camoufox-program-status.md).

## Responsibility boundaries

- **BrowserForge** only generates constrained candidate identity parameters. It is not responsible for what the browser actually returns.
- **Camoufox** applies the identity at the Firefox engine layer. It is not a management platform and not a Chromium emulation.
- **The standalone Host** is responsible for asset binding, Artifact replay, Profile exclusivity, and the browser lifecycle.
- **EngineAdapter** is responsible for desktop invocation, package verification, transport, and capability state. It does not regenerate the identity.
- **VeriSilo** is responsible for the Silo, Vault, user policy, network, migration, UI, and the semantics of evidence.

Profile, Identity Artifact, Engine Binding, Network Policy, and Runtime Evidence keep independent
lifecycles. A configuration declaration cannot replace a runtime observation, and a Host's self-report cannot be promoted to verified automatically.

## Why Camoufox

- It controls signals at the Firefox engine layer, which avoids treating JS getters, extensions, or stealth scripts as a complete identity layer;
- It can reuse prebuilt assets and a narrow downstream patch, which fits an individual developer's resources;
- It can first prove the minimal vertical slice "generate → store → apply → cold-launch replay → website observation";
- It is easier to maintain than building Chromium, Camoufox, and a virtualization backend at the same time.

## Stable product interpretation of FP1–FP4

FP1–FP4 are not four groups of "fingerprint features" that may expand without limit. They are one qualification chain with a clear end point:

```text
FP1 = Identity determinism
FP2 = Identity consistency and replay
FP3 = Network-bound coherence
FP4 = Ordinary-site product compatibility
```

FP4 answers one bounded question only: with the exact Engine, Artifact, required Network Policy, and
Persistent Profile that passed FP2/FP3, can the frozen set of representative ordinary-website core tasks be completed on a native Windows Host, without
a browser crash, unbounded hang, Profile corruption, identity/network binding regression, or dirty close of an owned lifecycle?

So FP4 is not an anti-detection score, a proof of being "undetectable", universal site compatibility, a bypass of CAPTCHA or account risk controls,
a DNS/TLS/QUIC qualification, or an installer, signing, UI, or release Gate. Nor is its goal to keep adding spoofed fields.
An external website outage, structure drift, region/consent wall, CAPTCHA, rate limit, or third-party resource failure stays
`Inconclusive`. Only a direct, attributable product failure under the frozen inputs counts as `Failed`. For the necessary attribution, run only
one pinned upstream Camoufox control for the failed task. Do not build a full A/B matrix for passing items.

FP4 is the product-level go/no-go for Camoufox-first. A pass only supports continuing this route. It does not mean commercial product or release readiness. Only if a
capability for ordinary websites that the target users require hits a reproducible limit inherent to Camoufox/Firefox does it form valid evidence for reassessing Controlled
Chromium. No FP5 is created after FP4 closes. The clean M3-WI Attempt 4 completed afterward proves only its
frozen test-only desktop/Host qualification boundary.

The exact sites, tasks, budgets, verdicts, and immutable evidence are still governed by
the [FP4 frozen contract](camoufox-fp4-ordinary-site-compatibility-contract.md) and the
[FP4 aggregate result](../apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp4-result.json).
This section fixes only their long-term product meaning. It does not rewrite any historical attempt or widen its claim.

## Platform roles

- **Native Windows**: Windows assets, desktop lifecycle, file locks, Job Objects, and conclusions specific to Windows;
- **Linux/build host**: development, static verification, and engine cross-builds on demand. It does not replace the Windows runtime;
- **Tauri/EngineAdapter**: consume only capabilities that have passed the corresponding standalone/runtime Gate;
- **Temporary build machine**: a one-off compile tool. It is not product architecture or a long-term source of truth.

## Not adopted now

| Route                                                   | Decision                                                                                                   |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Keep polishing only Standard                            | Standard stays, but does not replace the Managed Engine risk verification                                  |
| Controlled Chromium                                     | Deferred. Reassess when there is a clear Chromium compatibility need and maintenance resources to carry it |
| BrowserForge alone as the execution layer               | Rejected. It does not change the browser implementation                                                    |
| JS/extension stealth as the main identity layer         | Rejected. Only limited experiments or observation are allowed                                              |
| WSL/VMware/Hyper-V/Remote as the default identity layer | Deferred. Environment isolation is not a coherent identity                                                 |
| Maintaining several controlled engines at once          | Rejected as a parallel investment for now                                                                  |

## Version and release principles

- Pin the accepted engine, source revision, patch series, toolchain, and assets;
- Do not follow `latest` at runtime, and do not treat automatic download as an install policy;
- A self-built archive does not pose as an upstream release. The version the identity declares must match the real engine binding;
- An upstream upgrade verifies only the affected capabilities and does not automatically rerun all historical research;
- Build/provenance, runtime qualification, signed release, and product shipped are different Gates.

## Historical qualification chain and current execution routing

```text
FP1 deterministic Artifact projection
→ FP2 cross-realm consistency and replay
→ FP3 network/geo/timezone/locale coherence
→ FP4 ordinary-site compatibility
→ clean M3-WI Attempt 4 test-only qualification
→ production package/signing, Managed Silo UI, and current-user NSIS implementation
```

The above is a historical qualification chain that has closed. It is not a new execution order. The current phase, candidate status, and next task are
maintained by the [status page](camoufox-program-status.md). The current `v0.1.0-rc2` has completed install acceptance in an exact pristine Windows Sandbox, but is not a public release.
Camoufox-first is still the current Managed Engine strategy. The accepted product direction is **B — NARROW /
DIFFERENTIATE**: it does not switch engines, does not remove Standard Silo, and does not widen the route into a broad workstation
feature-parity race. Read the old M3-WI, FP1/FP2 generation, diagnostic build, and one-shot execution history only when
investigating the corresponding evidence. They are not a default process template for new tasks.

## Conditions for reassessment or change

The following need a new explicit architecture decision:

- The main users or target sites must depend on Chromium APIs or the extension ecosystem;
- Camoufox cannot meet a clear compatibility target or is no longer maintained;
- The product needs direct control of Chromium-specific TLS/QUIC/V8 behavior;
- The team and revenue can carry the long-term patch/build/upgrade of another pinned engine;
- WSL, a VM, or Remote is to become the default execution layer for an ordinary Silo;
- The lifecycles of Profile, Artifact, network secrets, or evidence are to be merged;
- The distinction among `configured/applied/observed/verified/unavailable` is to be weakened.

A local bug, patch, builder difference, or test failure does not reopen the architecture automatically. Fix only the direct owning seam, and limit the
conclusion the evidence can support to the corresponding layer.
