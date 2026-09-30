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
  const managedLinux = JSON.parse(
    readFileSync(
      new URL(
        "../apps/desktop/src-tauri/tauri.managed-browser.linux.conf.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  for (const fontDependency of [
    "fontconfig",
    "fonts-unfonts-core",
    "fonts-unfonts-extra",
  ]) {
    assert.ok(managedLinux.bundle.linux.deb.depends.includes(fontDependency));
    assert.ok(!linux.bundle.linux.deb.depends.includes(fontDependency));
  }
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

test("Debian is built before AppImage bundling and signed resources stay outside the ELF scan", () => {
  const managed = plan("--engine-package", "artifacts/linux-engine-package");
  assert.equal(managed.commands.length, 2);
  assert.equal(managed.commands[0][0], "build");
  assert.equal(managed.commands[1][0], "bundle");
  const format = (args) => args[args.indexOf("--bundles") + 1];
  assert.equal(format(managed.commands[0]), "deb");
  assert.equal(format(managed.commands[1]), "appimage");
  const overlay = "src-tauri/tauri.managed-browser.appimage.linux.conf.json";
  assert.ok(!managed.commands[0].includes(overlay));
  assert.ok(managed.commands[1].includes(overlay));
  assert.ok(!managed.commands[1].includes("--locked"));
  assert.match(
    managed.appimagePackage.replaceAll("\\", "/"),
    /usr\/share\/VeriSilo\/managed-browser\/engine-package$/u,
  );
  const appimage = JSON.parse(
    readFileSync(
      new URL(
        "../apps/desktop/src-tauri/tauri.managed-browser.appimage.linux.conf.json",
        import.meta.url,
      ),
      "utf8",
    ),
  );
  assert.equal(appimage.bundle.resources, null);
  assert.equal(
    appimage.bundle.linux.appimage.files[
      "/usr/share/VeriSilo/managed-browser/engine-package"
    ],
    "target/verisilo-managed-browser-resources/engine-package",
  );
  const firstAppimage = plan(
    "--engine-package", "unused", "--bundles", "appimage",
  );
  assert.equal(firstAppimage.commands.length, 1);
  assert.equal(firstAppimage.args[0], "build");
  assert.ok(firstAppimage.args.includes(overlay));
  const bundle = plan(
    "--engine-package", "unused", "--bundles", "appimage", "--bundle-only",
  );
  assert.equal(bundle.args[0], "bundle");
  assert.ok(bundle.args.includes(overlay));
  assert.ok(!bundle.args.includes("--locked"));
  assert.equal(bundle.args.filter((arg) => arg === "--verbose").length, 2);
});

test("bundle selection rejects non-Linux formats and ambiguous reuse", () => {
  for (const options of [
    ["--bundles", "nsis"],
    ["--bundles", ""],
    ["--bundles", "deb,deb"],
    ["--bundle-only"],
  ]) {
    const result = spawnSync(
      process.execPath,
      [script, "--standard", ...options, "--dry-run"],
      { encoding: "utf8", windowsHide: true },
    );
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, /Use --bundles/u);
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
