# Parallel development and module boundaries

VeriSilo stays a single repository with a single desktop product. Worktrees isolate source code. Named Vaults, ports, and separate build outputs isolate development instances.

The agents' automated task flow (lane decision, automatic worktree setup, scope guard, lane-level verification, integration merging)
is in [Agent task routing and automated workflow](agent-task-routing.md) and is driven by `scripts/agent-task.mjs`.
This document describes the underlying conventions and the manual procedure it depends on.

## Code ownership

| Work                                                                        | Owning code                                                                 | Minimum verification                                                         |
| --------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Pages, forms, interactions                                                  | `apps/desktop/src/features/{silos,identity,vault,network,environments,cli}` | desktop check/test; preview of the affected pages                            |
| Page assembly                                                               | `apps/desktop/src/App.tsx`                                                  | desktop check/test; preview of the affected pages                            |
| Cross-page session, refresh, async invalidation, and operation coordination | `apps/desktop/src/workspace/useDesktopWorkspace.ts`                         | Vault UI tests; UI check after locking                                       |
| Standard creation draft and cleanup                                         | `apps/desktop/src/features/silos/useSiloDraft.ts`                           | Vault UI cleanup tests; the creation page                                    |
| Shared business operations                                                  | `apps/desktop/src-tauri/src/application/`                                   | core harness `application::` tests                                           |
| Window, tray, Tauri entry point                                             | `apps/desktop/src-tauri/src/lib.rs`, `commands.rs`                          | desktop Rust check; the corresponding native window path                     |
| CLI / HTTP entry points                                                     | `local_api.rs`, `bin/verisilo-cli.rs`                                       | desktop Rust check/test; local API contract                                  |
| Lifecycle, package verification, network, storage                           | `launcher.rs`, `engine.rs`, `mihomo.rs`, `proxy_relay.rs`, `vault.rs`       | owning module focused tests                                                  |
| Host and engine                                                             | The matching Python, patch, and lock files in `apps/camoufox-host/`         | owning Host/patch tests; native runtime evidence for the affected capability |
| User acceptance and release                                                 | `tests/windows/`, `docs/acceptance/`, existing release scripts              | the matching acceptance on the exact candidate                               |

`application` does not depend on Tauri and does not start a local HTTP service. It holds `DesktopCore` and receives explicit data and resource roots.
Tauri commands and the Local API call the same business operations. `lib.rs` holds the window-layer state, the Vault instance lock, and the HTTP server.
The core harness compiles the production business source through path modules, so no copy has to be maintained.

The front end calls the back end through `desktop-api.ts`. Feature components do not import `App.tsx` or the workspace coordinator in reverse.
The local state of the creation, editing, and managed forms, the Vault settings, and the environment panel is kept in each component.
The coordinator still owns Vault session invalidation, cross-page data refresh, and operation ordering, and it clears the creation draft in one place when the Vault locks.

## Shared baseline

The canonical fork point for agent tasks is the remote ref `origin/baseline/dev`. The local
`baseline/dev` is a work ref that must equal it exactly (to inspect: `node scripts/agent-task.mjs baseline`).
`start` first runs fetch/prune on origin and fails closed when the local or remote ref is missing or they differ. A manual procedure should also create work branches from
this synchronized pair of refs and must not fork from an arbitrary local HEAD.
Only integration may advance the local ref explicitly with `baseline advance`, after a round of merge verification passes, and then publish
`origin/baseline/dev` without force with `baseline publish`. After a task or integration branch finishes verify + check
and is committed, it publishes its own remote branch. Do not force push, `git push --all`, or `git push --mirror`.
`codex/camoufox-m3-engine-adapter` is kept as the stable mainline but is no longer the canonical development source for new tasks.

A new worktree contains only the files in commits. Before branching, commit the source you currently need (including new files).
Do not mix locally generated Host build directories, browser packages, or old evidence into source commits.
Existing uncommitted work cannot be carried over by creating a worktree from an old HEAD.

