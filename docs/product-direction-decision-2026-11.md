# Decision: commercial fingerprint browser direction

- Status: **Accepted**, 2026-11-10.
- Decided by: the repository owner set the goal ("build VeriSilo into a commercial-grade fingerprint browser; change anything that conflicts") and delegated the detailed decisions to the lead agent. This record is that delegated decision.
- Scope: product positioning, engine strategy, data portability, automation, teams and sync, business model and compliance, and sequencing.
- Supersedes in part:
  - [Camoufox-first Managed Engine decision](camoufox-managed-engine-decision.md): the deferral of Controlled Chromium and the rule against two controlled engines. Its reasoning about Camoufox, pinned assets, Artifact replay and evidence stays valid.
  - [Simprint source due diligence](simprint-source-due-diligence-2026-09-09.md): verdict B ("narrow and differentiate; do not compete broadly"). That report named this exact change as its stop condition: a return to "a more fully featured commercial fingerprint browser workstation" reopens the investment thesis. This decision reopens it.
  - [Non-goals](non-goals.md): the bans on cookie import/export and on any cloud sync.

## Context

On 2026-11-10 VeriSilo had:

- a public prerelease, `v0.1.0-rc5`, for Windows (current-user NSIS, outer Authenticode unsigned) and native Linux x86_64 (Debian and AppImage);
- Standard Silos on system Chrome and Edge, and Managed Silos on a VeriSilo-built Camoufox 152 engine with an internally CMS-signed package and 18 locale presets;
- at most two concurrent local Managed sessions; every other runtime mode is exclusive;
- a loopback local API, a CLI and Camoufox page actions, but no Playwright or CDP attach;
- no groups, tags, bulk operations, proxy library, cookie import/export, cross-machine migration, team features, auto-update or English desktop UI.

The previous product direction deliberately avoided feature parity with commercial multi-profile browsers. The owner has now set feature parity plus integrity as the goal.

## D1. Positioning

**Decision.** VeriSilo is a commercial multi-profile fingerprint browser. Workstation features that customers in this category expect (profile management at scale, proxy library, import and export, automation, teams) are in scope. Integrity and evidence remain the differentiator.

**Rules that follow.**

- A roadmap item must name the customer job it serves and must not break the integrity invariants in the [north star](identity-platform-north-star.md). Matching a competitor is a valid reason to build something. It is not a reason to drop an invariant.
- Product copy describes coherence, isolation, stability and evidence. It never claims to be undetectable or to bypass anti-fraud systems (see D7).

## D2. Engine strategy

**Decision.** Controlled Chromium becomes the planned primary commercial engine. Camoufox stays the shipping managed engine until Controlled Chromium passes its qualification, and it stays supported afterwards as the Firefox-family engine.

**Why.**

- Chromium-family browsers carry most desktop web traffic, so a Chromium identity blends into a much larger population than a Firefox identity.
- Customers rely on Chrome extensions and on Chromium automation (Puppeteer, Playwright over CDP).
- A real Chromium network stack gives a TLS and HTTP/2 shape that matches the claimed Chrome version without separate emulation.
- The leading products in the category ship a Chromium engine, and several also ship a Firefox engine.

**Route.**

