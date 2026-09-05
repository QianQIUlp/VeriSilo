#!/usr/bin/env node
// Agent task routing: lane rules plus deterministic task worktree bootstrap.
//
// The LANES config below is the single source of truth for lane scope and
// verification; docs/agent-task-routing.md explains the workflow and refers
// here. Subcommands:
//
//   node scripts/agent-task.mjs start    --lane <lane> --task "<task>" [--name <slug>]
//   node scripts/agent-task.mjs verify   [--lane <lane>]
//   node scripts/agent-task.mjs check    [--lane <lane>]
//   node scripts/agent-task.mjs list
//   node scripts/agent-task.mjs baseline [advance <sha|ref>] [--force]
//
// `verify` and `check` read .agent-task.json in the current task worktree;
// pass --lane to run them against the current checkout without metadata.
// Exit codes: 0 ok · 1 verify failure · 2 lane scope violation ·
// 3 WORKSPACE CONTAMINATION (new changes in the primary checkout).
//
// Tasks always fork from the canonical baseline ref `baseline/dev`; only an
// explicit integration action may move that ref.

import { spawnSync } from "node:child_process";
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
const PORT_RANGE_START = 15400;
const PORT_RANGE_SIZE = 512;
const PORT_SCAN_LIMIT = 64;

// Lane = responsibility + default modification boundary. A lane may host any
// number of concurrent task worktrees; the worktree ("task") is created per
// job, never per lane.
export const LANES = {
  ui: {
    label: "UI / UX",
    hint: "页面、表单、交互、预览与前端表现层",
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
    ],
    verify: [
      "pnpm --filter @verisilo/desktop check",
      "pnpm --filter @verisilo/desktop test",
    ],
    verifyExtra:
      "用 preview 场景核对受影响交互（node scripts/dev-desktop.mjs ui --port <port> --preview）；preview 是 UI 证据，不是桌面 runtime Gate。",
  },
  core: {
    label: "Core application/business logic",
    hint: "桌面业务层、领域模型、Tauri/CLI 入口、EngineAdapter 接入",
    allow: [
      "apps/desktop/src-tauri/src/**",
      "apps/desktop/src-tauri/Cargo.toml",
      "apps/desktop/src-tauri/Cargo.lock",
      "crates/verisilo-desktop-core-harness/**",
    ],
    verify: [
      "cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml",
      "cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib application::",
    ],
    verifyExtra:
      "触到窗口/托盘/进程路径时补 owning module focused tests；业务结论以 core harness 测试为准，不以编译成功冒充。",
  },
  host: {
    label: "Host / Engine",
    hint: "Python Host、Camoufox 补丁、engine package 构建脚本",
    allow: [
      "apps/camoufox-host/**",
      "tests/fixtures/camoufox/**",
      "tests/fingerprint-probe/**",
      "scripts/*camoufox*",
      "scripts/*engine*",
      "scripts/*managed-browser*",
    ],
    verify: [
      "python apps/camoufox-host/test_package_contract.py",
      "python apps/camoufox-host/test_page_command.py",
    ],
    verifyExtra:
      "test_identity_artifact.py 需要本机 python 可用 numpy，有条件则一并运行。内核/包/指纹结论必须来自真实 runtime evidence，静态检查与单测不能冒充。",
  },
  qa: {
    label: "QA / acceptance",
    hint: "缺陷复现、验收测试、evidence package",
    allow: ["tests/**", "docs/qa/**", "docs/acceptance/**"],
    verify: [],
    verifyExtra:
      "验证=证据：可复现步骤 + 实际观察结果 + 所针对的确切候选版本。产品修复必须回到 owning lane 的独立任务，不要在同一分支里顺手修。",
  },
  integration: {
    label: "Integration / cross-cutting",
    hint: "汇总任务分支、共享契约变更、跨层任务、RC 候选",
    allow: ["**"],
    verify: [
      "pnpm check",
      "pnpm test",
      "pnpm --filter @verisilo/desktop build",
      "cargo check --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml",
      "cargo test --offline --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --lib",
      "cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib",
      "python apps/camoufox-host/test_package_contract.py",
      "python apps/camoufox-host/test_page_command.py",
      "node --test scripts/dev-desktop.test.mjs",
      "node --test scripts/agent-task.test.mjs",
    ],
    verifyExtra:
      "完整自动化通过后，真实安装与用户旅程验收仍按 acceptance 流程在专用环境对确定候选执行，不与开发实例混用。已知 flake：desktop lib 全测中 fake Host 握手超时偶发，单独串行重跑确认后记录。",
  },
};

