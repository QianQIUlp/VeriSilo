# Simprint source-level due diligence

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

Status: **Accepted product-direction input**. This document records a frozen source-level review. It is not
Simprint runtime qualification, and it is not a current VeriSilo baseline or release evidence.

## Frozen review objects

| Object                    | Review revision                            |
| ------------------------- | ------------------------------------------ |
| Simprint main             | `d2dc939bfae8efda8e4e4d791587d7a364ca4dd8` |
| `simprint-browser-kernel` | `simprint/m144`, `9f7b8e0`                 |
| VeriSilo review baseline  | `f6ae647764b2577f2286c1f208c9f02acd4b6fde` |

These revisions define only the input objects of the source review at that time. They are not the current VeriSilo canonical baseline.

## What Simprint did better in the reviewed source

Advantages clearly visible in the review include:

- desktop Profile CRUD breadth, plus workstation capabilities such as groups, tags, workspaces and a recycle bin;
- REST API, 52 MCP tools, a per-environment CDP endpoint and multi-window input sync;
- account import, extension management, proxy asset usability/health checks;
- updater/release cadence;
- a broader Chromium source-patch surface.

Its Chromium patch surface includes deeper Chromium/net-layer work that Camoufox/Firefox does not currently match directly,
including TLS-related patches and MAC/network-layer changes. VeriSilo should not belittle
these real advantages in order to maintain its own narrative.

`VeriSilo has real differentiators != VeriSilo is globally more advanced`: none of the user-value
arguments below weakens the facts about Simprint's lead recorded in this section.

## VeriSilo's structural difference

The difference in VeriSilo's current implementation is not "we are local and they are not". It is:

- **Identity Integrity**: the Resolved Identity Artifact is versioned, SHA-256-protected, persisted stably,
  locked at the first successful launch, and bound to the engine/runtime;
- **Execution Integrity**: a declared Artifact must be executed by a declared and verified engine/package;
- **Runtime Evidence**: it clearly distinguishes `configured`, `applied`, `observed`, `verified` and
  `unavailable`, and binds evidence to the correct Silo;
- **Network Integrity**: diagnostics and evidence belong to the Silo, provider, selector and node.
  The must-proxy path fails closed before launch and does not degrade silently;
- **Engine/package trust**: exact asset binding, package tree, detached CMS signature,
  Desktop public signer pin and fail-closed verification.

`local-first itself is NOT a differentiator`. VeriSilo is not positioned as an open-source AdsPower, a free
clone of a commercial fingerprint browser, or a replacement for a full browser workstation.

## Accepted product verdict

**B — NARROW / DIFFERENTIATE**

This means: do not stop VeriSilo, and do not chase the breadth or feature parity of Simprint/AdsPower. Strengthen the integrity/evidence
capabilities that already exist, are structurally different and are perceivable by users. This conclusion does not mean an immediate architecture rewrite, and it does not
remove Standard Silo. Camoufox-first can still serve as the current Managed Engine strategy.

By default, a future large feature proposal needs to strengthen at least one of:

1. Identity Integrity;
2. Execution Integrity;
3. Network Integrity;
4. Runtime Evidence;
5. Attribution.

If a feature strengthens none of these five, it cannot automatically enter the roadmap merely because commercial fingerprint browsers generally have it.

### Why not A — STOP / ADOPT

The option "stop VeriSilo and adopt Simprint directly" is not chosen, because the Simprint reviewed source does not cover
the core structural capabilities VeriSilo claims:

- Resolved Identity Artifact / identity integrity (versioning, integrity, lock at first successful launch,
  replayability);
- Runtime Evidence / website-visible observation (the semantics of `configured`/`applied`/`observed`/
  `verified`/`unavailable`, and evidence ownership);
- precise network attribution;
- equivalent unified fail-closed semantics (such as the must-proxy launch preflight).

No equivalent mechanisms were found in the reviewed source, so VeriSilo's structural claims cannot be simply replaced or folded in.
Keep the source-evidence boundary in mind: this is a source-level conclusion, not runtime qualification.

### Why not C — CONTINUE BROADLY

The option "keep expanding across the board and compete head-on with Simprint" is not chosen, because Simprint already has substantial real coverage
and a clear lead in these directions:

