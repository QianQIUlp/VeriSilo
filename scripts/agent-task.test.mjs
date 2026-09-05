import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  BASELINE_REF,
  LANES,
  META_FILE,
  RESTRICTED,
  WORKTREE_ROOT_NAME,
  classifyPath,
  pickPort,
  portFromHash,
  slugify,
  taskHash,
  taskNames,
} from "./agent-task.mjs";

const SCRIPT = fileURLToPath(new URL("./agent-task.mjs", import.meta.url));
const TASK = "fixture routing guard task";

// Disposable git repositories under the OS temp dir; never touch the real
// checkout. `t.after` removes them even when assertions fail.
function makeFixture(t) {
  const root = mkdtempSync(join(tmpdir(), "agent-task-fixture-"));
  t.after(() =>
    rmSync(root, {
      recursive: true,
      force: true,
      maxRetries: 5,
      retryDelay: 200,
    }),
  );
  const git = (args, cwd = root) => {
    const result = spawnSync("git", args, { cwd, encoding: "utf8" });
    if (result.status !== 0) {
      throw new Error(`git ${args.join(" ")} failed: ${result.stderr}`);
    }
    return result.stdout.trim();
  };
  const commit = (message) => {
    git([
      "-c",
      "user.name=fx",
      "-c",
      "user.email=fx@example.com",
      "commit",
      "--allow-empty",
      "-q",
      "-m",
      message,
    ]);
    return git(["rev-parse", "HEAD"]);
  };
  git(["init", "-q", "-b", "main"]);
  const c0 = commit("c0");
  return { root, git, commit, c0 };
}

const runScript = (args, cwd) =>
  spawnSync(process.execPath, [SCRIPT, ...args], { cwd, encoding: "utf8" });

const wtDir = (fx) =>
  join(fx.root, WORKTREE_ROOT_NAME, taskNames("ui", TASK).dir);
const wtMeta = (fx) =>
  JSON.parse(readFileSync(join(wtDir(fx), META_FILE), "utf8"));

const VIOLATION = (path, lane) => {
  const verdict = classifyPath(path, lane);
  assert.equal(
    verdict.status,
    "violation",
    `${path} should violate ${lane}: ${JSON.stringify(verdict)}`,
  );
  return verdict;
};
const OK = (path, lane) => {
  const verdict = classifyPath(path, lane);
  assert.equal(
    verdict.status,
    "ok",
    `${path} should be in scope for ${lane}: ${JSON.stringify(verdict)}`,
  );
  return verdict;
};

test("every lane has label, hint, allow and verify config", () => {
  for (const [id, lane] of Object.entries(LANES)) {
    assert.ok(lane.label && lane.hint, id);
    assert.ok(Array.isArray(lane.allow) && lane.allow.length > 0, id);
    assert.ok(Array.isArray(lane.verify), id);
  }
  assert.deepEqual(Object.keys(LANES), [
    "ui",
    "core",
    "host",
    "qa",
    "integration",
  ]);
});

test("ui lane owns frontend surfaces but not the API seam or contracts", () => {
  OK("apps/desktop/src/features/silos/CreateSiloPanel.tsx", "ui");
  OK("apps/desktop/src/workspace/useDesktopWorkspace.ts", "ui");
  OK("apps/desktop/src/App.tsx", "ui");
  OK("apps/desktop/src/formatters.test.ts", "ui");
  OK("apps/desktop/src/styles.css", "ui");
  OK("apps/desktop/preview.html", "ui");
  assert.equal(
    VIOLATION("apps/desktop/src/desktop-api.ts", "ui").kind,
    "restricted",
  );
  assert.equal(
    VIOLATION("packages/contracts/src/models.ts", "ui").kind,
    "restricted",
  );
  assert.equal(
    VIOLATION("apps/desktop/src-tauri/src/application/silos.rs", "ui").kind,
    "out-of-scope",
  );
  assert.equal(
    VIOLATION("apps/desktop/src-tauri/src/domain.rs", "ui").kind,
    "out-of-scope",
  );
});

