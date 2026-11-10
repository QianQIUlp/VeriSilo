#!/usr/bin/env node
// Agent task routing: lane rules plus deterministic task worktree bootstrap.
//
// The LANES config below is the single source of truth for lane scope and
// verification; docs/agent-task-routing.md explains the workflow and refers
// here. Subcommands:
//
//   node scripts/agent-task.mjs start    --lane <lane> --task "<task>" [--name <slug>]
//   node scripts/agent-task.mjs verify   [--lane <lane>] [--full]
//   node scripts/agent-task.mjs check    [--lane <lane>]
//   node scripts/agent-task.mjs list
//   node scripts/agent-task.mjs publish
//   node scripts/agent-task.mjs promote
//   node scripts/agent-task.mjs baseline [advance <sha|ref>|publish] [--force]
//
// `verify` and `check` read .agent-task.json in the current task worktree;
// pass --lane to run them against the current checkout without metadata.
// Verification tiers: the integration lane defaults to lightweight
// merge-level guards (`verifyLight`) and runs the full matrix only with
// `verify --full`; other lanes always run their own (already minimal) set.
// A successful verify records `validated.verify` in .agent-task.json;
// `promote` requires that record at the exact HEAD being promoted.
// Exit codes: 0 ok · 1 verify failure · 2 lane scope violation ·
// 3 WORKSPACE CONTAMINATION (new changes in the primary checkout).
//
// Tasks always fork from the synchronized canonical baseline
// `origin/baseline/dev`; the local `baseline/dev` ref moves only through
// explicit actions: `promote` (eligible single task) or `baseline advance`
// (integration).

import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

export const WORKTREE_ROOT_NAME = ".verisilo-worktrees";
export const META_FILE = ".agent-task.json";
export const BRANCH_PREFIX = "agent";
export const BASELINE_REF = "baseline/dev";
export const REMOTE_BASELINE_REF = "origin/baseline/dev";
const PORT_RANGE_START = 15400;
const PORT_RANGE_SIZE = 512;
const PORT_SCAN_LIMIT = 64;

// Lane = responsibility + default modification boundary. A lane may host any
// number of concurrent task worktrees; the worktree ("task") is created per
// job, never per lane.
export const LANES = {
  ui: {
    label: "UI / UX",
    hint: "Pages, forms, interactions, previews, and the front-end presentation layer (including the product website apps/site)",
    allow: [
      "apps/desktop/src/features/**",
      "apps/desktop/src/shared/**",
      "apps/desktop/src/workspace/**",
      "apps/desktop/src/preview/**",
      "apps/desktop/src/*.ts",
      "apps/desktop/src/*.test.ts",
      "apps/desktop/src/*.test.tsx",
      "apps/desktop/src/App.tsx",
      "apps/desktop/src/main.tsx",
      "apps/desktop/src/styles.css",
      "apps/desktop/preview.html",
      "apps/desktop/index.html",
      "apps/site/**",
    ],
    verify: [
      "python scripts/check_managed_contracts.py",
      "pnpm --filter @verisilo/desktop check",
      "pnpm --filter @verisilo/desktop test",
    ],
    verifyExtra:
      "Check the affected interactions in preview scenarios (node scripts/dev-desktop.mjs ui --port <port> --preview). Preview is UI evidence, not a desktop runtime Gate. When apps/site changes, the standard is pnpm site:check + site:build and a local Preview.",
  },
  core: {
    label: "Core application/business logic",
    hint: "Desktop business layer, domain model, Tauri/CLI entry points, EngineAdapter integration",
    allow: [
      "apps/desktop/src-tauri/src/**",
      "apps/desktop/src-tauri/Cargo.toml",
      "apps/desktop/src-tauri/Cargo.lock",
      "crates/verisilo-desktop-core-harness/**",
    ],
    verify: [
      "python scripts/check_managed_contracts.py",
      "cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml",
      "cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib application::",
    ],
    verifyExtra:
      "When the change touches window/tray/process paths, add focused tests in the owning module. Business conclusions rest on core harness tests; a successful compile does not stand in for them.",
  },
  host: {
    label: "Host / Engine",
    hint: "Python Host, Camoufox patches, engine package build scripts",
    allow: [
      "apps/camoufox-host/**",
      "tests/fixtures/camoufox/**",
      "tests/fingerprint-probe/**",
      "scripts/*camoufox*",
      "scripts/*engine*",
      "scripts/*managed-browser*",
    ],
    verify: [
      "python scripts/check_managed_contracts.py",
      "python apps/camoufox-host/test_package_contract.py",
      "python apps/camoufox-host/test_page_command.py",
    ],
    verifyExtra:
      "test_identity_artifact.py needs numpy in the local python; run it as well when that is available. Conclusions about the engine, package, or fingerprint must come from real runtime evidence. Static checks and unit tests cannot stand in for it.",
  },
  qa: {
    label: "QA / acceptance",
    hint: "Defect reproduction, acceptance tests, evidence package",
    allow: ["tests/**", "docs/qa/**", "docs/acceptance/**"],
    verify: [],
    verifyExtra:
      "Verification = evidence: reproducible steps + the actual observed result + the exact candidate version targeted. Product fixes must go back to a separate task in the owning lane. Do not fix them in passing on the same branch.",
  },
  integration: {
    label: "Integration / cross-cutting",
    hint: "Merging task branches, shared contract changes, cross-layer tasks, RC candidates",
    allow: ["**"],
    // verifyLight: what the integration lane runs by default — lightweight
    // merge-level guards (built-in `git diff --check` over baseline..HEAD and
    // the worktree) plus agent-selected focused tests. `verify --full` runs
    // `verify`, the complete matrix below.
    verifyLight: [],
    verify: [
      "python scripts/check_managed_contracts.py",
      "pnpm check",
      "pnpm test",
      "pnpm site:build",
      "cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check",
      "cargo fmt --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml -- --check",
      "cargo fmt --manifest-path crates/verisilo-remote-backend/Cargo.toml -- --check",
      "cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml",
      "cargo test --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --lib -- --skip probe_and_inspect_use_verge_pipe_when_http_controller_is_closed --skip runtime_guard_accepts_live_verge_rule_mode_selector --skip live_verge_runs_two_isolated_silos_without_changing_main_clash",
      "cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib -- --skip probe_and_inspect_use_verge_pipe_when_http_controller_is_closed --skip runtime_guard_accepts_live_verge_rule_mode_selector --skip live_verge_runs_two_isolated_silos_without_changing_main_clash",
      "python apps/camoufox-host/test_package_contract.py",
      "python apps/camoufox-host/test_page_command.py",
      "node --test scripts/agent-task.test.mjs",
    ],
    verifyExtra:
      "Default is the lightweight tier: merge/conflict/ancestry/scope/diff guards plus focused tests directly related to the new uncertainty introduced by this composition. Do not rerun the full matrix by default. When each input task has passed verify/check and the merge adds no new cross-impact, you may advance/publish directly. Escalate to --full when any of these applies: shared contract/DTO; dependencies or lockfile; workflow/config; several tasks touch the same owning seam; conflicts were resolved by hand; cross-layer behavior that exists only after composition; a formal QA/RC/release candidate; contradictory or failed evidence; the user explicitly asks for a full regression. After full automation passes, real installation and user-journey acceptance still run per the acceptance flow, in a dedicated environment, against a determined candidate, and are not mixed with development instances. Known flake: in the full desktop lib test run, the fake Host handshake occasionally times out; rerun it alone and serially to confirm, then record it.",
  },
};

