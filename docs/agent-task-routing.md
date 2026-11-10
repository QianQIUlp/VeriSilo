# Agent task routing and automated workflow

Users describe a task in natural language only (for example "redesign the UX of creating a Silo", "fix the Camoufox launch exception", "check whether the install flow has bugs").
The agent does the rest: decide the lane → create a task worktree → edit within the boundary → lane-level verification → scope check → hand off to integration.
The user is never asked to specify a worktree, Vault, port, or the directories that may be edited.

The source of truth is the `LANES` / `RESTRICTED` / `SHARED` configuration at the top of [scripts/agent-task.mjs](../scripts/agent-task.mjs). This document explains the workflow. A configuration change must edit the script and update this document in step.

## Standard workflow

```bash
# 1) Decide the lane (see the table below) and create the task worktree in the primary checkout.
#    start runs fetch/prune on origin and requires local baseline/dev to equal origin/baseline/dev exactly.
#    It does not depend on which branch the current shell is on.
node scripts/agent-task.mjs start --lane ui --task "Redesign the Create Silo UX" --name create-silo-ux
#    Prints JSON: branch / worktree directory / vault / port / baseline, plus the next-step commands

# 2) Enter the task worktree and install dependencies
cd .verisilo-worktrees/<dir>
pnpm install

# 3) As the lane requires, start a preview or a real instance with the exact command start printed (it already includes its own Vault and port)

# 4) Work, and commit in small steps

# 5) Before finishing, both of these must pass
node scripts/agent-task.mjs verify   # minimal sufficient lane verification (integration defaults to the lightweight tier, --full runs the whole matrix); exit 1 = failed
node scripts/agent-task.mjs check    # exit 0 = passed · exit 2 = scope violation · exit 3 = WORKSPACE CONTAMINATION

# 6) Commit, run verify + check again on the final HEAD, then publish. publish allows only a non-forced fast-forward.
git add -A && git commit -m "..."
node scripts/agent-task.mjs verify
node scripts/agent-task.mjs check
node scripts/agent-task.mjs publish

# 7a) A single-lane task whose canonical baseline has not advanced can be promoted directly (see "Three verification tiers")
node scripts/agent-task.mjs promote

# 7b) Otherwise hand it to integration: find tasks with list; the remote branch is the auditable input
node scripts/agent-task.mjs list
```

`check` and `verify` must run inside the task worktree. If the current git toplevel does not match the directory that holds the task metadata, they fail immediately
and never continue silently in the wrong directory. `start` is idempotent: the same lane + the same task description reuses the existing branch and worktree
(if the port is busy it is reassigned automatically). But if the canonical baseline has advanced, the old task's metadata no longer matches the new baseline, and
start refuses to reuse it and explains the difference. To continue the old task, enter its worktree. To start a new task, change the description or use `--name`.
The task text hash goes into the branch name (`agent/<lane>/<slug>-<hash6>`). For a Chinese task, pass `--name <short-English-slug>` to get a readable branch name.

To abandon or clean up a one-off task: `git worktree remove --force .verisilo-worktrees/<dir> && git branch -D agent/<lane>/<branch>`.
On Windows, the node_modules junction can leave the directory behind. In that case run `rm -rf .verisilo-worktrees/<dir>`
(the global pnpm store uses hard links with independent link counts, so deleting a task worktree does not affect it).

## Choosing a lane

Lane = responsibility and modification boundary. Task (worktree) = one actual piece of work. **One lane can have any number of task worktrees at the same time** without conflict. Do not assume a lane has only one branch.

