import assert from "node:assert/strict";
import test from "node:test";
import {
  LANES,
  RESTRICTED,
  classifyPath,
  pickPort,
  portFromHash,
  slugify,
  taskHash,
  taskNames,
} from "./agent-task.mjs";

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