// RESTRICTED: no lane (except integration) may modify these, even if a lane
// allowlist covers them. They are shared contracts or workflow-wide truth.
export const RESTRICTED = [
  {
    glob: "packages/contracts/**",
    reason:
      "Shared DTO/protocol contract: needs an explicit contract task with callers updated in step; a single lane must not change it on its own",
  },
  {
    glob: "apps/desktop/src/desktop-api.ts",
    reason: "Front-end/back-end contract seam: goes through an explicit contract task together with packages/contracts",
  },
  { glob: "package.json", reason: "Root workspace configuration" },
  { glob: "pnpm-lock.yaml", reason: "Root dependency lockfile" },
  { glob: "pnpm-workspace.yaml", reason: "Workspace definition" },
  { glob: "AGENTS.md", reason: "Agent collaboration docs" },
  {
    glob: "docs/*.md",
    reason: "Top-level docs (status page / north star / workflow) are a cross-cutting source of truth",
  },
  { glob: ".github/**", reason: "CI/workflow configuration" },
  { glob: ".gitignore", reason: "Repository-wide ignore rules" },
  { glob: ".prettierignore", reason: "Repository-wide formatting rules" },
  {
    glob: "scripts/agent-task.mjs",
    reason: "The task routing script itself (source of truth for lanes)",
  },
  { glob: "scripts/agent-task.test.mjs", reason: "Tests for the task routing script" },
];

// SHARED: cross-cutting by default, but a lane allowlist entry may claim it.
export const SHARED = [
  {
    glob: "scripts/**",
    reason: "Root scripts are cross-cutting by default (except the build/verify scripts of the host lane)",
  },
  { glob: "docs/**", reason: "Docs are cross-cutting by default (except qa/acceptance evidence)" },
  { glob: "apps/desktop/src-tauri/tauri.conf.json", reason: "Desktop shell/packaging configuration" },
  { glob: "apps/desktop/vite.config.ts", reason: "Shared dev server configuration" },
];

const LANE_IDS = Object.keys(LANES);
const VAULT_NAME_RE = /^[a-z0-9][a-z0-9_-]{0,31}$/;

export function globToRegExp(pattern) {
  let source = "";
  for (let i = 0; i < pattern.length; i += 1) {
    const char = pattern[i];
    if (char === "*") {
      if (pattern[i + 1] === "*") {
        i += 1;
        if (pattern[i + 1] === "/") {
          i += 1;
          source += "(?:.*/)?";
        } else {
          source += ".*";
        }
      } else {
        source += "[^/]*";
      }
    } else if (char === "?") {
      source += "[^/]";
    } else {
      source += char.replace(/[.+^${}()|[\]\\]/g, "\\$&");
    }
  }
  return new RegExp(`^${source}$`);
}

function matchesAny(patterns, path) {
  for (const pattern of patterns) {
    const glob = typeof pattern === "string" ? pattern : pattern.glob;
    if (glob.includes("*") || glob.includes("?")) {
      if (globToRegExp(glob).test(path)) return true;
    } else if (path === glob || path.startsWith(`${glob}/`)) {
      return true;
    }
  }
  return false;
}