test("core lane owns the desktop backend and harness, not the frontend or shell config", () => {
  OK("apps/desktop/src-tauri/src/application/silos.rs", "core");
  OK("apps/desktop/src-tauri/src/domain.rs", "core");
  OK("apps/desktop/src-tauri/Cargo.toml", "core");
  OK("crates/verisilo-desktop-core-harness/src/lib.rs", "core");
  assert.equal(
    VIOLATION("apps/desktop/src-tauri/tauri.conf.json", "core").kind,
    "shared",
  );
  assert.equal(
    VIOLATION("apps/desktop/src/App.tsx", "core").kind,
    "out-of-scope",
  );
  assert.ok(
    VIOLATION("apps/desktop/src/App.tsx", "core").reason.includes("ui"),
  );
});

test("host lane owns the python host, fixtures and its build scripts", () => {
  OK("apps/camoufox-host/host_v1.py", "host");
  OK("apps/camoufox-host/test_page_command.py", "host");
  OK("tests/fixtures/camoufox/fake-host-v1.py", "host");
  OK("tests/fingerprint-probe/probe.html", "host");
  OK("scripts/build-camoufox-host-package.py", "host");
  OK("scripts/verify-engine-source.mjs", "host");
  assert.equal(VIOLATION("scripts/dev-desktop.mjs", "host").kind, "shared");
  assert.equal(
    VIOLATION("apps/desktop/src-tauri/src/launcher.rs", "host").kind,
    "out-of-scope",
  );
});

test("qa lane owns tests and acceptance evidence, never product code", () => {
  OK("tests/windows/install-smoke.mjs", "qa");
  OK("docs/qa/repro-vault-lock.md", "qa");
  OK("docs/acceptance/rc1-notes.md", "qa");
  assert.equal(
    VIOLATION("docs/camoufox-program-status.md", "qa").kind,
    "restricted",
  );
  assert.equal(
    VIOLATION("apps/desktop/src/features/silos/SiloList.tsx", "qa").kind,
    "out-of-scope",
  );
});

test("integration lane may touch everything; workflow truth stays restricted for others", () => {
  for (const path of [
    "packages/contracts/src/models.ts",
    "AGENTS.md",
    "scripts/agent-task.mjs",
    "docs/development-worktrees.md",
    "package.json",
  ]) {
    assert.equal(classifyPath(path, "integration").status, "ok", path);
  }
  assert.equal(VIOLATION("AGENTS.md", "ui").kind, "restricted");
  assert.equal(VIOLATION("scripts/agent-task.mjs", "core").kind, "restricted");
});

test("windows path separators and ./ prefixes are normalized", () => {
  assert.equal(classifyPath("apps\\desktop\\src\\App.tsx", "ui").status, "ok");
  assert.equal(classifyPath("./apps/desktop/src/App.tsx", "ui").status, "ok");
  assert.equal(
    classifyPath("packages\\contracts\\src\\models.ts", "ui").kind,
    "restricted",
  );
});

test("slugs are deterministic, ascii and length-bounded", () => {
  assert.equal(slugify("重新设计创建 Silo 的 UX"), "silo-ux");
  assert.equal(slugify("Fix Camoufox launch exception"), "fix-camoufox-launch");
  assert.equal(slugify("修复启动异常"), "task");
  assert.ok(
    slugify("a very long task title with many words in it").length <= 24,
  );
  assert.equal(slugify("Check install flow", 24), "check-install-flow");
});

test("task names are stable for identical tasks and distinct for different ones", () => {
  const a = taskNames("ui", "重新设计创建 Silo 的 UX");
  const b = taskNames("ui", "重新设计创建 Silo 的 UX");
  assert.deepEqual(a, b);
  assert.equal(a.branch, `agent/ui/${a.slug}-${a.hash}`);
  assert.equal(a.dir, `ui-${a.slug}-${a.hash}`);
  assert.notDeepEqual(taskNames("ui", "task one"), taskNames("ui", "task two"));
  assert.equal(taskHash("x").length, 6);
});