| Lane          | Where the main output lands                                                                 | Example tasks                                                                                  | Signal not to choose it                                                                                               |
| ------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `ui`          | Desktop UI, forms, interactions, copy, styles, previews; the product website `apps/site/**` | Redesign the UX of creating a Silo; add a preview scenario; update the website release surface | Needs a backend DTO or business behavior change; a site dependency change would touch the restricted `pnpm-lock.yaml` |
| `core`        | Desktop business layer, domain model, Tauri/CLI entry points, EngineAdapter integration     | Fix the Vault write order when creating a Silo; fix a CLI output problem                       | Only front-end presentation changes, or the defect is in the Python Host                                              |
| `host`        | Python Host, Camoufox patches, engine package build scripts                                 | Fix a Camoufox launch exception (located in host_v1); update the patch series                  | The defect ends up in the Rust launcher/adapter. That is `core`                                                       |
| `qa`          | Reproduction steps, acceptance tests, evidence (no product code changes)                    | Check whether the install flow has bugs; regress a candidate                                   | A fix is already identified → the fix task goes back to the owning lane                                               |
| `integration` | Cross-layer work, shared contracts, merging development state                               | Add a field to NetworkProfile across front end and back end; merge this batch of tasks for QA  | It can be done within a single layer → use that lane                                                                  |

Rules for choosing:

1. Pick the lane by the **main change or output**, not by "who reported the problem". For a bug found by QA, the fix task belongs to the owning lane.
2. Pick the lane whose allow scope the expected change will land in. Choose `integration` only when the work is clearly cross-layer.
3. When unsure: do read-only investigation first (no `start`), and `start` once you have located the main layer to change. Or `start` on your current best judgment, and `check` at the end will tell you whether you went out of scope. Revert or escalate. Do not force the change in.
4. Do not put a task in the wrong lane just to "get it running". To change lanes, return to the primary checkout, run `start` again, and move the finished work over.
5. **Default task splitting** (avoid mechanical splitting):
   - A fix the user has authorized is done as one task in the owning lane through `diagnose → minimal fix → focused validation` (an implementation-authorized task). Do not split into `QA reproduction → fix task` by default. Use a separate QA task only when the user explicitly asks for independent QA, ownership is still unknown and the investigation itself is valuable, or independent acceptance is needed.
   - A coherent product slice that naturally spans Host/Core/UI/contracts is implemented directly as one integration task, and the baseline is advanced when it is done. Do not split into `integration implementation → second integration merge task`.
   - Parallel tasks accumulate into one natural integration batch by default. Each task is not required to advance the baseline as soon as it finishes.

## Canonical baseline (B0 → B1)

All tasks fork from the synchronized remote canonical baseline. The baseline never implicitly equals "the HEAD of the branch the
shell happened to be on when start ran":

```text
origin/baseline/dev = B0
baseline/dev = B0 (local work ref; must equal the line above exactly)
├─ agent/ui/...      (worktree, forked from B0)
├─ agent/core/...    (worktree, forked from B0)
├─ agent/qa/...      (worktree, forked from B0)
└─ integration merges and verifies
   ↓ explicit advance (the only way to advance)
baseline/dev = origin/baseline/dev = B1   new tasks afterward all start from B1
```

- The canonical identity is the remote ref `origin/baseline/dev`. The local `baseline/dev` is a work ref and must equal it exactly in the normal state.
- Both can be resolved with `git rev-parse` and audited through the Git reflog and remote history. There is no database, daemon, or registry.
- A new task can be created only after fetch/prune, from a baseline where the two are exactly equal. When the remote ref is missing or local and remote differ, `start` fails closed and does not start from an arbitrary shell HEAD.
- **The baseline advances only explicitly.** Integration uses `baseline advance <sha|ref>` + `baseline publish`. An eligible single-lane task uses `promote` (see "Three verification tiers"). Publishing allows only a fast-forward or the first bootstrap. Force is forbidden.
- After a normal task finishes verify + check + commit, run `node scripts/agent-task.mjs publish`. After fetch, the script checks that `origin/agent/...` and the local SHA are equal. If the remote cannot fast-forward, it reports `REMOTE_DIVERGENCE` and does not overwrite.
- `codex/camoufox-m3-engine-adapter` is kept as the stable mainline but is no longer the canonical development source for new tasks. `stable/checkpoint-*` and historical refs do not take part in daily routing. Do not run `git push --all` / `--mirror`.
- To see the current target and sync state: `node scripts/agent-task.mjs baseline`.

## Modification boundaries (scope guard and contamination guard)