// Classification order: integration allows all, lane deny, RESTRICTED,
// lane allow, SHARED, out of scope.
export function classifyPath(path, laneId) {
  const normalized = path.replaceAll("\\", "/").replace(/^\.\//, "");
  const lane = LANES[laneId];
  if (!lane) throw new Error(`Unknown lane: ${laneId}`);
  if (laneId === "integration") return { status: "ok", kind: "integration" };
  if (lane.deny && matchesAny(lane.deny, normalized)) {
    return {
      status: "violation",
      kind: "deny",
      reason: "This path is explicitly excluded from the lane",
    };
  }
  for (const rule of RESTRICTED) {
    if (globToRegExp(rule.glob).test(normalized) || normalized === rule.glob) {
      return { status: "violation", kind: "restricted", reason: rule.reason };
    }
  }
  if (matchesAny(lane.allow, normalized))
    return { status: "ok", kind: "allow" };
  for (const rule of SHARED) {
    if (matchesAny([rule.glob], normalized)) {
      return { status: "violation", kind: "shared", reason: rule.reason };
    }
  }
  const owners = LANE_IDS.filter(
    (id) => id !== laneId && matchesAny(LANES[id].allow, normalized),
  );
  return {
    status: "violation",
    kind: "out-of-scope",
    reason: owners.length
      ? `In the default scope of lane ${owners.join("/")}`
      : "Not in the default scope of any lane",
  };
}

export function slugify(text, max = 24) {
  const words = String(text)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  let out = "";
  for (const word of words) {
    const candidate = out ? `${out}-${word}` : word;
    if (candidate.length > max) break;
    out = candidate;
  }
  if (!out) out = "task";
  return out;
}

export function taskHash(text) {
  return createHash("sha256")
    .update(String(text), "utf8")
    .digest("hex")
    .slice(0, 6);
}

export function taskNames(laneId, taskText, nameOverride) {
  const slug = nameOverride ? slugify(nameOverride, 24) : slugify(taskText, 24);
  const hash = taskHash(taskText);
  const branch = `${BRANCH_PREFIX}/${laneId}/${slug}-${hash}`;
  const dir = `${laneId}-${slug}-${hash}`;
  let vault = `${laneId}-${slug}-${hash}`;
  if (vault.length > 32)
    vault = `${laneId}-${slug.slice(0, 24 - laneId.length)}-${hash}`;
  if (!VAULT_NAME_RE.test(vault))
    throw new Error(`Generated vault name is invalid: ${vault}`);
  return { slug, hash, branch, dir, vault };
}

export function portFromHash(hash) {
  return PORT_RANGE_START + (parseInt(hash.slice(0, 4), 16) % PORT_RANGE_SIZE);
}

function git(args, cwd) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(
      `git ${args.join(" ")} failed:\n${result.stderr || result.stdout}`,
    );
  }
  return result.stdout.trim();
}

function gitOk(args, cwd) {
  return spawnSync("git", args, { cwd, encoding: "utf8" }).status === 0;
}

// The local work ref is useful for branch operations, but remote/baseline is
// the canonical source of truth. Callers that create tasks must use the
// synchronized pair returned by synchronizedBaseline().
function readBaseline(root) {
  const result = spawnSync(
    "git",
    ["rev-parse", "--verify", "--quiet", `${BASELINE_REF}^{commit}`],
    { cwd: root, encoding: "utf8" },
  );
  return result.status === 0 ? result.stdout.trim() : null;
}

function readRef(root, ref) {
  const result = spawnSync(
    "git",
    ["rev-parse", "--verify", "--quiet", `${ref}^{commit}`],
    { cwd: root, encoding: "utf8" },
  );
  return result.status === 0 ? result.stdout.trim() : null;
}

function fetchOrigin(root) {
  git(["fetch", "--prune", "origin"], root);
}

function synchronizedBaseline(root, { allowMissingRemote = false } = {}) {
  fetchOrigin(root);
  const local = readBaseline(root);
  const remote = readRef(root, REMOTE_BASELINE_REF);
  if (!local) {
    throw new Error(
      `local canonical work ref ${BASELINE_REF} does not exist; refusing to continue (fail closed when the baseline is uncertain).`,
    );
  }
  if (!remote) {
    if (allowMissingRemote) return { local, remote: null };
    throw new Error(
      `remote canonical baseline ${REMOTE_BASELINE_REF} does not exist; refusing to create a task. Integration must complete the baseline bootstrap first.`,
    );
  }
  if (local !== remote) {
    throw new Error(
      `BASELINE_DIVERGENCE: local ${BASELINE_REF}=${local} != ${REMOTE_BASELINE_REF}=${remote}; refusing to start from an unsynchronized baseline.`,
    );
  }
  return { local, remote };
}

function remoteBranchRef(branch) {
  return `refs/remotes/origin/${branch}`;
}

function assertClean(root, subject) {
  // Raw output on purpose: the git() helper trims stdout, which would strip
  // the leading status column of unstaged entries (" M path" → "M path") and
  // break the META_FILE exclusion below.
  const { stdout } = spawnSync("git", ["status", "--porcelain", "-uall"], {
    cwd: root,
    encoding: "utf8",
  });
  const dirty = (stdout ?? "")
    .split("\n")
    .filter((line) => line.slice(3).trim() !== META_FILE)
    .filter((line) => line.length > 0)
    .join("\n");
  if (dirty) {
    throw new Error(
      `${subject} requires a clean worktree; finish verify/check, commit, and then publish.\n${dirty}`,
    );
  }
}

function assertRemoteFastForward(root, branch, local) {
  const remote = readRef(root, remoteBranchRef(branch));
  if (
    remote &&
    remote !== local &&
    !gitOk(["merge-base", "--is-ancestor", remote, local], root)
  ) {
    throw new Error(
      `REMOTE_DIVERGENCE: origin/${branch}=${remote} cannot fast-forward to local=${local}; refusing to overwrite the remote.`,
    );
  }
  return remote;
}

// The worktree directory that has `branch` checked out, if any. `git branch -f`
// refuses a branch that is checked out in any worktree.
function worktreeHoldingBranch(root, branch) {
  const list = git(["worktree", "list", "--porcelain"], root);
  let current = null;
  for (const line of list.split("\n")) {
    if (line.startsWith("worktree ")) {
      current = line.slice("worktree ".length);
    } else if (line.startsWith("branch ")) {
      const name = line.slice("branch ".length).replace("refs/heads/", "");
      if (name === branch && current) return current;
    }
  }
  return null;
}

// Move local `baseline/dev` to an already-verified fast-forward target. When a
// worktree (typically the primary checkout) holds the baseline branch checked
// out, fast-forward that worktree instead so the ref and its files stay in
// sync; a conflicting dirty state there fails closed. Never creates a commit.
function moveLocalBaseline(root, next) {
  const holder = worktreeHoldingBranch(root, BASELINE_REF);
  if (holder) git(["merge", "--ff-only", next], holder);
  else git(["branch", "-f", BASELINE_REF, next], root);
}