- desktop workstation breadth (Profile CRUD, groups, tags, workspaces, recycle-bin);
- automation (REST API, 52 MCP tools, per-environment CDP, multi-window input sync);
- Chromium source-patch breadth (including deep TLS/MAC/net-layer changes);
- proxy usability and asset health checks;
- extensions;
- release/update/product maturity.

Catching up with a more mature product on the feature-parity track has no structural return.

Taking both together: **VeriSilo should continue to exist, but should keep investing only on the condition that its own structural differences have real user value.**
That user value is developed in the next section.

## User-value thesis

The premise of verdict B is not "a more elegant architecture". It is that the following capabilities have real value for specific users. This section is a product hypothesis (product thesis) based on
the source review. It is not proof from market research.

### Identity Artifact / Identity Integrity

The user value is not "a more elegant architecture". It is:

- long-term identity anti-drift;
- the ability to answer what identity an account actually ran under in the past;
- auditable configuration changes;
- stable identity replay.

For high-value, long-lived accounts/identities whose environment must not be casually broken, this is real value. For throwaway
environments created and discarded at will, the incremental value is lower.

### Runtime Evidence

The core semantics are: **what was configured != what actually runs**. The practical value of Evidence includes:

- after launch, knowing whether the engine, identity and network were really applied;
- less manual work opening detection sites to confirm one by one;
- when an account incident happens, being able to look back at the runtime facts instead of guessing;
- stale evidence does not pass for current.

Evidence must eventually become a low-friction product experience, and cannot forever remain only internal engineering evidence.

### Fail-closed network

This is one of the directions with the highest real user value in the current differentiation. The core incident model:

```text
required proxy fails
→ silently falls back to direct
→ real network identity exposed
→ multiple persistent identities may be correlated
```

VeriSilo's value is not "more proxy features". It is: **when the user explicitly declares must-proxy, the system would rather refuse to
start than continue silently on the wrong network.**

Under the Evidence precision boundary, this section does not claim that Simprint's main browser traffic has been proven at runtime to leak directly.
The direct observation from the reviewed source is that at least one auxiliary IP-detection path contains a direct fallback, and that no equivalent
unified must-proxy launch contract was found.

### Engine/package trust

An honest record: the value ordinary users directly perceive in CMS signing, certificate pins and exact executable provenance may be lower than for the first three. Its main incremental value is:

- preventing silent engine drift;
- supply-chain integrity;
- audit / controlled runtime.

Simprint already has a content-addressed kernel binding, so it cannot be written that "the other side has no engine
integrity at all". The difference is mainly the strength of the guarantee (exact executable provenance, detached CMS signature,
public signer pin and fail-closed verification).

### Encrypted Vault

User value:

- avoiding plaintext on disk when credentials for many accounts and proxies are stored in one place;
- a more controllable lifecycle for sensitive material;
- zeroize and similar measures are defense-in-depth.

The Vault is a supporting capability and does not by itself make up the whole product positioning.

## Counterfactual: would we start VeriSilo today?

Accepted answer: **ONLY_IF**.

Starting VeriSilo from scratch today is reasonable only if all of the following hold:

1. Target users really need identity artifact + runtime evidence + fail-closed
   execution/network semantics, and not merely multiple browser environments;
2. The project accepts not competing with Simprint on Chromium patch breadth: it currently uses Camoufox/Managed Engine,
   and Chromium-specific patch breadth such as SSL/MAC is not something it must match;
3. The project accepts that VeriSilo may trail on workstation/product breadth for a long time, and stands on integrity,
   evidence, attribution and trust, not on feature count.

If the goal is only an ordinary multi-environment desktop workbench + more CRUD + automation + multi-window operational convenience, the counterfactual
answer is **NO**. For that need, the more reasonable choice from scratch is to adopt or fork the existing Simprint or a similar
product, not to rebuild VeriSilo.

This does not require stopping VeriSilo now. It is a future strategic guardrail, used to judge when "continued independent investment"
loses its justification.

## 3–6 month investment judgment

Accepted conclusion: **YES — CONDITIONALLY**.

