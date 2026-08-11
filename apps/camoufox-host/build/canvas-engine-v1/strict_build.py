#!/usr/bin/env python3
"""Strict one-shot FF152/Camoufox Windows x86_64 build driver.

The OCI image supplies only the Linux build environment. Exact Camoufox,
Firefox, VeriSilo and output directories are mounted at the fixed paths below.
The driver refuses reused run directories, validates every bound input before
extraction, applies the 50 upstream patches with checked return codes, applies
the FF152 MIDL compatibility patch before the Canvas identity patch, and
records the candidate archive and build provenance. It does not install,
launch or verify the resulting browser.
"""

from __future__ import annotations

import argparse
import configparser
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
    "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json"
)
DOWNSTREAM_PATCH_REL = Path(
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/"
    "0001-verisilo-canvas-export-key.patch"
)
MIDL_COMPAT_PATCH_REL = Path(
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/"
    "0000-verisilo-ff152-midl-cross-build-input.patch"
)
STRICT_BUILD_PATH_REL = Path(
    "apps/camoufox-host/build/canvas-engine-v1/strict_build.py"
)

EXPECTED_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1"
EXPECTED_UPSTREAM_COMMIT = "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d"
EXPECTED_UPSTREAM_TREE = "1435d544d9b61dee7fcf74cf92462952ca43d38e"
EXPECTED_BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
EXPECTED_BASE_AMD64_MANIFEST_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
EXPECTED_OUTPUT = "camoufox-152.0.4-beta.28-win.x86_64.zip"
EXPECTED_SOURCE_DIR = "camoufox-152.0.4-beta.28"
EXPECTED_VERSION = "152.0.4"
EXPECTED_RELEASE = "beta.28"
DEFAULT_MOZ_BUILD_DATE = "20260811045234"
EXPECTED_UPSTREAM_PATCH_PROGRAM = "GNU patch 2.7.6"
EXPECTED_UPSTREAM_PATCH_COMMAND = [
    "patch",
    "-p1",
    "--batch",
    "--binary",
    "--forward",
    "--ignore-whitespace",
    "--fuzz=2",
    "--no-backup-if-mismatch",
    "-i",
    "{patch}",
]


class BuildFailure(RuntimeError):
    """A typed fail-closed source or build boundary."""


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