function repoInfo(cwd) {
  const root = git(["rev-parse", "--show-toplevel"], cwd);
  const gitDir = resolve(git(["rev-parse", "--absolute-git-dir"], cwd));
  const commonDir = resolve(root, git(["rev-parse", "--git-common-dir"], cwd));
  return {
    root,
    gitDir,
    commonDir,
    isPrimary: gitDir === commonDir,
    head: git(["rev-parse", "HEAD"], cwd),
    branch: git(["rev-parse", "--abbrev-ref", "HEAD"], cwd),
    dirty: git(["status", "--porcelain"], root).split("\n").filter(Boolean)
      .length,
  };
}

function findMeta(startDir) {
  let dir = resolve(startDir);
  for (;;) {
    const candidate = join(dir, META_FILE);
    if (existsSync(candidate)) return candidate;
    const parent = resolve(dir, "..");
    if (parent === dir) return null;
    dir = parent;
  }
}

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function existingMetas(root) {
  const dir = join(root, WORKTREE_ROOT_NAME);
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => join(dir, entry.name, META_FILE))
    .filter((file) => existsSync(file))
    .map((file) => ({ file, meta: readJson(file) }));
}

function isPortFree(port) {
  return new Promise((done) => {
    const server = createServer();
    server.once("error", () => done(false));
    server.once("listening", () => server.close(() => done(true)));
    server.listen(port, "127.0.0.1");
  });
}

export async function pickPort(base, claimed) {
  for (let offset = 0; offset < PORT_SCAN_LIMIT; offset += 1) {
    const port =
      PORT_RANGE_START + ((base - PORT_RANGE_START + offset) % PORT_RANGE_SIZE);
    if (claimed.has(port)) continue;
    if (await isPortFree(port)) return port;
  }
  throw new Error("No free port available in the agent task range.");
}

function assertPrimary(info) {
  if (!info.isPrimary) {
    throw new Error(
      "start must run in the primary checkout; task worktrees must fork from the shared baseline.\nYou are in a linked worktree. Return to the primary checkout directory and run it again.",
    );
  }
}

async function cmdStart({ lane, task, name }) {
  if (!LANES[lane]) {
    throw new Error(
      `Unknown lane: ${lane}. Known lanes: ${LANE_IDS.join(", ")}`,
    );
  }
  if (!task || !task.trim()) throw new Error('--task "<task description>" is required');
  const info = repoInfo(process.cwd());
  assertPrimary(info);

  // Preflight: fetch the remote canonical source and require the local work
  // ref to be its exact synchronized copy. Never fork from shell HEAD.
  const { local: baselineSha } = synchronizedBaseline(info.root);

  const taskText = task.trim();
  const names = taskNames(lane, taskText, name);
  const { branch, dir, vault } = names;
  const worktreePath = join(info.root, WORKTREE_ROOT_NAME, dir);
  const metaPath = join(worktreePath, META_FILE);

  // Preflight: an existing worktree path must belong to exactly this task.
  if (existsSync(worktreePath)) {
    if (!existsSync(metaPath)) {
      throw new Error(
        `The worktree path already exists but has no task metadata; refusing to reuse or overwrite it:\n  ${worktreePath}\nIf you confirm it is a leftover directory, delete it manually and run start again.`,
      );
    }
    const meta = readJson(metaPath);
    const mismatches = [];
    if (meta.lane !== lane) mismatches.push(`lane: ${meta.lane} != ${lane}`);
    if (meta.task !== taskText)
      mismatches.push(
        `task: ${JSON.stringify(meta.task)} != ${JSON.stringify(taskText)}`,
      );
    if (meta.branch !== branch)
      mismatches.push(`branch: ${meta.branch} != ${branch}`);
    if (meta.baseline !== baselineSha)
      mismatches.push(
        `baseline: task was created at ${meta.baseline.slice(0, 12)}, the canonical baseline is now ${baselineSha.slice(0, 12)} (advanced by integration?)`,
      );
    if (mismatches.length > 0) {
      throw new Error(
        `A task worktree with the same name already exists, but its metadata does not match the current request; refusing to guess or overwrite:\n  ${mismatches.join("\n  ")}\nTo continue the old task, enter its worktree. To start a new task, change the task description or --name.`,
      );
    }
    if (!(await isPortFree(meta.port))) {
      const claimed = new Set(
        existingMetas(info.root)
          .map(({ meta: other }) => other.port)
          .filter(Boolean),
      );
      meta.port = await pickPort(portFromHash(taskHash(meta.task)), claimed);
      writeFileSync(metaPath, `${JSON.stringify(meta, null, 2)}\n`);
      console.log(`Previous port is busy; reassigned to ${meta.port}.`);
    }
    console.log(`Task worktree already exists; resuming:`);
    printMeta(meta, info.root);
    return;
  }

  // Preflight: an agent branch without its worktree is a leftover, not a
  // candidate for silent reuse.
  if (gitOk(["rev-parse", "--verify", "--quiet", branch], info.root)) {
    throw new Error(
      `Branch ${branch} already exists but has no matching task worktree; refusing to reuse it.\nIf you confirm it is a leftover branch: git branch -D ${branch}`,
    );
  }

  if (info.dirty > 0) {
    console.warn(
      `⚠ The primary checkout has ${info.dirty} uncommitted change(s); they are not in the canonical baseline and will not appear in the new worktree.`,
    );
  }

  const claimedPorts = new Set(
    existingMetas(info.root)
      .map(({ meta }) => meta.port)
      .filter(Boolean),
  );
  const port = await pickPort(portFromHash(names.hash), claimedPorts);
  git(["worktree", "add", "-b", branch, worktreePath, baselineSha], info.root);

  const meta = {
    version: 2,
    task: taskText,
    lane,
    branch,
    baseline: baselineSha,
    baselineRef: REMOTE_BASELINE_REF,
    worktree: relative(info.root, worktreePath).replaceAll("\\", "/"),
    vault,
    port,
    createdAt: new Date().toISOString(),
    // Snapshot of the primary checkout at task start; `check` compares this
    // against the live state to detect filesystem-level contamination
    // (e.g. a task writing through `../../` into the primary checkout).
    primary: {
      root: info.root,
      // -uall so untracked files are enumerated individually instead of
      // collapsed into directories; snapshot and check must agree on format.
      dirtySnapshot: git(["status", "--porcelain", "-uall"], info.root)
        .split("\n")
        .filter(Boolean),
    },
  };
  writeFileSync(metaPath, `${JSON.stringify(meta, null, 2)}\n`);
  console.log("Created task worktree from canonical baseline:");
  printMeta(meta, info.root);
}

