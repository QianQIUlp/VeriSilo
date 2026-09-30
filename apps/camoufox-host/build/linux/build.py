#!/usr/bin/env python3
"""Build the native x86_64 Linux RC5 engine from the pinned Formal-v3 inputs.

This builds a candidate and records its bytes. It does not reuse the Windows
qualification result or download an upstream prebuilt browser.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path

HOST_ROOT = Path(__file__).resolve().parents[2]
REPO_ROOT = HOST_ROOT.parents[1]
sys.path.insert(0, str(HOST_ROOT))
from browser_tree import build_tree_manifest
from package_contract import (
    FORMAL_V3_ENGINE_REVISION, FORMAL_V3_SOURCE_LOCK_SHA256,
    PACKAGE_ASSET_LOCK_SCHEMA, sha256_file,
)

LOCK = HOST_ROOT / "lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json"
ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005", "0006", "0007"]
RECORD_TYPE = "verisilo-camoufox-linux-build/v1"
CLAIMS = {"compiled": True, "runtimeVerified": False, "windowsRuntimeObserved": False}
ARCHIVE_NAME = "camoufox-152.0.4-beta.28-verisilo-linux.x86_64.zip"


def run(args: list[str], cwd: Path | None = None, env: dict | None = None) -> str:
    return subprocess.run(args, cwd=cwd, env=env, check=True,
                          capture_output=True, text=True).stdout.strip()


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def pinned_lock() -> dict:
    if sha256_file(LOCK) != FORMAL_V3_SOURCE_LOCK_SHA256:
        raise ValueError("Formal-v3 source lock differs from the frozen RC5 input")
    lock = json.loads(LOCK.read_bytes())
    if lock["completeAppliedPatchOrder"] != ORDER:
        raise ValueError("RC5 downstream patch order differs")
    for item in lock["completePatchSeries"]:
        path = REPO_ROOT / item["path"]
        if path.stat().st_size != item["sizeBytes"] or sha256_file(path) != item["sha256"]:
            raise ValueError(f"RC5 patch binding differs: {item['id']}")
    return lock


def extract_browser(archive: Path, destination: Path) -> None:
    """Reject links/escapes and retain native executable modes from the ZIP."""
    destination.mkdir(parents=True)
    with zipfile.ZipFile(archive) as stream:
        seen = set()
        for item in stream.infolist():
            name = item.filename
            parts = name.rstrip("/").split("/")
            mode = item.external_attr >> 16
            if (name.startswith("/") or "\\" in name or ":" in parts[0]
                or any(part in ("", ".", "..") for part in parts)
                or name in seen or mode & 0o170000 == 0o120000):
                raise ValueError(f"irregular browser archive member: {name}")
            seen.add(name)
            target = destination.joinpath(*parts)
            if item.is_dir():
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with stream.open(item) as source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
                target.chmod(0o755 if mode & 0o111 else 0o644)
    for name in ("camoufox", "camoufox-bin", "updater", "crashreporter"):
        path = destination / name
        if path.is_file():
            path.chmod(0o755)
    with (destination / "camoufox-bin").open("rb") as stream:
        header = stream.read(20)
    if header[:5] != b"\x7fELF\x02" or header[18:20] != b"\x3e\x00":
        raise ValueError("browser output is not an ELF x86_64 executable")


def build(args: argparse.Namespace) -> None:
    if sys.platform != "linux" or platform.machine() not in ("x86_64", "amd64"):
        raise ValueError("engine construction requires native x86_64 Linux")
    lock = pinned_lock()
    work = args.work_root.absolute()
    out = args.out.absolute()
    if work.exists() or out.exists() or work == out or work in out.parents or out in work.parents:
        raise ValueError("work-root and out must be fresh, separate directories")
    work.mkdir(parents=True)
    out.mkdir(parents=True)
    if shutil.disk_usage(work).free < 30 * 1024**3:
        raise ValueError("native Firefox build needs at least 30 GiB of free disk space")
    memory = dict(re.findall(r"^(MemTotal|SwapTotal):\s+(\d+)", Path("/proc/meminfo").read_text(), re.M))
    if sum(int(value) for value in memory.values()) * 1024 < 8 * 1024**3:
        raise ValueError("native Firefox build needs at least 8 GiB RAM plus swap")
    started = datetime.now(timezone.utc).isoformat()
    upstream = work / "upstream"
    if args.upstream_root:
        checkout = args.upstream_root.resolve(strict=True)
    else:
        checkout = work / "checkout"
        subprocess.run(["git", "clone", "--no-checkout", "--filter=blob:none", lock["upstream"]["repository"], str(checkout)], check=True)
        subprocess.run(["git", "-C", str(checkout), "checkout", "--detach", lock["upstream"]["commit"]], check=True)
    if (run(["git", "-C", str(checkout), "rev-parse", "HEAD"]) != lock["upstream"]["commit"]
        or run(["git", "-C", str(checkout), "rev-parse", "HEAD^{tree}"]) != lock["upstream"]["tree"]):
        raise ValueError("upstream commit/tree differs from pinned RC5 source")
    exported = work / "upstream.tar"
    subprocess.run(["git", "-C", str(checkout), "archive", "--output", str(exported), lock["upstream"]["commit"]], check=True)
    upstream.mkdir()
    with tarfile.open(exported) as stream:
        stream.extractall(upstream, filter="data")
    exported.unlink()
    for item in lock["sourceInputs"]["upstreamPatches"] + lock["sourceInputs"]["recipeFiles"]:
        path = upstream / item["path"]
        if path.stat().st_size != item["sizeBytes"] or sha256_file(path) != item["sha256"]:
            raise ValueError(f"upstream input binding differs: {item['path']}")
    source_archive = upstream / "firefox-152.0.4.source.tar.xz"
    if args.firefox_archive:
        shutil.copyfile(args.firefox_archive, source_archive)
    else:
        with urllib.request.urlopen(lock["firefoxSource"]["url"], timeout=120) as download, source_archive.open("xb") as target:
            shutil.copyfileobj(download, target)
    sha512 = hashlib.sha512()
    with source_archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            sha512.update(chunk)
    if source_archive.stat().st_size != lock["firefoxSource"]["sizeBytes"] or sha512.hexdigest() != lock["firefoxSource"]["sha512"]:
        raise ValueError("Firefox source archive differs from the pinned RC5 input")
    env = dict(os.environ, BUILD_TARGET="linux,x86_64", CARGO_BUILD_JOBS=str(args.jobs),
               MOZ_BUILD_DATE="20260811045234", TZ="Etc/UTC", LANG="C.UTF-8", LC_ALL="C.UTF-8",
               MOZBUILD_STATE_PATH=str(work / "mozbuild"))
    spec = importlib.util.spec_from_file_location("rc5_patch_recipe", HOST_ROOT / "build/r1-formal-v3/strict_build.py")
    recipe = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(recipe)
    recipe.VERISILO_ROOT = REPO_ROOT
    log = recipe.BuildLog(out / "engine-build.log")
    try:
        log.run(["make", "setup-minimal"], cwd=upstream, env=env, label="setup-minimal")
        source = upstream / "camoufox-152.0.4-beta.28"
        log.run([str(source / "mach"), "--no-interactive", "bootstrap", "--application-choice=browser", "--no-system-changes"], cwd=source, env=env, label="bootstrap")
        rustup = Path.home() / ".cargo/bin/rustup"
        log.run([str(rustup), "toolchain", "install", "1.90.0", "--profile", "minimal"], cwd=source, env=env, label="rust-toolchain")
        env["RUSTUP_TOOLCHAIN"] = "1.90.0"
        log.run([sys.executable, "scripts/patch.py", "--mozconfig-only", "152.0.4", "beta.28"], cwd=upstream, env=env, label="linux-mozconfig")
        recipe._apply_upstream_patches(lock, upstream, source, env, log)
        recipe._apply_patches(lock, source, env, log)
        # Bound memory use on ordinary native Linux builders, including CI.
        with (source / "mozconfig").open("a") as stream:
            stream.write(f"\nmk_add_options MOZ_MAKE_FLAGS=-j{args.jobs}\n")
        (source / "_READY").touch()
        log.run([str(source / "mach"), "build"], cwd=source, env=env, label="build-linux-x86_64")
        log.run(["make", "package-linux", "arch=x86_64"], cwd=upstream, env=env, label="package-linux-x86_64")
    finally:
        log.close()
    browser = out / "browser"
    extract_browser(upstream / "camoufox-152.0.4-beta.28-lin.x86_64.zip", browser)
    # The installed browser is immutable/root-owned. Camoufox must never need
    # to write this cache metadata through a symlink into the install tree.
    write_json(browser / "version.json", {"version": "152.0.4", "build": "beta.28", "prerelease": True})
    archive = out / ARCHIVE_NAME
    with zipfile.ZipFile(archive, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as stream:
        for path in sorted(browser.rglob("*")):
            if path.is_file():
                stream.write(path, path.relative_to(browser).as_posix())
    tree_path = out / "browser-tree-manifest.json"
    write_json(tree_path, build_tree_manifest(browser))
    metadata = (browser / "application.ini").read_text(encoding="utf-8")
    def ini_value(key: str) -> str:
        value = re.search(rf"^{key}=(.+)$", metadata, re.M)
        if value is None:
            raise ValueError(f"native browser has no {key}")
        return value.group(1).strip()
    revision = run(["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD"])
    source_tree = run(["git", "-C", str(REPO_ROOT), "rev-parse", "HEAD^{tree}"])
    record = {"recordType": RECORD_TYPE, "platform": "linux-x86_64", "target": "x86_64-pc-linux-gnu",
              "engineRevision": FORMAL_V3_ENGINE_REVISION, "claims": CLAIMS,
              "startedAtUtc": started, "completedAtUtc": datetime.now(timezone.utc).isoformat(),
              "upstream": lock["upstream"], "completeAppliedPatchOrder": ORDER,
              "source": {"commit": revision, "tree": source_tree, "sourceLockSha256": sha256_file(LOCK), "recipeSha256": sha256_file(Path(__file__))},
              "archive": {"name": archive.name, "sha256": sha256_file(archive), "sizeBytes": archive.stat().st_size,
                          "browserExecutableSha256": sha256_file(browser / "camoufox-bin"), "buildId": ini_value("BuildID"),
                          "sourceStamp": ini_value("SourceStamp"), "propertiesJsonSha256": sha256_file(browser / "properties.json")},
              "browserTree": {"sha256": sha256_file(tree_path), "sizeBytes": tree_path.stat().st_size}}
    result_path = out / "linux-build-result.json"
    write_json(result_path, record)
    write_json(out / "runtime-asset-lock.json", {"schema": PACKAGE_ASSET_LOCK_SCHEMA, "assetKind": "self-built", "verified": False,
        "evidenceClass": "compiled-not-runtime-verified", "package": "camoufox", "release": "v152.0.4-beta.28",
        "platform": "linux-x86_64", "pythonPackage": "camoufox==0.5.4", "engineRevision": FORMAL_V3_ENGINE_REVISION,
        "sha256": record["archive"]["sha256"], "browserExecutableSha256": record["archive"]["browserExecutableSha256"],
        "sizeBytes": record["archive"]["sizeBytes"], "executableRelativePath": "camoufox-bin", "buildId": ini_value("BuildID"),
        "sourceStamp": ini_value("SourceStamp"), "propertiesJsonSha256": record["archive"]["propertiesJsonSha256"],
        "sourceBinding": {"commit": revision, "tree": source_tree, "sourceLockSha256": sha256_file(LOCK), "completeAppliedPatchOrder": ORDER},
        "buildResultSha256": sha256_file(result_path), "browserTreeManifestSha256": sha256_file(tree_path)})
    print(json.dumps({"buildResult": str(result_path), "browserRoot": str(browser), "runtimeVerified": False}))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work-root", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--restore-build", type=Path, help="Verify/re-extract an unchanged archived native engine build")
    parser.add_argument("--upstream-root", type=Path)
    parser.add_argument("--firefox-archive", type=Path)
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if args.jobs < 1:
        parser.error("--jobs must be positive")
    try:
        if args.restore_build:
            root = args.restore_build.absolute()
            record = json.loads((root / "linux-build-result.json").read_bytes())
            archive = root / ARCHIVE_NAME
            if (record.get("recordType") != RECORD_TYPE or record.get("platform") != "linux-x86_64"
                or record.get("claims") != CLAIMS or record.get("completeAppliedPatchOrder") != ORDER
                or record.get("source", {}).get("sourceLockSha256") != FORMAL_V3_SOURCE_LOCK_SHA256
                or record.get("source", {}).get("recipeSha256") != sha256_file(Path(__file__))
                or record.get("archive", {}).get("name") != ARCHIVE_NAME
                or record["archive"].get("sha256") != sha256_file(archive)
                or record["archive"].get("sizeBytes") != archive.stat().st_size):
                raise ValueError("restored native engine build has different inputs/bytes")
            pinned_lock()
            extract_browser(archive, root / "browser")
            from browser_tree import load_tree_manifest, verify_tree
            tree_path = root / "browser-tree-manifest.json"
            if record["browserTree"] != {"sha256": sha256_file(tree_path), "sizeBytes": tree_path.stat().st_size}:
                raise ValueError("restored native engine tree digest differs")
            verify_tree(root / "browser", load_tree_manifest(tree_path))
            from package_contract import load_package_asset_lock, verify_package_browser_root
            asset = load_package_asset_lock(root / "runtime-asset-lock.json")
            if asset["buildResultSha256"] != sha256_file(root / "linux-build-result.json"):
                raise ValueError("restored native engine build record digest differs")
            verify_package_browser_root(asset, root / "browser", tree_path)
            print(json.dumps({"restoredBuild": str(root), "runtimeVerified": False}))
            return 0
        if args.work_root is None or args.out is None:
            parser.error("--work-root and --out are required for a new native build")
        build(args)
        return 0
    except (OSError, ValueError, subprocess.CalledProcessError) as exc:
        print(f"Linux engine build failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
