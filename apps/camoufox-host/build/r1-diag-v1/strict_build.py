#!/usr/bin/env python3
"""Strict one-shot R1 diagnostic Camoufox build driver.

This driver is embedded in the independent R1 diagnostic builder image.  The
build mode, source lock, complete patch order, and diagnostic gate are fixed by
the image/checkout contract; none can be selected through the environment.
The driver never installs or launches the produced browser.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import zipfile
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath


VERISILO_ROOT = Path("/inputs/verisilo")
UPSTREAM_REPO = Path("/inputs/upstream")
FIREFOX_ARCHIVE = Path("/inputs/firefox-152.0.4.source.tar.xz")
INPUT_ROOT = Path("/inputs")
BUILD_HOME = Path("/build-home")
WORK_ROOT = Path("/work")
OUT_ROOT = Path("/out")
EMBEDDED_GATE = Path("/usr/local/lib/verisilo-r1-diag/diag_gate.py")

LOCK_REL = Path(
    "apps/camoufox-host/lock/"
    "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
)
RECIPE_REL = Path("apps/camoufox-host/build/r1-diag-v1")
STRICT_BUILD_REL = RECIPE_REL / "strict_build.py"
DIAG_GATE_REL = RECIPE_REL / "diag_gate.py"
EXPECTED_SOURCE_DIR = "camoufox-152.0.4-beta.28"
EXPECTED_OUTPUT = "camoufox-152.0.4-beta.28-win.x86_64.zip"
DEFAULT_MOZ_BUILD_DATE = "20260811045234"
PINNED_RUST_TOOLCHAIN = "1.90.0"
EXPECTED_FIXED_ENVIRONMENT = {
    "BUILD_TARGET": "windows,x86_64",
    "CARGO_BUILD_JOBS": "1",
    "LANG": "C.UTF-8",
    "LC_ALL": "C.UTF-8",
    "MOZ_BUILD_DATE": DEFAULT_MOZ_BUILD_DATE,
    "RUST_TOOLCHAIN": PINNED_RUST_TOOLCHAIN,
    "TZ": "Etc/UTC",
}
RUSTUP_BIN = Path("/build-home/.cargo/bin/rustup")
RUSTC_BIN = Path("/build-home/.cargo/bin/rustc")
EXPECTED_BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
EXPECTED_BASE_AMD64_MANIFEST_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
UPSTREAM_PATCH_COMMAND = (
    "patch -p1 --batch --binary --forward --ignore-whitespace "
    "--fuzz=2 --no-backup-if-mismatch"
)
DOWNSTREAM_PATCH_COMMAND = (
    "patch -p1 --batch --binary --forward --fuzz=0 "
    "--no-backup-if-mismatch"
)
EXPECTED_COMPLETE_PATCH_PATHS = [
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0000-verisilo-ff152-midl-cross-build-input.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0001-verisilo-canvas-export-key.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0002-verisilo-juggler-bounded-close.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0003-verisilo-gpc-canonical-pref-projection.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0004-verisilo-remove-worker-gpc-mask-override.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch",
]
EXPECTED_INCREMENTAL_PATCH_PATHS = EXPECTED_COMPLETE_PATCH_PATHS[3:]


class BuildFailure(RuntimeError):
    """Typed fail-closed source, gate, or build boundary."""


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _sha(path: Path, algorithm: str = "sha256") -> str:
    digest = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _strict_json(path: Path) -> dict:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict:
        result: dict = {}
        for key, value in pairs:
            if key in result:
                raise BuildFailure(f"duplicate JSON key in {path.name}: {key}")
            result[key] = value
        return result

    try:
        value = json.loads(
            path.read_bytes().decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                BuildFailure(f"invalid JSON number in {path.name}: {token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise BuildFailure(f"invalid JSON in {path.name}: {exc}") from exc
    if type(value) is not dict:
        raise BuildFailure(f"{path.name} must contain one JSON object")
    return value


def _write_json(path: Path, value: dict) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _capture(command: list[str], cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip()
        raise BuildFailure(f"command failed ({completed.returncode}): {detail}")
    return completed.stdout.strip()


def _git(repo: Path, *arguments: str) -> str:
    return _capture(
        ["git", "-c", f"safe.directory={repo}", "-C", str(repo), *arguments]
    )


class BuildLog:
    def __init__(self, path: Path):
        self.path = path
        self._stream = path.open("w", encoding="utf-8", newline="\n")
        self._closed = False

    def close(self) -> None:
        if not self._closed:
            self._stream.close()
            self._closed = True

    def note(self, message: str) -> None:
        if self._closed:
            raise BuildFailure("cannot append to a closed build log")
        line = f"[{_utc_now()}] {message}"
        print(line, flush=True)
        self._stream.write(line + "\n")
        self._stream.flush()

    def run(
        self,
        command: list[str],
        *,
        cwd: Path,
        env: dict[str, str],
        label: str,
    ) -> None:
        self.note(f"start {label}: {shlex.join(command)}")
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        assert process.stdout is not None
        for line in process.stdout:
            print(line, end="", flush=True)
            self._stream.write(line)
            self._stream.flush()
        if process.wait() != 0:
            raise BuildFailure(f"{label} failed")
        self.note(f"success {label}")


def _mount_table() -> dict[str, dict[str, object]]:
    mounts: dict[str, dict[str, object]] = {}
    for line in Path("/proc/self/mountinfo").read_text(encoding="utf-8").splitlines():
        fields = line.split()
        try:
            separator = fields.index("-")
        except ValueError as exc:
            raise BuildFailure("invalid mountinfo entry") from exc
        if separator < 6 or len(fields) <= separator + 3:
            raise BuildFailure("truncated mountinfo entry")
        mounts[fields[4]] = {
            "options": set(fields[5].split(",")),
            "root": fields[3],
            "filesystem": fields[separator + 1],
            "source": fields[separator + 2],
        }
    return mounts


def _validate_mounts() -> dict[str, dict[str, object]]:
    mounts = _mount_table()
    required = {INPUT_ROOT: True, BUILD_HOME: False, WORK_ROOT: False, OUT_ROOT: False}
    selected: dict[str, dict[str, object]] = {}
    for path, read_only in required.items():
        entry = mounts.get(path.as_posix())
        if entry is None:
            raise BuildFailure(f"required path is not an exact bind mount: {path}")
        options = entry["options"]
        if read_only and "ro" not in options:
            raise BuildFailure(f"input mount must be read-only: {path}")
        if not read_only and "rw" not in options:
            raise BuildFailure(f"run-owned mount must be read-write: {path}")
        selected[path.as_posix()] = entry
    identities = {
        (
            selected[path.as_posix()]["filesystem"],
            selected[path.as_posix()]["source"],
            selected[path.as_posix()]["root"],
        )
        for path in (BUILD_HOME, WORK_ROOT, OUT_ROOT)
    }
    if len(identities) != 3:
        raise BuildFailure("build-home, work and out must be distinct mounts")
    return selected


def _validate_resources(lock: dict) -> dict:
    gate = lock["buildBinding"]["resourceGate"]
    free = shutil.disk_usage(WORK_ROOT).free
    swap = 0
    for line in Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
        if line.startswith("SwapTotal:"):
            swap = int(line.split()[1]) * 1024
            break
    cpu = os.cpu_count() or 0
    if free < gate["minimumFreeBytes"]:
        raise BuildFailure("builder free space is below the frozen minimum")
    if swap < gate["minimumSwapBytes"]:
        raise BuildFailure("builder swap is below the frozen minimum")
    if cpu < gate["minimumLogicalCpu"]:
        raise BuildFailure("builder CPU count is below the frozen minimum")
    return {"freeBytes": free, "swapBytes": swap, "logicalCpu": cpu}


def _pin_rust_toolchain(
    lock: dict, cwd: Path, environment: dict[str, str], log: BuildLog
) -> str:
    version = lock["buildBinding"]["recipe"]["fixedEnvironment"].get(
        "RUST_TOOLCHAIN"
    )
    if version != PINNED_RUST_TOOLCHAIN:
        raise BuildFailure("R1 Rust toolchain binding is not exact")
    if "RUSTUP_TOOLCHAIN" in environment:
        raise BuildFailure("R1 Rust toolchain cannot be overridden by environment")
    if not RUSTUP_BIN.is_file() or not RUSTC_BIN.is_file():
        raise BuildFailure("mozbootstrap did not install the Rust toolchain manager")
    log.run(
        [
            str(RUSTUP_BIN),
            "toolchain",
            "install",
            version,
            "--profile",
            "minimal",
        ],
        cwd=cwd,
        env=environment,
        label="install-pinned-rust-toolchain",
    )
    log.run(
        [str(RUSTUP_BIN), "default", version],
        cwd=cwd,
        env=environment,
        label="select-pinned-rust-toolchain",
    )
    environment["RUSTUP_TOOLCHAIN"] = version
    log.run(
        [str(RUSTC_BIN), "-vV"],
        cwd=cwd,
        env=environment,
        label="verify-pinned-rust-toolchain",
    )
    return version


def _load_gate_module():
    if not EMBEDDED_GATE.is_file() or EMBEDDED_GATE.is_symlink():
        raise BuildFailure("embedded diagnostic gate is missing")
    spec = importlib.util.spec_from_file_location("verisilo_r1_diag_gate", EMBEDDED_GATE)
    if spec is None or spec.loader is None:
        raise BuildFailure("cannot load embedded diagnostic gate")
    module = importlib.util.module_from_spec(spec)
    had_previous = spec.name in sys.modules
    previous = sys.modules.get(spec.name)
    sys.modules[spec.name] = module
    try:
        spec.loader.exec_module(module)
    finally:
        if had_previous:
            sys.modules[spec.name] = previous
        else:
            sys.modules.pop(spec.name, None)
    return module


def _recipe_file(lock: dict, relative: str) -> dict:
    for item in lock["buildBinding"]["recipe"]["files"]:
        if item["path"] == relative:
            return item
    raise BuildFailure(f"recipe file is not bound by the source lock: {relative}")


def _validate_recipe_and_mode(lock: dict) -> None:
    if lock.get("schema") != "verisilo-r1-diag-source-binding/v2":
        raise BuildFailure("unexpected R1 diagnostic source-lock schema")
    if (
        lock.get("engineRevision") != "verisilo-camoufox-152.0.4-beta.28-r1-diag-v2"
        or lock.get("buildMode") != "diagnostic"
        or lock.get("diagnosticOnly") is not True
        or lock.get("formalEligible") is not False
    ):
        raise BuildFailure("R1 diagnostic mode contract is not exact")
    binding = lock.get("buildBinding")
    if type(binding) is not dict or type(binding.get("builderImageBinding")) is not dict:
        raise BuildFailure("R1 diagnostic builder binding is missing")
    required = set(binding["builderImageBindingRequiredFields"])
    if set(binding["builderImageBinding"]) != required:
        raise BuildFailure("R1 diagnostic builder binding fields are not exact")
    recipe = binding.get("recipe")
    if type(recipe) is not dict:
        raise BuildFailure("R1 diagnostic recipe binding is malformed")
    if recipe.get("name") != "camoufox-152.0.4-beta.28-r1-diag-v2":
        raise BuildFailure("R1 diagnostic recipe name is not exact")
    if recipe.get("fixedEnvironment") != EXPECTED_FIXED_ENVIRONMENT:
        raise BuildFailure("R1 diagnostic fixed environment is not exact")
    recipe_paths = [item["path"] for item in recipe["files"]]
    expected_paths = [
        (RECIPE_REL / "Dockerfile").as_posix(),
        STRICT_BUILD_REL.as_posix(),
        DIAG_GATE_REL.as_posix(),
        (RECIPE_REL / "build_host.py").as_posix(),
    ]
    if recipe_paths != expected_paths:
        raise BuildFailure("R1 diagnostic recipe file order is not exact")
    for relative in expected_paths:
        item = _recipe_file(lock, relative)
        path = VERISILO_ROOT / relative
        if not path.is_file() or path.stat().st_size != item["sizeBytes"]:
            raise BuildFailure(f"R1 recipe file size mismatch: {relative}")
        if _sha(path) != item["sha256"]:
            raise BuildFailure(f"R1 recipe file digest mismatch: {relative}")
    if _sha(Path(__file__)) != _recipe_file(lock, STRICT_BUILD_REL.as_posix())["sha256"]:
        raise BuildFailure("embedded strict driver differs from the locked recipe")
    if _sha(EMBEDDED_GATE) != _recipe_file(lock, DIAG_GATE_REL.as_posix())["sha256"]:
        raise BuildFailure("embedded diagnostic gate differs from the locked recipe")
    dockerfile = VERISILO_ROOT / (RECIPE_REL / "Dockerfile")
    expected_from = "FROM ubuntu:24.04@" + EXPECTED_BASE_INDEX_DIGEST
    if dockerfile.read_text(encoding="utf-8").splitlines()[0] != expected_from:
        raise BuildFailure("R1 Dockerfile base digest is not exact")


def _validate_patch_contract(lock: dict) -> None:
    complete = lock.get("completePatchSeries")
    if type(complete) is not list or not all(type(item) is dict for item in complete) or [item.get("id") for item in complete] != [
        "0000", "0001", "0002", "0003", "0004", "9000"
    ]:
        raise BuildFailure("R1 complete patch IDs are not exact")
    if [item.get("path") for item in complete] != EXPECTED_COMPLETE_PATCH_PATHS:
        raise BuildFailure("R1 complete patch paths are not exact")
    if lock.get("completeAppliedPatchOrder") != [
        "0000", "0001", "0002", "0003", "0004", "9000"
    ]:
        raise BuildFailure("R1 complete patch application order is not exact")
    incremental = lock.get("r1IncrementalPatches")
    if type(incremental) is not list or not all(type(item) is dict for item in incremental) or [item.get("id") for item in incremental] != [
        "0003", "0004", "9000"
    ]:
        raise BuildFailure("R1 incremental patch IDs are not exact")
    if [item.get("path") for item in incremental] != EXPECTED_INCREMENTAL_PATCH_PATHS:
        raise BuildFailure("R1 incremental patch paths are not exact")
    source_inputs = lock.get("sourceInputs")
    if type(source_inputs) is not dict:
        raise BuildFailure("R1 source-input binding is malformed")
    downstream = source_inputs.get("downstreamPatches")
    if type(downstream) is not list or not all(type(item) is dict for item in downstream) or [item.get("path") for item in downstream] != EXPECTED_COMPLETE_PATCH_PATHS:
        raise BuildFailure("R1 downstream patch paths are not exact")


def _verify_windows_toolchain_manifest(lock: dict, source: Path) -> dict:
    manifest = lock["buildBinding"]["windowsToolchain"]["selectionManifest"]
    if type(manifest) is not dict:
        raise BuildFailure("Windows toolchain selection manifest binding is malformed")
    if set(manifest) != {"path", "sha256", "size"}:
        raise BuildFailure("Windows toolchain selection manifest binding is malformed")
    relative = PurePosixPath(manifest["path"])
    if relative.is_absolute() or any(part in {"", ".", ".."} for part in relative.parts):
        raise BuildFailure("Windows toolchain selection manifest path is not canonical")
    path = source.joinpath(*relative.parts)
    if not path.is_file() or path.is_symlink():
        raise BuildFailure("Windows toolchain selection manifest is missing")
    if path.stat().st_size != manifest["size"] or _sha(path) != manifest["sha256"]:
        raise BuildFailure("Windows toolchain selection manifest differs from the lock")
    return dict(manifest)


def _validate_builder_identity(lock: dict, environment: dict[str, str]) -> dict:
    if sys.platform != "linux" or platform.machine().lower() not in {"x86_64", "amd64"}:
        raise BuildFailure("builder must be a Linux x86_64 container")
    binding = lock["buildBinding"]["builderImageBinding"]
    image_id = environment.get("VERISILO_BUILDER_IMAGE_ID", "")
    archive_sha = environment.get("VERISILO_BUILDER_IMAGE_SAVE_SHA256", "")
    if image_id != binding["imageId"] or archive_sha != binding["savedArchiveSha256"]:
        raise BuildFailure("builder image differs from the bound R1 diagnostic image")
    if environment.get("VERISILO_BASE_IMAGE_INDEX_DIGEST") != EXPECTED_BASE_INDEX_DIGEST:
        raise BuildFailure("base image index digest differs from the lock")
    if environment.get("VERISILO_BASE_AMD64_MANIFEST_DIGEST") != EXPECTED_BASE_AMD64_MANIFEST_DIGEST:
        raise BuildFailure("base image manifest digest differs from the lock")
    return {
        "imageId": image_id,
        "savedArchiveSha256": archive_sha,
        "baseIndexDigest": EXPECTED_BASE_INDEX_DIGEST,
        "baseLinuxAmd64ManifestDigest": EXPECTED_BASE_AMD64_MANIFEST_DIGEST,
    }


def _ordered_upstream_patch_paths(upstream: Path) -> list[str]:
    paths = [
        path.relative_to(upstream).as_posix()
        for path in (upstream / "patches").rglob("*.patch")
        if path.is_file()
    ]
    paths.sort(key=lambda value: Path(value).name)
    return [p for p in paths if "roverfox" not in Path(p).parts] + [
        p for p in paths if "roverfox" in Path(p).parts
    ]


def _validate_checkout_inputs(lock: dict, environment: dict[str, str]) -> dict:
    upstream = lock["upstream"]
    actual = {
        "commit": _git(UPSTREAM_REPO, "rev-parse", "HEAD"),
        "tree": _git(UPSTREAM_REPO, "rev-parse", "HEAD^{tree}"),
        "tag": _git(UPSTREAM_REPO, "rev-parse", f"refs/tags/{upstream['tag']}^{{commit}}"),
        "status": _git(UPSTREAM_REPO, "status", "--short", "--untracked-files=all", "--ignored=matching"),
    }
    if (
        actual["commit"] != upstream["commit"]
        or actual["tree"] != upstream["tree"]
        or actual["tag"] != upstream["commit"]
        or actual["status"]
    ):
        raise BuildFailure("upstream checkout differs from the v2 source lock")
    expected_upstream = [item["path"] for item in lock["sourceInputs"]["upstreamPatches"]]
    if _ordered_upstream_patch_paths(UPSTREAM_REPO) != expected_upstream:
        raise BuildFailure("upstream patch order differs from the v2 source lock")
    for item in lock["sourceInputs"]["upstreamPatches"] + lock["sourceInputs"]["recipeFiles"]:
        path = UPSTREAM_REPO / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"] or _sha(path) != item["sha256"]:
            raise BuildFailure(f"upstream input digest mismatch: {item['path']}")
    firefox = lock["firefoxSource"]
    if (
        not FIREFOX_ARCHIVE.is_file()
        or FIREFOX_ARCHIVE.stat().st_size != firefox["sizeBytes"]
        or _sha(FIREFOX_ARCHIVE, "sha512") != firefox["sha512"]
    ):
        raise BuildFailure("Firefox source archive differs from the v2 source lock")

    verisilo_status = _git(VERISILO_ROOT, "status", "--short", "--untracked-files=all", "--ignored=matching")
    verisilo_commit = _git(VERISILO_ROOT, "rev-parse", "HEAD")
    verisilo_tree = _git(VERISILO_ROOT, "rev-parse", "HEAD^{tree}")
    if verisilo_status:
        raise BuildFailure("VeriSilo checkout is not completely clean")
    if verisilo_commit != environment.get("VERISILO_SOURCE_COMMIT"):
        raise BuildFailure("VeriSilo commit differs from host binding")
    if verisilo_tree != environment.get("VERISILO_SOURCE_TREE"):
        raise BuildFailure("VeriSilo tree differs from host binding")
    lock_sha = _sha(VERISILO_ROOT / LOCK_REL)
    if lock_sha != environment.get("VERISILO_SOURCE_LOCK_SHA256"):
        raise BuildFailure("R1 v2 source lock differs from host binding")

    complete = lock["completeAppliedPatchOrder"]
    specs = {item["id"]: item for item in lock["completePatchSeries"]}
    if complete != ["0000", "0001", "0002", "0003", "0004", "9000"]:
        raise BuildFailure("complete R1 patch order is not exact")
    if set(specs) != set(complete):
        raise BuildFailure("complete R1 patch series ids are not exact")
    for patch_id in complete:
        item = specs[patch_id]
        path = VERISILO_ROOT / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"] or _sha(path) != item["sha256"]:
            raise BuildFailure(f"R1 patch digest mismatch: {patch_id}")
    expected_incremental = {item["id"] for item in lock["r1IncrementalPatches"]}
    if expected_incremental != {"0003", "0004", "9000"}:
        raise BuildFailure("R1 incremental patch set is not exact")
    return {
        "verisiloCommit": verisilo_commit,
        "verisiloTree": verisilo_tree,
        "sourceLockSha256": lock_sha,
        "upstreamCommit": actual["commit"],
        "upstreamTree": actual["tree"],
        "upstreamPatchCount": len(expected_upstream),
        "completeAppliedPatchOrder": complete,
    }


def _run_diagnostic_gate(lock: dict, result_dir: Path) -> dict:
    gate = _load_gate_module()
    series_dir = VERISILO_ROOT / lock["diagnosticGate"]["seriesDir"]
    try:
        expected = gate.expected_series_from_lock(lock)
        result = gate.evaluate(gate.MODE_DIAGNOSTIC, series_dir, expected)
    except (KeyError, TypeError, ValueError) as exc:
        raise BuildFailure(f"diagnostic gate contract is malformed: {exc}") from exc
    payload = json.loads(result.to_json())
    _write_json(result_dir / "diagnostic-gate-result.json", payload)
    if (
        not result.ok
        or not result.diagnosticOnly
        or result.formalEligible
        or result.details.get("purpose") != lock["diagnosticPurpose"]
    ):
        raise BuildFailure(f"diagnostic patch gate rejected the series: {result.reason}")
    return payload


def _safe_extract(archive: Path, destination: Path) -> None:
    root = destination.resolve()
    with tarfile.open(archive, "r:") as bundle:
        for member in bundle.getmembers():
            target = (destination / member.name).resolve()
            if target != root and root not in target.parents:
                raise BuildFailure("upstream archive member escapes workspace")
            if not (member.isdir() or member.isfile()):
                raise BuildFailure("upstream archive contains an unsupported member")
        bundle.extractall(destination)


def _export_upstream(workspace: Path, log: BuildLog) -> Path:
    archive = workspace / "upstream.tar"
    log.run(
        ["git", "-c", f"safe.directory={UPSTREAM_REPO}", "-C", str(UPSTREAM_REPO), "archive", "-o", str(archive), "HEAD"],
        cwd=workspace,
        env=dict(os.environ),
        label="export-pinned-upstream",
    )
    extracted = workspace / "upstream"
    extracted.mkdir()
    _safe_extract(archive, extracted)
    log.note(f"exported exact upstream commit; archive sha256={_sha(archive)}")
    shutil.copy2(FIREFOX_ARCHIVE, extracted / FIREFOX_ARCHIVE.name)
    return extracted


def _verify_seams(lock: dict, source: Path, patch_id: str, stage: str) -> None:
    entries = [item for item in lock["patchSeams"] if item["id"] == patch_id]
    if not entries:
        raise BuildFailure(f"no seam binding exists for patch {patch_id}")
    for item in entries:
        path = source / item["path"]
        expected = item[f"{stage}Sha256"]
        if expected is None:
            if path.exists():
                raise BuildFailure(f"patch {patch_id} seam unexpectedly exists: {item['path']}")
            continue
        if not path.is_file() or _sha(path) != expected:
            raise BuildFailure(f"patch {patch_id} {stage} seam mismatch: {item['path']}")


def _apply_upstream_patches(
    lock: dict,
    upstream: Path,
    source: Path,
    environment: dict[str, str],
    log: BuildLog,
) -> list[str]:
    applied: list[str] = []
    for index, item in enumerate(lock["sourceInputs"]["upstreamPatches"], start=1):
        patch_path = upstream / item["path"]
        if not patch_path.is_file() or _sha(patch_path) != item["sha256"]:
            raise BuildFailure(f"upstream patch digest mismatch during application: {item['path']}")
        command = UPSTREAM_PATCH_COMMAND.split() + ["-i", str(patch_path)]
        log.run(
            command,
            cwd=source,
            env=environment,
            label=f"apply-upstream-patch-{index:02d}",
        )
        applied.append(item["path"])
    return applied


def _apply_patches(lock: dict, source: Path, environment: dict[str, str], log: BuildLog) -> list[str]:
    specs = {item["id"]: item for item in lock["completePatchSeries"]}
    applied: list[str] = []
    for patch_id in lock["completeAppliedPatchOrder"]:
        item = specs[patch_id]
        _verify_seams(lock, source, patch_id, "pre")
        command_text = UPSTREAM_PATCH_COMMAND if item["kind"] == "upstream" else DOWNSTREAM_PATCH_COMMAND
        command = command_text.split() + ["-i", str(VERISILO_ROOT / item["path"])]
        log.run(command, cwd=source, env=environment, label=f"apply-patch-{patch_id}")
        _verify_seams(lock, source, patch_id, "post")
        applied.append(patch_id)
    return applied


def _read_ini_value(path: Path, key: str) -> str | None:
    if not path.is_file():
        return None
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        name, separator, value = line.partition("=")
        if separator and name.strip() == key:
            return value.strip()
    return None


def _locked_crt_files(lock: dict, environment: dict[str, str]) -> list[Path]:
    crt = lock["buildBinding"]["windowsToolchain"]["crt"]
    relative = PurePosixPath(crt["relativePath"])
    if relative.is_absolute() or any(part in {"", ".", ".."} for part in relative.parts):
        raise BuildFailure("packaged CRT path is not canonical")
    root = Path(environment["MOZBUILD_STATE_PATH"]).joinpath(*relative.parts)
    if not root.is_dir() or root.is_symlink():
        raise BuildFailure("locked packaged CRT directory is missing")
    expected = {row["path"]: row for row in crt["files"]}
    actual = {path.name: path for path in root.iterdir() if path.is_file() and not path.is_symlink()}
    if set(actual) != set(expected):
        raise BuildFailure("packaged CRT directory differs from the source lock")
    for name, row in expected.items():
        path = actual[name]
        if path.stat().st_size != row["size"] or _sha(path) != row["sha256"]:
            raise BuildFailure(f"packaged CRT file differs from the source lock: {name}")
    return [actual[name] for name in sorted(expected)]


def _windows_package_command(crt_files: list[Path]) -> list[str]:
    if not crt_files or any(not path.is_absolute() for path in crt_files):
        raise BuildFailure("packaged CRT argv is not absolute")
    return [
        "python3",
        "scripts/package.py",
        "windows",
        "--includes",
        "settings/chrome.css",
        "settings/camoucfg.jvv",
        "settings/properties.json",
        *(str(path) for path in crt_files),
        "--version",
        "152.0.4",
        "--release",
        "beta.28",
        "--arch",
        "x86_64",
        "--fonts",
        "macos",
        "linux",
    ]


def _freeze_archive(lock: dict, upstream: Path, result_dir: Path) -> dict:
    expected = lock["output"]["archiveName"]
    outputs = sorted(path for path in upstream.glob("camoufox-*.zip") if path.is_file())
    if [path.name for path in outputs] != [expected]:
        raise BuildFailure("build output archive set is not exact")
    candidate = result_dir / expected
    shutil.copy2(outputs[0], candidate)
    tree_entries = []
    with zipfile.ZipFile(candidate) as bundle:
        names = bundle.namelist()
        exe_names = [name for name in names if PurePosixPath(name).name == "camoufox.exe"]
        if exe_names != [lock["output"]["executableMember"]]:
            raise BuildFailure("archive executable member differs from the lock")
        executable_sha = hashlib.sha256(bundle.read(exe_names[0])).hexdigest()
        application = bundle.read(lock["output"]["applicationIniMember"]).decode("utf-8", "replace")
        platform_ini = bundle.read(lock["output"]["platformIniMember"]).decode("utf-8", "replace")
        for info in bundle.infolist():
            member = info.filename[:-1] if info.is_dir() and info.filename.endswith("/") else info.filename
            if info.is_dir():
                tree_entries.append({"path": member, "type": "directory"})
            else:
                data = bundle.read(info.filename)
                tree_entries.append({
                    "path": member,
                    "type": "file",
                    "sizeBytes": len(data),
                    "sha256": hashlib.sha256(data).hexdigest(),
                })
    tree_entries.sort(key=lambda row: row["path"])
    tree_manifest = {
        "schema": "verisilo-windows-extraction-tree/v1",
        "entries": tree_entries,
    }
    tree_path = result_dir / "windows-extraction-tree.json"
    _write_json(tree_path, tree_manifest)
    tree_sha = _sha(tree_path)
    return {
        "name": candidate.name,
        "sizeBytes": candidate.stat().st_size,
        "sha256": _sha(candidate),
        "camoufoxExeSha256": executable_sha,
        "buildId": next((line.split("=", 1)[1].strip() for line in application.splitlines() if line.startswith("BuildID=")), None),
        "sourceStamp": next((line.split("=", 1)[1].strip() for line in (application + "\n" + platform_ini).splitlines() if line.startswith("SourceStamp=")), None),
        "treeManifest": {
            "name": tree_path.name,
            "sha256": tree_sha,
            "sizeBytes": tree_path.stat().st_size,
            "entryCount": len(tree_entries),
        },
    }


def execute(args: argparse.Namespace) -> dict:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", args.run_id):
        raise BuildFailure("run-id must be 8-64 lowercase letters, digits or hyphens")
    if not re.fullmatch(r"\d{14}", args.moz_build_date):
        raise BuildFailure("MOZ_BUILD_DATE must contain exactly 14 digits")
    _validate_mounts()
    workspace = WORK_ROOT / args.run_id
    result_dir = OUT_ROOT / args.run_id
    if workspace.exists() or result_dir.exists():
        raise BuildFailure("one-shot run workspace/output already exists")
    workspace.mkdir(parents=False)
    result_dir.mkdir(parents=False)
    log = BuildLog(result_dir / "build.log")
    started = _utc_now()
    try:
        lock = _strict_json(VERISILO_ROOT / LOCK_REL)
        _validate_recipe_and_mode(lock)
        _validate_patch_contract(lock)
        environment = dict(os.environ)
        builder = _validate_builder_identity(lock, environment)
        resources = _validate_resources(lock)
        inputs = _validate_checkout_inputs(lock, environment)
        gate_result = _run_diagnostic_gate(lock, result_dir)
        log.note("R1 diagnostic gate accepted before source extraction")
        upstream = _export_upstream(workspace, log)
        source_archive = upstream / FIREFOX_ARCHIVE.name
        if not source_archive.is_file():
            raise BuildFailure("Firefox source archive was not exported")
        environment.update(
            {
                "BUILD_TARGET": "windows,x86_64",
                "CARGO_BUILD_JOBS": "1",
                "HOME": "/build-home",
                "LANG": "C.UTF-8",
                "LC_ALL": "C.UTF-8",
                "MOZBUILD_STATE_PATH": "/build-home/.mozbuild",
                "MOZ_BUILD_DATE": args.moz_build_date,
                "TZ": "Etc/UTC",
            }
        )
        log.run(["make", "setup-minimal"], cwd=upstream, env=environment, label="setup-minimal")
        source = upstream / EXPECTED_SOURCE_DIR
        if not source.is_dir():
            raise BuildFailure("setup-minimal did not materialize the expected source")
        toolchain_manifest = _verify_windows_toolchain_manifest(lock, source)
        log.run(["make", "mozbootstrap"], cwd=upstream, env=environment, label="mozbootstrap")
        rust_toolchain = _pin_rust_toolchain(
            lock, upstream, environment, log
        )
        log.run(
            ["python3", "scripts/patch.py", "--mozconfig-only", "152.0.4", "beta.28"],
            cwd=upstream,
            env=environment,
            label="mozconfig-only",
        )
        upstream_applied = _apply_upstream_patches(
            lock, upstream, source, environment, log
        )
        applied = _apply_patches(lock, source, environment, log)
        log.note(
            f"upstream patch stack applied: {len(upstream_applied)} patches; "
            "complete downstream patch order applied: " + " -> ".join(applied)
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during patching")
        (source / "_READY").touch()
        log.run([str(source / "mach"), "configure"], cwd=source, env=environment, label="configure")
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during configure")
        log.run([str(source / "mach"), "build"], cwd=source, env=environment, label="build-windows-x86_64")
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during build")
        crt_files = _locked_crt_files(lock, environment)
        log.run(
            _windows_package_command(crt_files),
            cwd=upstream,
            env=environment,
            label="package-windows-x86_64",
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during package")
        archive = _freeze_archive(lock, upstream, result_dir)
        log.note(f"diagnostic archive frozen: sha256={archive['sha256']}")
        log.close()
        result = {
            "recordType": "verisilo-camoufox-r1-diag-build-run/v2",
            "runId": args.run_id,
            "startedAtUtc": started,
            "completedAtUtc": _utc_now(),
            "engineRevision": lock["engineRevision"],
            "buildMode": lock["buildMode"],
            "diagnosticOnly": True,
            "formalEligible": False,
            "browserLaunches": 0,
            "builder": builder,
            "resourcesAtStart": resources,
            "inputs": inputs,
            "diagnosticGate": gate_result,
            "rustToolchain": rust_toolchain,
            "upstreamPatchCount": len(upstream_applied),
            "completeAppliedPatchOrder": applied,
            "archive": archive,
            "buildLog": {"name": "build.log", "sha256": _sha(result_dir / "build.log"), "sizeBytes": (result_dir / "build.log").stat().st_size},
            "claims": {"compiled": True, "diagnosticOnly": True, "formalEligible": False, "windowsRuntimeObserved": False, "verified": False},
        }
        _write_json(result_dir / "build-result.json", result)
        return result
    except Exception as exc:
        if not log._closed:
            log.note(f"failed: {exc}")
            log.close()
        _write_json(
            result_dir / "build-failure.json",
            {
                "recordType": "verisilo-camoufox-r1-diag-build-failure/v2",
                "runId": args.run_id,
                "failedAtUtc": _utc_now(),
                "reason": str(exc),
                "browserLaunches": 0,
                "buildLog": {"name": "build.log", "sha256": _sha(result_dir / "build.log"), "sizeBytes": (result_dir / "build.log").stat().st_size},
            },
        )
        raise
    finally:
        log.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--moz-build-date", default=DEFAULT_MOZ_BUILD_DATE)
    args = parser.parse_args()
    try:
        execute(args)
    except BuildFailure as exc:
        print(f"build-failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