test("vault names always satisfy the app's validate_vault_name rules", () => {
  for (const lane of Object.keys(LANES)) {
    for (const task of [
      "short",
      "一个特别特别特别长的中文任务描述没有任何 ascii 单词",
      "a".repeat(200),
    ]) {
      const { vault } = taskNames(lane, task);
      assert.match(vault, /^[a-z0-9][a-z0-9_-]{0,31}$/);
      assert.ok(vault.length <= 32, `${vault} (${vault.length})`);
    }
  }
  const longSlug = taskNames("ui", "long", "x".repeat(60));
  assert.ok(longSlug.vault.length <= 32);
});

test("ports stay inside the dedicated agent range and avoid claimed entries", () => {
  const port = portFromHash("ffffff");
  assert.ok(port >= 15400 && port < 15400 + 512);
});

test("pickPort skips claimed and busy ports deterministically", async () => {
  const allClaimed = new Set();
  for (let p = 15400; p < 15400 + 512; p += 1) allClaimed.add(p);
  await assert.rejects(() => pickPort(15400, allClaimed));

  const base = portFromHash(taskHash("port probe task"));
  const first = await pickPort(base, new Set());
  assert.ok(first >= 15400 && first < 15400 + 512);
  const second = await pickPort(base, new Set([first]));
  assert.notEqual(first, second);
});

test("start forks tasks from the canonical baseline, not the calling HEAD", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const c1 = fx.commit("c1 on the default branch");
  assert.notEqual(c1, fx.c0);

  const result = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const meta = wtMeta(fx);
  assert.equal(meta.baseline, fx.c0);
  assert.equal(meta.baselineRef, BASELINE_REF);
  assert.equal(
    fx.git(["-C", wtDir(fx), "rev-parse", "HEAD"]),
    fx.c0,
    "worktree HEAD must be the canonical baseline",
  );
  assert.ok(Array.isArray(meta.primary.dirtySnapshot));
  assert.equal(
    meta.primary.root.replaceAll("\\", "/"),
    fx.root.replaceAll("\\", "/"),
  );
});

test("start fails fast when the canonical baseline ref is missing", (t) => {
  const fx = makeFixture(t);
  const result = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /baseline\/dev/);
});

test("rerunning start resumes the same task worktree idempotently", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const first = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.equal(first.status, 0, first.stderr);
  const second = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.equal(second.status, 0, second.stderr);
  assert.match(second.stdout, /already exists/);
  const branches = fx
    .git(["branch", "--list", "agent/*"])
    .split("\n")
    .filter(Boolean);
  assert.equal(branches.length, 1);
});

test("start refuses reuse when the canonical baseline has advanced", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const first = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.equal(first.status, 0, first.stderr);
  const c1 = fx.commit("c1");
  const advance = runScript(["baseline", "advance", c1], fx.root);
  assert.equal(advance.status, 0, advance.stderr);

  const again = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.notEqual(again.status, 0);
  assert.match(again.stderr, /baseline/);
  const dirs = fx.git(["worktree", "list"]).split("\n");
  assert.equal(dirs.length, 2, "no second worktree may appear");
});

test("start refuses leftover branches and metadata-less worktree paths", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const names = taskNames("ui", TASK);

  fx.git(["branch", names.branch, fx.c0]);
  const branchClash = runScript(
    ["start", "--lane", "ui", "--task", TASK],
    fx.root,
  );
  assert.notEqual(branchClash.status, 0);
  assert.match(branchClash.stderr, /拒绝复用/);
  fx.git(["branch", "-D", names.branch]);

  mkdirSync(join(fx.root, WORKTREE_ROOT_NAME, names.dir), { recursive: true });
  writeFileSync(join(fx.root, WORKTREE_ROOT_NAME, names.dir, "keep.txt"), "x");
  const pathClash = runScript(
    ["start", "--lane", "ui", "--task", TASK],
    fx.root,
  );
  assert.notEqual(pathClash.status, 0);
  assert.match(pathClash.stderr, /没有任务元数据/);
});