function printMeta(meta, root) {
  console.log(JSON.stringify(meta, null, 2));
  const worktreeAbs = join(root, meta.worktree ?? "");
  const script = "scripts/agent-task.mjs";
  console.log(`
Next steps:
  cd ${worktreeAbs}
  pnpm install
${devHint(meta)}
  node ${script} verify    # lane minimal sufficient verification (integration defaults to the lightweight tier, --full runs the whole matrix; exit 1 = failed)
  node ${script} check     # scope guard (exit 2 = out of scope) + primary checkout contamination guard (exit 3 = contamination)
  # When done: git add -A && git commit → node ${script} verify → node ${script} check → node ${script} publish
  # For a single-lane task whose baseline has not advanced, after publish you can run node ${script} promote to advance the canonical baseline directly
`);
}

function devHint(meta) {
  const dev = `node scripts/dev-desktop.mjs`;
  switch (meta.lane) {
    case "ui":
      return `  # UI preview (mock data, no kernel started): ${dev} ui --port ${meta.port} --preview
  # Real desktop instance: ${dev} ui --port ${meta.port} --vault ${meta.vault}`;
    case "qa":
      return `  # When a real instance is needed to reproduce: ${dev} core --port ${meta.port} --vault ${meta.vault}`;
    case "host":
      return `  # Host tests do not need a desktop instance; when you do: ${dev} core --port ${meta.port} --vault ${meta.vault}`;
    default:
      return `  # Real desktop instance: ${dev} core --port ${meta.port} --vault ${meta.vault}`;
  }
}

function cmdPublish() {
  const { meta, root } = resolveTaskContext({});
  if (!meta) {
    throw new Error("publish must run inside a task worktree that has .agent-task.json. ");
  }
  const info = repoInfo(root);
  if (info.branch !== meta.branch) {
    throw new Error(
      `The current branch ${info.branch} does not match the task branch ${meta.branch}; refusing to publish.`,
    );
  }
  assertClean(root, "task publish");
  const local = info.head;
  fetchOrigin(root);
  assertRemoteFastForward(root, meta.branch, local);
  git(
    [
      "push",
      "origin",
      `refs/heads/${meta.branch}:refs/heads/${meta.branch}`,
    ],
    root,
  );
  fetchOrigin(root);
  const remote = readRef(root, remoteBranchRef(meta.branch));
  if (remote !== local) {
    throw new Error(
      `Remote SHA mismatch after publish: local=${local}, origin/${meta.branch}=${remote ?? "missing"}.`,
    );
  }
  console.log(`Published ${meta.branch}: ${local} = origin/${meta.branch}. ✓`);
}

// promote: an eligible single task advances the canonical baseline without a
// separate integration task. Every precondition fails closed; once the
// baseline has moved on, the only answer is PROMOTION_REQUIRES_INTEGRATION.
function cmdPromote() {
  const { meta, root } = resolveTaskContext({});
  if (!meta) {
    throw new Error(
      "promote must run inside a task worktree that has .agent-task.json.",
    );
  }
  if (meta.lane === "integration") {
    throw new Error(
      "An integration task is itself the integration: use baseline advance <sha> + baseline publish; promote is for single-lane tasks.",
    );
  }
  const info = repoInfo(root);
  if (info.branch !== meta.branch) {
    throw new Error(
      `The current branch ${info.branch} does not match the task branch ${meta.branch}; refusing to promote.`,
    );
  }
  assertClean(root, "task promote");
  const head = info.head;

  // 1) Fetch and fail closed unless the canonical baseline is still exactly
  //    the one this task forked from (no automatic merge/rebase, ever).
  fetchOrigin(root);
  const localBaseline = readBaseline(root);
  const remoteBaseline = readRef(root, REMOTE_BASELINE_REF);
  if (localBaseline !== meta.baseline || remoteBaseline !== meta.baseline) {
    throw new Error(
      `PROMOTION_REQUIRES_INTEGRATION: the canonical baseline has moved away from the task start ` +
        `${meta.baseline.slice(0, 12)} (local=${localBaseline?.slice(0, 12) ?? "missing"}, ` +
        `remote=${remoteBaseline?.slice(0, 12) ?? "missing"}). Do not merge/rebase automatically; ` +
        `hand the published task branch to integration for a lightweight merge.`,
    );
  }

  // 2) The task branch must be a strict descendant of its start baseline.
  if (
    head === meta.baseline ||
    !gitOk(["merge-base", "--is-ancestor", meta.baseline, head], root)
  ) {
    throw new Error("task HEAD is not a strict descendant of its start baseline; refusing to promote.");
  }

  // 3) Lane verify must have succeeded at this exact HEAD (recorded by
  //    `verify` in .agent-task.json). This is what lets promote reuse the
  //    task's own validation instead of re-running it.
  const verified = meta.validated?.verify;
  if (verified?.head !== head) {
    throw new Error(
      `promote requires this task to have run lane verify successfully at the current HEAD (${head.slice(0, 12)}) ` +
        `(.agent-task.json has no matching validated.verify record). Order: commit → verify → publish → promote.`,
    );
  }

  // 4) Cheap guards re-run fresh in-process: scope (RESTRICTED / shared
  //    contracts / out-of-scope) and primary-checkout contamination.
  const violations = changedFiles(root, meta.baseline).filter(
    (file) => classifyPath(file, meta.lane).status !== "ok",
  );
  if (violations.length > 0) {
    throw new Error(
      `promote refused: the task contains ${violations.length} out-of-scope or restricted change(s) (RESTRICTED / shared / out of scope):\n` +
        `${violations.map((file) => `  ${file}`).join("\n")}\n` +
        `Cross-layer and shared-contract changes must go through integration.`,
    );
  }
  const contamination = contaminationFindings(meta);
  if (contamination && contamination.length > 0) {
    throw new Error(
      `WORKSPACE CONTAMINATION: new changes appeared in the primary checkout since the task started; refusing to promote.\n` +
        `${contamination.join("\n")}`,
    );
  }

  // 5) The task branch must already be published with remote == local so the
  //    promoted baseline is fully auditable from origin.
  const remoteTask = readRef(root, remoteBranchRef(meta.branch));
  if (remoteTask !== head) {
    throw new Error(
      `promote requires the task branch to be published with the remote SHA equal to local (origin/${meta.branch}=${remoteTask?.slice(0, 12) ?? "missing"}, local=${head.slice(0, 12)}); run publish first.`,
    );
  }

  // 6) Fast-forward the local work ref (handles a primary checkout holding
  //    baseline/dev), then publish it non-forced.
  moveLocalBaseline(root, head);
  cmdBaselinePublish({ root });

  // 7) Three-way equality: local baseline, remote baseline, task branch.
  fetchOrigin(root);
  const published = readRef(root, REMOTE_BASELINE_REF);
  const local = readBaseline(root);
  if (published !== head || local !== head) {
    throw new Error(
      `Three-way SHA mismatch after promote: task=${head.slice(0, 12)}, local baseline=${local?.slice(0, 12) ?? "missing"}, ${REMOTE_BASELINE_REF}=${published?.slice(0, 12) ?? "missing"}.`,
    );
  }
  console.log(
    `Promoted ${meta.branch} → ${BASELINE_REF}: ${head.slice(0, 12)} = ${REMOTE_BASELINE_REF}. ✓`,
  );
}