def _canonical_tree(root: Path) -> dict:
    rows: list[dict] = []
    total = 0
    if not root.is_dir():
        raise BuildFailure(f"bound input tree is missing: {root.name}")
    files = sorted(
        (path for path in root.rglob("*") if path.is_file()),
        key=lambda path: path.relative_to(root).as_posix(),
    )
    for path in files:
        if path.is_symlink():
            raise BuildFailure(f"bound input tree contains a symlink: {path.name}")
        data = path.read_bytes()
        total += len(data)
        rows.append(
            {
                "path": path.relative_to(root).as_posix(),
                "size": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    encoded = json.dumps(
        rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return {
        "fileCount": len(rows),
        "totalBytes": total,
        "canonicalTreeSha256": hashlib.sha256(encoded).hexdigest(),
    }


def _provenance_tree_rows(root: Path) -> list[dict]:
    if not root.is_dir() or root.is_symlink():
        raise BuildFailure(f"provenance tree root is not a real directory: {root}")
    rows: list[dict] = []

    def visit(directory: Path, relative: Path) -> None:
        with os.scandir(directory) as stream:
            entries = sorted(stream, key=lambda entry: entry.name)
        for entry in entries:
            entry_relative = relative / entry.name
            path_text = entry_relative.as_posix()
            metadata = entry.stat(follow_symlinks=False)
            row: dict[str, object] = {
                "mode": stat.S_IMODE(metadata.st_mode),
                "path": path_text,
            }
            if stat.S_ISLNK(metadata.st_mode):
                row.update({"target": os.readlink(entry.path), "type": "symlink"})
            elif stat.S_ISDIR(metadata.st_mode):
                row["type"] = "directory"
                rows.append(row)
                visit(Path(entry.path), entry_relative)
                continue
            elif stat.S_ISREG(metadata.st_mode):
                file_path = Path(entry.path)
                row.update(
                    {
                        "sha256": _sha(file_path),
                        "size": metadata.st_size,
                        "type": "file",
                    }
                )
            else:
                raise BuildFailure(f"unsupported entry in provenance tree: {path_text}")
            rows.append(row)

    visit(root, Path())
    return rows


def _freeze_provenance_tree(root: Path, destination: Path) -> dict:
    rows = _provenance_tree_rows(root)
    encoded = json.dumps(
        rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    destination.write_bytes(encoded + b"\n")
    return {
        "directoryCount": sum(row["type"] == "directory" for row in rows),
        "fileCount": sum(row["type"] == "file" for row in rows),
        "manifest": destination.name,
        "manifestSha256": hashlib.sha256(encoded + b"\n").hexdigest(),
        "symlinkCount": sum(row["type"] == "symlink" for row in rows),
        "totalFileBytes": sum(
            int(row.get("size", 0)) for row in rows if row["type"] == "file"
        ),
    }


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
        [
            "git",
            "-c",
            f"safe.directory={repo}",
            "-C",
            str(repo),
            *arguments,
        ]
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
            raise BuildFailure("cannot append to the closed build log")
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
        returncode = process.wait()
        if returncode != 0:
            raise BuildFailure(f"{label} failed with exit code {returncode}")
        self.note(f"success {label}")


def _decode_mount_field(value: str) -> str:
    return re.sub(
        r"\\([0-7]{3})",
        lambda match: chr(int(match.group(1), 8)),
        value,
    )


def _mount_table() -> dict[str, dict[str, object]]:
    result: dict[str, dict[str, object]] = {}
    for line in Path("/proc/self/mountinfo").read_text(encoding="utf-8").splitlines():
        fields = line.split()
        try:
            separator = fields.index("-")
        except ValueError as exc:
            raise BuildFailure("invalid /proc/self/mountinfo entry") from exc
        if separator < 6 or len(fields) <= separator + 3:
            raise BuildFailure("truncated /proc/self/mountinfo entry")
        mountpoint = _decode_mount_field(fields[4])
        result[mountpoint] = {
            "mountId": fields[0],
            "device": fields[2],
            "root": _decode_mount_field(fields[3]),
            "options": sorted(fields[5].split(",")),
            "filesystem": fields[separator + 1],
            "source": _decode_mount_field(fields[separator + 2]),
            "superOptions": sorted(fields[separator + 3].split(",")),
        }
    return result


def validate_mounts() -> dict[str, dict[str, object]]:
    mounts = _mount_table()
    required = {
        INPUT_ROOT: True,
        BUILD_HOME: False,
        WORK_ROOT: False,
        OUT_ROOT: False,
    }
    selected: dict[str, dict[str, object]] = {}
    for path, read_only in required.items():
        if path.as_posix() not in mounts:
            raise BuildFailure(f"required path is not an exact bind mount: {path}")
        entry = mounts[path.as_posix()]
        options = set(entry["options"])
        if read_only and "ro" not in options:
            raise BuildFailure(f"input mount must be read-only: {path}")
        if not read_only and "rw" not in options:
            raise BuildFailure(f"run-owned mount must be read-write: {path}")
        selected[path.as_posix()] = entry

    run_mounts = [selected[path.as_posix()] for path in (BUILD_HOME, WORK_ROOT, OUT_ROOT)]
    identities = {
        (entry["device"], entry["root"], entry["source"]) for entry in run_mounts
    }
    if len(identities) != len(run_mounts):
        raise BuildFailure("build-home, work and out must be distinct bind mounts")

    expected_inputs = {
        "firefox-152.0.4.source.tar.xz",
        "upstream",
        "verisilo",
    }
    if {path.name for path in INPUT_ROOT.iterdir()} != expected_inputs:
        raise BuildFailure("/inputs must contain exactly the three frozen inputs")
    for path in (BUILD_HOME, WORK_ROOT, OUT_ROOT):
        if path.is_symlink() or any(path.iterdir()):
            raise BuildFailure(f"run-owned mount must start empty: {path}")
    return selected


def ordered_upstream_patch_paths(upstream: Path) -> list[str]:
    paths = [
        path.relative_to(upstream).as_posix()
        for path in (upstream / "patches").rglob("*.patch")
        if path.is_file()
    ]
    paths.sort(key=lambda value: Path(value).name)
    normal = [path for path in paths if "roverfox" not in Path(path).parts]
    roverfox = [path for path in paths if "roverfox" in Path(path).parts]
    return normal + roverfox


def upstream_patch_command(patch_path: Path) -> list[str]:
    return [
        *EXPECTED_UPSTREAM_PATCH_COMMAND[:-2],
        "-i",
        str(patch_path),
    ]


def downstream_patch_command(patch_path: Path) -> list[str]:
    return [
        "patch",
        "-p1",
        "--batch",
        "--binary",
        "--forward",
        "--fuzz=0",
        "--no-backup-if-mismatch",
        "-i",
        str(patch_path),
    ]


def _safe_patch_surface_path(value: str) -> str:
    if (
        not value
        or value.startswith("/")
        or "\\" in value
        or "\x00" in value
        or "\r" in value
        or "\n" in value
    ):
        raise BuildFailure("upstream patch contains an unsafe surface path")
    parts = value.split("/")
    if any(part in {"", ".", ".."} for part in parts):
        raise BuildFailure("upstream patch surface path is not canonical")
    if PurePosixPath(value).as_posix() != value:
        raise BuildFailure("upstream patch surface path is not POSIX-canonical")
    return value


def _patch_header_path(line: bytes, prefix: bytes) -> str | None:
    if not line.startswith(prefix):
        return None
    raw = line[len(prefix) :]
    if b"\t" in raw or raw.startswith(b'"'):
        raise BuildFailure("upstream patch uses an unsupported file header")
    try:
        value = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise BuildFailure("upstream patch file header is not UTF-8") from exc
    if value == "/dev/null":
        return value
    expected_root = "a/" if prefix == b"--- " else "b/"
    if not value.startswith(expected_root):
        raise BuildFailure("upstream patch file header has an invalid root")
    return _safe_patch_surface_path(value[2:])


def upstream_patch_surface(lock: dict, upstream: Path) -> tuple[list[str], int, str]:
    application = lock["sourceInputs"].get("upstreamPatchApplication")
    if type(application) is not dict:
        raise BuildFailure("source lock has no upstream patch application contract")
    expected_keys = {
        "command",
        "createdPathCount",
        "debrisBaselineCanonicalization",
        "headerPairCount",
        "pathListCanonicalization",
        "postPatchSurface",
        "prePatchDebrisBaseline",
        "prePatchSurface",
        "programVersion",
        "surfaceCanonicalization",
        "surfacePathCount",
        "surfacePathListSha256",
    }
    if set(application) != expected_keys:
        raise BuildFailure("upstream patch application contract field set is not exact")
    if application.get("programVersion") != EXPECTED_UPSTREAM_PATCH_PROGRAM:
        raise BuildFailure("upstream patch program version contract is not exact")
    if application.get("command") != EXPECTED_UPSTREAM_PATCH_COMMAND:
        raise BuildFailure("upstream patch command contract is not exact")
    if application.get("pathListCanonicalization") != (
        "ordinal-sorted safe relative POSIX paths, UTF-8, each LF-terminated"
    ):
        raise BuildFailure("upstream patch path canonicalization is not exact")
    if application.get("surfaceCanonicalization") != (
        "same path order; compact sorted-key UTF-8 JSON array; file rows contain "
        "path,sha256,size,type=file; absent rows contain path,type=absent; mode and "
        "mtime excluded"
    ):
        raise BuildFailure("upstream patch surface canonicalization is not exact")
    if application.get("debrisBaselineCanonicalization") != (
        "ordinal-sorted relative POSIX paths; orig path list is UTF-8 with each "
        "path LF-terminated; canonical orig state is a compact sorted-key UTF-8 "
        "JSON array with path,sha256,size; mode and mtime excluded"
    ):
        raise BuildFailure("upstream patch debris canonicalization is not exact")

    paths: set[str] = set()
    created_paths: set[str] = set()
    header_pairs = 0
    for item in lock["sourceInputs"]["upstreamPatches"]:
        patch_path = upstream / item["path"]
        pending_old: str | None = None
        for line in patch_path.read_bytes().splitlines():
            old_path = _patch_header_path(line, b"--- ")
            if old_path is not None:
                if pending_old is not None:
                    raise BuildFailure("upstream patch has unpaired old file headers")
                pending_old = old_path
                continue
            new_path = _patch_header_path(line, b"+++ ")
            if new_path is None:
                if pending_old is not None:
                    raise BuildFailure(
                        "upstream patch file headers are not directly paired"
                    )
                continue
            if pending_old is None:
                raise BuildFailure("upstream patch has an unpaired new file header")
            if pending_old == "/dev/null" and new_path == "/dev/null":
                raise BuildFailure("upstream patch has an empty file-header pair")
            if pending_old != "/dev/null" and new_path != "/dev/null":
                if pending_old != new_path:
                    raise BuildFailure("upstream patch rename is not supported")
                paths.add(pending_old)
            elif pending_old == "/dev/null":
                paths.add(new_path)
                created_paths.add(new_path)
            else:
                raise BuildFailure("upstream patch deletion is not supported")
            header_pairs += 1
            pending_old = None
        if pending_old is not None:
            raise BuildFailure("upstream patch ends with an unpaired old file header")

    ordered = sorted(paths)
    encoded = "".join(f"{path}\n" for path in ordered).encode("utf-8")
    path_list_sha256 = hashlib.sha256(encoded).hexdigest()
    if header_pairs != application.get("headerPairCount"):
        raise BuildFailure("upstream patch header-pair count differs from the lock")
    if len(created_paths) != application.get("createdPathCount"):
        raise BuildFailure("upstream patch created-path count differs from the lock")
    if len(ordered) != application.get("surfacePathCount"):
        raise BuildFailure("upstream patch surface path count differs from the lock")
    if path_list_sha256 != application.get("surfacePathListSha256"):
        raise BuildFailure("upstream patch surface path list differs from the lock")
    return ordered, header_pairs, path_list_sha256


def patch_surface_state(source: Path, paths: list[str]) -> dict:
    source_root = source.resolve(strict=True)
    rows: list[dict] = []
    total = 0
    file_count = 0
    absent_count = 0
    for value in paths:
        safe_value = _safe_patch_surface_path(value)
        relative = PurePosixPath(safe_value)
        candidate = source.joinpath(*relative.parts)
        cursor = source
        for part in relative.parts[:-1]:
            cursor /= part
            if cursor.is_symlink():
                raise BuildFailure("upstream patch surface has a symlink ancestor")
            if not cursor.exists():
                break
        try:
            candidate.resolve(strict=False).relative_to(source_root)
        except ValueError as exc:
            raise BuildFailure("upstream patch surface escapes the source root") from exc
        try:
            metadata = candidate.lstat()
        except FileNotFoundError:
            absent_count += 1
            rows.append({"path": safe_value, "type": "absent"})
            continue
        if not stat.S_ISREG(metadata.st_mode):
            raise BuildFailure("upstream patch surface contains a non-regular file")
        data = candidate.read_bytes()
        total += len(data)
        file_count += 1
        rows.append(
            {
                "path": safe_value,
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
                "type": "file",
            }
        )
    encoded = json.dumps(
        rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return {
        "absentCount": absent_count,
        "canonicalSurfaceSha256": hashlib.sha256(encoded).hexdigest(),
        "fileCount": file_count,
        "surfacePathCount": len(paths),
        "totalFileBytes": total,
    }


def verify_patch_surface(lock: dict, source: Path, paths: list[str], field: str) -> None:
    application = lock["sourceInputs"]["upstreamPatchApplication"]
    expected = application.get(field)
    if type(expected) is not dict or patch_surface_state(source, paths) != expected:
        raise BuildFailure(f"upstream patch surface mismatch at {field}")


def verify_patch_program(environment: dict[str, str], source: Path) -> None:
    completed = subprocess.run(
        ["patch", "--version"],
        cwd=source,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    first_line = completed.stdout.splitlines()[0] if completed.stdout else ""
    if completed.returncode != 0 or first_line != EXPECTED_UPSTREAM_PATCH_PROGRAM:
        raise BuildFailure("upstream patch program differs from the source lock")


def validate_builder_identity(
    lock: dict, environment: dict[str, str], machine: str
) -> dict:
    normalized_machine = machine.lower()
    if sys.platform != "linux" or normalized_machine not in {"x86_64", "amd64"}:
        raise BuildFailure("builder must be a Linux x86_64 OCI container")
    image_id = environment.get("VERISILO_BUILDER_IMAGE_ID", "")
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", image_id):
        raise BuildFailure("VERISILO_BUILDER_IMAGE_ID must be an immutable sha256 ID")
    image_archive_sha256 = environment.get("VERISILO_BUILDER_IMAGE_SAVE_SHA256", "")
    if not re.fullmatch(r"[0-9a-f]{64}", image_archive_sha256):
        raise BuildFailure("builder image archive SHA-256 is missing or invalid")
    index_digest = environment.get("VERISILO_BASE_IMAGE_INDEX_DIGEST")
    manifest_digest = environment.get("VERISILO_BASE_AMD64_MANIFEST_DIGEST")
    if index_digest != EXPECTED_BASE_INDEX_DIGEST:
        raise BuildFailure("Ubuntu OCI index digest does not match the frozen recipe")
    if manifest_digest != EXPECTED_BASE_AMD64_MANIFEST_DIGEST:
        raise BuildFailure("Ubuntu linux/amd64 manifest digest does not match the recipe")
    binding = lock["buildBinding"].get("builderImageBinding")
    if type(binding) is not dict:
        raise BuildFailure("source lock has no frozen builder image binding")
    required_fields = set(
        lock["buildBinding"].get("builderImageBindingRequiredFields", [])
    )
    if set(binding) != required_fields:
        raise BuildFailure("source lock builder image field set is not exact")
    if image_id != binding.get("imageId"):
        raise BuildFailure("builder image ID differs from the source lock")
    if image_archive_sha256 != binding.get("savedArchiveSha256"):
        raise BuildFailure("builder image archive differs from the source lock")
    return {
        "imageId": image_id,
        "imageArchiveSha256": image_archive_sha256,
        "baseIndexDigest": index_digest,
        "baseLinuxAmd64ManifestDigest": manifest_digest,
        "machine": normalized_machine,
    }


def _meminfo_bytes(key: str) -> int:
    for line in Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
        name, _, value = line.partition(":")
        if name == key:
            match = re.fullmatch(r"\s*(\d+)\s+kB\s*", value)
            if not match:
                break
            return int(match.group(1)) * 1024
    raise BuildFailure(f"cannot read {key} from /proc/meminfo")


def validate_resources(lock: dict, work_root: Path) -> dict:
    gate = lock["buildBinding"]["resourceGate"]
    free = shutil.disk_usage(work_root).free
    swap = _meminfo_bytes("SwapTotal")
    cpu = os.cpu_count() or 0
    if free < gate["minimumFreeBytes"]:
        raise BuildFailure(
            f"builder free space {free} is below {gate['minimumFreeBytes']}"
        )
    if swap < gate["minimumSwapBytes"]:
        raise BuildFailure(f"builder swap {swap} is below {gate['minimumSwapBytes']}")
    if cpu < gate["minimumLogicalCpu"]:
        raise BuildFailure(f"builder CPU count {cpu} is below the frozen minimum")
    return {"freeBytes": free, "swapBytes": swap, "logicalCpu": cpu}


def validate_bound_inputs(lock: dict, environment: dict[str, str]) -> dict:
    if lock.get("engineRevision") != EXPECTED_ENGINE_REVISION:
        raise BuildFailure("unexpected source binding engine revision")
    upstream = lock["upstream"]
    if upstream.get("commit") != EXPECTED_UPSTREAM_COMMIT:
        raise BuildFailure("unexpected upstream commit in source lock")
    if upstream.get("tree") != EXPECTED_UPSTREAM_TREE:
        raise BuildFailure("unexpected upstream tree in source lock")

    actual_head = _git(UPSTREAM_REPO, "rev-parse", "HEAD")
    actual_tree = _git(UPSTREAM_REPO, "rev-parse", "HEAD^{tree}")
    actual_tag = _git(
        UPSTREAM_REPO, "rev-parse", f"refs/tags/{upstream['tag']}^{{commit}}"
    )
    status = _git(
        UPSTREAM_REPO,
        "status",
        "--short",
        "--untracked-files=all",
        "--ignored=matching",
    )
    if (actual_head, actual_tree, actual_tag) != (
        upstream["commit"],
        upstream["tree"],
        upstream["commit"],
    ):
        raise BuildFailure("upstream checkout commit/tree/tag mismatch")
    if status:
        raise BuildFailure("upstream checkout is not completely clean")

    inputs = lock["sourceInputs"]
    expected_patch_paths = [item["path"] for item in inputs["upstreamPatches"]]
    if ordered_upstream_patch_paths(UPSTREAM_REPO) != expected_patch_paths:
        raise BuildFailure("upstream patch closure/order differs from the source lock")
    for item in inputs["upstreamPatches"] + inputs["recipeFiles"]:
        path = UPSTREAM_REPO / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"]:
            raise BuildFailure(f"bound upstream file size mismatch: {item['path']}")
        if _sha(path) != item["sha256"]:
            raise BuildFailure(f"bound upstream file digest mismatch: {item['path']}")
    for item in inputs["inputTrees"]:
        actual = _canonical_tree(UPSTREAM_REPO / item["path"])
        expected = {
            "fileCount": item["fileCount"],
            "totalBytes": item["totalBytes"],
            "canonicalTreeSha256": item["canonicalTreeSha256"],
        }
        if actual != expected:
            raise BuildFailure(f"bound upstream tree mismatch: {item['path']}")

    firefox = lock["firefoxSource"]
    if (
        not FIREFOX_ARCHIVE.is_file()
        or FIREFOX_ARCHIVE.stat().st_size != firefox["sizeBytes"]
        or _sha(FIREFOX_ARCHIVE, "sha512") != firefox["sha512"]
    ):
        raise BuildFailure("Firefox source archive size/SHA-512 mismatch")

    downstream_patches = inputs.get("downstreamPatches")
    expected_downstream_paths = [
        MIDL_COMPAT_PATCH_REL.as_posix(),
        DOWNSTREAM_PATCH_REL.as_posix(),
    ]
    if (
        type(downstream_patches) is not list
        or any(
            type(item) is not dict
            or set(item) != {"applyAfterUpstream", "path", "sha256", "sizeBytes"}
            or item["applyAfterUpstream"] is not True
            for item in downstream_patches
        )
    ):
        raise BuildFailure("VeriSilo downstream patch order/contract mismatch")
    if [item["path"] for item in downstream_patches] != expected_downstream_paths:
        raise BuildFailure("VeriSilo downstream patch order/contract mismatch")
    for item in downstream_patches:
        path = VERISILO_ROOT / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"]:
            raise BuildFailure("VeriSilo downstream patch size mismatch")
        if _sha(path) != item["sha256"]:
            raise BuildFailure("VeriSilo downstream patch digest mismatch")

    for item in lock["buildBinding"]["recipe"]["files"]:
        path = VERISILO_ROOT / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"]:
            raise BuildFailure(f"build recipe file size mismatch: {item['path']}")
        if _sha(path) != item["sha256"]:
            raise BuildFailure(f"build recipe file digest mismatch: {item['path']}")

    executed_driver = Path(__file__).resolve()
    if _sha(executed_driver) != _sha(VERISILO_ROOT / STRICT_BUILD_PATH_REL):
        raise BuildFailure("image-embedded build driver differs from the frozen recipe")

    verisilo_head = _git(VERISILO_ROOT, "rev-parse", "HEAD")
    verisilo_tree = _git(VERISILO_ROOT, "rev-parse", "HEAD^{tree}")
    verisilo_status = _git(
        VERISILO_ROOT,
        "status",
        "--short",
        "--untracked-files=all",
        "--ignored=matching",
    )
    if verisilo_status:
        raise BuildFailure("VeriSilo build checkout is not completely clean")
    if verisilo_head != environment.get("VERISILO_SOURCE_COMMIT"):
        raise BuildFailure("VeriSilo commit differs from the host launcher binding")
    if verisilo_tree != environment.get("VERISILO_SOURCE_TREE"):
        raise BuildFailure("VeriSilo tree differs from the host launcher binding")
    source_lock_sha256 = _sha(VERISILO_ROOT / LOCK_REL)
    if source_lock_sha256 != environment.get("VERISILO_SOURCE_LOCK_SHA256"):
        raise BuildFailure("source lock differs from the host launcher binding")

    return {
        "sourceLockSha256": source_lock_sha256,
        "verisiloCommit": verisilo_head,
        "verisiloTree": verisilo_tree,
        "firefoxArchiveSha512": firefox["sha512"],
        "downstreamPatches": [dict(item) for item in downstream_patches],
        "upstreamCommit": actual_head,
        "upstreamTree": actual_tree,
        "upstreamPatchCount": len(expected_patch_paths),
    }


def _safe_extract_tar(archive: Path, destination: Path) -> None:
    destination_resolved = destination.resolve()
    with tarfile.open(archive, "r:") as bundle:
        members = bundle.getmembers()
        for member in members:
            target = (destination / member.name).resolve()
            if target != destination_resolved and destination_resolved not in target.parents:
                raise BuildFailure("git archive member escapes the workspace")
            if not (member.isfile() or member.isdir()):
                raise BuildFailure("git archive contains a non-regular member")
        bundle.extractall(destination)


def export_upstream(workspace: Path, log: BuildLog) -> Path:
    archive = workspace / "upstream.tar"
    completed = subprocess.run(
        [
            "git",
            "-c",
            f"safe.directory={UPSTREAM_REPO}",
            "-C",
            str(UPSTREAM_REPO),
            "archive",
            "--format=tar",
            f"--output={archive}",
            EXPECTED_UPSTREAM_COMMIT,
        ],
        check=False,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=120,
    )
    if completed.returncode != 0:
        raise BuildFailure("failed to export the exact upstream commit")
    upstream = workspace / "upstream"
    upstream.mkdir()
    _safe_extract_tar(archive, upstream)
    log.note(f"exported exact upstream commit; archive sha256={_sha(archive)}")
    return upstream


def patch_debris_state(source: Path) -> dict:
    orig_files: list[dict] = []
    reject_paths: list[str] = []
    for path in source.rglob("*.orig"):
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise BuildFailure("patch backup baseline contains a non-regular file")
        data = path.read_bytes()
        orig_files.append(
            {
                "path": path.relative_to(source).as_posix(),
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
            }
        )
    for path in source.rglob("*.rej"):
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise BuildFailure("patch reject state contains a non-regular file")
        reject_paths.append(path.relative_to(source).as_posix())
    orig_files.sort(key=lambda row: row["path"])
    reject_paths.sort()
    return {"origFiles": orig_files, "rejectPaths": reject_paths}


def patch_debris_summary(state: dict) -> dict:
    orig_files = state["origFiles"]
    reject_paths = state["rejectPaths"]
    path_bytes = "".join(f"{row['path']}\n" for row in orig_files).encode("utf-8")
    canonical = json.dumps(
        orig_files, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return {
        "canonicalOrigSha256": hashlib.sha256(canonical).hexdigest(),
        "origCount": len(orig_files),
        "origPathListSha256": hashlib.sha256(path_bytes).hexdigest(),
        "rejectCount": len(reject_paths),
        "totalOrigBytes": sum(row["size"] for row in orig_files),
    }


def capture_patch_debris_baseline(lock: dict, source: Path) -> dict:
    state = patch_debris_state(source)
    if state["rejectPaths"]:
        raise BuildFailure("pre-patch source contains reject files")
    expected = lock["sourceInputs"]["upstreamPatchApplication"].get(
        "prePatchDebrisBaseline"
    )
    if type(expected) is not dict or patch_debris_summary(state) != expected:
        raise BuildFailure("pre-patch backup baseline differs from the source lock")
    return state


def verify_patch_debris_unchanged(source: Path, baseline: dict) -> None:
    current = patch_debris_state(source)
    if current["rejectPaths"]:
        raise BuildFailure("patch application left reject files")
    if current["origFiles"] != baseline["origFiles"]:
        raise BuildFailure("patch application changed the exact backup baseline")


def _verify_seams(lock: dict, source: Path, field: str) -> None:
    for seam in lock["seamFiles"]:
        path = source / seam["path"]
        if not path.is_file() or _sha(path) != seam[field]:
            raise BuildFailure(f"Canvas seam mismatch at {field}: {seam['path']}")


def _verify_midl_compatibility_seams(lock: dict, source: Path, field: str) -> None:
    for seam in lock["midlCompatibilitySeamFiles"]:
        path = source / seam["path"]
        if not path.is_file() or _sha(path) != seam[field]:
            raise BuildFailure(
                f"FF152 MIDL compatibility seam mismatch at {field}: {seam['path']}"
            )


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


def _exact_directory_names(
    parent: Path, expected: object, label: str
) -> None:
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


def _locked_crt_rows(value: object) -> list[dict]:
    if type(value) is not list or not value:
        raise BuildFailure("packaged CRT file lock must be a non-empty array")
    rows: list[dict] = []
    for item in value:
        if type(item) is not dict or set(item) != {"path", "sha256", "size"}:
            raise BuildFailure("packaged CRT file lock is malformed")
        relative = _locked_relative_path(item["path"], "packaged CRT member")
        if len(relative.parts) != 1 or relative.suffix.lower() != ".dll":
            raise BuildFailure("packaged CRT members must be root DLL names")
        if type(item["size"]) is not int or item["size"] <= 0:
            raise BuildFailure("packaged CRT member size is invalid")
        if (
            type(item["sha256"]) is not str
            or re.fullmatch(r"[0-9a-f]{64}", item["sha256"]) is None
        ):
            raise BuildFailure("packaged CRT member SHA-256 is invalid")
        rows.append(dict(item))
    if [row["path"] for row in rows] != sorted(
        {str(row["path"]) for row in rows}
    ):
        raise BuildFailure("packaged CRT member lock must be unique and ordinal-sorted")
    return rows


def _exact_flat_tree(root: Path, expected_files: object, expected_tree: object) -> dict:
    rows = _locked_crt_rows(expected_files)
    if type(expected_tree) is not dict or set(expected_tree) != {
        "canonicalTreeSha256",
        "fileCount",
        "totalBytes",
    }:
        raise BuildFailure("packaged CRT tree lock is malformed")
    actual_rows: list[dict] = []
    for entry in sorted(os.scandir(root), key=lambda item: item.name):
        metadata = entry.stat(follow_symlinks=False)
        if not stat.S_ISREG(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
            raise BuildFailure("packaged CRT directory must contain only regular files")
        path = Path(entry.path)
        actual_rows.append(
            {
                "path": entry.name,
                "sha256": _sha(path),
                "size": metadata.st_size,
            }
        )
    if actual_rows != rows:
        raise BuildFailure("packaged CRT files differ from the source lock")
    actual_tree = _canonical_tree(root)
    if actual_tree != expected_tree:
        raise BuildFailure("packaged CRT tree differs from the source lock")
    return actual_tree


def verify_windows_toolchain_manifest(lock: dict, source: Path) -> dict:
    binding = lock["buildBinding"].get("windowsToolchain")
    if type(binding) is not dict:
        raise BuildFailure("Windows toolchain binding is missing")
    manifest = binding.get("selectionManifest")
    if type(manifest) is not dict or set(manifest) != {"path", "sha256", "size"}:
        raise BuildFailure("Windows toolchain selection manifest lock is malformed")
    relative = _locked_relative_path(manifest["path"], "toolchain selection manifest")
    parent = _real_directory(
        source,
        relative.parent,
        "Windows toolchain selection manifest parent",
    )
    path = parent / relative.name
    try:
        metadata = path.lstat()
    except FileNotFoundError as exc:
        raise BuildFailure("Windows toolchain selection manifest is missing") from exc
    if (
        not stat.S_ISREG(metadata.st_mode)
        or stat.S_ISLNK(metadata.st_mode)
        or type(manifest["size"]) is not int
        or metadata.st_size != manifest["size"]
        or type(manifest["sha256"]) is not str
        or _sha(path) != manifest["sha256"]
    ):
        raise BuildFailure("Windows toolchain selection manifest differs from the lock")
    return dict(manifest)


def resolve_bound_windows_toolchain(lock: dict, mozbuild: Path) -> dict:
    binding = lock["buildBinding"].get("windowsToolchain")
    if type(binding) is not dict:
        raise BuildFailure("Windows toolchain binding is missing")

    compiler = binding.get("compiler")
    sdk = binding.get("windowsSdk")
    crt = binding.get("crt")
    if type(compiler) is not dict or type(sdk) is not dict or type(crt) is not dict:
        raise BuildFailure("Windows toolchain binding is malformed")

    compiler_relative = _locked_relative_path(
        compiler.get("relativePath"), "MSVC compiler directory"
    )
    compiler_path = _real_directory(mozbuild, compiler_relative, "MSVC compiler")
    _exact_directory_names(
        compiler_path.parent,
        compiler.get("versionDirectoryNames"),
        "MSVC compiler",
    )
    if compiler_path.name != compiler.get("version"):
        raise BuildFailure("MSVC compiler version differs from the source lock")

    include_relative = _locked_relative_path(
        sdk.get("includeRelativePath"), "Windows SDK include directory"
    )
    lib_relative = _locked_relative_path(
        sdk.get("libRelativePath"), "Windows SDK library directory"
    )
    include_path = _real_directory(mozbuild, include_relative, "Windows SDK include")
    lib_path = _real_directory(mozbuild, lib_relative, "Windows SDK library")
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

    crt_relative = _locked_relative_path(crt.get("relativePath"), "packaged CRT")
    crt_path = _real_directory(mozbuild, crt_relative, "packaged CRT")
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
    crt_rows = _locked_crt_rows(crt.get("files"))
    crt_tree = _exact_flat_tree(crt_path, crt_rows, crt.get("tree"))
    evidence = {
        "compilerVersion": compiler["version"],
        "packagedCrt": {
            "architecture": crt["architecture"],
            "family": crt["family"],
            "redistVersion": crt["redistVersion"],
            **crt_tree,
        },
        "selectionManifest": dict(binding["selectionManifest"]),
        "windowsSdkVersion": sdk["version"],
    }
    return {
        "crtFiles": [crt_path / row["path"] for row in crt_rows],
        "crtPath": crt_path,
        "evidence": evidence,
    }


def windows_package_command(crt_files: list[Path]) -> list[str]:
    if (
        not crt_files
        or any(not path.is_absolute() for path in crt_files)
        or [path.name for path in crt_files]
        != sorted({path.name for path in crt_files})
    ):
        raise BuildFailure("packaged CRT argv must be absolute, unique and ordinal-sorted")
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
        EXPECTED_VERSION,
        "--release",
        EXPECTED_RELEASE,
        "--arch",
        "x86_64",
        "--fonts",
        "macos",
        "linux",
    ]


def _command_versions(environment: dict[str, str], workspace: Path) -> dict:
    commands = {
        "python": ["python3", "--version"],
        "go": ["go", "version"],
        "clang": ["clang-18", "--version"],
        "lld": ["ld.lld", "--version"],
        "rustc": ["rustc", "-Vv"],
        "cargo": ["cargo", "-Vv"],
        "rustup": ["rustup", "show", "active-toolchain"],
        "patch": ["patch", "--version"],
        "make": ["make", "--version"],
        "sevenZip": ["7z", "i"],
    }
    result: dict[str, str] = {}
    for name, command in commands.items():
        completed = subprocess.run(
            command,
            cwd=workspace,
            env=environment,
            check=False,
            capture_output=True,
            text=True,
            timeout=120,
        )
        if completed.returncode != 0:
            raise BuildFailure(f"required tool version command failed: {name}")
        result[name] = (completed.stdout or completed.stderr).strip()
    return result


def _dpkg_closure() -> dict:
    value = _capture(["dpkg-query", "-W", "-f=${Package}=${Version}\\n"])
    encoded = (value + "\n").encode("utf-8")
    return {
        "packageCount": len(value.splitlines()),
        "sha256": hashlib.sha256(encoded).hexdigest(),
        "text": value,
    }


def _validate_zip(path: Path, expected_crt_files: object) -> dict:
    crt_rows = _locked_crt_rows(expected_crt_files)
    expected_crt_names = {row["path"] for row in crt_rows}
    with zipfile.ZipFile(path) as bundle:
        corrupt = bundle.testzip()
        if corrupt is not None:
            raise BuildFailure("candidate ZIP CRC verification failed")
        infos = bundle.infolist()
        names: list[str] = []
        casefold_names: set[str] = set()
        for info in infos:
            original = info.filename
            if "\\" in original:
                raise BuildFailure("candidate ZIP contains a backslash path")
            member = original[:-1] if info.is_dir() and original.endswith("/") else original
            parts = member.split("/")
            if (
                not member
                or member.startswith("/")
                or any(part in {"", ".", ".."} for part in parts)
                or ":" in parts[0]
                or ((info.external_attr >> 16) & 0o170000) == 0o120000
            ):
                raise BuildFailure("candidate ZIP contains an unsafe member")
            normalized = "/".join(parts)
            folded = normalized.casefold()
            if folded in casefold_names:
                raise BuildFailure("candidate ZIP contains a Windows path collision")
            casefold_names.add(folded)
            names.append(normalized)

        required = [
            "camoufox.exe",
            "application.ini",
            "platform.ini",
            "properties.json",
            "camoufox.cfg",
        ]
        missing = [name for name in required if name not in names]
        if missing:
            raise BuildFailure("candidate ZIP is missing required binding members")
        observed_crt_names = {
            name
            for name in names
            if "/" not in name
            and re.fullmatch(
                r"(?:concrt|msvcp|vccorlib|vcruntime)[^/]*\.dll",
                name,
                re.IGNORECASE,
            )
        }
        if observed_crt_names != expected_crt_names:
            raise BuildFailure("candidate ZIP CRT member closure differs from the lock")
        crt_member_hashes: dict[str, str] = {}
        for row in crt_rows:
            data = bundle.read(row["path"])
            digest = hashlib.sha256(data).hexdigest()
            if len(data) != row["size"] or digest != row["sha256"]:
                raise BuildFailure("candidate ZIP CRT member differs from the lock")
            crt_member_hashes[row["path"]] = digest
        member_hashes = {
            name: hashlib.sha256(bundle.read(name)).hexdigest() for name in required
        }
        parser = configparser.ConfigParser(interpolation=None)
        parser.optionxform = str
        parser.read_string(bundle.read("application.ini").decode("utf-8"))
        build_id = parser.get("App", "BuildID")
        source_stamp = parser.get("App", "SourceStamp")
        if not re.fullmatch(r"[0-9a-fA-F]{40}", source_stamp):
            raise BuildFailure("candidate SourceStamp is not a 40-hex revision")
    return {
        "memberCount": len(infos),
        "packagedCrtMemberSha256": crt_member_hashes,
        "requiredMemberSha256": member_hashes,
        "buildId": build_id,
        "sourceStamp": source_stamp,
    }


def _write_json(path: Path, value: dict) -> None:
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def execute(args: argparse.Namespace) -> dict:
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{7,63}", args.run_id):
        raise BuildFailure("run-id must be 8-64 lowercase letters, digits or hyphens")
    if not re.fullmatch(r"\d{14}", args.moz_build_date):
        raise BuildFailure("MOZ_BUILD_DATE must contain exactly 14 digits")
    for fixed in (
        INPUT_ROOT,
        VERISILO_ROOT,
        UPSTREAM_REPO,
        FIREFOX_ARCHIVE,
        BUILD_HOME,
        WORK_ROOT,
        OUT_ROOT,
    ):
        if not fixed.is_absolute() or fixed == Path("/"):
            raise BuildFailure("fixed build paths must be absolute and narrow")

    mounts = validate_mounts()
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
        host_environment = dict(os.environ)
        builder = validate_builder_identity(
            lock, host_environment, platform.machine()
        )
        resources = validate_resources(lock, WORK_ROOT)
        inputs = validate_bound_inputs(lock, host_environment)
        dpkg_before = _dpkg_closure()
        (result_dir / "dpkg-packages.txt").write_text(
            dpkg_before["text"] + "\n", encoding="utf-8", newline="\n"
        )
        dpkg_binding = {
            "manifest": "dpkg-packages.txt",
            "packageCount": dpkg_before["packageCount"],
            "sha256": dpkg_before["sha256"],
        }
        log.note("builder, mounts, resource gate and all bound inputs verified")

        upstream = export_upstream(workspace, log)
        source_archive = upstream / FIREFOX_ARCHIVE.name
        shutil.copyfile(FIREFOX_ARCHIVE, source_archive)

        environment = dict(host_environment)
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
        toolchain_manifest = verify_windows_toolchain_manifest(lock, source)
        log.run(["make", "mozbootstrap"], cwd=upstream, env=environment, label="mozbootstrap")
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

        patch_surface_paths, _, _ = upstream_patch_surface(lock, upstream)
        verify_patch_program(environment, source)
        verify_patch_surface(
            lock, source, patch_surface_paths, "prePatchSurface"
        )
        patch_debris_baseline = capture_patch_debris_baseline(lock, source)
        log.note(
            "upstream patch program, pre-patch surface and debris baseline verified"
        )

        patch_paths = lock["sourceInputs"]["upstreamPatches"]
        for index, item in enumerate(patch_paths, start=1):
            log.run(
                upstream_patch_command(upstream / item["path"]),
                cwd=source,
                env=environment,
                label=f"upstream-patch-{index:02d}",
            )
            verify_patch_debris_unchanged(source, patch_debris_baseline)
        verify_patch_surface(
            lock, source, patch_surface_paths, "postPatchSurface"
        )
        _verify_seams(lock, source, "postUpstreamPatchSha256")
        _verify_midl_compatibility_seams(
            lock, source, "postUpstreamPatchSha256"
        )
        log.note(
            "all 50 upstream patches, bounded patch surface and downstream seams verified"
        )

        log.run(
            downstream_patch_command(VERISILO_ROOT / MIDL_COMPAT_PATCH_REL),
            cwd=source,
            env=environment,
            label="verisilo-ff152-midl-cross-build-patch",
        )
        verify_patch_debris_unchanged(source, patch_debris_baseline)
        _verify_midl_compatibility_seams(
            lock, source, "postCompatibilityPatchSha256"
        )
        _verify_seams(lock, source, "postUpstreamPatchSha256")

        log.run(
            downstream_patch_command(VERISILO_ROOT / DOWNSTREAM_PATCH_REL),
            cwd=source,
            env=environment,
            label="verisilo-canvas-patch",
        )
        verify_patch_debris_unchanged(source, patch_debris_baseline)
        _verify_midl_compatibility_seams(
            lock, source, "postCompatibilityPatchSha256"
        )
        _verify_seams(lock, source, "postDownstreamPatchSha256")
        if verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during patching")
        (source / "_READY").touch()
        log.note("VeriSilo MIDL compatibility and Canvas patch postimages verified")

        log.run(
            [str(source / "mach"), "configure"],
            cwd=source,
            env=environment,
            label="configure-windows-x86_64-and-bootstrap-toolchains",
        )
        _verify_midl_compatibility_seams(
            lock, source, "postCompatibilityPatchSha256"
        )
        _verify_seams(lock, source, "postDownstreamPatchSha256")
        if verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during configure")

        mozbuild = Path(environment["MOZBUILD_STATE_PATH"])
        bound_toolchain = resolve_bound_windows_toolchain(lock, mozbuild)
        toolchain_before = {
            "versions": _command_versions(environment, upstream),
            "buildHomeTree": _freeze_provenance_tree(
                BUILD_HOME, result_dir / "build-home-before-build.json"
            ),
            "windowsToolchain": bound_toolchain["evidence"],
        }
        log.note("Windows configure completed and pre-build toolchains were frozen")

        log.run(
            [str(source / "mach"), "build"],
            cwd=source,
            env=environment,
            label="build-windows-x86_64",
        )
        if verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during build")
        toolchain_after_build = resolve_bound_windows_toolchain(lock, mozbuild)
        log.run(
            windows_package_command(toolchain_after_build["crtFiles"]),
            cwd=upstream,
            env=environment,
            label="package-windows-x86_64",
        )
        if verify_windows_toolchain_manifest(lock, source) != toolchain_manifest:
            raise BuildFailure("Windows toolchain selection manifest changed during package")
        toolchain_after_package = resolve_bound_windows_toolchain(lock, mozbuild)
        _verify_midl_compatibility_seams(
            lock, source, "postCompatibilityPatchSha256"
        )
        _verify_seams(lock, source, "postDownstreamPatchSha256")
        toolchain_after = {
            "versions": _command_versions(environment, upstream),
            "buildHomeTree": _freeze_provenance_tree(
                BUILD_HOME, result_dir / "build-home-after-build.json"
            ),
            "windowsToolchain": toolchain_after_package["evidence"],
        }
        dpkg_after = _dpkg_closure()
        if dpkg_after != dpkg_before:
            raise BuildFailure("container dpkg closure changed during the build")

        outputs = sorted(path for path in upstream.glob("camoufox-*.zip") if path.is_file())
        if [path.name for path in outputs] != [EXPECTED_OUTPUT]:
            raise BuildFailure("upstream root must contain exactly the expected archive")
        candidate = result_dir / EXPECTED_OUTPUT
        shutil.copy2(outputs[0], candidate)
        archive = {
            "name": candidate.name,
            "sizeBytes": candidate.stat().st_size,
            "sha256": _sha(candidate),
            **_validate_zip(
                candidate, lock["buildBinding"]["windowsToolchain"]["crt"]["files"]
            ),
            "windowsExtractionTreePending": True,
        }
        if archive["buildId"] != args.moz_build_date:
            raise BuildFailure("candidate BuildID does not match frozen MOZ_BUILD_DATE")

        completed = _utc_now()
        log.note(f"candidate archive frozen: sha256={archive['sha256']}")
        log.close()
        build_log = {
            "name": "build.log",
            "sha256": _sha(result_dir / "build.log"),
            "sizeBytes": (result_dir / "build.log").stat().st_size,
        }
        result = {
            "recordType": "verisilo-camoufox-build-run/v1",
            "runId": args.run_id,
            "startedAtUtc": started,
            "completedAtUtc": completed,
            "target": "x86_64-pc-windows-msvc",
            "engineRevision": EXPECTED_ENGINE_REVISION,
            "mozBuildDate": args.moz_build_date,
            "builder": builder,
            "mounts": mounts,
            "resourcesAtStart": resources,
            "inputs": inputs,
            "dpkgClosure": dpkg_binding,
            "toolchainBeforeBuild": toolchain_before,
            "toolchainAfterCompile": toolchain_after_build["evidence"],
            "toolchainAfterPackage": toolchain_after,
            "buildLog": build_log,
            "archive": archive,
            "claims": {
                "compiled": True,
                "windowsRuntimeObserved": False,
                "canvasApplied": False,
                "verified": False,
            },
        }
        _write_json(result_dir / "build-result.json", result)
        return result
    except Exception as exc:
        if not log._closed:
            log.note(f"failed: {exc}")
            log.close()
        failure = {
            "recordType": "verisilo-camoufox-build-failure/v1",
            "runId": args.run_id,
            "failedAtUtc": _utc_now(),
            "reason": str(exc),
            "buildLog": {
                "name": "build.log",
                "sha256": _sha(result_dir / "build.log"),
                "sizeBytes": (result_dir / "build.log").stat().st_size,
            },
        }
        _write_json(result_dir / "build-failure.json", failure)
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
