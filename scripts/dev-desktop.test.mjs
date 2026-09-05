import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./dev-desktop.mjs", import.meta.url));
const plan = (name, port, extra = []) => {
  const result = spawnSync(
    process.execPath,
    [script, name, "--port", String(port), ...extra, "--dry-run"],
    { encoding: "utf8", windowsHide: true },
  );
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
};

test("workspaces separate application arguments, Vaults and Vite endpoints", () => {
  const ui = plan("ui", 1421);
  const core = plan("core", 1422);
  assert.equal(ui.vault, "dev-ui");
  assert.equal(core.vault, "dev-core");
  assert.notEqual(ui.url, core.url);
  assert.deepEqual(ui.args.slice(-4), ["--", "--", "--vault", "dev-ui"]);
  const config = JSON.parse(ui.args[ui.args.indexOf("--config") + 1]);
  assert.equal(config.build.devUrl, ui.url);
  const preview = plan("ui", 1423, ["--preview"]);
  assert.equal(preview.vault, null);
  assert.equal(preview.url, "http://127.0.0.1:1423/preview.html");
  assert.ok(!preview.args.includes("--vault"));
});

test("explicit vault overrides are validated and passed through", () => {
  const result = plan("core", 1424, ["--vault", "ui-create-silo-ux-3f9a2c"]);
  assert.equal(result.vault, "ui-create-silo-ux-3f9a2c");
  assert.deepEqual(result.args.slice(-4), [
    "--",
    "--",
    "--vault",
    "ui-create-silo-ux-3f9a2c",
  ]);
  for (const bad of ["Default", "-leading", "with space", "x".repeat(33)]) {
    const attempt = spawnSync(
      process.execPath,
      [script, "core", "--port", "1421", "--vault", bad, "--dry-run"],
      { encoding: "utf8", windowsHide: true },
    );
    assert.notEqual(attempt.status, 0, bad);
  }
});

test("invalid names and ports cannot become shell arguments or data paths", () => {
  for (const args of [
    ["../default"],
    ["ui", "--port", "1421;whoami"],
    ["ui", "--port", "0"],
  ]) {
    const result = spawnSync(process.execPath, [script, ...args, "--dry-run"], {
      encoding: "utf8",
      windowsHide: true,
    });
    assert.notEqual(result.status, 0);
  }
});
