# VeriSilo agent guide

VeriSilo is being built into a commercial-grade multi-profile fingerprint browser. This file is the entry point for coding agents.

## Language

The repository is English-only: code, comments, UI text, CLI output, error messages, tests and docs. Write everything new in English. Chinese characters remain only where they are data: font-family names and locale values in identity fixtures, captured machine output, and hash-pinned evidence files.

## Workflow

1. Sync first: `git fetch origin`, then `git status -sb`. If `main` is behind `origin/main`, run `git pull --ff-only` before reading or changing anything.
2. Work directly on `main` in this checkout.
3. Run the checks that cover the change (see below).
4. Commit with a clear message and push: `git push origin main`.
5. Update [docs/program-status.md](docs/program-status.md) when a work package changes state.

Never force push, rewrite published history, or push with `--all` or `--mirror`.

Ask the owner before opening an RC or release, spending money (certificates, build machines, hosting), changing the north star or an accepted decision, or doing anything destructive or irreversible.

## What to read

1. [docs/program-status.md](docs/program-status.md): the current phase and next task.
2. That phase in [docs/roadmap.md](docs/roadmap.md), then the code and tests you will change.
3. Only for product, architecture or boundary changes: the [north star](docs/identity-platform-north-star.md) and the [commercial direction decision](docs/product-direction-decision-2026-11.md).
4. Only for Camoufox engine work: the [Camoufox status and evidence index](docs/camoufox-program-status.md).

Historical task contracts, QA records and old runs are not default context. Read them only to check a specific fact. If documents conflict, the north star and accepted decisions win over status pages, and status pages win over historical records. If the status page contradicts the code or the evidence, report the conflict instead of picking the convenient answer.

## Checks

Use the cheapest check that honestly proves the change. Details are in [docs/development.md](docs/development.md).

- TypeScript packages: `pnpm check` and `pnpm test`, or `pnpm --filter <package> check|test`.
- Rust core: `pnpm desktop-core:verify`. Desktop crate: `cargo check --locked --manifest-path apps/desktop/src-tauri/Cargo.toml`.
- UI-only changes: UI Preview (`pnpm desktop:dev ui --port 1421 --preview`).
- Tauri commands, Vault, runtime or front-to-back behavior: Tauri dev with a named Vault (`pnpm desktop:dev core --port 1422`), or the CLI against a real backend.
- Installers and release acceptance: only after the owner opens an RC.

Preview or mock results are never runtime evidence. A passing build or test never stands in for a runtime result.

## Boundaries that never move

- Profile, Identity Artifact, Engine, Network Policy and Evidence keep separate lifecycles.
- `configured`, `applied`, `observed`, `verified` and `unavailable` are never mixed.
- A required proxy never falls back to a direct connection.
- Native Windows conclusions need native Windows evidence. Linux, WSL or Wine results never substitute.
- Only one controlled engine is in active build-out at a time. WSL, Hyper-V, VMware and remote environments stay frozen until roadmap Phase 7.
- Standard Silo is permanent.
- No "undetectable" or anti-fraud-bypass claims, no CAPTCHA solving, no bot-evasion input simulation, no payment or KYC fraud tooling, no automated mass account registration.