function resolveTaskContext({ lane }) {
  const metaFile = findMeta(process.cwd());
  if (metaFile) {
    const meta = readJson(metaFile);
    const root = resolve(metaFile, "..");
    // Fail fast when task commands run outside the task worktree they
    // describe; silently continuing from the wrong directory is how
    // contamination and wrong-root diffs used to slip through.
    const toplevel = resolve(
      git(["rev-parse", "--show-toplevel"], process.cwd()),
    );
    if (toplevel !== root) {
      throw new Error(
        `Task commands must run inside the task worktree: the current git toplevel (${toplevel}) does not match the directory that holds the task metadata (${root}).`,
      );
    }
    return { meta, root, lane: lane ?? meta.lane };
  }
  if (!lane) {
    throw new Error(
      `${META_FILE} not found. Run inside a task worktree, or pass --lane <lane> explicitly (runs against the current checkout).`,
    );
  }
  if (!LANES[lane])
    throw new Error(
      `Unknown lane: ${lane}. Known lanes: ${LANE_IDS.join(", ")}`,
    );
  const root = git(["rev-parse", "--show-toplevel"], process.cwd());
  return { meta: null, root, lane };
}

function changedFiles(root, baseline) {
  const files = new Set();
  const add = (out) =>
    out
      .split("\n")
      .filter(Boolean)
      .forEach((f) => files.add(f.trim()));
  if (baseline) add(git(["diff", "--name-only", `${baseline}..HEAD`], root));
  add(git(["diff", "--name-only", "HEAD"], root));
  add(git(["ls-files", "--others", "--exclude-standard"], root));
  // The guard never reports its own bookkeeping as a task change.
  files.delete(META_FILE);
  return [...files];
}

// Compare the primary checkout's live dirty state against the snapshot taken
// at task start. Line-level comparison: pre-existing dirty entries are never
// attributed to the task; only newly appearing entries are contamination.
function contaminationFindings(meta) {
  if (!meta?.primary?.root) return null;
  const snapshot = new Set(meta.primary.dirtySnapshot ?? []);
  return git(["status", "--porcelain", "-uall"], meta.primary.root)
    .split("\n")
    .filter(Boolean)
    .filter((line) => !snapshot.has(line));
}

async function cmdCheck({ lane }) {
  const { meta, root, lane: laneId } = resolveTaskContext({ lane });
  const files = changedFiles(root, meta?.baseline);
  const violations = [];
  const inScope = [];
  for (const file of files) {
    const verdict = classifyPath(file, laneId);
    if (verdict.status === "ok") inScope.push(file);
    else violations.push({ file, ...verdict });
  }
  console.log(
    `Scope check — lane ${laneId} (${LANES[laneId].label})${meta ? `, task: ${meta.task}` : ""}`,
  );
  console.log(`  in scope: ${inScope.length} file(s)`);
  if (violations.length === 0) {
    console.log("  no boundary violations. ✓");
  } else {
    console.log(`  OUT OF SCOPE: ${violations.length} file(s):`);
    for (const { file, kind, reason } of violations) {
      console.log(`    ${file}`);
      console.log(`      → ${kind}: ${reason}`);
    }
  }

  const contamination = meta ? contaminationFindings(meta) : null;
  if (contamination === null) {
    console.log(
      "\nWorkspace contamination check: skipped (metadata has no primary checkout snapshot, v1 task).",
    );
  } else if (contamination.length === 0) {
    console.log(
      "\nWorkspace contamination check: no new changes in the primary checkout since the task started. ✓",
    );
  }

  if (violations.length > 0) {
    const baselineRef = meta?.baseline ?? "HEAD";
    console.log(`
Do not silently accept out-of-scope changes. Pick one:
  1) A drive-by change → revert it:
       git restore --source=${baselineRef} --staged --worktree -- <file>
       (delete untracked files directly)
  2) The task is genuinely cross-layer → escalate: split the cross-layer part into an explicit integration task (start --lane integration in the primary checkout). Do not mix it into this lane's branch.`);
    process.exitCode = 2;
  }

  if (contamination && contamination.length > 0) {
    console.log(`
WORKSPACE CONTAMINATION
  The primary checkout (${meta.primary.root}) has new changes relative to the task start snapshot:
${contamination.map((line) => `    ${line}`).join("\n")}
  This is filesystem-level contamination (for example, writing into the primary checkout from the worktree through ../../). It is a different kind of problem from lane scope.
  Do not delete or overwrite automatically:
    - Out-of-scope writes that belong to this task → revert the files in the primary checkout (git restore -- <file>; delete untracked files)
    - Legitimate changes by the user or another task → report them as they are and leave them alone`);
    process.exitCode = 3;
  }
}