The reason to keep investing is not "a lot of code has already been written", because sunk cost is not a reason. The reason is that identity integrity,
runtime evidence, fail-closed network/execution and attribution were, in the Simprint reviewed
source at that time, structural capabilities that were really missing or clearly weaker, and that may also produce user value.

The premise for continued investment over the next 3–6 months is to execute B — NARROW / DIFFERENTIATE, with focus on:

- identity integrity;
- evidence UX;
- expected-vs-observed reconciliation;
- fail-closed execution/network;
- attribution;
- engine/package trust;
- the minimal product UI needed to support these capabilities.

Explicit stop condition: if the actual roadmap turns back into "build a commercial fingerprint browser workstation with more features", competing
on teams/workspaces, heavy automation, general RPA or large-scale Chromium patch breadth, then the reason to
keep investing in VeriSilo independently needs to be reevaluated, and by default investment should not continue on sunk cost alone.

> If the project returns to broad feature-parity competition, the investment thesis must
> be re-opened.

This is a condition for reevaluation, not an irreversible permanent ban. Genuinely new user needs can still reopen the decision.

## What VeriSilo should keep

1. Resolved Identity Artifact / immutable identity chain;
2. Runtime Evidence + website-visible observation;
3. fail-closed Network Policy + attribution;
4. encrypted Vault / secret handling;
5. engine package trust + EngineAdapter contract;
6. CLI / diagnostics / direct runtime verification;
7. the minimal Desktop UX that supports the capabilities above.

## What VeriSilo should stop competing on by default

Unless a new concrete user need reopens the decision, the default roadmap no longer actively expands the following directions:

- broad team/workspace/recycle-bin workstation breadth;
- generic REST API for every resource;
- huge MCP tool catalog;
- general RPA;
- multi-window syncer;
- account-import automation;
- generic proxy asset-management platform;
- Chromium patch arms race for feature count alone;
- competitor feature checklist parity;
- turning VM/Hyper-V/Remote into the ordinary Silo execution layer (Windows Sandbox acceptance is test-lab
  evidence, not a reason for VeriSilo to add a Hyper-V product capability).

This is not an order to delete existing implementations: capabilities already landed are kept according to their own lifecycles. It means the default roadmap no longer actively
expands these directions.

## Differentiated roadmap

The first priority direction is to connect the "expected identity" and the "identity the website actually observes" into an explainable product loop:

```text
Resolved Identity Artifact
        ↓ expected
website observed.json
        ↓ observed
automatic comparison
        ↓
matched / mismatched / unavailable / stale
```

That is: expected identity + observed website-visible identity → reconciliation → mismatch
explanation → evidence presentation. The user should ultimately be able to see, with low friction: **the identity I declared, what the website actually
saw, where the two agree, where they differ, and which fields currently cannot be verified.**

The discipline of Evidence semantics stays unchanged:

> expected-vs-observed reconciliation is the main product route to narrowing the `observed → verified` gap.
> A result can be upgraded to `verified` only when a future capability-specific verification contract is satisfied.

Equal observed values do not automatically raise the result to `verified`. `unavailable` and `stale` remain legitimate results.

This section records product direction, investment condition and roadmap priority. Concrete implementation
(the evidence comparison engine, presentation, Artifact schema or RuntimeEvidence state machine changes) belongs to a
later product task and is not executed within this documentation task.

## Evidence precision

### Fingerprint seed

The source review found that a certain noise seed path falls back to
`"default"` when there is no actual environment/profile id. The correct conclusion is:

> Source review found a potential cross-profile fixed-noise correlation defect.

This review did not run a runtime experiment on that defect, so it cannot be written as "it is proven that different Profiles can be
correlated".

### Proxy/direct fallback

The source review found that at least one IP/timezone/language detection auxiliary path has a direct fallback, and it did not
find a VeriSilo-style unified must-proxy launch preflight. The correct conclusion is:

> Simprint did not show an equivalent unified fail-closed launch contract, and at least one auxiliary IP-detection path contains direct fallback.

This does not mean Simprint's main browser traffic has been proven at runtime to leak directly after a proxy failure.

### Runtime evidence mechanism

The conclusion that can be stated is: the reviewed source did not show a systematic runtime evidence mechanism equivalent to VeriSilo's.
The original review was source-level only and never ran Simprint. This document does not imply any Simprint
runtime experiment.