// RESTRICTED: no lane (except integration) may modify these, even if a lane
// allowlist covers them. They are shared contracts or workflow-wide truth.
export const RESTRICTED = [
  {
    glob: "packages/contracts/**",
    reason:
      "共享 DTO/协议契约：需要显式契约任务，调用方同步更新，不能由单个 lane 私改",
  },
  {
    glob: "apps/desktop/src/desktop-api.ts",
    reason: "前端/后端契约接缝：随 packages/contracts 一起走显式契约任务",
  },
  { glob: "package.json", reason: "根工作区配置" },
  { glob: "pnpm-lock.yaml", reason: "根依赖锁" },
  { glob: "pnpm-workspace.yaml", reason: "工作区定义" },
  { glob: "AGENTS.md", reason: "Agent 协作文档" },
  {
    glob: "docs/*.md",
    reason: "顶层文档（状态页/北极星/工作流）属于跨面事实源",
  },
  { glob: ".github/**", reason: "CI/工作流配置" },
  { glob: ".gitignore", reason: "仓库级忽略规则" },
  { glob: ".prettierignore", reason: "仓库级格式规则" },
  {
    glob: "scripts/agent-task.mjs",
    reason: "任务路由脚本本身（lanes 事实源）",
  },
  { glob: "scripts/agent-task.test.mjs", reason: "任务路由脚本测试" },
];

// SHARED: cross-cutting by default, but a lane allowlist entry may claim it.
export const SHARED = [
  {
    glob: "scripts/**",
    reason: "根脚本默认跨面（host lane 的构建/验证脚本除外）",
  },
  { glob: "docs/**", reason: "文档默认跨面（qa/acceptance evidence 除外）" },
  { glob: "apps/desktop/src-tauri/tauri.conf.json", reason: "桌面壳/打包配置" },
  { glob: "apps/desktop/vite.config.ts", reason: "共享开发服务器配置" },
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
      reason: "此路径被该 lane 显式排除",
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
      ? `属于 lane ${owners.join("/")} 的默认范围`
      : "不属于任何 lane 的默认范围",
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

// The canonical development/integration baseline; tasks fork from this ref,
// never from whatever HEAD the calling shell happens to be on.
function readBaseline(root) {
  const result = spawnSync(
    "git",
    ["rev-parse", "--verify", "--quiet", `${BASELINE_REF}^{commit}`],
    { cwd: root, encoding: "utf8" },
  );
  return result.status === 0 ? result.stdout.trim() : null;
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
      "start 必须在主检出（primary checkout）运行，任务 worktree 应从共享 baseline 分叉。\n当前位于链接工作树，请回到主检出目录再执行。",
    );
  }
}

