# VeriSilo agent guide

VeriSilo is being built into a commercial-grade multi-profile fingerprint browser. This file tells an agent what to read, how to run a task, and which boundaries never move.

## Language

All agent-facing files are in English: this file, everything under `docs/`, code comments, and the messages printed by `scripts/agent-task.mjs`. Write new documents and comments in English. Product UI text stays as it is (mostly Chinese) until roadmap P2.1 adds the English and Chinese catalogs; when a document quotes a UI string, keep the original and add an English gloss.

## What to read

1. [docs/program-status.md](docs/program-status.md): the current phase, work package and next task. Always read it first.
2. The current phase's section of [docs/roadmap.md](docs/roadmap.md), then the owning code and tests of the work package.
3. Only if the task can change product semantics, architecture or long-term capability boundaries: the [north star](docs/identity-platform-north-star.md) and the accepted decisions ([commercial direction](docs/product-direction-decision-2026-11.md), [Camoufox-first engine](docs/camoufox-managed-engine-decision.md)).
4. Only for Camoufox engine work: the [Camoufox status and evidence index](docs/camoufox-program-status.md).
5. Only when delegating complex work, creating a Gate or reviewing an evidence package: the [agent operating model](docs/agent-operating-model.md).

Historical task contracts, old runs and superseded checkpoints are not default context. Read them only to investigate the matching fact.

If sources truly conflict, authority runs: north star → accepted decisions → current task contract → program status → implementation and direct evidence. A historical contract's old "current state" never overrides the status page. If the status page contradicts the code or the evidence, stop and report the conflict instead of picking the convenient answer.

## How to run a task

Do not ask the user for a worktree, Vault, port or baseline. Follow the [task routing workflow](docs/agent-task-routing.md):

1. Pick the lane (`ui`, `core`, `host`, `qa`, `integration`) by where the main change lands. Lane scope, restricted paths and verification commands are defined only in the configuration at the top of [scripts/agent-task.mjs](scripts/agent-task.mjs).
2. `node scripts/agent-task.mjs start --lane <lane> --task "<roadmap WP id and title>" --name <slug>`. The script fetches and prunes `origin` and requires local `baseline/dev` to equal `origin/baseline/dev` exactly.
3. Work inside the task worktree within the lane's scope.
4. `verify` and `check` must pass. `check` reports two different problems: a scope violation inside the worktree (exit 2) and WORKSPACE CONTAMINATION of the primary checkout (exit 3).
5. Commit, re-run `verify` and `check` on the final HEAD, then `publish` the task branch. Push only with exact refspecs and confirm the remote SHA after fetching. Never force push, and never use `git push --all` or `--mirror`.
6. A single-lane task that still forks from the current baseline, has passed `verify` at HEAD, has no restricted or out-of-scope change, and has a clean, published worktree may `promote`. If the baseline moved, `promote` returns `PROMOTION_REQUIRES_INTEGRATION`; hand the branch to integration and do not merge or rebase on your own.
7. Integration merges verified task branches with lightweight checks by default (remote SHA, ancestry, conflicts, scope, `git diff --check`, focused tests). Use `verify --full` only for shared contracts, lockfiles, workflows or configuration, several tasks on one seam, conflict resolution, or QA and RC candidates. Only integration (`baseline advance` and `baseline publish`) or a qualifying `promote` moves the baseline.
8. Update [docs/program-status.md](docs/program-status.md) when a work package changes state.

Task splitting:

- An authorized fix is one task in its owning lane: diagnose, minimal fix, focused validation. Do not split it into a QA task and a fix task unless the user asks for independent QA, ownership is unknown, or an independent acceptance is needed.
- A coherent product slice that crosses Host, core, UI and contracts is one `integration` task.
- Parallel tasks collect into one natural integration batch; the baseline does not have to advance after each one.

For worktree isolation and desktop layout details, read [module boundaries and development entry points](docs/development-worktrees.md).

## Verification cost

Use the lowest level that honestly proves the change:

- UI-only changes: UI Preview (real components, mock API, Vite HMR).
- Tauri commands, application logic, Vault, runtime or front-to-back integration: Tauri dev.
- Installer behavior, production packaging and release acceptance: an RC installer flow, and only after the user explicitly opens an RC or release Gate.
- Prefer the CLI with a real backend for Vault, Silo lifecycle, Managed, network, persistence and recovery behavior. Use Preview for what the CLI cannot express, and record the coverage boundary. Mock or Preview evidence never counts as runtime evidence.
- Ordinary pre-RC verification does not run a desktop production build.

Match process to risk:

- Documentation or mechanical edits: check references, formatting and the diff.
- Ordinary code fixes: check the owning seam and callers, make the minimal fix, run focused tests.
- Browser, engine build, evidence claims, releases or hard-to-reverse operations: frozen inputs, direct evidence and explicit stop conditions.
- Product, architecture, security or data-boundary changes: an explicit decision first.

Evidence stays valid until a relevant input changes. Host runtime evidence does not expire because of an unrelated site change, and so on. Integration verifies only what the composition adds. A `failed` or `inconclusive` attempt is kept and never retried with unchanged inputs or cherry-picked samples, but it does not end its engineering Gate by itself. Fix a confirmed causal blocker within the original authorization, then run a new, clearly versioned attempt.

## Boundaries that never move

- Every roadmap item names the customer job it serves and keeps the integrity invariants (Identity Integrity, Execution Integrity, Network Integrity, Runtime Evidence, Attribution).
- Profile, Identity Artifact, Engine, Network Policy and Evidence keep separate lifecycles.
- `configured`, `applied`, `observed`, `verified` and `unavailable` are never mixed. A configuration, a passing test or a successful build never stands in for a runtime or product Gate.
- A required proxy never falls back to a direct connection.
- Native Windows conclusions need native Windows evidence. Linux, WSL or Wine results never substitute.
- Only one controlled engine is in active build-out at a time. WSL, Hyper-V, VMware and remote environments stay frozen until roadmap Phase 7.
- Standard Silo is permanent.
- No "undetectable" or anti-fraud-bypass claims, no CAPTCHA solving, no bot-evasion input simulation, no payment or KYC fraud tooling, no automated mass account registration.
- Agents never open an RC or release Gate, spend money, or change the north star or an accepted decision without the user's explicit instruction.