Create work branches such as `qiu/ui` and `qiu/core` from the shared checkpoint. Each worktree installs its own dependencies and keeps its own
`node_modules/.vite`, Rust target, staging, and release outputs. Do not link these writable directories to another worktree.

## Three-tier development loop

For daily development, choose how to run by "the lowest-cost run level sufficient to verify the current change". From fastest to slowest, the three tiers are:
UI Preview → Desktop Dev → RC/Release acceptance.
For normal development, use the cheapest execution mode that exercises the changed
behavior. Do not build or install a release artifact merely to verify a change that
can be exercised in UI Preview or Tauri dev.

The caller explicitly assigns both the name and the port. Neither may repeat across instances running at the same time.
The script sets the Vite port and the Tauri devUrl together, and passes `--vault dev-<name>` through the app arguments.
It does not change system environment variables, install the product, copy user Profiles, or create an engine package.
`--dry-run` shows the actual arguments without starting.

### Mode A: UI Preview (feedback in seconds)

```powershell
# UI development: mock data in the browser; does not start Tauri or a real browser engine
pnpm desktop:worktree ui --port 1421 --preview
# Open http://127.0.0.1:1421/preview.html?scenario=overview
```

Real React components + an in-memory mock API + Vite HMR. It does not start the Rust backend, does not read or write a real Vault,
and does not start Host / Camoufox. It suits CSS, layout, components, copy, form UX, and
loading/error/empty/running states. The preview page provides overview, empty list, locked, loading, first use,
running, launch failure, and a standalone managed creation form. It uses real components and an in-memory sample API. Operations that are not simulated
report an explicit error. A refresh discards the simulated data. The simulated submit in the managed form is for interaction checks only. The desktop network exit check button
is still a real public network query that runs after explicit consent. `preview.html` is a development entry point. The default production
build packages only `index.html` and does not load the mock API.

Verified behavior (2026-09, measured in a task worktree): after saving a `.tsx` / `.ts` / CSS file, Vite immediately broadcasts
`hmr update`. React Fast Refresh updates the component in place and keeps the page's JS state, with no full page refresh
and no need to restart the dev server. Changing a module that Fast Refresh cannot handle, such as hooks, triggers a full reload.
This is expected. No state persistence is added for "100% state retention".

### Mode B: Desktop Dev (real desktop, still not an installer)

```powershell
# Real desktop development: uses the dev-core Vault; does not use the default Vault
pnpm desktop:worktree core --port 1422
# An agent task passes its own separate Vault (agent-task.mjs start prints the exact command)
pnpm desktop:worktree core --port 15437 --vault ui-create-silo-ux-3f9a2c
```

Real React → Tauri dev → real application → named Vault/runtime. It reaches
Host / Engine only when the task needs it. It suits Tauri commands, application logic, Vault, runtime, front-end/back-end integration, and
real desktop behavior. The same Vite instance that `tauri dev` starts through `beforeDevCommand` serves both the
desktop WebView (`index.html`) and the browser preview (`preview.html`).

- Front-end changes (`.tsx` / `.ts` / CSS / front-end assets): Vite HMR updates the current Tauri WebView directly.
  It does not recompile Rust and does not reinstall the app.
- Rust changes (`src-tauri/**/*.rs`, including `src/application/`): the Tauri dev watcher detects the change →
  cargo compiles incrementally (only the affected crate is rebuilt) → the development app restarts automatically.
  It does not build an installer and does not run the install flow.

Verified behavior (2026-09, measured in a task worktree): after changing the window title in `lib.rs`, the watcher log
`File src-tauri\src\lib.rs changed. Rebuilding application...` → only
`verisilo-desktop` was recompiled (14.25s; a cold build in the same worktree takes 3m12s) → the app process restarted automatically and the change took effect.
During this, the WebView's front-end HMR updates took effect instantly in the same process, and no installer was produced at any point.

### Mode C: RC / Release acceptance