function cmdBaselinePublish(info) {
  fetchOrigin(info.root);
  const local = readBaseline(info.root);
  if (!local) {
    throw new Error(
      `local canonical work ref ${BASELINE_REF} does not exist; cannot publish the baseline.`,
    );
  }
  const remote = assertRemoteFastForward(info.root, "baseline/dev", local);
  git(
    [
      "push",
      "origin",
      `refs/heads/${BASELINE_REF}:refs/heads/${BASELINE_REF}`,
    ],
    info.root,
  );
  fetchOrigin(info.root);
  const published = readRef(info.root, REMOTE_BASELINE_REF);
  if (published !== local) {
    throw new Error(
      `SHA mismatch after baseline publish: local=${local}, ${REMOTE_BASELINE_REF}=${published ?? "missing"}.`,
    );
  }
  console.log(
    `${BASELINE_REF} published: ${local} = ${REMOTE_BASELINE_REF}.` +
      (remote ? " fast-forward ✓" : " bootstrap ✓"),
  );
}

// baseline [advance <sha|ref>|publish] [--force]: the canonical development /
// integration baseline only moves through this explicit action, never as a
// side effect of someone committing on a feature branch.
function cmdBaseline(args, { force }) {
  const info = repoInfo(process.cwd());
  assertPrimary(info);
  const [action, target] = args;
  if (action === "publish") {
    cmdBaselinePublish(info);
    return;
  }
  if (!action) {
    const { local: current, remote } = synchronizedBaseline(info.root, {
      allowMissingRemote: true,
    });
    if (!current) {
      console.log(
        `${BASELINE_REF} does not exist. Integration creates it explicitly: git branch ${BASELINE_REF} <verified baseline SHA>`,
      );
      process.exitCode = 1;
      return;
    }
    const subject = git(["log", "-1", "--format=%s", current], info.root);
    console.log(`${BASELINE_REF} → ${current}  ${subject}`);
    console.log(
      `${REMOTE_BASELINE_REF} → ${remote ?? "missing (bootstrap pending)"}`,
    );
    return;
  }
  if (action !== "advance") {
    throw new Error(
      `Unknown baseline action: ${action} (usage: baseline [advance <sha|ref>|publish] [--force])`,
    );
  }
  if (!target) {
    throw new Error("baseline advance requires <sha|ref>.");
  }
  const { local: current, remote } = synchronizedBaseline(info.root, {
    // The one-time migration bootstrap may advance the already-validated
    // local baseline before origin/baseline/dev exists. `start` never permits
    // this exception; the next explicit `baseline publish` closes it.
    allowMissingRemote: true,
  });
  if (!remote) {
    console.warn(
      `⚠ ${REMOTE_BASELINE_REF} does not exist; allowing this integration bootstrap. You must run baseline publish afterwards.`,
    );
  }
  const resolved = spawnSync(
    "git",
    ["rev-parse", "--verify", "--quiet", `${target}^{commit}`],
    { cwd: info.root, encoding: "utf8" },
  );
  if (resolved.status !== 0) {
    throw new Error(`Cannot resolve the baseline target: ${target}`);
  }
  const next = resolved.stdout.trim();
  if (!current) {
    git(["branch", BASELINE_REF, next], info.root);
    console.log(`${BASELINE_REF} created at ${next}`);
    return;
  }
  if (next === current) {
    console.log(
      `${BASELINE_REF} already at ${current.slice(0, 12)}; unchanged.`,
    );
    return;
  }
  const descendant = gitOk(
    ["merge-base", "--is-ancestor", current, next],
    info.root,
  );
  if (!descendant && !force) {
    throw new Error(
      `The target ${next.slice(0, 12)} is not a descendant of the current baseline ${current.slice(0, 12)} (rollback or fork).\nTo roll back deliberately, confirm explicitly with --force.`,
    );
  }
  moveLocalBaseline(info.root, next);
  console.log(
    `${BASELINE_REF}: ${current.slice(0, 12)} → ${next.slice(0, 12)}${descendant ? "" : " (forced)"}`,
  );
}

function resolvePnpm() {
  for (const candidate of ["pnpm", "corepack pnpm"]) {
    const probe = spawnSync(candidate, ["--version"], {
      shell: true,
      encoding: "utf8",
    });
    if (probe.status === 0) return candidate;
  }
  return null;
}

// Async command runner so independent toolchain groups can run in parallel
// while commands inside one group stay serial.
function runCommandAsync(command, cwd, pnpmCmd) {
  let cmd = command;
  if (pnpmCmd && cmd.startsWith("pnpm "))
    cmd = `${pnpmCmd} ${cmd.slice("pnpm ".length)}`;
  console.log(`\n▶ ${cmd}`);
  return new Promise((resolve) => {
    const child = spawn(cmd, { shell: true, stdio: "inherit", cwd });
    child.on("exit", (code) => resolve(code === 0));
    child.on("error", () => resolve(false));
  });
}

// `git diff --check` exits non-zero when it finds problems (leftover conflict
// markers, whitespace errors); the report goes to stdout.
function gitDiffCheckIssues(root, range) {
  const result = spawnSync("git", ["diff", "--check", range], {
    cwd: root,
    encoding: "utf8",
  });
  if (result.status === 0) return null;
  return (result.stdout || result.stderr).trim();
}

// A successful verify records the exact HEAD it validated, so `promote` can
// reuse the task's own validation instead of re-running it. Cheap guards
// (scope/contamination) are re-run fresh at promote time.
function recordVerified(root, meta, laneId) {
  if (!meta) return;
  meta.validated = {
    ...(meta.validated ?? {}),
    verify: {
      lane: laneId,
      head: git(["rev-parse", "HEAD"], root),
      at: new Date().toISOString(),
    },
  };
  writeFileSync(join(root, META_FILE), `${JSON.stringify(meta, null, 2)}\n`);
}