1. **E0, evaluation (time-boxed, about three weeks).** Compare these bases with evidence, then record the choice as an addendum to this decision:
   - ungoogled-chromium plus an existing BSD-3-Clause fingerprint patch set, for example [adryfish/fingerprint-chromium](https://github.com/adryfish/fingerprint-chromium) or the patches published by [clearcote-browser](https://github.com/clearcotelabs/clearcote-browser);
   - VeriSilo's own patch set on upstream Chromium;
   - a licensed third-party kernel.
     Criteria: license and redistribution terms; patch surface and code quality; how fast it follows Chrome stable; reproducible build on VeriSilo infrastructure; coverage of the coherence surfaces (UA and client hints, navigator, screen, WebGL and WebGPU, Canvas, audio, fonts, media devices, voices, timezone and locale, WebRTC, TLS and HTTP/2); and whether the seed-to-identity derivation can be made deterministic and bound to a VeriSilo Artifact.
2. **Build-out (roadmap Phase 5).** Build pipeline, Chromium Artifact family, adapter integration, qualification chain CP1 to CP4 (the Chromium counterpart of FP1 to FP4), signed packaging, updates and per-Silo extensions.
3. **Maintenance.** After release, follow Chrome stable within one major version, about four weeks. A stale engine is both a security risk and a visible fingerprint.

**Constraints.**

- Only one controlled engine is in active build-out at a time. While Controlled Chromium is in build-out, Camoufox receives maintenance only: security rebases, regressions and evidence fixes.
- Chromium builds need dedicated build capacity. Plan for a builder with roughly 32 or more vCPUs, 64 GB RAM and 300 GB of SSD, owned or rented. This is an estimate to be confirmed in E0.
- Every engine follows the same rules as Camoufox: pinned inputs, exact provenance, signed package, verified before launch, no runtime download of "latest".

## D3. Data portability

**Decision.**

- **Silo packages.** A user can export one or more stopped Silos into an encrypted, integrity-checked package and import it on another machine or account. The package keeps Profile, Artifact, Engine Binding, Network Policy and evidence history as separate parts. Proxy credentials and controller secrets are excluded unless the user explicitly includes them.
- **Cookie import and export.** A user can import cookies into a Silo and export them from a Silo, in Netscape `cookies.txt` and JSON formats.
- **Still forbidden.** VeriSilo never reads, imports, clones or modifies the user's default browser profile on its own initiative.

**Integrity rule.** An imported Managed Silo replays its configured identity, but host-bound signals (for example device pixel ratio and glyph rendering) can differ on the new machine. After import, the evidence for the Silo is `stale` until a fresh observation runs, and any difference shows as a mismatch.

## D4. Automation

**Decision.**

- Local API v2 with an OpenAPI description, full Silo CRUD, scoped and expiring tokens, revocation and an audit log. It listens on loopback only, and automation scopes are off until the user grants them.
- Per-Silo browser attach: Playwright and Puppeteer over CDP for Chromium Silos, and the Playwright Firefox protocol for Camoufox Silos, through an authenticated loopback endpoint that VeriSilo owns.
- An MCP server for AI assistants, built on the same scopes.
- RPA builders and multi-window input synchronization are later options, considered only after the API is stable.

The [threat model](threat-model.md) must be updated before each automation surface ships.

## D5. Teams and sync

**Decision.**

- Multi-device and team use go through end-to-end encrypted sync. Keys stay with users.
- A self-hostable, open-source sync server comes first. A VeriSilo-hosted instance can follow as a paid service.
- A Silo has one holder at a time, enforced by a lease. Sharing has roles (owner, admin, member) and per-Silo permissions such as "launch only", "no export" and "cannot view proxy credentials".
- No telemetry by default. Crash reports are opt-in and redacted.

## D6. Business model and compliance

**Decision.**

- **Open core.** The application stays MPL-2.0 open source with full local single-user features. Revenue comes from services that cost money to run: hosted team sync, priority support, and possibly a commercial engine update channel. Final pricing is decided in roadmap Phase 6.
- **Before any paid offering:**
  - terms of service, an acceptable-use policy and a privacy policy;
  - a human review of every third-party license (Chromium and its dependencies, Camoufox under MPL-2.0, BrowserForge under Apache-2.0, all npm and Cargo dependencies; Mihomo is GPL and is not bundled);
  - an Authenticode code-signing certificate, or a managed signing service;
  - confirmation that the chosen payment provider or merchant of record accepts this software category, since some restrict multi-profile and anti-detect browsers;
  - an export-control check for the bundled cryptography.
- These are business and legal questions. The roadmap tracks them, but a qualified person must sign off on them.

## D7. Acceptable-use limits

VeriSilo will not build:

- CAPTCHA solving or CAPTCHA farm integrations;
- synthetic "human-like" input whose purpose is to defeat bot detection;
- tools for payment fraud, identity-verification (KYC) evasion or chargeback abuse;
- automated mass account registration.

Product copy never promises undetectability or bypassing anti-fraud systems. These limits keep VeriSilo signable, payable and listable in browser extension stores, and they keep its claims credible.

## D8. Sequencing

1. Phase 1: engineering foundation. E0 engine evaluation runs in parallel in the `host` lane.
2. Phase 2: commercial single-user release (signed, auto-updating, bilingual).
3. Phase 3: profile management at scale.
4. Phase 4: automation and integrations.
5. Phase 5: Controlled Chromium engine.
6. Phase 6: teams, sync and commerce.
7. Phase 7: platform and environment expansion (macOS, isolated machines, remote).

Phases 3 and 4 can overlap with Phase 5 when they touch different lanes. WSL, Hyper-V, VMware and remote environment expansion stays frozen until Phase 7. Details are in the [roadmap](roadmap.md).

## What stays valid

- The Silo domain model and its separate lifecycles.
- The evidence vocabulary and the rule that observed equality is not verification.
- Fail-closed network behavior and the ban on direct fallback.
- All accepted Camoufox evidence within its recorded boundaries: M0 to M2-W, M3-0, FP1 to FP4, clean M3-WI Attempt 4, and the rc2 Windows Sandbox installed acceptance.
- Standard Silo as a permanent entry layer.

## Reopen conditions

Reopen this decision if any of these happens:

- E0 finds no Chromium base with acceptable license, quality and maintenance cost;
- a payment, signing or store provider refuses the product category and no alternative exists;
- legal review finds the planned scope unlawful in a target market;
- the owner changes the product goal.