Use it only when the user has explicitly opened the RC/release gate, and an RC is being formed, the installer itself or the install/upgrade/uninstall behavior changed,
production resource/package behavior needs verification, or pre-release user-journey acceptance is needed:
production build → installer → install in a dedicated environment → user journey → acceptance evidence.
RC candidates and install acceptance run in a dedicated environment on a determined candidate, following the [release process](release.md) and the
[acceptance runbook](acceptance/manual-windows-acceptance-runbook.md),
and are not mixed with any development instance. Do not treat Mode C as the ordinary development loop.

### Level selection rules

The agent chooses automatically from the owning layer and the real boundary to verify. The user does not need to specify it each time:

| Change                                                           | Level  |
| ---------------------------------------------------------------- | ------ |
| Button spacing, copy, form layout, CSS, ordinary UI states       | Mode A |
| Behavior of an application / Tauri command such as `create_silo` | Mode B |
| React UI + application contract                                  | Mode B |
| Installer upgrade-install, production packaging behavior         | Mode C |
| Preparing a release candidate                                    | Mode C |

HMR only shortens the feedback loop. It does not lower the verification standard: quick visual/behavior feedback ≠ full correctness verification ≠ RC acceptance.
Preview can show that component rendering, interaction, and state display are correct. It cannot show that the Vault was really written, the Tauri command is
correct, the Engine really started, or the installer is correct. Conversely, do not run a full installer to verify a CSS change.
A real Managed launch still consumes an existing, verified development package. UI preview cannot replace it,
and ordinary Pre-RC QA must not build a package on its own for this.

## CLI-first / HMR fallback

For Vault, Silo, lifecycle, Managed, Network, persistence, and recovery behavior that can be honestly verified with the CLI + a real backend,
prefer the CLI. When the CLI cannot express a behavior, record the coverage boundary. Do not automatically mark QA as BLOCKED, and do not
extend the CLI for every UI/browser scenario. Verify visuals, interactions, copy, and states with Vite HMR/Preview. Mock/Preview
evidence cannot stand in for runtime evidence. Enter Desktop Dev only when neither CLI nor HMR can verify honestly and a real Tauri/runtime
integration is truly needed. Even then, do not install, package, or build a release.

## Merging and shared resources

- A change to `desktop-api.ts`, `packages/contracts`, a Rust DTO, the Host protocol, or a data format first forms one small shared commit.
  Callers update afterward and implement in parallel. Do not define the same field separately in several worktrees.
- `shared/` and the common `styles.css` are coordinated by one explicit task. Feature work should prefer editing its own components.
  This round keeps the CSS cascade and does not use a large style rearrangement to change the existing UI.
- The same Mihomo controller/selector group shares node state. A parallel test that changes nodes uses a separate instance
  or runs serially. A named Vault does not isolate an external proxy program.
- A fixed engine package can be reused as read-only input. A new package uses a new output directory and must not overwrite a package someone else is verifying.
- Install, upgrade-install, and uninstall are run by one acceptance task in a dedicated environment. A worktree name cannot isolate install directories or system registration.
- Test work commits the reproduction and the matching test. Product fixes go back to the owning module. Avoid a single unbounded "fix all bugs" branch.

## Verification commands

```powershell
pnpm --filter @verisilo/desktop check
pnpm --filter @verisilo/desktop test
node --test scripts/dev-desktop.test.mjs
cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib application::
cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
```

The above is the check/test set for ordinary development and Pre-RC. It does not include an explicit desktop production build. Only after the user explicitly opens the
RC/release gate does Mode C run package/installer/install acceptance per the [release process](release.md).
For other logic changes, run the matching module tests. Conclusions about native Windows browsers, packages, Profiles, networks, or installation continue to follow the
[program status](program-status.md) and the owning acceptance. This structural adjustment does not update any product Gate.

This round does not split `launcher.rs` / `engine.rs` / `vault.rs` into further crates, and does not change the Host protocol or the engine patches.
They can already be worked on separately from the UI and the desktop business entry points. If concrete file contention appears later, extract internal modules along the owning seam.