async function cmdStart({ lane, task, name }) {
  if (!LANES[lane]) {
    throw new Error(
      `Unknown lane: ${lane}. Known lanes: ${LANE_IDS.join(", ")}`,
    );
  }
  if (!task || !task.trim()) throw new Error('需要 --task "<任务描述>"');
  const info = repoInfo(process.cwd());
  assertPrimary(info);

  // Preflight: the canonical baseline must resolve; tasks never fork from
  // whatever HEAD the calling shell happens to be on.
  const baselineSha = readBaseline(info.root);
  if (!baselineSha) {
    throw new Error(
      `canonical baseline ref ${BASELINE_REF} 不存在，拒绝创建任务（baseline 不确定时 fail fast）。\n` +
        `由 integration 显式建立一次：git branch ${BASELINE_REF} <已验证的基线 SHA>`,
    );
  }

  const taskText = task.trim();
  const names = taskNames(lane, taskText, name);
  const { branch, dir, vault } = names;
  const worktreePath = join(info.root, WORKTREE_ROOT_NAME, dir);
  const metaPath = join(worktreePath, META_FILE);

  // Preflight: an existing worktree path must belong to exactly this task.
  if (existsSync(worktreePath)) {
    if (!existsSync(metaPath)) {
      throw new Error(
        `worktree 路径已存在但没有任务元数据，拒绝复用或覆盖：\n  ${worktreePath}\n确认是残留目录后手动删除，再重新 start。`,
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
        `baseline: task 创建于 ${meta.baseline.slice(0, 12)}，canonical baseline 现在是 ${baselineSha.slice(0, 12)}（已被 integration 推进？）`,
      );
    if (mismatches.length > 0) {
      throw new Error(
        `已存在同名 task worktree，但元数据与当前请求不一致，拒绝猜测或覆盖：\n  ${mismatches.join("\n  ")}\n继续旧任务请直接进入 worktree；开新任务请更换任务描述或 --name。`,
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
      `分支 ${branch} 已存在但没有对应的任务 worktree，拒绝复用。\n确认是残留分支后：git branch -D ${branch}`,
    );
  }

  if (info.dirty > 0) {
    console.warn(
      `⚠ 主检出有 ${info.dirty} 个未提交变更；它们不在 canonical baseline 中，也不会出现在新 worktree 里。`,
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
    baselineRef: BASELINE_REF,
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
  // Always route task commands through the primary checkout's copy of this
  // script: worktree copies only update when the baseline advances.
  const script = join(root, "scripts", "agent-task.mjs").replaceAll("\\", "/");
  console.log(`
Next steps:
  cd ${worktreeAbs}
  pnpm install
${devHint(meta)}
  node ${script} verify    # lane 最小充分验证（exit 1=失败）
  node ${script} check     # scope guard（exit 2=越界）+ 主检出污染守卫（exit 3=contamination）
  # 完成后提交：git add -A && git commit；integration agent 用 list 发现并合并 agent/* 分支
`);
}

function devHint(meta) {
  const dev = `node scripts/dev-desktop.mjs`;
  switch (meta.lane) {
    case "ui":
      return `  # UI 预览（模拟数据，不启动内核）：${dev} ui --port ${meta.port} --preview
  # 真实桌面实例：${dev} ui --port ${meta.port} --vault ${meta.vault}`;
    case "qa":
      return `  # 需要真实实例复现时：${dev} core --port ${meta.port} --vault ${meta.vault}`;
    case "host":
      return `  # Host 测试不需要桌面实例；需要时：${dev} core --port ${meta.port} --vault ${meta.vault}`;
    default:
      return `  # 真实桌面实例：${dev} core --port ${meta.port} --vault ${meta.vault}`;
  }
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
        `任务命令必须在任务 worktree 内运行：当前 git toplevel（${toplevel}）与任务元数据所在目录（${root}）不一致。`,
      );
    }
    return { meta, root, lane: lane ?? meta.lane };
  }
  if (!lane) {
    throw new Error(
      `未找到 ${META_FILE}。请在任务 worktree 内运行，或用 --lane <lane> 显式指定（对当前检出执行）。`,
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
      "\nWorkspace contamination check: skipped（metadata 无主检出快照，v1 任务）.",
    );
  } else if (contamination.length === 0) {
    console.log(
      "\nWorkspace contamination check: 主检出自任务启动以来无新增修改. ✓",
    );
  }

  if (violations.length > 0) {
    const baselineRef = meta?.baseline ?? "HEAD";
    console.log(`
不要静默接受越界修改。二选一：
  1) 顺手修改 → 回退：
       git restore --source=${baselineRef} --staged --worktree -- <file>
       （未跟踪文件直接删除）
  2) 任务天然跨层 → 升级：把跨层部分拆成显式 integration 任务（在主检出 start --lane integration），不要在本 lane 分支里混入。`);
    process.exitCode = 2;
  }

  if (contamination && contamination.length > 0) {
    console.log(`
WORKSPACE CONTAMINATION
  主检出（${meta.primary.root}）相对任务启动快照出现新增修改：
${contamination.map((line) => `    ${line}`).join("\n")}
  这是 filesystem 级污染（例如从 worktree 用 ../../ 写进主检出），与 lane scope 是两类问题。
  不要自动删除或覆盖：
    - 属于本任务的越界写入 → 在主检出回退对应文件（git restore -- <file>；未跟踪文件删除）
    - 用户或其他任务的合法修改 → 如实报告，不要动它`);
    process.exitCode = 3;
  }
}

// baseline [advance <sha|ref>] [--force]: the canonical development /
// integration baseline only moves through this explicit action, never as a
// side effect of someone committing on a feature branch.
function cmdBaseline(args, { force }) {
  const info = repoInfo(process.cwd());
  assertPrimary(info);
  const current = readBaseline(info.root);
  const [action, target] = args;
  if (!action) {
    if (!current) {
      console.log(
        `${BASELINE_REF} 不存在。由 integration 显式建立：git branch ${BASELINE_REF} <已验证的基线 SHA>`,
      );
      process.exitCode = 1;
      return;
    }
    const subject = git(["log", "-1", "--format=%s", current], info.root);
    console.log(`${BASELINE_REF} → ${current}  ${subject}`);
    return;
  }
  if (action !== "advance") {
    throw new Error(
      `Unknown baseline action: ${action}（用法：baseline [advance <sha|ref>] [--force]）`,
    );
  }
  if (!target) {
    throw new Error("baseline advance 需要 <sha|ref>。");
  }
  const resolved = spawnSync(
    "git",
    ["rev-parse", "--verify", "--quiet", `${target}^{commit}`],
    { cwd: info.root, encoding: "utf8" },
  );
  if (resolved.status !== 0) {
    throw new Error(`无法解析 baseline 目标：${target}`);
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
      `目标 ${next.slice(0, 12)} 不是当前 baseline ${current.slice(0, 12)} 的后代（回退或分叉）。\n确需回退请加 --force 显式确认。`,
    );
  }
  git(["branch", "-f", BASELINE_REF, next], info.root);
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

function runCommand(command, cwd, pnpmCmd) {
  let cmd = command;
  if (pnpmCmd && cmd.startsWith("pnpm "))
    cmd = `${pnpmCmd} ${cmd.slice("pnpm ".length)}`;
  console.log(`\n▶ ${cmd}`);
  const result = spawnSync(cmd, { shell: true, stdio: "inherit", cwd });
  return result.status === 0;
}

async function cmdVerify({ lane }) {
  const { meta, root, lane: laneId } = resolveTaskContext({ lane });
  const config = LANES[laneId];
  if (config.verify.length === 0) {
    console.log(`Lane ${laneId} 没有自动化验证命令。${config.verifyExtra}`);
    return;
  }
  const pnpmCmd = resolvePnpm();
  if (!pnpmCmd) {
    throw new Error(
      "找不到 pnpm（尝试了 pnpm 与 corepack pnpm）。请在有 pnpm 的环境运行，或安装 corepack。",
    );
  }
  const failed = [];
  for (const command of config.verify) {
    if (!runCommand(command, root, pnpmCmd)) failed.push(command);
  }
  if (failed.length > 0) {
    console.error(
      `\nverify FAILED (${failed.length}/${config.verify.length}):`,
    );
    for (const command of failed) console.error(`  ✘ ${command}`);
    process.exitCode = 1;
    return;
  }
  console.log(`\nverify PASSED for lane ${laneId}. ✓`);
  console.log(`补充要求：${config.verifyExtra}`);
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
      help: { type: "boolean", default: false },
    },
  });
  const [command] = positionals;
  const taskText = values.task ?? positionals.slice(1).join(" ");
  if (values.help || !command) {
    console.log(`Usage:
  node scripts/agent-task.mjs start    --lane <${LANE_IDS.join("|")}> --task "<任务描述>" [--name <slug>]
  node scripts/agent-task.mjs verify   [--lane <lane>]   # lane 最小充分验证（在任务 worktree 内运行）
  node scripts/agent-task.mjs check    [--lane <lane>]   # scope guard + 污染守卫（在任务 worktree 内运行）
  node scripts/agent-task.mjs list                       # 列出活跃 agent 任务
  node scripts/agent-task.mjs baseline [advance <sha|ref>] [--force]   # 查看/显式推进 canonical baseline
Exit codes: 0 ok · 1 verify 失败 · 2 lane scope 越界 · 3 workspace contamination`);
    process.exitCode = command ? 0 : 1;
    return;
  }
  if (command === "start") {
    await cmdStart({ lane: values.lane, task: taskText, name: values.name });
    return;
  }
  if (command === "verify") return cmdVerify({ lane: values.lane });
  if (command === "check") return cmdCheck({ lane: values.lane });
  if (command === "list") return cmdList();
  if (command === "baseline") return cmdBaseline(positionals.slice(1), values);
  throw new Error(`Unknown command: ${command}`);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  await main();
}