test("check reports WORKSPACE CONTAMINATION for new primary changes, not pre-existing dirty state", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  // Dirty before the task starts: must never be attributed to the task.
  writeFileSync(join(fx.root, "preexisting.txt"), "before\n");
  const started = runScript(["start", "--lane", "ui", "--task", TASK], fx.root);
  assert.equal(started.status, 0, started.stderr);

  const before = runScript(["check"], wtDir(fx));
  assert.equal(before.status, 0, before.stdout + before.stderr);
  assert.match(before.stdout, /无新增修改/);

  // The real incident: write into the primary through a relative path.
  mkdirSync(join(fx.root, "packages", "contracts", "src"), { recursive: true });
  const target = join(fx.root, "packages", "contracts", "src", "models.ts");
  writeFileSync(target, "// contaminated\n");
  const during = runScript(["check"], wtDir(fx));
  assert.equal(during.status, 3);
  assert.match(during.stdout, /WORKSPACE CONTAMINATION/);
  assert.match(during.stdout, /models\.ts/);
  assert.doesNotMatch(
    during.stdout.match(/WORKSPACE CONTAMINATION[\s\S]*$/)[0],
    /preexisting\.txt/,
  );

  rmSync(target);
  const after = runScript(["check"], wtDir(fx));
  assert.equal(after.status, 0, after.stdout + after.stderr);
});

test("task commands refuse to run outside the task worktree root", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  // Metadata sitting in a subdirectory of a larger checkout: the git toplevel
  // does not match the task root, so commands must fail loudly.
  mkdirSync(join(fx.root, "apps", "desktop"), { recursive: true });
  writeFileSync(
    join(fx.root, "apps", "desktop", META_FILE),
    JSON.stringify({ version: 2, lane: "ui", task: TASK, baseline: fx.c0 }),
  );
  const result = runScript(["check"], join(fx.root, "apps", "desktop"));
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /不一致/);
});

test("baseline advance moves the ref explicitly; backward moves need --force", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const printed = runScript(["baseline"], fx.root);
  assert.equal(printed.status, 0, printed.stderr);
  assert.match(printed.stdout, new RegExp(`baseline/dev → ${fx.c0}`));

  const c1 = fx.commit("c1");
  const advance = runScript(["baseline", "advance", c1], fx.root);
  assert.equal(advance.status, 0, advance.stderr);
  assert.equal(fx.git(["rev-parse", BASELINE_REF]), c1);

  const backward = runScript(["baseline", "advance", fx.c0], fx.root);
  assert.notEqual(backward.status, 0);
  assert.match(backward.stderr, /--force/);

  const forced = runScript(["baseline", "advance", fx.c0, "--force"], fx.root);
  assert.equal(forced.status, 0, forced.stderr);
  assert.equal(fx.git(["rev-parse", BASELINE_REF]), fx.c0);
});

test("new tasks fork from the advanced baseline (B0 → B1 model)", (t) => {
  const fx = makeFixture(t);
  fx.git(["branch", BASELINE_REF, fx.c0]);
  const c1 = fx.commit("integration round 1");
  const advance = runScript(["baseline", "advance", c1], fx.root);
  assert.equal(advance.status, 0, advance.stderr);

  const result = runScript(
    ["start", "--lane", "core", "--task", TASK],
    fx.root,
  );
  assert.equal(result.status, 0, result.stderr || result.stdout);
  const meta = JSON.parse(
    readFileSync(
      join(fx.root, WORKTREE_ROOT_NAME, taskNames("core", TASK).dir, META_FILE),
      "utf8",
    ),
  );
  assert.equal(meta.baseline, c1);
});
