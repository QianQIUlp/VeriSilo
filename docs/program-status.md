# Program status

- Status: **current routing page**. This is the only page that records the current phase, the current work package and the next task for the whole product.
- Updated: 2026-11-10.
- Current goal: a commercial-grade multi-profile fingerprint browser, per the [north star](identity-platform-north-star.md) and the [commercial direction decision](product-direction-decision-2026-11.md).
- Plan: [roadmap](roadmap.md). Engine Gate history and evidence for Camoufox: [Camoufox status and evidence index](camoufox-program-status.md).

## Refs and releases

| Item                         | Value                                                                                                              |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Development branch           | `main`. Work happens directly on `main` and is pushed to `origin/main`. There are no other long-lived branches.    |
| Current public prerelease    | `v0.1.0-rc5` (`PUBLIC_GITHUB_PRERELEASE`), source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`. Release gate closed. |
| Release Gate                 | Closed. Only the owner opens a new RC or release Gate, by explicit instruction.                                    |
| Outer Authenticode           | Unsigned on every release so far. The internal engine CMS signature is a separate boundary.                        |
| Strict standard-user install | `NOT_PROVEN` on Windows (the rc2 Sandbox account was an administrator).                                            |

## Current phase

**Phase 1: engineering foundation**, with the **E0 Controlled Chromium evaluation** allowed in parallel. Phase 0 (repository reset) is complete.

| Work package                                  | State       | Evidence |
| --------------------------------------------- | ----------- | -------- |
| P1.1 Startup and lock-poison resilience       | not started |          |
| P1.2 Generated cross-boundary types           | not started |          |
| P1.3 Typed error contract                     | not started |          |
| P1.4 Platform modules and dependency hygiene  | not started |          |
| P1.5 Cargo workspace and CI completeness      | not started |          |
| P1.6 Vault and local-attack-surface hardening | not started |          |
| P1.7 Frontend workspace split                 | not started |          |
| E0 Controlled Chromium evaluation             | not started |          |

## Next task

Start **P1.5 Cargo workspace and CI completeness** first. It fixes the failing `pnpm format:check` that every release workflow runs, and it gives every later work package a lint and test safety net. Then P1.1 and P1.4, then P1.2 followed by P1.3. P1.6 and P1.7 can run at any point. E0 needs the owner's approval before any build machine is rented.

## Phase 0 record (2026-11-10)

- Product direction re-anchored: new [north star](identity-platform-north-star.md), [commercial direction decision](product-direction-decision-2026-11.md), [roadmap](roadmap.md), [architecture review](architecture-review-2026-11.md) and this page.
- The repository's docs and comments are English. Converting the product text (desktop UI, CLI and backend messages, extension, website, Host messages) to English-only is in progress. Historical records carry a header that points to the original Chinese text in Git at `1cf0f29`.
- The agent task workflow (`scripts/agent-task.mjs`, lanes, task worktrees, `baseline/dev`) was removed. Development now happens directly on `main`; see [AGENTS.md](../AGENTS.md).

## Product facts that later work must not overstate

- Standard Silos isolate Profiles only. Their fingerprint is the real machine's.
- Camoufox Managed Silos: FP1 to FP4 and clean M3-WI Attempt 4 passed on one native Windows host, all `verified:false`. TLS and QUIC are observed coherent but have no formal verifier. WebGPU has an open policy boundary. Login, payment and CAPTCHA flows were never tested.
- At most two concurrent Managed sessions; other modes are exclusive.
- rc5's bounded installed report is `INCONCLUSIVE`; its full legacy contract report is still `Pending`.
- The Camoufox engine is pinned to `v152.0.4-beta.28` (Firefox 152.0.4). Rebasing it is roadmap P2.5.

## Known issues carried into Phase 1

- Two historical runners pin their contract documents by SHA-256, and both pins were already stale at `1cf0f29` because commit `770a4eb` (2026-09-07) edited the documents. The English translation changes the M3-WI contract hash again. The frozen bytes are still in Git:
  - `apps/camoufox-host/run_m3_wi_clean_windows.py` pins `acdc725d…`, which is `docs/camoufox-m3-wi-clean-contract.md` at commit `26ded60`.
  - `apps/camoufox-host/run_fp4_windows.py` pins `db7fe9c0…`, which is `docs/camoufox-fp4-ordinary-site-compatibility-contract.md` at commit `7183071`.
    These runners belong to closed Gates. Re-running one requires restoring its contract file from that commit first.
- `pnpm format:check` fails on `main`, so the release workflows would fail at that step. P1.5 fixes it.

## Update rules

- Replace the phase table, next task and known issues when they change. Do not append history; Git keeps it.
- Record evidence by reference (commit, run ID, report path), not by copying it.
- Never write a plan, a configured value or a self-reported result as a verified capability.
