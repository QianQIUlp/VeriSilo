import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(
  new URL("./build-linux-desktop.mjs", import.meta.url),
);
const plan = (...args) => {
  const result = spawnSync(process.execPath, [script, ...args, "--dry-run"], {
    encoding: "utf8",
    windowsHide: true,
  });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
};

test("Linux uses native target and committed dependencies; managed packages have an explicit resource layer", () => {
  const standard = plan("--standard");
  const managed = plan("--engine-package", "artifacts/linux-engine-package");
  assert.equal(standard.platform, "linux-x64");
  assert.equal(standard.enginePackage, null);
  assert.equal(managed.profile, "managed-browser-linux");
  assert.ok(
    managed.args.includes("src-tauri/tauri.managed-browser.linux.conf.json"),
  );
  assert.ok(
    !standard.args.includes("src-tauri/tauri.managed-browser.linux.conf.json"),
  );
  assert.deepEqual(standard.args.slice(-2), ["--", "--locked"]);
  assert.ok(standard.args.includes("x86_64-unknown-linux-gnu"));
  assert.ok(!standard.args.some((arg) => /nsis|windows|unsigned/u.test(arg)));
  const linux = JSON.parse(
    readFileSync(
      new URL(
        "../apps/desktop/src-tauri/tauri.linux.conf.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  assert.deepEqual(linux.bundle.targets, ["deb", "appimage"]);
  assert.deepEqual(linux.bundle.resources, []);
  assert.ok(linux.bundle.linux.deb.depends.includes("openssl"));
});

test("packaging cannot silently omit the Managed engine or combine conflicting profiles", () => {
  for (const args of [[], ["--standard", "--engine-package", "unused"]]) {
    const result = spawnSync(process.execPath, [script, ...args, "--dry-run"], {
      encoding: "utf8",
      windowsHide: true,
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Pass --engine-package/u);
  }
});

test(
  "actual Linux package builds reject foreign hosts before staging or invoking Cargo",
  { skip: process.platform === "linux" && process.arch === "x64" },
  () => {
    const result = spawnSync(process.execPath, [script, "--standard"], {
      encoding: "utf8",
      windowsHide: true,
    });
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /must be built on native Linux x64/u);
  },
);