async function cmdVerify({ lane, full }) {
  const { meta, root, lane: laneId } = resolveTaskContext({ lane });
  const config = LANES[laneId];
  if (config.verify.length === 0) {
    console.log(`Lane ${laneId} has no automated verification commands. ${config.verifyExtra}`);
    recordVerified(root, meta, laneId);
    return;
  }
  // Integration defaults to the lightweight tier; --full opts into the whole
  // matrix. Lanes without verifyLight always run their (already minimal) set.
  const light = !full && Array.isArray(config.verifyLight);
  const commands = light ? config.verifyLight : config.verify;
  if (light) {
    console.log(
      `integration verify (lightweight): merge-level guards only; run focused tests directly related to the new uncertainty from this composition. ` +
        `Do not rerun the full matrix by default; run verify --full when a full regression is needed.`,
    );
    const issues = [
      gitDiffCheckIssues(
        root,
        meta?.baseline ? `${meta.baseline}..HEAD` : "HEAD",
      ),
      gitDiffCheckIssues(root, "HEAD"),
    ].filter(Boolean);
    if (issues.length > 0) {
      console.error(
        `\nverify FAILED (git diff --check):\n${issues.join("\n")}`,
      );
      process.exitCode = 1;
      return;
    }
  }
  let pnpmCmd = null;
  if (commands.length > 0) {
    pnpmCmd = resolvePnpm();
    if (!pnpmCmd) {
      throw new Error(
        "pnpm not found (tried pnpm and corepack pnpm). Run in an environment that has pnpm, or install corepack.",
      );
    }
  }
  // Independent toolchains (JS / Rust / Python / node scripts) run as
  // concurrent groups; commands within a group stay serial because they may
  // share state (cargo build lock, pnpm workspace store).
  const groups = new Map();
  for (const command of commands) {
    const key = command.startsWith("cargo ")
      ? "rust"
      : command.startsWith("python ")
        ? "python"
        : command.startsWith("node ")
          ? "node"
          : "js";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(command);
  }
  const runGroup = async (groupCommands) => {
    const failedHere = [];
    for (const command of groupCommands) {
      if (!(await runCommandAsync(command, root, pnpmCmd))) failedHere.push(command);
    }
    return failedHere;
  };
  const failed = (await Promise.all([...groups.values()].map(runGroup))).flat();
  if (failed.length > 0) {
    console.error(`\nverify FAILED (${failed.length}/${commands.length}):`);
    for (const command of failed) console.error(`  ✘ ${command}`);
    process.exitCode = 1;
    return;
  }
  console.log(
    `\nverify PASSED for lane ${laneId}${light ? " (lightweight)" : ""}. ✓`,
  );
  recordVerified(root, meta, laneId);
  console.log(`Additional requirement: ${config.verifyExtra}`);
}

function cmdList() {
  const info = repoInfo(process.cwd());
  const metas = existingMetas(info.root);
  const registered = new Set(
    git(["worktree", "list", "--porcelain"], info.root)
      .split("\n")
      .filter((line) => line.startsWith("worktree "))
      .map((line) => resolve(line.slice("worktree ".length))),
  );
  if (metas.length === 0) {
    console.log("No agent task worktrees.");
    return;
  }
  console.log("Active agent tasks:\n");
  for (const { meta } of metas) {
    const path = join(info.root, meta.worktree ?? "");
    const live = registered.has(path) ? "live" : "PRUNED";
    console.log(
      `[${meta.lane}] ${meta.branch}  (${live})\n  task:  ${meta.task}\n  vault: ${meta.vault}  port: ${meta.port}\n  path:  ${path}\n  base:  ${meta.baseline.slice(0, 12)} (${meta.baselineRef ?? "HEAD@start"})`,
    );
  }
}

async function main() {
  const scriptPath = fileURLToPath(import.meta.url);
  const { positionals, values } = parseArgs({
    args: process.argv.slice(2),
    allowPositionals: true,
    options: {
      lane: { type: "string" },
      task: { type: "string" },
      name: { type: "string" },
      force: { type: "boolean", default: false },
      full: { type: "boolean", default: false },
      help: { type: "boolean", default: false },
    },
  });
  const [command] = positionals;
  const taskText = values.task ?? positionals.slice(1).join(" ");
  if (values.help || !command) {
    console.log(`Usage:
  node scripts/agent-task.mjs start    --lane <${LANE_IDS.join("|")}> --task "<task description>" [--name <slug>]
  node scripts/agent-task.mjs verify   [--lane <lane>] [--full]   # lane verification; integration defaults to the lightweight tier, --full runs the whole matrix
  node scripts/agent-task.mjs check    [--lane <lane>]   # scope guard + contamination guard (run inside the task worktree)
  node scripts/agent-task.mjs list                       # list active agent tasks
  node scripts/agent-task.mjs publish                    # publish the current committed task branch and check the remote SHA
  node scripts/agent-task.mjs promote                    # an eligible single-lane task advances the canonical baseline directly (commit → verify → publish → promote)
  node scripts/agent-task.mjs baseline [advance <sha|ref>|publish] [--force]   # show/advance/publish the canonical baseline
Exit codes: 0 ok · 1 verify failed · 2 lane scope violation · 3 workspace contamination`);
    process.exitCode = command ? 0 : 1;
    return;
  }
  if (command === "start") {
    await cmdStart({ lane: values.lane, task: taskText, name: values.name });
    return;
  }
  if (command === "verify") {
    return cmdVerify({ lane: values.lane, full: values.full });
  }
  if (command === "check") return cmdCheck({ lane: values.lane });
  if (command === "list") return cmdList();
  if (command === "publish") return cmdPublish();
  if (command === "promote") return cmdPromote();
  if (command === "baseline") return cmdBaseline(positionals.slice(1), values);
  throw new Error(`Unknown command: ${command}`);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main();
}