`check` reports two different kinds of problem. Do not confuse them:

1. **Scope violation (exit 2)**: the change is inside the task worktree but crosses the lane boundary:
   - `RESTRICTED`: no lane (except `integration`) may change it, even if it falls inside the lane's own allow list. This includes `packages/contracts/**`, `apps/desktop/src/desktop-api.ts`, the root `package.json`/`pnpm-lock.yaml`, `AGENTS.md`, top-level `docs/*.md`, `.github/**`, and the routing script itself.
   - `SHARED`: rejected by default, but a lane allow entry can override it (the host build/verify scripts, and qa's `docs/qa/**` and `docs/acceptance/**`).
   - `check` classifies every file in `baseline..HEAD` plus uncommitted and untracked files. On exit 2, pick one of the two options. Do not accept silently:
     1. **A drive-by change** → `git restore --source=<baseline> --staged --worktree -- <file>` (delete untracked files directly);
     2. **The task is genuinely cross-layer** → run `start --lane integration` in the primary checkout to split out an explicit cross-layer task. Do not mix it into this lane's branch.
2. **WORKSPACE CONTAMINATION (exit 3)**: the task wrote changes into the **primary checkout** (a filesystem-level scope violation, for example writing into the primary checkout from the worktree through a relative `../../` path). `start` snapshots the dirty state of the primary checkout, and `check` compares `git status --porcelain -uall` with that snapshot:
   - Only **new** entries are reported. Dirty state that existed before the task started is not attributed to the current task (known limitation: further content changes to an entry that already existed cannot be told apart, and are not falsely reported).
   - When contamination is found, **do not delete or overwrite automatically**. If an out-of-scope write belongs to this task, revert the file in the primary checkout. If it is a legitimate change by the user or another task, report it as it is and leave it alone.

**Shared contracts are handled explicitly.** A change to `packages/contracts`, `desktop-api.ts`, a Rust DTO, the Host protocol, or a data format must first be committed in small steps by an `integration` contract task. Caller tasks then follow and adapt. No lane may define the same field on its own.

## Minimum sufficient verification (lane level)

The commands are those in the script's `LANES[...].verify` (the `verify` subcommand runs them automatically):

| Lane          | Automated verification                                                                                                                                                                                                                                                                                                                                                              | Agent supplementary duty                                                                                                                                           |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ui`          | desktop check + desktop test                                                                                                                                                                                                                                                                                                                                                        | Manually check the affected interactions with preview scenarios. Preview is UI evidence, not a runtime Gate                                                        |
| `core`        | desktop cargo check + harness `application::` tests                                                                                                                                                                                                                                                                                                                                 | When the change touches window/tray/process paths, add focused tests in the owning module                                                                          |
| `host`        | package contract + page command tests                                                                                                                                                                                                                                                                                                                                               | `test_identity_artifact.py` needs numpy (run it when available). Conclusions about the engine, package, or fingerprint must come from real runtime evidence        |
| `qa`          | (no automated commands)                                                                                                                                                                                                                                                                                                                                                             | Verification = evidence: reproduction steps + actual observation + the exact candidate version targeted. Fixes go back to the owning lane                          |
| `integration` | Default lightweight tier (merge/conflict/ancestry/scope/`git diff --check` + focused tests). With `verify --full`: recursive check/test, site build, fmt check of three Rust crates, cargo check/test of two crates, Host tests, script self-tests (Pre-RC does not include a desktop production build; live tests that depend on the user's local provider inventory are excluded) | After full automation, real installation and user-journey acceptance still run per the acceptance flow, in a dedicated environment, against a determined candidate |

Do not let "a configuration declaration / passing tests / a successful compile" stand in for a runtime/product Gate not yet obtained. Lane verification covers only the scope its name refers to.

## Three verification tiers: direct promote / lightweight integration / full integration

**Evidence remains valid until a relevant input changes.** Host runtime evidence is not invalidated by an unrelated
site change. Site build evidence is not invalidated by an unrelated Rust change. A core lifecycle test is not invalidated automatically by an
unrelated Host source change. Verification targets only the "genuinely new uncertainty". It does not re-prove facts that have not changed.

### A. Direct promotion (an eligible single task skips integration)

When a single-lane task meets all of the following, `node scripts/agent-task.mjs promote` advances the canonical
baseline directly, with no new integration task:

1. The task was created from the current canonical baseline, and after fetch the local and remote baselines still equal the starting baseline in the task metadata
   exactly. Otherwise it returns `PROMOTION_REQUIRES_INTEGRATION`. Do not merge/rebase automatically;
2. The task branch is a strict descendant of the starting baseline;
3. The lane's `verify` has run successfully at the current HEAD (recorded in `validated.verify` in `.agent-task.json`);
4. The worktree is clean. The scope guard is re-run in-process at promote time, and RESTRICTED / SHARED / out-of-scope changes are always rejected;
5. The task branch has been `publish`ed and the remote SHA equals the local one.

Order: commit → verify → check → publish → promote. promote fast-forwards the local `baseline/dev`
(if the primary checkout has that ref checked out, it fast-forwards that checkout in step), publishes `origin/baseline/dev` without force, and checks that the
local / remote / task SHAs all agree. A task in the integration lane does not use promote, because it is itself the integration. It uses
`baseline advance` + `baseline publish`.

### B. Lightweight integration (default)

When merging several verified task branches, integration verifies lightly by default:

- Check the remote input SHAs, check ancestry / intended commits, and detect merges and conflicts;
- Check scope / the final diff (`check`) + `git diff --check` (built into `verify`);
- Run focused tests directly related to the uncertainty this composition genuinely adds.

`node scripts/agent-task.mjs verify` runs the lightweight tier by default in the integration lane. **Do not rerun the full matrix by default.**
When the merge has no conflicts, the input tasks have each passed lane verify/check, and the merged files and interfaces have no new cross-impact, run
`baseline advance` + `baseline publish` directly. Existing task validation can be reused.

### C. Full integration (explicit escalation)

`node scripts/agent-task.mjs verify --full` keeps the full matrix (recursive check/test, site build,
fmt check of three Rust crates, cargo check/test of two crates, Host, workflow tests;
no desktop production build). Triggers:

- A shared protocol / DTO / contracts change;
- A dependency or lockfile change;
- A workflow / config change;
- Several tasks touch the same owning seam;
- A merge conflict needs manual resolution;
- Cross-layer behavior whose correctness exists only after composition;
- A formal QA / RC / release candidate;
- Contradictory or failed evidence;
- The user explicitly asks for a full regression.

## Run levels (Mode A / B / C)

Beyond lane-level verify, when running a verification instance use "the lowest-cost run level sufficient to verify the current change"
(the full definition of the three tiers is in the "Three-tier development loop" section of [development-worktrees.md](development-worktrees.md)):

- **Mode A: UI Preview**: `node scripts/dev-desktop.mjs <name> --port <port> --preview`.
  For pure UI/UX changes (styles, layout, components, copy, form UX, loading/error/empty/running states).
  Real components + mock API + Vite HMR. It does not start the Rust backend and does not read a real Vault.
- **Mode B: Desktop Dev**: `node scripts/dev-desktop.mjs core --port <port> --vault <vault>`.
  Use it when the change touches a Tauri command, application, Vault/runtime, or front-end/back-end integration. Front-end HMR, and an automatic restart after
  incremental Rust compilation. It does not build an installer.
- **Mode C: RC / Release**: enter only when installer/install behavior changes, production packaging behavior changes, or for release acceptance,
  and only when the user has explicitly opened the RC/release gate. It follows the dedicated-environment acceptance flow and is not mixed with development instances.

Pure front-end lanes such as `ui` / `qa` default to Mode A. `core` / `host` and cross-layer tasks default to Mode B.
Do not build or install a release artifact for a change that Preview or Tauri dev can cover.

## CLI-first and the RC user gate

- For Vault, Silo, lifecycle, Managed, Network, persistence, and recovery behavior that can be honestly verified with the CLI + a real backend,
  prefer the CLI. This is not a second desktop/browser automation.
- When the CLI cannot express a behavior, record the coverage boundary. Do not automatically mark the whole QA round BLOCKED, and do not extend the CLI for every
  UI/browser scenario. For visual, interaction, copy, or state checks, use Vite HMR/Preview. Mock/Preview
  evidence cannot stand in for runtime evidence.
- Enter Desktop Dev only when neither the CLI nor HMR can verify honestly and a real Tauri/runtime integration is truly needed. Even then, do not
  install, package, or build a release.
- An agent must not start packaging, RC, installer, or clean Windows acceptance on its own because "there seem to be few bugs", the code builds, or static checks pass.
  The user must explicitly open the RC/release gate with a new instruction. The Pre-RC QA
  convergence signal is used only for the release-readiness judgment. It is not an automatic trigger.

## Integration workflow

0. An eligible single-lane task prefers direct promote (see "Three verification tiers") and does not enter this flow. An integration task is needed only when several tasks,
   a shared contract, or an escalation condition applies.
1. After each task worktree finishes verify + check, it commits and publishes its own `agent/<lane>/...` branch with `node scripts/agent-task.mjs publish`.
   The input must be fetchable from origin. An isolated local commit cannot take its place.
2. `start --lane integration --task "..."` creates the integration worktree. After fetch, integration merges only remote task SHAs that it has checked.
3. Run `git merge --no-ff origin/agent/<lane>/<branch>` one by one. Resolve conflicts on the principle of the owning lane's owning code.
   A contract conflict goes back to an explicit contract task.
4. **Lightweight verification** is the default: remote input SHA / ancestry / conflicts / scope / `git diff --check` + focused tests directly related to this
   composition (`verify` runs the lightweight tier by default in the integration lane). Run `verify --full` only when a
   full integration escalation condition applies. At either tier, no explicit desktop production build is run.
5. After verification passes, integration runs `baseline advance <integration result SHA>`, then `baseline publish`. After fetch it must
   show that `baseline/dev == origin/baseline/dev`. Publishing allows only a fast-forward or the first bootstrap. Force is forbidden.
6. The current product phase, release status and next task follow the [program status page](program-status.md). The `v0.1.0-rc5`
   public prerelease has been published and the release gate is closed.
   A new candidate build or release acceptance runs only after the user explicitly opens the RC/release gate and the release-readiness conditions are met.
   Install / upgrade-install / user-journey acceptance runs in a dedicated environment and is not mixed with development instances.

## Run isolation and shared resources

| Resource                             | Rule                                                                                                                                      |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Vault                                | One per task (start generates it and validates ≤32 lowercase characters). Never use `default`. Pass it to the dev instance with `--vault` |
| Port                                 | One Vite port per task (from 15400; start probes for a free one and skips ports other tasks have claimed)                                 |
| node_modules / Rust target / staging | Each worktree owns its own. Do not link across worktrees                                                                                  |
| Mihomo                               | The same controller/selector group switches nodes for everyone: use a separate instance for parallel network tests, or run them serially  |
| engine package                       | A fixed package is reused read-only. A new candidate writes to a new directory and does not overwrite a package someone else is accepting |
| Install/uninstall                    | Run by an acceptance task in a dedicated environment. A worktree name does not isolate install directories or system registration         |

## Environment notes

- Node ≥22 (local fnm: `$APPDATA/fnm/node-versions/v22.23.2/installation`). pnpm 11 can be called through `corepack pnpm`. `verify` probes between `pnpm` and `corepack pnpm` automatically. In Git Bash the pnpm shim may not work because of path rewriting. This is a known situation.
- Host `test_identity_artifact.py` depends on numpy in the local python, which is missing in the current environment (non-blocking, run conditionally).
- Known occasional flake: in the full desktop Rust lib test run, the fake Host one-second handshake times out. Rerun the failing item alone and serially to confirm before drawing a conclusion, and record it as it is.

For the underlying conventions (code ownership table, shared baseline principle, merge rules), see [development-worktrees.md](development-worktrees.md).
