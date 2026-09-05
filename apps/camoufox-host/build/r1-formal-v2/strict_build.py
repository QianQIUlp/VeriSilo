#!/usr/bin/env python3
"""Strict one-shot Formal R1 Windows-target Camoufox build driver.

The image fixes the source lock and complete patch order. It builds and
packages the candidate but never installs or launches the produced browser.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import stat
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
LOCK_REL = Path(
    "apps/camoufox-host/lock/"
    "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v2-source.json"
)
RECIPE_REL = Path("apps/camoufox-host/build/r1-formal-v2")
STRICT_BUILD_REL = RECIPE_REL / "strict_build.py"
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
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0003a-verisilo-gpc-preferences-namespace-compile-repair.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0004-verisilo-remove-worker-gpc-mask-override.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1/0005-verisilo-voices-final.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1/0006-verisilo-gpc-projection-after-user-prefs.patch",
]
EXPECTED_PATCH_BINDINGS = {
    "0000": ("8d407bdc4010f7b2989f206a70909bfa9ad89046ddb9e17fa76092c864433600", 1184),
    "0001": ("4fa6d3bbf203e2385e29a72ec2669ee17a571281be7ee2a73598e38918069b02", 2121),
    "0002": ("efb006d5b2b05756fc310b52eb48e0bdab5e8b23e780fa08534a7fc099c22ce7", 3059),
    "0003": ("3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b", 2774),
    "0003a": ("c2f9a9f88ba8aeb610eb1cb29f2515f1d79fcf582393397a571bc3206889588c", 500),
    "0004": ("5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185", 412),
    "0005": ("998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec", 344),
    "0006": ("bafc1e422049866bb2d053bc90bb8c70f01860cfa9d9f8c474277e1ff5c0ca7c", 859),
}


class BuildFailure(RuntimeError):
    """Typed fail-closed source or build boundary."""


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


def _recipe_file(lock: dict, relative: str) -> dict:
    for item in lock["buildBinding"]["recipe"]["files"]:
        if item["path"] == relative:
            return item
    raise BuildFailure(f"recipe file is not bound by the source lock: {relative}")


def _validate_recipe_and_mode(lock: dict) -> None:
    if lock.get("schema") != "verisilo-r1-formal-source-binding/v1":
        raise BuildFailure("unexpected Formal R1 source-lock schema")
    if (
        lock.get("engineRevision")
        != "verisilo-camoufox-152.0.4-beta.28-r1-formal-v2"
        or lock.get("buildMode") != "formal"
        or lock.get("diagnosticOnly") is not False
    ):
        raise BuildFailure("Formal R1 source/build claim boundary is not exact")
    optional_claims = {
        "formalSource": True,
        "formalR1Passed": False,
        "browserLaunches": 0,
        "windowsRuntimeObserved": False,
        "runtimeVerified": False,
    }
    if any(key in lock and lock[key] != value for key, value in optional_claims.items()):
        raise BuildFailure("Formal R1 source/build claim boundary is not exact")
    binding = lock.get("buildBinding")
    if type(binding) is not dict:
        raise BuildFailure("Formal R1 build binding is missing")
    recipe = binding.get("recipe")
    if type(recipe) is not dict:
        raise BuildFailure("Formal R1 recipe binding is malformed")
    if recipe.get("name") != "camoufox-152.0.4-beta.28-r1-formal-v2":
        raise BuildFailure("Formal R1 recipe name is not exact")
    if recipe.get("fixedEnvironment") != EXPECTED_FIXED_ENVIRONMENT:
        raise BuildFailure("Formal R1 fixed environment is not exact")
    expected_paths = [
        (RECIPE_REL / "Dockerfile").as_posix(),
        STRICT_BUILD_REL.as_posix(),
    ]
    if [item.get("path") for item in recipe.get("files", [])] != expected_paths:
        raise BuildFailure("Formal R1 recipe file order is not exact")
    for relative in expected_paths:
        item = _recipe_file(lock, relative)
        path = VERISILO_ROOT / relative
        if (
            not path.is_file()
            or path.stat().st_size != item.get("sizeBytes")
            or _sha(path) != item.get("sha256")
        ):
            raise BuildFailure(f"Formal R1 recipe file mismatch: {relative}")
    if _sha(Path(__file__)) != _recipe_file(
        lock, STRICT_BUILD_REL.as_posix()
    )["sha256"]:
        raise BuildFailure("embedded strict driver differs from the locked recipe")
    dockerfile = VERISILO_ROOT / (RECIPE_REL / "Dockerfile")
    expected_from = "FROM ubuntu:24.04@" + EXPECTED_BASE_INDEX_DIGEST
    if dockerfile.read_text(encoding="utf-8").splitlines()[0] != expected_from:
        raise BuildFailure("Formal R1 Dockerfile base digest is not exact")


def _validate_patch_contract(lock: dict) -> None:
    order = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005", "0006"]
    complete = lock.get("completePatchSeries")
    if (
        type(complete) is not list
        or not all(type(item) is dict for item in complete)
        or [item.get("id") for item in complete] != order
        or [item.get("path") for item in complete]
        != EXPECTED_COMPLETE_PATCH_PATHS
        or lock.get("completeAppliedPatchOrder") != order
    ):
        raise BuildFailure("Formal R1 patch series/order is not exact")
    downstream = lock.get("sourceInputs", {}).get("downstreamPatches")
    if (
        type(downstream) is not list
        or [item.get("path") for item in downstream]
        != EXPECTED_COMPLETE_PATCH_PATHS
    ):
        raise BuildFailure("Formal R1 downstream patch paths are not exact")
    incremental = lock.get("r1IncrementalPatches")
    if incremental is not None and (
        type(incremental) is not list
        or [item.get("id") for item in incremental] != ["0003", "0003a", "0004", "0005", "0006"]
        or [item.get("path") for item in incremental] != EXPECTED_COMPLETE_PATCH_PATHS[3:]
    ):
        raise BuildFailure("Formal R1 incremental patch order is not exact")
    if any(
        ("diagnosticOnly" in item and item.get("diagnosticOnly") is not False)
        for item in complete
    ):
        raise BuildFailure("Formal R1 patch series contains diagnostics")
    if any(
        (item.get("sha256"), item.get("sizeBytes"))
        != EXPECTED_PATCH_BINDINGS[item["id"]]
        for item in complete
    ):
        raise BuildFailure("Formal R1 patch binding drifted")
    seams = lock.get("patchSeams")
    if (
        type(seams) is not list
        or {item.get("id") for item in seams} != set(order)
        or len({(item.get("id"), item.get("path")) for item in seams}) != len(seams)
    ):
        raise BuildFailure("Formal R1 seam binding is not exact")
    voice = [item for item in seams if item.get("id") == "0005"]
    if voice != [
        {
            "id": "0005",
            "path": "dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp",
            "preSha256": "c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f",
            "postSha256": "c43447ff66ad5b03b21a9c76d0202c23a699904868a282f2d53e63e01227093e",
        }
    ]:
        raise BuildFailure("Formal R1 0005 seam binding is not exact")
    gpc_timing = [item for item in seams if item.get("id") == "0006"]
    if gpc_timing != [
        {
            "id": "0006",
            "path": "toolkit/xre/nsAppRunner.cpp",
            "preSha256": "7847e88093beeff74aa8a7e89f5e5f1e3ea0d6b1f9dece21f97387940fbe8b94",
            "postSha256": "51252fbfa75731f63a5b5a3f1134a252b370c151aaaa1135a6df930ff9ba23b3",
        }
    ]:
        raise BuildFailure("Formal R1 0006 seam binding is not exact")


def _locked_relative_path(value: object, label: str) -> PurePosixPath:
    if type(value) is not str or not value or value.startswith("/"):
        raise BuildFailure(f"{label} must be a safe relative POSIX path")
    if "\\" in value or "\x00" in value or "\r" in value or "\n" in value:
        raise BuildFailure(f"{label} must be a safe relative POSIX path")
    parts = value.split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise BuildFailure(f"{label} must be a canonical relative POSIX path")
    relative = PurePosixPath(value)
    if relative.as_posix() != value:
        raise BuildFailure(f"{label} must be a canonical relative POSIX path")
    return relative


def _real_directory(root: Path, relative: PurePosixPath, label: str) -> Path:
    cursor = root
    paths = [root]
    for part in relative.parts:
        cursor /= part
        paths.append(cursor)
    for path in paths:
        try:
            metadata = path.lstat()
        except FileNotFoundError as exc:
            raise BuildFailure(f"{label} directory is missing") from exc
        if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            raise BuildFailure(f"{label} must be a real directory")
    return cursor


def _exact_directory_names(parent: Path, expected: object, label: str) -> None:
    if (
        type(expected) is not list
        or not expected
        or any(type(name) is not str or not name for name in expected)
        or expected != sorted(set(expected))
    ):
        raise BuildFailure(f"{label} directory-name lock is malformed")
    try:
        entries = sorted(os.scandir(parent), key=lambda entry: entry.name)
    except (FileNotFoundError, NotADirectoryError) as exc:
        raise BuildFailure(f"{label} version root is missing") from exc
    actual: list[str] = []
    for entry in entries:
        metadata = entry.stat(follow_symlinks=False)
        if not stat.S_ISDIR(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            raise BuildFailure(f"{label} version root contains a non-directory")
        actual.append(entry.name)
    if actual != expected:
        raise BuildFailure(f"{label} directory versions differ from the source lock")


def _verify_windows_toolchain_manifest(lock: dict, source: Path) -> dict:
    manifest = lock["buildBinding"]["windowsToolchain"]["selectionManifest"]
    if type(manifest) is not dict:
        raise BuildFailure("Windows toolchain selection manifest binding is malformed")
    if set(manifest) != {"path", "sha256", "size"}:
        raise BuildFailure("Windows toolchain selection manifest binding is malformed")
    relative = _locked_relative_path(
        manifest["path"], "Windows toolchain selection manifest"
    )
    parent = _real_directory(
        source, relative.parent, "Windows toolchain selection manifest parent"
    )
    path = parent / relative.name
    try:
        metadata = path.lstat()
    except FileNotFoundError as exc:
        raise BuildFailure("Windows toolchain selection manifest is missing") from exc
    if (
        not stat.S_ISREG(metadata.st_mode)
        or stat.S_ISLNK(metadata.st_mode)
        or metadata.st_size != manifest["size"]
        or _sha(path) != manifest["sha256"]
    ):
        raise BuildFailure("Windows toolchain selection manifest differs from the lock")
    return dict(manifest)


def _resolve_bound_windows_toolchain(lock: dict, mozbuild: Path) -> dict:
    binding = lock["buildBinding"].get("windowsToolchain")
    if type(binding) is not dict:
        raise BuildFailure("Windows toolchain binding is missing")
    compiler = binding.get("compiler")
    sdk = binding.get("windowsSdk")
    crt = binding.get("crt")
    if type(compiler) is not dict or type(sdk) is not dict or type(crt) is not dict:
        raise BuildFailure("Windows toolchain binding is malformed")

    compiler_path = _real_directory(
        mozbuild,
        _locked_relative_path(compiler.get("relativePath"), "MSVC compiler"),
        "MSVC compiler",
    )
    _exact_directory_names(
        compiler_path.parent, compiler.get("versionDirectoryNames"), "MSVC compiler"
    )
    if compiler_path.name != compiler.get("version"):
        raise BuildFailure("MSVC compiler version differs from the source lock")

    include_path = _real_directory(
        mozbuild,
        _locked_relative_path(sdk.get("includeRelativePath"), "Windows SDK include"),
        "Windows SDK include",
    )
    lib_path = _real_directory(
        mozbuild,
        _locked_relative_path(sdk.get("libRelativePath"), "Windows SDK library"),
        "Windows SDK library",
    )
    _exact_directory_names(
        include_path.parent,
        sdk.get("includeVersionDirectoryNames"),
        "Windows SDK include",
    )
    _exact_directory_names(
        lib_path.parent,
        sdk.get("libVersionDirectoryNames"),
        "Windows SDK library",
    )
    if include_path.name != sdk.get("version") or lib_path.name != sdk.get("version"):
        raise BuildFailure("Windows SDK version differs from the source lock")

    crt_path = _real_directory(
        mozbuild,
        _locked_relative_path(crt.get("relativePath"), "packaged CRT"),
        "packaged CRT",
    )
    if (
        crt_path.name != crt.get("family")
        or crt_path.parent.name != crt.get("architecture")
        or crt_path.parent.parent.name != crt.get("redistVersion")
    ):
        raise BuildFailure("packaged CRT path differs from the source lock")
    _exact_directory_names(
        crt_path.parent.parent.parent,
        crt.get("redistDirectoryNames"),
        "MSVC redistributable",
    )
    crt_files = _locked_crt_files(lock, {"MOZBUILD_STATE_PATH": str(mozbuild)})
    return {
        "crtFiles": crt_files,
        "evidence": {
            "compilerVersion": compiler["version"],
            "windowsSdkVersion": sdk["version"],
            "packagedCrt": {
                "relativePath": crt["relativePath"],
                "files": [
                    {
                        "path": path.name,
                        "sha256": _sha(path),
                        "sizeBytes": path.stat().st_size,
                    }
                    for path in crt_files
                ],
            },
            "selectionManifest": dict(binding["selectionManifest"]),
        },
    }


def _validate_builder_identity(lock: dict, environment: dict[str, str]) -> dict:
    if sys.platform != "linux" or platform.machine().lower() not in {
        "x86_64",
        "amd64",
    }:
        raise BuildFailure("builder must be a Linux x86_64 container")
    image_id = environment.get("VERISILO_BUILDER_IMAGE_ID", "")
    saved_archive_sha = environment.get("VERISILO_BUILDER_IMAGE_SAVE_SHA256", "")
    if re.fullmatch(r"sha256:[0-9a-f]{64}", image_id) is None:
        raise BuildFailure("builder image ID is not immutable")
    if saved_archive_sha and re.fullmatch(r"[0-9a-f]{64}", saved_archive_sha) is None:
        raise BuildFailure("builder image archive digest is not canonical")
    if (
        environment.get("VERISILO_BASE_IMAGE_INDEX_DIGEST")
        != EXPECTED_BASE_INDEX_DIGEST
        or environment.get("VERISILO_BASE_AMD64_MANIFEST_DIGEST")
        != EXPECTED_BASE_AMD64_MANIFEST_DIGEST
    ):
        raise BuildFailure("builder base image binding differs from the source lock")
    return {
        "imageId": image_id,
        "savedArchiveSha256": saved_archive_sha or None,
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


def _validate_checkout_inputs(
    lock: dict, environment: dict[str, str]
) -> dict:
    upstream = lock["upstream"]
    actual = {
        "commit": _git(UPSTREAM_REPO, "rev-parse", "HEAD"),
        "tree": _git(UPSTREAM_REPO, "rev-parse", "HEAD^{tree}"),
        "tag": _git(
            UPSTREAM_REPO,
            "rev-parse",
            f"refs/tags/{upstream['tag']}^{{commit}}",
        ),
        "status": _git(
            UPSTREAM_REPO,
            "status",
            "--short",
            "--untracked-files=all",
            "--ignored=matching",
        ),
    }
    if (
        actual["commit"] != upstream["commit"]
        or actual["tree"] != upstream["tree"]
        or actual["tag"] != upstream["commit"]
        or actual["status"]
    ):
        raise BuildFailure("upstream checkout differs from the Formal source lock")
    expected_upstream = [
        item["path"] for item in lock["sourceInputs"]["upstreamPatches"]
    ]
    if _ordered_upstream_patch_paths(UPSTREAM_REPO) != expected_upstream:
        raise BuildFailure("upstream patch order differs from the Formal source lock")
    for item in (
        lock["sourceInputs"]["upstreamPatches"]
        + lock["sourceInputs"]["recipeFiles"]
    ):
        path = UPSTREAM_REPO / item["path"]
        if (
            not path.is_file()
            or path.stat().st_size != item["sizeBytes"]
            or _sha(path) != item["sha256"]
        ):
            raise BuildFailure(f"upstream input digest mismatch: {item['path']}")
    firefox = lock["firefoxSource"]
    if (
        not FIREFOX_ARCHIVE.is_file()
        or FIREFOX_ARCHIVE.stat().st_size != firefox["sizeBytes"]
        or _sha(FIREFOX_ARCHIVE, "sha512") != firefox["sha512"]
    ):
        raise BuildFailure("Firefox source archive differs from the Formal source lock")

    verisilo_status = _git(
        VERISILO_ROOT,
        "status",
        "--short",
        "--untracked-files=all",
        "--ignored=matching",
    )
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
        raise BuildFailure("Formal R1 source lock differs from host binding")

    order = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005", "0006"]
    complete = lock["completeAppliedPatchOrder"]
    specs = {item["id"]: item for item in lock["completePatchSeries"]}
    if complete != order or set(specs) != set(order):
        raise BuildFailure("complete Formal R1 patch order is not exact")
    for patch_id in complete:
        item = specs[patch_id]
        path = VERISILO_ROOT / item["path"]
        if (
            not path.is_file()
            or path.stat().st_size != item["sizeBytes"]
            or _sha(path) != item["sha256"]
        ):
            raise BuildFailure(f"Formal R1 patch digest mismatch: {patch_id}")
        with path.open("rb") as stream:
            if stream.readline().rstrip(b"\r\n") == (
                b"# VERISILO-DIAGNOSTIC-MARKER: v1"
            ):
                raise BuildFailure("Formal R1 patch carries a diagnostic marker")
    return {
        "verisiloCommit": verisilo_commit,
        "verisiloTree": verisilo_tree,
        "sourceLockSha256": lock_sha,
        "upstreamCommit": actual["commit"],
        "upstreamTree": actual["tree"],
        "upstreamPatchCount": len(expected_upstream),
        "completeAppliedPatchOrder": complete,
    }


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
        raise BuildFailure(
            "run-id must be 8-64 lowercase letters, digits or hyphens"
        )
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
        log.note("Formal R1 source/patch contract accepted before extraction")
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
        log.run(
            ["make", "setup-minimal"],
            cwd=upstream,
            env=environment,
            label="setup-minimal",
        )
        source = upstream / EXPECTED_SOURCE_DIR
        if not source.is_dir():
            raise BuildFailure(
                "setup-minimal did not materialize the expected source"
            )
        toolchain_manifest = _verify_windows_toolchain_manifest(lock, source)
        log.run(
            ["make", "mozbootstrap"],
            cwd=upstream,
            env=environment,
            label="mozbootstrap",
        )
        rust_toolchain = _pin_rust_toolchain(
            lock, upstream, environment, log
        )
        log.run(
            [
                "python3",
                "scripts/patch.py",
                "--mozconfig-only",
                "152.0.4",
                "beta.28",
            ],
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
            "complete Formal downstream order applied: " + " -> ".join(applied)
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure(
                "Windows toolchain selection manifest changed during patching"
            )
        (source / "_READY").touch()
        log.run(
            [str(source / "mach"), "configure"],
            cwd=source,
            env=environment,
            label="configure",
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure(
                "Windows toolchain selection manifest changed during configure"
            )
        mozbuild = Path(environment["MOZBUILD_STATE_PATH"])
        toolchain_before = _resolve_bound_windows_toolchain(lock, mozbuild)
        log.run(
            [str(source / "mach"), "build"],
            cwd=source,
            env=environment,
            label="build-windows-x86_64",
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure(
                "Windows toolchain selection manifest changed during build"
            )
        toolchain_for_package = _resolve_bound_windows_toolchain(lock, mozbuild)
        if toolchain_for_package["evidence"] != toolchain_before["evidence"]:
            raise BuildFailure("bound Windows toolchain changed during build")
        log.run(
            _windows_package_command(toolchain_for_package["crtFiles"]),
            cwd=upstream,
            env=environment,
            label="package-windows-x86_64",
        )
        if _verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure(
                "Windows toolchain selection manifest changed during package"
            )
        toolchain_after = _resolve_bound_windows_toolchain(lock, mozbuild)
        if toolchain_after["evidence"] != toolchain_before["evidence"]:
            raise BuildFailure("bound Windows toolchain changed during package")
        archive = _freeze_archive(lock, upstream, result_dir)
        log.note(f"Formal candidate archive frozen: sha256={archive['sha256']}")
        log.close()
        result = {
            "recordType": "verisilo-camoufox-r1-formal-build-run/v1",
            "runId": args.run_id,
            "startedAtUtc": started,
            "completedAtUtc": _utc_now(),
            "engineRevision": lock["engineRevision"],
            "buildMode": "formal",
            "diagnosticOnly": False,
            "formalSource": True,
            "formalR1Passed": False,
            "browserLaunches": 0,
            "windowsRuntimeObserved": False,
            "runtimeVerified": False,
            "builder": builder,
            "resourcesAtStart": resources,
            "inputs": inputs,
            "rustToolchain": rust_toolchain,
            "windowsToolchain": toolchain_after["evidence"],
            "upstreamPatchCount": len(upstream_applied),
            "completeAppliedPatchOrder": applied,
            "archive": archive,
            "buildLog": {
                "name": "build.log",
                "sha256": _sha(result_dir / "build.log"),
                "sizeBytes": (result_dir / "build.log").stat().st_size,
            },
            "claims": {
                "compiled": True,
                "formalSource": True,
                "browserLaunches": 0,
                "formalR1Passed": False,
                "windowsRuntimeObserved": False,
                "runtimeVerified": False,
            },
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
                "recordType": "verisilo-camoufox-r1-formal-build-failure/v1",
                "runId": args.run_id,
                "failedAtUtc": _utc_now(),
                "reason": str(exc),
                "browserLaunches": 0,
                "windowsRuntimeObserved": False,
                "runtimeVerified": False,
                "buildLog": {
                    "name": "build.log",
                    "sha256": _sha(result_dir / "build.log"),
                    "sizeBytes": (result_dir / "build.log").stat().st_size,
                },
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
