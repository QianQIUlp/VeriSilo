#!/usr/bin/env python3
"""Linux host launcher for the independent R1 diagnostic build family.

The launcher has separate image-preparation and engine-build phases.  It never
injects a driver into a container: the engine phase invokes only the bound
image's own ENTRYPOINT and records the exact command, mounts, environment
metadata, image binding, and retained logs.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
import hashlib
import io
import json
import os
import platform
import re
import stat
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path


DATA_MOUNT = Path("/mnt/camoufox-build")
RUNS_ROOT = DATA_MOUNT / "runs"
DURABLE_EVIDENCE_ROOT = Path("/var/lib/verisilo/camoufox-build-evidence")
BOOT_ID_PATH = Path("/proc/sys/kernel/random/boot_id")
FIREFOX_ARCHIVE_NAME = "firefox-152.0.4.source.tar.xz"
EXPECTED_INPUT_NAMES = {"verisilo", "upstream", FIREFOX_ARCHIVE_NAME}
LOCK_REL = Path(
    "apps/camoufox-host/lock/"
    "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
)
RECIPE_REL = Path("apps/camoufox-host/build/r1-diag-v1")
DOCKERFILE_REL = RECIPE_REL / "Dockerfile"
EXPECTED_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-r1-diag-v2"
EXPECTED_BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
EXPECTED_BASE_AMD64_MANIFEST_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
OWNER_NAME = ".verisilo-r1-diag-build-owner.json"
RUN_ID_RE = re.compile(r"[a-z0-9][a-z0-9-]{7,63}")
QUALIFICATION_ID_RE = re.compile(
    r"r1diag-durable-qual-[a-z0-9][a-z0-9-]{7,47}"
)
DOCKER_ENV = {
    "HOME": "/var/empty",
    "LANG": "C.UTF-8",
    "LC_ALL": "C.UTF-8",
    "PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    "TZ": "Etc/UTC",
}
DOCKER = [
    "/usr/bin/sudo",
    "-n",
    "/usr/bin/env",
    "-i",
    *(f"{key}={value}" for key, value in DOCKER_ENV.items()),
    "/usr/bin/docker",
]
DURABLE_SOURCE_FILES = (
    "builder-image.tar",
    "builder-image-result.json",
    "builder-image-inspect.json",
    "builder-build-context.tar",
    "buildx.log",
    "buildx-metadata.json",
    "docker-save.log",
)
DURABLE_PREFLIGHT_NAME = "retention-preflight.json"
DURABLE_BUNDLE_FILES = (DURABLE_PREFLIGHT_NAME, *DURABLE_SOURCE_FILES)
DURABLE_MANIFEST_NAME = "durable-manifest.json"
RETENTION_RECEIPT_NAME = "retention-receipt.json"
BUILD_CONTEXT_NAME = "builder-build-context.tar"
BUILD_CONTEXT_MEMBERS = (
    "Dockerfile",
    "strict_build.py",
    "diag_gate.py",
    "build_host.py",
)
QUALIFICATION_REQUEST_NAME = "qualification-request.json"
QUALIFICATION_RESULT_NAME = "qualification-result.json"
QUALIFICATION_SENTINEL_NAME = "qualification-sentinel.bin"
MOUNT_IDENTITY_FIELDS = {"target", "source", "filesystemType", "uuid"}
REQUIRED_BINDING_FIELDS = {
    "imageId",
    "savedArchiveSha256",
    "savedArchiveSizeBytes",
    "recipeSourceCommit",
    "recipeSourceTree",
    "recipeSourceLockSha256",
    "dockerfileSha256",
    "baseIndexDigest",
    "baseLinuxAmd64ManifestDigest",
    "buildxLogSha256",
    "buildxLogSizeBytes",
    "buildxMetadataSha256",
    "imageInspectSha256",
    "hostToolingSha256",
}
REQUIRED_DURABLE_LOCK_EVIDENCE_FIELDS = {
    "bindingProposalCanonicalSha256",
    "buildContextSha256",
    "buildContextSizeBytes",
    "builderImageResultSha256",
    "durableManifestCanonicalSha256",
    "durableManifestSha256",
    "durableQualificationId",
    "durableQualificationResultSha256",
    "reReadable",
    "retained",
    "retentionReceiptCanonicalSha256",
    "retentionReceiptSha256",
    "runId",
    "sourceCommit",
    "sourceLockSha256",
    "sourceTree",
}
UNBOUND_LOCK_STATUS = "recipe-frozen-durable-evidence-unbound"
BOUND_LOCK_STATUS = (
    "builder-bound-durable-evidence-awaiting-diagnostic-engine-build"
)
HISTORICAL_FAILED_RUN_ID = "r1diag-builder-20260823t0435z"
HISTORICAL_SUPERSEDED_RUN_ID = "r1diag-builder-20260823t0504z"
UNBOUND_LINEAGE_CURRENT = {
    "bindingState": "unbound",
    "durableEvidence": "retained-but-recipe-superseded",
    "phaseB2": "closed-pending-rebuild",
    "reasonCodes": [
        "unbounded_rustup_stable_broke_firefox_152_triplet_detection",
    ],
}
BOUND_LINEAGE_CURRENT = {
    "bindingState": "bound",
    "durableEvidence": "retained-and-reread",
    "phaseB2": "accepted",
    "reasonCodes": [],
}
DURABLE_CONTRACT_SCHEMA = (
    "verisilo-r1-diag-durable-builder-evidence-contract/v1"
)
EXPECTED_DURABLE_CONTRACT = {
    "schema": DURABLE_CONTRACT_SCHEMA,
    "scratchRoot": DATA_MOUNT.as_posix(),
    "durableRoot": DURABLE_EVIDENCE_ROOT.as_posix(),
    "qualificationRequired": True,
    "qualificationRequestSchema": (
        "verisilo-r1-diag-durable-root-qualification-request/v1"
    ),
    "qualificationResultSchema": (
        "verisilo-r1-diag-durable-root-qualification-result/v1"
    ),
    "bundleSchema": "verisilo-r1-diag-durable-builder-evidence/v1",
    "bundleRequiredFiles": [
        *DURABLE_BUNDLE_FILES,
        DURABLE_MANIFEST_NAME,
        RETENTION_RECEIPT_NAME,
    ],
    "manifestName": DURABLE_MANIFEST_NAME,
    "retentionReceiptName": RETENTION_RECEIPT_NAME,
    "retentionPreflightName": DURABLE_PREFLIGHT_NAME,
    "retentionPreflightSchema": (
        "verisilo-r1-diag-durable-retention-preflight/v1"
    ),
    "retentionReceiptSchema": (
        "verisilo-r1-diag-durable-builder-retention-receipt/v1"
    ),
    "imageSaveReference": "immutable-image-id",
    "prepareBoundImageSource": "fixed-durable-root-direct-child-by-run-id",
    "rehydration": "inspect-or-load-verified-archive-then-exact-id",
    "dockerPullPolicy": "never",
    "dockerExecutable": "/usr/bin/docker",
    "sudoExecutable": "/usr/bin/sudo",
    "dockerEnvironment": DOCKER_ENV,
    "environmentOverride": False,
}
EXPECTED_COMPLETE_PATCH_PATHS = [
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0000-verisilo-ff152-midl-cross-build-input.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0001-verisilo-canvas-export-key.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28/0002-verisilo-juggler-bounded-close.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0003-verisilo-gpc-canonical-pref-projection.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/0004-verisilo-remove-worker-gpc-mask-override.patch",
    "apps/camoufox-host/patches/camoufox/v152.0.4-beta.28-r1-diag/9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch",
]


class HostBuildFailure(RuntimeError):
    """Typed fail-closed host preparation/build boundary."""


class _DuplicateJsonKey(ValueError):
    pass


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _matches(pattern: str, value: object) -> bool:
    return type(value) is str and re.fullmatch(pattern, value) is not None


def _sha(path: Path, algorithm: str = "sha256") -> str:
    digest = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict:
    result: dict = {}
    for key, value in pairs:
        if key in result:
            raise _DuplicateJsonKey(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _decode_json(data: bytes, label: str) -> object:
    try:
        return json.loads(
            data.decode("utf-8"), object_pairs_hook=_reject_duplicate_keys
        )
    except (UnicodeDecodeError, json.JSONDecodeError, _DuplicateJsonKey) as exc:
        raise HostBuildFailure(f"invalid JSON: {label}") from exc


def _strict_json(path: Path) -> dict:
    try:
        with _open_regular_readonly(path, f"JSON evidence {path}") as stream:
            value = _decode_json(stream.read(), str(path))
    except OSError as exc:
        raise HostBuildFailure(f"invalid JSON: {path}") from exc
    if type(value) is not dict:
        raise HostBuildFailure(f"JSON object required: {path}")
    return value


def _write_json_exclusive(path: Path, value: dict) -> None:
    try:
        with path.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(
                json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False)
                + "\n"
            )
    except FileExistsError as exc:
        raise HostBuildFailure(f"refusing to overwrite provenance: {path}") from exc


def _canonical_json_sha(value: dict) -> str:
    encoded = json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _fsync_directory(path: Path) -> None:
    if os.name == "nt":  # Linux-only launcher; keeps no-browser tests portable.
        return
    descriptor = -1
    try:
        descriptor = os.open(
            path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
        )
        os.fsync(descriptor)
    except OSError as exc:
        raise HostBuildFailure(f"durable directory fsync failed: {path}") from exc
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _write_bytes_exclusive_fsync(path: Path, data: bytes) -> None:
    try:
        with path.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
    except FileExistsError as exc:
        raise HostBuildFailure(f"refusing to overwrite durable evidence: {path}") from exc
    except OSError as exc:
        raise HostBuildFailure(f"durable evidence write failed: {path}") from exc


def _write_json_exclusive_fsync(path: Path, value: dict) -> None:
    data = (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode("utf-8")
    _write_bytes_exclusive_fsync(path, data)


def _copy_file_exclusive_fsync(source: Path, target: Path) -> dict:
    try:
        with _open_regular_readonly(
            source, f"durable source {source}"
        ) as input_stream, target.open("xb") as output_stream:
            before = _fd_identity(os.fstat(input_stream.fileno()))
            digest = hashlib.sha256()
            copied = 0
            for chunk in iter(lambda: input_stream.read(1024 * 1024), b""):
                output_stream.write(chunk)
                digest.update(chunk)
                copied += len(chunk)
            after = _fd_identity(os.fstat(input_stream.fileno()))
            if before != after or copied != after["sizeBytes"]:
                raise HostBuildFailure(
                    f"durable source changed during copy: {source.name}"
                )
            output_stream.flush()
            os.fsync(output_stream.fileno())
    except FileExistsError as exc:
        raise HostBuildFailure(f"refusing to overwrite durable evidence: {target}") from exc
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure(f"durable evidence copy failed: {source.name}") from exc
    record = _file_record(target)
    if record["sha256"] != digest.hexdigest() or record["sizeBytes"] != copied:
        raise HostBuildFailure(f"durable target differs after copy: {target.name}")
    return record


def _docker_environment() -> dict[str, str]:
    # Never inherit caller-controlled daemon, context, proxy, PATH, or credential
    # configuration.  Absolute executables plus this exact environment make
    # ambient host variables irrelevant instead of turning ordinary proxy
    # settings into a host-specific build failure.
    return dict(DOCKER_ENV)


def _fd_identity(metadata: os.stat_result) -> dict:
    return {
        "device": metadata.st_dev,
        "inode": metadata.st_ino,
        "mode": stat.S_IMODE(metadata.st_mode),
        "sizeBytes": metadata.st_size,
        "mtimeNs": metadata.st_mtime_ns,
        "ctimeNs": metadata.st_ctime_ns,
    }


@contextmanager
def _open_regular_readonly(path: Path, label: str):
    flags = os.O_RDONLY | getattr(os, "O_CLOEXEC", 0) | getattr(
        os, "O_NOFOLLOW", 0
    )
    descriptor = -1
    try:
        if path.is_symlink():
            raise HostBuildFailure(f"{label} must not be a symlink")
        descriptor = os.open(path, flags)
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_size <= 0:
            raise HostBuildFailure(f"{label} is not a non-empty regular file")
        stream = os.fdopen(descriptor, "rb", closefd=True)
        descriptor = -1
        with stream:
            yield stream
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure(f"{label} is unavailable") from exc
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _snapshot_open_stream(stream) -> dict:
    before = _fd_identity(os.fstat(stream.fileno()))
    if before["sizeBytes"] <= 0:
        raise HostBuildFailure("open evidence stream is empty")
    digest = hashlib.sha256()
    stream.seek(0)
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
    after = _fd_identity(os.fstat(stream.fileno()))
    if before != after:
        raise HostBuildFailure("open evidence stream changed while being read")
    return {"sha256": digest.hexdigest(), **after}


def _capture(
    command: list[str],
    cwd: Path | None = None,
    environment: dict[str, str] | None = None,
) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
        env=environment,
    )
    if completed.returncode != 0:
        raise HostBuildFailure((completed.stderr or completed.stdout).strip())
    return completed.stdout.strip()


def _git(repo: Path, *arguments: str) -> str:
    return _capture(["git", "-C", str(repo), *arguments])


def _run_logged(command: list[str], cwd: Path, log_path: Path) -> int:
    environment = (
        _docker_environment() if command[: len(DOCKER)] == DOCKER else None
    )
    with log_path.open("x", encoding="utf-8", newline="\n") as stream:
        stream.write(f"[{_utc_now()}] start: {' '.join(command)}\n")
        stream.flush()
        process = subprocess.Popen(
            command,
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            env=environment,
        )
        assert process.stdout is not None
        for line in process.stdout:
            stream.write(line)
            stream.flush()
        returncode = process.wait()
        stream.write(f"[{_utc_now()}] exit={returncode}\n")
        return returncode


def _run_logged_with_binary_stdin(
    command: list[str],
    cwd: Path,
    log_path: Path,
    input_path: Path,
    expected_input: dict,
) -> int:
    environment = (
        _docker_environment() if command[: len(DOCKER)] == DOCKER else None
    )
    with _open_regular_readonly(
        input_path, f"binary command input {input_path}"
    ) as input_stream:
        before = _snapshot_open_stream(input_stream)
        if (
            before["sha256"] != expected_input.get("sha256")
            or before["sizeBytes"] != expected_input.get("sizeBytes")
        ):
            raise HostBuildFailure("binary command input differs from its snapshot")
        with log_path.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(f"[{_utc_now()}] start: {' '.join(command)}\n")
            stream.flush()
            input_stream.seek(0)
            process = subprocess.Popen(
                command,
                cwd=cwd,
                stdin=input_stream,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding="utf-8",
                errors="replace",
                env=environment,
            )
            assert process.stdout is not None
            for line in process.stdout:
                stream.write(line)
                stream.flush()
            returncode = process.wait()
            after = _snapshot_open_stream(input_stream)
            if after != before:
                raise HostBuildFailure(
                    "binary command input changed while the child consumed it"
                )
            stream.write(f"[{_utc_now()}] exit={returncode}\n")
            stream.flush()
            os.fsync(stream.fileno())
            return returncode


def _locked_context_members(recipe: dict) -> dict[str, dict]:
    members = {
        Path(item["path"]).name: item for item in recipe.get("files", [])
    }
    if set(members) != set(BUILD_CONTEXT_MEMBERS):
        raise HostBuildFailure("R1 build-context recipe members are not exact")
    return members


def _observe_build_context_tar(context: Path) -> dict:
    rows = []
    try:
        with _open_regular_readonly(
            context, f"R1 builder build context {context}"
        ) as context_stream:
            snapshot = _snapshot_open_stream(context_stream)
            context_stream.seek(0)
            with tarfile.open(fileobj=context_stream, mode="r:") as archive:
                members = archive.getmembers()
                if [member.name for member in members] != list(
                    BUILD_CONTEXT_MEMBERS
                ) or not all(member.isfile() for member in members):
                    raise HostBuildFailure(
                        "R1 builder build-context member set/order is not exact"
                    )
                for member in members:
                    stream = archive.extractfile(member)
                    if stream is None:
                        raise HostBuildFailure(
                            "R1 builder build-context member is unreadable"
                        )
                    data = stream.read()
                    digest = hashlib.sha256(data).hexdigest()
                    rows.append(
                        {
                            "name": member.name,
                            "sha256": digest,
                            "sizeBytes": len(data),
                        }
                    )
            if _snapshot_open_stream(context_stream) != snapshot:
                raise HostBuildFailure(
                    "R1 builder build context changed during validation"
                )
    except HostBuildFailure:
        raise
    except (OSError, tarfile.TarError, KeyError) as exc:
        raise HostBuildFailure("R1 builder build context is invalid") from exc
    return {
        "name": BUILD_CONTEXT_NAME,
        "sha256": snapshot["sha256"],
        "sizeBytes": snapshot["sizeBytes"],
        "members": rows,
    }


def _validate_build_context_tar(context: Path, recipe: dict) -> dict:
    observed = _observe_build_context_tar(context)
    locked = _locked_context_members(recipe)
    for row in observed["members"]:
        item = locked[row["name"]]
        if row["sizeBytes"] != item["sizeBytes"] or row["sha256"] != item["sha256"]:
            raise HostBuildFailure(
                f"R1 builder build-context member drifted: {row['name']}"
            )
    return observed


def _create_build_context(
    recipe_dir: Path, recipe: dict, context: Path
) -> dict:
    locked = _locked_context_members(recipe)
    try:
        with context.open("xb") as output_stream:
            with tarfile.open(fileobj=output_stream, mode="w") as archive:
                for name in BUILD_CONTEXT_MEMBERS:
                    source = recipe_dir / name
                    with _open_regular_readonly(
                        source, f"R1 recipe context member {source}"
                    ) as input_stream:
                        data = input_stream.read()
                    item = locked[name]
                    if (
                        len(data) != item["sizeBytes"]
                        or hashlib.sha256(data).hexdigest() != item["sha256"]
                    ):
                        raise HostBuildFailure(
                            f"R1 recipe changed before context freeze: {name}"
                        )
                    member = tarfile.TarInfo(name)
                    member.size = len(data)
                    member.mode = 0o644
                    member.uid = 0
                    member.gid = 0
                    member.uname = ""
                    member.gname = ""
                    member.mtime = 0
                    archive.addfile(member, io.BytesIO(data))
            output_stream.flush()
            os.fsync(output_stream.fileno())
    except FileExistsError as exc:
        raise HostBuildFailure("R1 builder build context already exists") from exc
    except HostBuildFailure:
        raise
    except (OSError, tarfile.TarError, KeyError) as exc:
        raise HostBuildFailure("R1 builder build context freeze failed") from exc
    return _validate_build_context_tar(context, recipe)


def _docker_image_save_command(image_id: str) -> list[str]:
    if not _matches(r"sha256:[0-9a-f]{64}", image_id):
        raise HostBuildFailure("Docker image save requires an immutable image ID")
    return [*DOCKER, "image", "save", image_id]


def _docker_image_load_command() -> list[str]:
    return [*DOCKER, "image", "load"]


def _save_binary_stdout(
    command: list[str],
    archive: Path,
    save_log: Path,
    cwd: Path,
) -> int:
    """Save a Docker archive through a launcher-owned binary output stream."""
    try:
        log_stream = save_log.open("x", encoding="utf-8", newline="\n")
    except OSError as exc:
        raise HostBuildFailure(f"cannot create docker save log: {save_log}") from exc

    with log_stream:
        log_stream.write(f"[{_utc_now()}] start: {' '.join(command)}\n")
        log_stream.flush()
        try:
            with archive.open("xb") as archive_stream:
                process = subprocess.Popen(
                    command,
                    cwd=cwd,
                    stdin=subprocess.DEVNULL,
                    stdout=archive_stream,
                    stderr=log_stream,
                    env=(
                        _docker_environment()
                        if command[: len(DOCKER)] == DOCKER
                        else None
                    ),
                )
                returncode = process.wait()
                archive_stream.flush()
                os.fsync(archive_stream.fileno())
        except FileExistsError as exc:
            log_stream.write(f"[{_utc_now()}] save-output-failed: {exc}\n")
            raise HostBuildFailure(f"refusing to overwrite builder image archive: {archive}") from exc
        except OSError as exc:
            log_stream.write(f"[{_utc_now()}] save-process-failed: {exc}\n")
            raise HostBuildFailure(f"builder image save output unavailable: {archive}") from exc
        log_stream.write(f"[{_utc_now()}] exit={returncode}\n")
        log_stream.flush()
        os.fsync(log_stream.fileno())

    if returncode == 0:
        _archive_provenance(archive)
    return returncode


def _archive_provenance(archive: Path) -> dict:
    """Verify the launcher can read the exact archive it is about to hash."""
    try:
        with _open_regular_readonly(
            archive, f"builder image archive {archive}"
        ) as stream:
            metadata = os.fstat(stream.fileno())
            _snapshot_open_stream(stream)
            launcher_uid = getattr(os, "getuid", lambda: None)()
            if launcher_uid is not None and metadata.st_uid != launcher_uid:
                raise HostBuildFailure(
                    "builder image archive is not owned by the launcher"
                )
    except HostBuildFailure:
        raise
    except FileNotFoundError as exc:
        raise HostBuildFailure(f"builder image archive is missing: {archive}") from exc
    except OSError as exc:
        raise HostBuildFailure(f"builder image archive is not launcher-readable: {archive}") from exc

    return {
        "archiveOwnerUid": getattr(metadata, "st_uid", None),
        "archiveMode": format(stat.S_IMODE(metadata.st_mode), "#06o"),
        "launcherReadable": True,
    }


def _file_record(path: Path) -> dict:
    try:
        with _open_regular_readonly(path, f"evidence {path}") as stream:
            snapshot = _snapshot_open_stream(stream)
        return {
            "name": path.name,
            "sha256": snapshot["sha256"],
            "sizeBytes": snapshot["sizeBytes"],
        }
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure(f"evidence is unreadable: {path}") from exc


def _expected_saved_image_identity(image_id: str) -> dict:
    if not _matches(r"sha256:[0-9a-f]{64}", image_id):
        raise HostBuildFailure("saved image proposal has no immutable image ID")
    expected_hex = image_id.removeprefix("sha256:")
    return {
        "configMember": f"{expected_hex}.json",
        "configSha256": expected_hex,
        "imageId": image_id,
        "manifestMember": "manifest.json",
    }


def _saved_image_config_members(image_id: str) -> tuple[str, str]:
    if not _matches(r"sha256:[0-9a-f]{64}", image_id):
        raise HostBuildFailure("saved image proposal has no immutable image ID")
    expected_hex = image_id.removeprefix("sha256:")
    return (
        f"{expected_hex}.json",
        f"blobs/sha256/{expected_hex}",
    )


def _validate_saved_image_identity(identity: dict, image_id: str) -> dict:
    expected = _expected_saved_image_identity(image_id)
    if (
        type(identity) is not dict
        or set(identity)
        != {"configMember", "configSha256", "imageId", "manifestMember"}
        or identity.get("configMember") not in _saved_image_config_members(image_id)
        or identity.get("configSha256") != expected["configSha256"]
        or identity.get("imageId") != image_id
        or identity.get("manifestMember") != "manifest.json"
    ):
        raise HostBuildFailure("saved image archive identity is malformed")
    return identity


def _validate_saved_image_stream(stream, image_id: str, label: str) -> dict:
    """Bind an open Docker-save stream back to the immutable image ID."""
    expected_identity = _expected_saved_image_identity(image_id)
    expected_hex = expected_identity["configSha256"]
    try:
        stream.seek(0)
        with tarfile.open(fileobj=stream, mode="r:*") as saved:
            members = saved.getmembers()
            names = [member.name for member in members]
            if len(names) != len(set(names)):
                raise HostBuildFailure("Docker image archive has duplicate members")
            if "manifest.json" not in names:
                raise HostBuildFailure("Docker image archive has no manifest.json")
            manifest_member = saved.getmember("manifest.json")
            if not manifest_member.isfile():
                raise HostBuildFailure("Docker image archive manifest is not a file")
            manifest_stream = saved.extractfile(manifest_member)
            if manifest_stream is None:
                raise HostBuildFailure("Docker image archive manifest is unreadable")
            manifest = _decode_json(
                manifest_stream.read(), f"{label}:manifest.json"
            )
            if type(manifest) is not list or len(manifest) != 1:
                raise HostBuildFailure("Docker image archive must contain one image")
            entry = manifest[0]
            if type(entry) is not dict or type(entry.get("Config")) is not str:
                raise HostBuildFailure("Docker image archive manifest is malformed")
            config_name = entry["Config"]
            if (
                config_name not in _saved_image_config_members(image_id)
                or config_name not in names
            ):
                raise HostBuildFailure(
                    "Docker image archive config does not name the proposed image ID"
                )
            config_member = saved.getmember(config_name)
            if not config_member.isfile():
                raise HostBuildFailure("Docker image archive config is not a file")
            config_stream = saved.extractfile(config_member)
            if config_stream is None:
                raise HostBuildFailure("Docker image archive config is unreadable")
            config_bytes = config_stream.read()
            _decode_json(config_bytes, f"{label}:{config_name}")
            config_sha = hashlib.sha256(config_bytes).hexdigest()
            if config_sha != expected_hex:
                raise HostBuildFailure(
                    "Docker image archive config digest differs from proposed image ID"
                )
    except HostBuildFailure:
        raise
    except (OSError, tarfile.TarError, KeyError) as exc:
        raise HostBuildFailure("Docker image archive is invalid") from exc
    finally:
        stream.seek(0)
    observed_identity = dict(expected_identity)
    observed_identity["configMember"] = config_name
    return _validate_saved_image_identity(observed_identity, image_id)


def _verify_open_archive(stream, binding: dict, label: str) -> dict:
    before = _snapshot_open_stream(stream)
    if (
        before["sha256"] != binding.get("savedArchiveSha256")
        or before["sizeBytes"] != binding.get("savedArchiveSizeBytes")
    ):
        raise HostBuildFailure("durable builder image archive differs from binding")
    tar_identity = _validate_saved_image_stream(stream, binding["imageId"], label)
    after = _snapshot_open_stream(stream)
    if before != after:
        raise HostBuildFailure("durable builder image archive changed during verification")
    return {"snapshot": after, "tarIdentity": tar_identity}


def _validate_saved_image_tar(archive: Path, image_id: str) -> dict:
    """Path wrapper used for the newly saved scratch archive."""
    with _open_regular_readonly(
        archive, f"Docker image archive {archive}"
    ) as stream:
        return _validate_saved_image_stream(stream, image_id, str(archive))


def _docker_image_inspect(image_id: str) -> dict | None:
    if not _matches(r"sha256:[0-9a-f]{64}", image_id):
        raise HostBuildFailure("Docker inspect requires an immutable image ID")
    completed = subprocess.run(
        [*DOCKER, "image", "inspect", image_id],
        check=False,
        capture_output=True,
        timeout=120,
        env=_docker_environment(),
    )
    if completed.returncode != 0:
        error = (completed.stderr or completed.stdout).decode(
            "utf-8", errors="replace"
        )
        if "no such image" in error.lower():
            return None
        raise HostBuildFailure(f"Docker image inspect failed: {error.strip()}")
    value = _decode_json(completed.stdout, "docker image inspect")
    if type(value) is not list or len(value) != 1 or type(value[0]) is not dict:
        raise HostBuildFailure("Docker image inspect result is malformed")
    if value[0].get("Id") != image_id:
        raise HostBuildFailure("Docker image inspect returned a different immutable ID")
    return value[0]


def _load_binary_stdin(
    binding: dict, archive: Path, log_path: Path, cwd: Path
) -> dict:
    try:
        with _open_regular_readonly(
            archive, f"durable builder image archive {archive}"
        ) as archive_stream:
            verified = _verify_open_archive(archive_stream, binding, str(archive))
            with log_path.open("x", encoding="utf-8", newline="\n") as log_stream:
                log_stream.write(
                    f"[{_utc_now()}] start: {' '.join(_docker_image_load_command())}\n"
                )
                log_stream.flush()
                archive_stream.seek(0)
                process = subprocess.Popen(
                    _docker_image_load_command(),
                    cwd=cwd,
                    stdin=archive_stream,
                    stdout=log_stream,
                    stderr=subprocess.STDOUT,
                    env=_docker_environment(),
                )
                returncode = process.wait()
                after = _snapshot_open_stream(archive_stream)
                if after != verified["snapshot"]:
                    raise HostBuildFailure(
                        "durable builder image archive changed during Docker load"
                    )
                log_stream.write(f"[{_utc_now()}] exit={returncode}\n")
                log_stream.flush()
                os.fsync(log_stream.fileno())
    except FileExistsError as exc:
        raise HostBuildFailure(f"refusing to overwrite Docker load log: {log_path}") from exc
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure("Docker image load could not be started") from exc
    if returncode != 0:
        raise HostBuildFailure(f"Docker image load failed with exit code {returncode}")
    return {
        "loadLogSha256": _sha(log_path),
        "loadLogSizeBytes": log_path.stat().st_size,
        "verifiedArchiveSha256": verified["snapshot"]["sha256"],
        "verifiedArchiveSizeBytes": verified["snapshot"]["sizeBytes"],
    }


def _require_exact_image_present(image_id: str) -> dict:
    inspected = _docker_image_inspect(image_id)
    if inspected is None:
        raise HostBuildFailure("bound Docker image is absent")
    return inspected


def _ensure_bound_image(
    binding: dict, archive: Path, load_log: Path, cwd: Path
) -> dict:
    with _open_regular_readonly(
        archive, f"durable builder image archive {archive}"
    ) as archive_stream:
        _verify_open_archive(archive_stream, binding, str(archive))
    inspected = _docker_image_inspect(binding["imageId"])
    if inspected is not None:
        return {
            "action": "already-present",
            "exactImageIdVerified": True,
            "imageId": binding["imageId"],
        }
    load_evidence = _load_binary_stdin(binding, archive, load_log, cwd)
    inspected = _docker_image_inspect(binding["imageId"])
    if inspected is None or inspected.get("Id") != binding["imageId"]:
        raise HostBuildFailure("loaded Docker image ID differs from binding")
    return {
        "action": "loaded-from-durable-archive",
        "exactImageIdVerified": True,
        "imageId": binding["imageId"],
        **load_evidence,
    }


def _owner(run_id: str) -> dict:
    if not RUN_ID_RE.fullmatch(run_id):
        raise HostBuildFailure("run-id is not exact")
    return {
        "recordType": "verisilo-r1-diag-build-owner/v1",
        "runId": run_id,
        "createdAtUtc": _utc_now(),
        "pid": os.getpid(),
    }


def _read_boot_id() -> str:
    try:
        value = BOOT_ID_PATH.read_text(encoding="ascii").strip().lower()
    except OSError as exc:
        raise HostBuildFailure("Linux boot ID is unavailable") from exc
    if not re.fullmatch(
        r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        value,
    ):
        raise HostBuildFailure("Linux boot ID is malformed")
    return value


def _mount_identity(root: Path) -> dict:
    output = _capture(
        [
            "/usr/bin/findmnt",
            "--json",
            "--target",
            str(root),
            "--output",
            "TARGET,SOURCE,FSTYPE,UUID",
        ]
    )
    value = _decode_json(output.encode("utf-8"), "findmnt")
    if type(value) is not dict or type(value.get("filesystems")) is not list:
        raise HostBuildFailure("findmnt result is malformed")
    filesystems = value["filesystems"]
    if len(filesystems) != 1 or type(filesystems[0]) is not dict:
        raise HostBuildFailure("findmnt did not identify one durable filesystem")
    row = filesystems[0]
    identity = {
        "target": row.get("target"),
        "source": row.get("source"),
        "filesystemType": row.get("fstype"),
        "uuid": row.get("uuid"),
    }
    if set(identity) != MOUNT_IDENTITY_FIELDS or not all(
        type(value) is str and value for value in identity.values()
    ):
        raise HostBuildFailure("durable mount identity is incomplete")
    target = identity["target"].rstrip("/") or "/"
    root_text = str(root.resolve())
    if target != "/" and root_text != target and not root_text.startswith(target + "/"):
        raise HostBuildFailure("findmnt target does not contain durable root")
    return identity


def _validate_durable_root() -> dict:
    try:
        if DURABLE_EVIDENCE_ROOT.is_symlink():
            raise HostBuildFailure("durable evidence root must not be a symlink")
        durable = DURABLE_EVIDENCE_ROOT.resolve(strict=True)
        if durable != DURABLE_EVIDENCE_ROOT:
            raise HostBuildFailure("durable evidence root path is not exact")
        durable_stat = durable.stat()
        scratch_stat = DATA_MOUNT.resolve(strict=True).stat()
        if not stat.S_ISDIR(durable_stat.st_mode):
            raise HostBuildFailure("durable evidence root is not a directory")
        if durable_stat.st_dev == scratch_stat.st_dev:
            raise HostBuildFailure(
                "durable evidence root shares the scratch filesystem"
            )
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure("durable evidence root is unavailable") from exc
    return _mount_identity(durable)


def _direct_child(root: Path, name: str, pattern: re.Pattern[str], label: str) -> Path:
    if not pattern.fullmatch(name) or Path(name).name != name:
        raise HostBuildFailure(f"{label} is not exact")
    child = root / name
    if child.parent != root:
        raise HostBuildFailure(f"{label} is not a direct durable-root child")
    return child


def _qualification_root(qualification_id: str) -> Path:
    return _direct_child(
        DURABLE_EVIDENCE_ROOT,
        qualification_id,
        QUALIFICATION_ID_RE,
        "qualification ID",
    )


def _bundle_root(run_id: str) -> Path:
    return _direct_child(DURABLE_EVIDENCE_ROOT, run_id, RUN_ID_RE, "source run ID")


def stage_durable_root_qualification(args: argparse.Namespace) -> int:
    mount_identity = _validate_durable_root()
    qualification_id = args.qualification_id
    qualification_root = _qualification_root(qualification_id)
    try:
        qualification_root.mkdir(mode=0o750)
    except FileExistsError as exc:
        raise HostBuildFailure("qualification directory already exists") from exc
    _fsync_directory(DURABLE_EVIDENCE_ROOT)
    sentinel = os.urandom(64)
    sentinel_path = qualification_root / QUALIFICATION_SENTINEL_NAME
    _write_bytes_exclusive_fsync(sentinel_path, sentinel)
    request = {
        "schema": "verisilo-r1-diag-durable-root-qualification-request/v1",
        "qualificationId": qualification_id,
        "stagedAtUtc": _utc_now(),
        "stagedBootId": _read_boot_id(),
        "mountIdentity": mount_identity,
        "sentinel": {
            "name": QUALIFICATION_SENTINEL_NAME,
            "sha256": hashlib.sha256(sentinel).hexdigest(),
            "sizeBytes": len(sentinel),
        },
        "status": "staged-awaiting-reboot",
    }
    _write_json_exclusive_fsync(
        qualification_root / QUALIFICATION_REQUEST_NAME, request
    )
    _fsync_directory(qualification_root)
    _validate_qualification_request(qualification_root, qualification_id)
    return 0


def _validate_qualification_request(
    qualification_root: Path, qualification_id: str
) -> dict:
    request_path = qualification_root / QUALIFICATION_REQUEST_NAME
    request = _strict_json(request_path)
    expected_fields = {
        "schema",
        "qualificationId",
        "stagedAtUtc",
        "stagedBootId",
        "mountIdentity",
        "sentinel",
        "status",
    }
    if (
        set(request) != expected_fields
        or request.get("schema")
        != "verisilo-r1-diag-durable-root-qualification-request/v1"
        or request.get("qualificationId") != qualification_id
        or request.get("status") != "staged-awaiting-reboot"
        or type(request.get("stagedAtUtc")) is not str
        or not _matches(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            request.get("stagedBootId"),
        )
        or type(request.get("mountIdentity")) is not dict
        or set(request["mountIdentity"]) != MOUNT_IDENTITY_FIELDS
        or not all(
            type(value) is str and value
            for value in request["mountIdentity"].values()
        )
        or type(request.get("sentinel")) is not dict
        or set(request["sentinel"]) != {"name", "sha256", "sizeBytes"}
        or request["sentinel"].get("name") != QUALIFICATION_SENTINEL_NAME
        or not _matches(
            r"[0-9a-f]{64}", request["sentinel"].get("sha256")
        )
        or request["sentinel"].get("sizeBytes") != 64
    ):
        raise HostBuildFailure("durable qualification request is malformed")
    sentinel = qualification_root / QUALIFICATION_SENTINEL_NAME
    record = _file_record(sentinel)
    if (
        record["sha256"] != request["sentinel"].get("sha256")
        or record["sizeBytes"] != request["sentinel"].get("sizeBytes")
    ):
        raise HostBuildFailure("durable qualification sentinel drifted")
    return request


def verify_durable_root_qualification(args: argparse.Namespace) -> int:
    mount_identity = _validate_durable_root()
    qualification_id = args.qualification_id
    qualification_root = _qualification_root(qualification_id)
    if qualification_root.is_symlink() or not qualification_root.is_dir():
        raise HostBuildFailure("qualification directory is unavailable")
    if {path.name for path in qualification_root.iterdir()} != {
        QUALIFICATION_REQUEST_NAME,
        QUALIFICATION_SENTINEL_NAME,
    }:
        raise HostBuildFailure("qualification directory is not in staged state")
    request = _validate_qualification_request(qualification_root, qualification_id)
    current_boot_id = _read_boot_id()
    if current_boot_id == request.get("stagedBootId"):
        raise HostBuildFailure("durable root qualification requires a different boot")
    if mount_identity != request.get("mountIdentity"):
        raise HostBuildFailure("durable root mount identity drifted after reboot")
    result = {
        "schema": "verisilo-r1-diag-durable-root-qualification-result/v1",
        "qualificationId": qualification_id,
        "verifiedAtUtc": _utc_now(),
        "stagedBootId": request["stagedBootId"],
        "verifiedBootId": current_boot_id,
        "mountIdentity": mount_identity,
        "requestSha256": _sha(
            qualification_root / QUALIFICATION_REQUEST_NAME
        ),
        "sentinel": request["sentinel"],
        "status": "qualified-after-reboot",
    }
    _write_json_exclusive_fsync(
        qualification_root / QUALIFICATION_RESULT_NAME, result
    )
    _fsync_directory(qualification_root)
    _validate_durable_qualification(qualification_id)
    return 0


def _validate_durable_qualification(qualification_id: str) -> dict:
    mount_identity = _validate_durable_root()
    qualification_root = _qualification_root(qualification_id)
    if qualification_root.is_symlink() or not qualification_root.is_dir():
        raise HostBuildFailure("qualified durable-root evidence is unavailable")
    if {path.name for path in qualification_root.iterdir()} != {
        QUALIFICATION_REQUEST_NAME,
        QUALIFICATION_RESULT_NAME,
        QUALIFICATION_SENTINEL_NAME,
    }:
        raise HostBuildFailure("durable qualification file set is not exact")
    request = _validate_qualification_request(qualification_root, qualification_id)
    result_path = qualification_root / QUALIFICATION_RESULT_NAME
    result = _strict_json(result_path)
    if (
        set(result)
        != {
            "schema",
            "qualificationId",
            "verifiedAtUtc",
            "stagedBootId",
            "verifiedBootId",
            "mountIdentity",
            "requestSha256",
            "sentinel",
            "status",
        }
        or result.get("schema")
        != "verisilo-r1-diag-durable-root-qualification-result/v1"
        or result.get("qualificationId") != qualification_id
        or result.get("status") != "qualified-after-reboot"
        or type(result.get("verifiedAtUtc")) is not str
        or not _matches(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            result.get("stagedBootId"),
        )
        or not _matches(
            r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
            result.get("verifiedBootId"),
        )
        or result.get("stagedBootId") != request.get("stagedBootId")
        or result.get("verifiedBootId") == request.get("stagedBootId")
        or result.get("sentinel") != request.get("sentinel")
        or result.get("requestSha256")
        != _sha(qualification_root / QUALIFICATION_REQUEST_NAME)
    ):
        raise HostBuildFailure("durable qualification result drifted")
    if (
        result.get("mountIdentity") != request.get("mountIdentity")
        or result.get("mountIdentity") != mount_identity
    ):
        raise HostBuildFailure("durable root mount identity drifted")
    return {
        "qualificationId": qualification_id,
        "mountIdentity": mount_identity,
        "requestSha256": result["requestSha256"],
        "resultSha256": _sha(result_path),
    }


def _validate_retention_preflight(
    path: Path,
    run_id: str,
    qualification: dict,
    mount_identity: dict,
) -> dict:
    value = _strict_json(path)
    if (
        set(value)
        != {
            "schema",
            "runId",
            "writtenAtUtc",
            "status",
            "durableQualificationId",
            "durableQualificationResultSha256",
            "mountIdentity",
            "launcherWriteFsyncReadVerified",
        }
        or value.get("schema")
        != "verisilo-r1-diag-durable-retention-preflight/v1"
        or value.get("runId") != run_id
        or type(value.get("writtenAtUtc")) is not str
        or value.get("status") != "reserved-before-docker-build"
        or value.get("durableQualificationId")
        != qualification.get("qualificationId")
        or value.get("durableQualificationResultSha256")
        != qualification.get("resultSha256")
        or value.get("mountIdentity") != mount_identity
        or qualification.get("mountIdentity") != mount_identity
        or value.get("launcherWriteFsyncReadVerified") is not True
    ):
        raise HostBuildFailure("durable retention preflight drifted")
    return {"value": value, "file": _file_record(path)}


def _reserve_durable_bundle(run_id: str, qualification: dict) -> dict:
    current_qualification = _validate_durable_qualification(
        qualification["qualificationId"]
    )
    if current_qualification != qualification:
        raise HostBuildFailure("durable qualification changed before reservation")
    mount_identity = _validate_durable_root()
    if mount_identity != qualification.get("mountIdentity"):
        raise HostBuildFailure("durable mount changed before reservation")
    bundle = _bundle_root(run_id)
    try:
        bundle.mkdir(mode=0o750)
    except FileExistsError as exc:
        raise HostBuildFailure("durable bundle already exists") from exc
    _fsync_directory(DURABLE_EVIDENCE_ROOT)
    preflight = {
        "schema": "verisilo-r1-diag-durable-retention-preflight/v1",
        "runId": run_id,
        "writtenAtUtc": _utc_now(),
        "status": "reserved-before-docker-build",
        "durableQualificationId": qualification["qualificationId"],
        "durableQualificationResultSha256": qualification["resultSha256"],
        "mountIdentity": mount_identity,
        "launcherWriteFsyncReadVerified": True,
    }
    preflight_path = bundle / DURABLE_PREFLIGHT_NAME
    _write_json_exclusive_fsync(preflight_path, preflight)
    _fsync_directory(bundle)
    _fsync_directory(DURABLE_EVIDENCE_ROOT)
    if {path.name for path in bundle.iterdir()} != {DURABLE_PREFLIGHT_NAME}:
        raise HostBuildFailure("durable reservation file set is not exact")
    validated = _validate_retention_preflight(
        preflight_path, run_id, qualification, mount_identity
    )
    return {
        "bundle": bundle,
        "qualification": qualification,
        "preflight": validated["value"],
        "preflightFile": validated["file"],
    }


def _run_root(path: str, run_id: str) -> tuple[Path, Path, dict]:
    raw_root = Path(path)
    try:
        if not raw_root.is_absolute() or raw_root.is_symlink():
            raise HostBuildFailure("run root must be an absolute non-symlink path")
        root = raw_root.resolve(strict=True)
        runs_root = RUNS_ROOT.resolve(strict=True)
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure("run root is unavailable") from exc
    if raw_root.absolute() != root or root.parent != runs_root:
        raise HostBuildFailure("run root must be one direct child of the R1 runs root")
    if root.name != run_id or not root.is_dir():
        raise HostBuildFailure("run root must be an existing exact run directory")
    if {path.name for path in root.iterdir()} != {"inputs"}:
        raise HostBuildFailure("new run root must contain only its prepared inputs")
    inputs = root / "inputs"
    if not inputs.is_dir() or inputs.is_symlink():
        raise HostBuildFailure("run inputs must be a real directory")
    owner = _owner(run_id)
    _write_json_exclusive(root / OWNER_NAME, owner)
    return root, inputs, owner


def _validate_input_names(inputs: Path) -> None:
    if {path.name for path in inputs.iterdir()} != EXPECTED_INPUT_NAMES:
        raise HostBuildFailure("input directory must contain exactly the three frozen inputs")
    for path in inputs.iterdir():
        if path.is_symlink():
            raise HostBuildFailure(f"input must not be a symlink: {path.name}")
    if not (inputs / "verisilo").is_dir() or not (inputs / "upstream").is_dir():
        raise HostBuildFailure("source checkout inputs must be directories")
    if not (inputs / FIREFOX_ARCHIVE_NAME).is_file():
        raise HostBuildFailure("Firefox source archive input must be a regular file")


def _validate_recipe(lock: dict, checkout: Path) -> dict:
    recipe = lock["buildBinding"].get("recipe")
    if type(recipe) is not dict:
        raise HostBuildFailure("R1 recipe binding is malformed")
    if recipe.get("name") != "camoufox-152.0.4-beta.28-r1-diag-v2":
        raise HostBuildFailure("R1 recipe name is not exact")
    expected = [
        (RECIPE_REL / "Dockerfile").as_posix(),
        (RECIPE_REL / "strict_build.py").as_posix(),
        (RECIPE_REL / "diag_gate.py").as_posix(),
        (RECIPE_REL / "build_host.py").as_posix(),
    ]
    if [item["path"] for item in recipe["files"]] != expected:
        raise HostBuildFailure("R1 recipe file order is not exact")
    for item in recipe["files"]:
        path = checkout / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"] or _sha(path) != item["sha256"]:
            raise HostBuildFailure(f"R1 recipe file mismatch: {item['path']}")
    dockerfile = checkout / DOCKERFILE_REL
    if dockerfile.read_text(encoding="utf-8").splitlines()[0] != "FROM ubuntu:24.04@" + EXPECTED_BASE_INDEX_DIGEST:
        raise HostBuildFailure("R1 Dockerfile base image is not pinned")
    return {
        "files": recipe["files"],
        "fixedEnvironment": recipe["fixedEnvironment"],
        "name": recipe["name"],
    }


def _validate_patch_contract(lock: dict) -> None:
    complete = lock.get("completePatchSeries")
    if type(complete) is not list or [item.get("id") for item in complete] != [
        "0000", "0001", "0002", "0003", "0004", "9000"
    ]:
        raise HostBuildFailure("R1 complete patch IDs are not exact")
    if [item.get("path") for item in complete] != EXPECTED_COMPLETE_PATCH_PATHS:
        raise HostBuildFailure("R1 complete patch paths are not exact")
    if lock.get("completeAppliedPatchOrder") != [
        "0000", "0001", "0002", "0003", "0004", "9000"
    ]:
        raise HostBuildFailure("R1 complete patch application order is not exact")
    incremental = lock.get("r1IncrementalPatches")
    if type(incremental) is not list or [item.get("id") for item in incremental] != [
        "0003", "0004", "9000"
    ]:
        raise HostBuildFailure("R1 incremental patch IDs are not exact")
    if [item.get("path") for item in incremental] != EXPECTED_COMPLETE_PATCH_PATHS[3:]:
        raise HostBuildFailure("R1 incremental patch paths are not exact")
    source_inputs = lock.get("sourceInputs")
    if type(source_inputs) is not dict:
        raise HostBuildFailure("R1 source-input binding is malformed")
    downstream = source_inputs.get("downstreamPatches")
    if type(downstream) is not list or [item.get("path") for item in downstream] != EXPECTED_COMPLETE_PATCH_PATHS:
        raise HostBuildFailure("R1 downstream patch paths are not exact")


def _validate_durable_contract(lock: dict) -> None:
    if lock.get("durableBuilderEvidenceContract") != EXPECTED_DURABLE_CONTRACT:
        raise HostBuildFailure("R1 durable builder evidence contract is not exact")
    binding = lock.get("buildBinding")
    if type(binding) is not dict:
        raise HostBuildFailure("R1 v2 lock has no buildBinding object")
    if binding.get("builderImageBindingRequiredFields") != sorted(
        REQUIRED_BINDING_FIELDS
    ):
        raise HostBuildFailure("R1 builder binding required fields drifted")
    if lock.get("builderImagePreparationEvidenceRequiredFields") != sorted(
        REQUIRED_DURABLE_LOCK_EVIDENCE_FIELDS
    ):
        raise HostBuildFailure("R1 durable evidence required fields drifted")
    if binding.get("binaryBinding") is not None:
        raise HostBuildFailure("R1 diagnostic binary binding must remain null")


def _validate_operational_lineage(lock: dict, binding_state: str) -> set[str]:
    lineage = lock.get("builderOperationalLineage")
    if type(lineage) is not dict or set(lineage) != {
        "current",
        "excludedFailedPreparation",
        "supersededPhaseC1",
    }:
        raise HostBuildFailure("R1 builder operational lineage is not exact")
    expected_current = (
        BOUND_LINEAGE_CURRENT
        if binding_state == "bound"
        else UNBOUND_LINEAGE_CURRENT
    )
    if lineage.get("current") != expected_current:
        raise HostBuildFailure(
            "R1 builder operational lineage disagrees with binding state"
        )
    failed = lineage.get("excludedFailedPreparation")
    superseded = lineage.get("supersededPhaseC1")
    if (
        type(failed) is not dict
        or failed.get("runId") != HISTORICAL_FAILED_RUN_ID
        or failed.get("classification") != "failed-before-binding"
        or type(superseded) is not dict
        or superseded.get("runId") != HISTORICAL_SUPERSEDED_RUN_ID
        or superseded.get("bindingCheckpointCommit")
        != "f267bb4ff3f00115a37546bbe0649d0db889a7d3"
        or superseded.get("bindingCorrectness") != "historically-accepted"
        or superseded.get("materialEvidence") != "permanently-lost"
        or superseded.get("dockerImage") != "permanently-lost"
        or superseded.get("operationallyConsumable") is not False
    ):
        raise HostBuildFailure("R1 historical builder lineage drifted")
    return {HISTORICAL_FAILED_RUN_ID, HISTORICAL_SUPERSEDED_RUN_ID}


def _reject_historical_preparation_run_id(run_id: str, lock: dict) -> None:
    retired = _validate_operational_lineage(lock, "unbound")
    if run_id in retired:
        raise HostBuildFailure(
            "historical R1 builder run-id cannot be reused"
        )


def _validate_binding_proposal(proposal: object) -> dict:
    if type(proposal) is not dict or set(proposal) != REQUIRED_BINDING_FIELDS:
        raise HostBuildFailure("builder image binding proposal fields are not exact")
    sha_fields = {
        "savedArchiveSha256",
        "recipeSourceLockSha256",
        "dockerfileSha256",
        "buildxLogSha256",
        "buildxMetadataSha256",
        "imageInspectSha256",
        "hostToolingSha256",
    }
    if not _matches(r"sha256:[0-9a-f]{64}", proposal.get("imageId")):
        raise HostBuildFailure("builder image ID is not immutable")
    if any(
        not _matches(r"[0-9a-f]{64}", proposal.get(field))
        for field in sha_fields
    ):
        raise HostBuildFailure("builder image proposal has a malformed SHA-256")
    if not _matches(r"[0-9a-f]{40}", proposal.get("recipeSourceCommit")):
        raise HostBuildFailure("builder image recipe commit is malformed")
    if not _matches(r"[0-9a-f]{40}", proposal.get("recipeSourceTree")):
        raise HostBuildFailure("builder image recipe tree is malformed")
    if proposal.get("baseIndexDigest") != EXPECTED_BASE_INDEX_DIGEST or proposal.get(
        "baseLinuxAmd64ManifestDigest"
    ) != EXPECTED_BASE_AMD64_MANIFEST_DIGEST:
        raise HostBuildFailure("builder image base binding drifted")
    for field in ("savedArchiveSizeBytes", "buildxLogSizeBytes"):
        if type(proposal.get(field)) is not int or proposal[field] <= 0:
            raise HostBuildFailure("builder image proposal has an invalid size")
    return proposal


def _validate_durable_evidence_record(evidence: object) -> dict:
    if (
        type(evidence) is not dict
        or set(evidence) != REQUIRED_DURABLE_LOCK_EVIDENCE_FIELDS
        or evidence.get("retained") is not True
        or evidence.get("reReadable") is not True
        or not _matches(RUN_ID_RE.pattern, evidence.get("runId"))
        or not _matches(
            QUALIFICATION_ID_RE.pattern, evidence.get("durableQualificationId")
        )
        or not _matches(r"[0-9a-f]{40}", evidence.get("sourceCommit"))
        or not _matches(r"[0-9a-f]{40}", evidence.get("sourceTree"))
        or type(evidence.get("buildContextSizeBytes")) is not int
        or evidence["buildContextSizeBytes"] <= 0
    ):
        raise HostBuildFailure("R1 v2 lock durable evidence is malformed")
    hash_fields = REQUIRED_DURABLE_LOCK_EVIDENCE_FIELDS - {
        "durableQualificationId",
        "buildContextSizeBytes",
        "reReadable",
        "retained",
        "runId",
        "sourceCommit",
        "sourceTree",
    }
    if any(
        not _matches(r"[0-9a-f]{64}", evidence.get(field))
        for field in hash_fields
    ):
        raise HostBuildFailure("R1 v2 lock durable evidence hash is malformed")
    return evidence


def _validate_verisilo(verisilo: Path, *, binding_state: str) -> tuple[dict, dict]:
    status = _git(verisilo, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise HostBuildFailure("VeriSilo input checkout is not clean")
    commit = _git(verisilo, "rev-parse", "HEAD")
    tree = _git(verisilo, "rev-parse", "HEAD^{tree}")
    lock_path = verisilo / LOCK_REL
    lock = _strict_json(lock_path)
    if (
        lock.get("schema") != "verisilo-r1-diag-source-binding/v2"
        or lock.get("engineRevision") != EXPECTED_ENGINE_REVISION
        or lock.get("buildMode") != "diagnostic"
        or lock.get("diagnosticOnly") is not True
        or lock.get("formalEligible") is not False
        or lock.get("browserLaunches") != 0
    ):
        raise HostBuildFailure("R1 v2 source lock identity is not exact")
    binding = lock.get("buildBinding")
    if type(binding) is not dict:
        raise HostBuildFailure("R1 v2 lock has no buildBinding object")
    actual_binding = binding.get("builderImageBinding")
    actual_evidence = lock.get("builderImagePreparationEvidence")
    if binding_state not in {"unbound", "bound"}:
        raise HostBuildFailure("R1 binding-state selector is invalid")
    expected_status = (
        BOUND_LOCK_STATUS if binding_state == "bound" else UNBOUND_LOCK_STATUS
    )
    if lock.get("status") != expected_status or binding.get("status") != expected_status:
        raise HostBuildFailure("R1 v2 lock operational status is not exact")
    if binding_state == "bound":
        _validate_binding_proposal(actual_binding)
        _validate_durable_evidence_record(actual_evidence)
    elif actual_binding is not None or actual_evidence is not None:
        raise HostBuildFailure(
            "prepare-image requires builderImageBinding and evidence to be null"
        )
    _validate_operational_lineage(lock, binding_state)
    _validate_durable_contract(lock)
    recipe = _validate_recipe(lock, verisilo)
    _validate_patch_contract(lock)
    return (
        {
            "commit": commit,
            "tree": tree,
            "lockPath": LOCK_REL.as_posix(),
            "lockSha256": _sha(lock_path),
            "dockerfileSha256": _sha(verisilo / DOCKERFILE_REL),
        },
        {"lock": lock, "recipe": recipe},
    )


def _validate_upstream(inputs: dict, lock: dict) -> dict:
    upstream = inputs / "upstream"
    expected = lock["upstream"]
    status = _git(upstream, "status", "--short", "--untracked-files=all", "--ignored=matching")
    commit = _git(upstream, "rev-parse", "HEAD")
    tree = _git(upstream, "rev-parse", "HEAD^{tree}")
    tag = _git(upstream, "rev-parse", f"refs/tags/{expected['tag']}^{{commit}}")
    if status or commit != expected["commit"] or tree != expected["tree"] or tag != expected["commit"]:
        raise HostBuildFailure("upstream input checkout differs from the R1 v2 lock")
    archive = inputs / FIREFOX_ARCHIVE_NAME
    source = lock["firefoxSource"]
    if not archive.is_file() or archive.stat().st_size != source["sizeBytes"] or _sha(archive, "sha512") != source["sha512"]:
        raise HostBuildFailure("Firefox source archive differs from the R1 v2 lock")
    return {
        "commit": commit,
        "tree": tree,
        "tag": expected["tag"],
        "archiveSha512": source["sha512"],
        "archiveSizeBytes": archive.stat().st_size,
    }


def _prepare_layout(root: Path, *, output_names: tuple[str, ...]) -> dict[str, Path]:
    provenance = root / "provenance"
    if provenance.exists():
        raise HostBuildFailure("provenance directory already exists")
    provenance.mkdir()
    result = {"provenance": provenance}
    for name in output_names:
        path = root / name
        if path.exists():
            raise HostBuildFailure(f"run root already contains {name}")
        path.mkdir()
        result[name] = path
    return result


def _builder_tag(run_id: str) -> str:
    return f"verisilo-camoufox-r1-diag-builder:{run_id}"


def _image_id_from_build_metadata(metadata: Path) -> str:
    value = _strict_json(metadata)
    image_id = value.get("containerimage.config.digest")
    if not _matches(r"sha256:[0-9a-f]{64}", image_id):
        raise HostBuildFailure(
            "buildx metadata returned no immutable config digest"
        )
    return image_id


def _validate_built_image_inspect(
    inspect_value: object, image_id: str, source: dict
) -> dict:
    if (
        type(inspect_value) is not list
        or len(inspect_value) != 1
        or type(inspect_value[0]) is not dict
    ):
        raise HostBuildFailure("docker inspect result is malformed")
    inspect_json = inspect_value[0]
    config = inspect_json.get("Config")
    labels = config.get("Labels") if type(config) is dict else None
    expected_labels = {
        "io.verisilo.recipe-source-commit": source["commit"],
        "io.verisilo.recipe-source-tree": source["tree"],
        "io.verisilo.recipe-source-lock-sha256": source["lockSha256"],
        "io.verisilo.recipe-dockerfile-sha256": source["dockerfileSha256"],
    }
    if (
        inspect_json.get("Id") != image_id
        or type(labels) is not dict
        or any(labels.get(key) != value for key, value in expected_labels.items())
    ):
        raise HostBuildFailure(
            "immutable image inspect differs from buildx metadata/recipe labels"
        )
    return inspect_json


def _tooling_sha_from_build_context(build_context: dict) -> str:
    """Derive tooling identity only from the context bytes buildx consumed."""
    members = build_context.get("members")
    if (
        type(members) is not list
        or [row.get("name") for row in members if type(row) is dict]
        != list(BUILD_CONTEXT_MEMBERS)
    ):
        raise HostBuildFailure("R1 build-context tooling members are not exact")
    rows = []
    for name, member in zip(BUILD_CONTEXT_MEMBERS, members):
        if (
            type(member) is not dict
            or set(member) != {"name", "sha256", "sizeBytes"}
            or member.get("name") != name
            or not _matches(r"[0-9a-f]{64}", member.get("sha256"))
            or type(member.get("sizeBytes")) is not int
            or member["sizeBytes"] <= 0
        ):
            raise HostBuildFailure("R1 build-context tooling member is malformed")
        rows.append(
            {
                "path": (RECIPE_REL / name).as_posix(),
                "sha256": member["sha256"],
                "sizeBytes": member["sizeBytes"],
            }
        )
    encoded = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _validate_prepared_result(path: Path, expected_run_id: str) -> dict:
    record = _strict_json(path)
    if (
        set(record)
        != {
            "recordType",
            "runId",
            "startedAtUtc",
            "completedAtUtc",
            "owner",
            "source",
            "upstream",
            "archiveProvenance",
            "buildContext",
            "bindingProposal",
            "status",
        }
        or type(record.get("startedAtUtc")) is not str
        or type(record.get("completedAtUtc")) is not str
        or
        record.get("recordType")
        != "verisilo-r1-diag-builder-image-result/v3"
        or record.get("status") != "prepared-awaiting-durable-retention"
        or record.get("runId") != expected_run_id
    ):
        raise HostBuildFailure(
            "builder image result is not pending durable retention"
        )
    proposal = _validate_binding_proposal(record.get("bindingProposal"))
    owner = record.get("owner")
    source = record.get("source")
    upstream = record.get("upstream")
    archive = record.get("archiveProvenance")
    build_context = record.get("buildContext")
    if (
        type(owner) is not dict
        or owner.get("recordType") != "verisilo-r1-diag-build-owner/v1"
        or owner.get("runId") != expected_run_id
        or type(source) is not dict
        or set(source)
        != {"commit", "tree", "lockPath", "lockSha256", "dockerfileSha256"}
        or source.get("commit") != proposal["recipeSourceCommit"]
        or source.get("tree") != proposal["recipeSourceTree"]
        or source.get("lockSha256") != proposal["recipeSourceLockSha256"]
        or source.get("dockerfileSha256") != proposal["dockerfileSha256"]
        or type(upstream) is not dict
        or set(upstream)
        != {"commit", "tree", "tag", "archiveSha512", "archiveSizeBytes"}
        or type(archive) is not dict
        or set(archive)
        != {"archiveOwnerUid", "archiveMode", "launcherReadable", "tarIdentity"}
        or archive.get("launcherReadable") is not True
        or type(archive.get("tarIdentity")) is not dict
        or type(build_context) is not dict
        or set(build_context) != {"name", "sha256", "sizeBytes", "members"}
        or build_context.get("name") != BUILD_CONTEXT_NAME
        or not _matches(r"[0-9a-f]{64}", build_context.get("sha256"))
        or type(build_context.get("sizeBytes")) is not int
        or build_context["sizeBytes"] <= 0
        or type(build_context.get("members")) is not list
        or any(type(row) is not dict for row in build_context["members"])
        or [row.get("name") for row in build_context["members"]]
        != list(BUILD_CONTEXT_MEMBERS)
        or any(
            set(row) != {"name", "sha256", "sizeBytes"}
            or not _matches(r"[0-9a-f]{64}", row.get("sha256"))
            or type(row.get("sizeBytes")) is not int
            or row["sizeBytes"] <= 0
            for row in build_context["members"]
        )
    ):
        raise HostBuildFailure("builder image result lineage is malformed")
    try:
        _validate_saved_image_identity(archive["tarIdentity"], proposal["imageId"])
    except HostBuildFailure as exc:
        raise HostBuildFailure("builder image result lineage is malformed") from exc
    if proposal["hostToolingSha256"] != _tooling_sha_from_build_context(
        build_context
    ):
        raise HostBuildFailure(
            "builder image tooling digest differs from frozen build context"
        )
    return record


def _durable_manifest_canonical_sha(manifest: dict) -> str:
    without_self_hash = dict(manifest)
    without_self_hash.pop("manifestCanonicalSha256", None)
    return _canonical_json_sha(without_self_hash)


def _retention_receipt_canonical_sha(receipt: dict) -> str:
    without_self_hash = dict(receipt)
    without_self_hash.pop("receiptCanonicalSha256", None)
    return _canonical_json_sha(without_self_hash)


def _retain_durable_bundle(
    provenance: Path, run_id: str, qualification: dict
) -> dict:
    current_qualification = _validate_durable_qualification(
        qualification["qualificationId"]
    )
    if current_qualification != qualification:
        raise HostBuildFailure("durable qualification changed before retention")
    bundle = _bundle_root(run_id)
    if (
        bundle.is_symlink()
        or not bundle.is_dir()
        or {path.name for path in bundle.iterdir()} != {DURABLE_PREFLIGHT_NAME}
    ):
        raise HostBuildFailure("durable bundle has no exact pre-build reservation")
    preflight = _validate_retention_preflight(
        bundle / DURABLE_PREFLIGHT_NAME,
        run_id,
        qualification,
        qualification["mountIdentity"],
    )
    file_records = [preflight["file"]]
    for name in DURABLE_SOURCE_FILES:
        file_records.append(
            _copy_file_exclusive_fsync(provenance / name, bundle / name)
        )
    result = _validate_prepared_result(
        bundle / "builder-image-result.json", run_id
    )
    proposal = result["bindingProposal"]
    source = result.get("source")
    if type(source) is not dict:
        raise HostBuildFailure("builder image result source is malformed")
    manifest = {
        "schema": "verisilo-r1-diag-durable-builder-evidence/v1",
        "runId": run_id,
        "writtenAtUtc": _utc_now(),
        "status": "durably-retained-awaiting-source-lock-binding",
        "retained": True,
        "fsyncCompleted": True,
        "source": {
            "commit": source.get("commit"),
            "tree": source.get("tree"),
            "lockSha256": source.get("lockSha256"),
        },
        "imageId": proposal["imageId"],
        "bindingProposalCanonicalSha256": _canonical_json_sha(proposal),
        "durableQualification": {
            "qualificationId": qualification["qualificationId"],
            "resultSha256": qualification["resultSha256"],
            "mountIdentity": qualification["mountIdentity"],
        },
        "files": file_records,
    }
    manifest["manifestCanonicalSha256"] = _durable_manifest_canonical_sha(
        manifest
    )
    _write_json_exclusive_fsync(bundle / DURABLE_MANIFEST_NAME, manifest)
    _fsync_directory(bundle)
    _fsync_directory(DURABLE_EVIDENCE_ROOT)
    reread = _validate_durable_bundle(run_id, require_receipt=False)
    receipt = {
        "schema": (
            "verisilo-r1-diag-durable-builder-retention-receipt/v1"
        ),
        "runId": run_id,
        "writtenAtUtc": _utc_now(),
        "status": "durably-retained-reread-verified-awaiting-source-lock-binding",
        "retained": True,
        "reReadable": True,
        "imageId": reread["proposal"]["imageId"],
        "manifestSha256": reread["manifestSha256"],
        "manifestCanonicalSha256": reread["manifestCanonicalSha256"],
        "builderImageResultSha256": reread["resultSha256"],
        "bindingProposalCanonicalSha256": reread["manifest"][
            "bindingProposalCanonicalSha256"
        ],
        "buildContextSha256": reread["result"]["buildContext"]["sha256"],
        "buildContextSizeBytes": reread["result"]["buildContext"]["sizeBytes"],
        "durableQualificationResultSha256": reread["qualification"][
            "resultSha256"
        ],
        "source": reread["manifest"]["source"],
    }
    receipt["receiptCanonicalSha256"] = _retention_receipt_canonical_sha(
        receipt
    )
    _write_json_exclusive_fsync(bundle / RETENTION_RECEIPT_NAME, receipt)
    _fsync_directory(bundle)
    _fsync_directory(DURABLE_EVIDENCE_ROOT)
    return _validate_durable_bundle(run_id)


def _validate_durable_bundle(
    run_id: str, *, require_receipt: bool = True
) -> dict:
    mount_identity = _validate_durable_root()
    bundle = _bundle_root(run_id)
    if bundle.is_symlink() or not bundle.is_dir():
        raise HostBuildFailure("durable bundle is unavailable")
    expected_names = set(DURABLE_BUNDLE_FILES) | {DURABLE_MANIFEST_NAME}
    if require_receipt:
        expected_names.add(RETENTION_RECEIPT_NAME)
    if {path.name for path in bundle.iterdir()} != expected_names:
        raise HostBuildFailure("durable bundle file set is not exact")
    manifest_path = bundle / DURABLE_MANIFEST_NAME
    if manifest_path.is_symlink() or not manifest_path.is_file():
        raise HostBuildFailure("durable manifest is not a regular file")
    manifest = _strict_json(manifest_path)
    required_manifest_fields = {
        "schema",
        "runId",
        "writtenAtUtc",
        "status",
        "retained",
        "fsyncCompleted",
        "source",
        "imageId",
        "bindingProposalCanonicalSha256",
        "durableQualification",
        "files",
        "manifestCanonicalSha256",
    }
    if (
        set(manifest) != required_manifest_fields
        or manifest.get("schema")
        != "verisilo-r1-diag-durable-builder-evidence/v1"
        or manifest.get("runId") != run_id
        or manifest.get("status")
        != "durably-retained-awaiting-source-lock-binding"
        or manifest.get("retained") is not True
        or manifest.get("fsyncCompleted") is not True
        or type(manifest.get("writtenAtUtc")) is not str
        or not _matches(
            r"[0-9a-f]{64}", manifest.get("manifestCanonicalSha256")
        )
        or manifest.get("manifestCanonicalSha256")
        != _durable_manifest_canonical_sha(manifest)
    ):
        raise HostBuildFailure("durable manifest is malformed or drifted")
    rows = manifest.get("files")
    if (
        type(rows) is not list
        or any(type(row) is not dict for row in rows)
        or [row.get("name") for row in rows] != list(DURABLE_BUNDLE_FILES)
    ):
        raise HostBuildFailure("durable manifest file order is not exact")
    observed_rows = []
    for row in rows:
        if type(row) is not dict or set(row) != {"name", "sha256", "sizeBytes"}:
            raise HostBuildFailure("durable manifest file record is malformed")
        name = row["name"]
        if Path(name).name != name or name not in DURABLE_BUNDLE_FILES:
            raise HostBuildFailure("durable manifest contains an unsafe file name")
        observed = _file_record(bundle / name)
        if observed != row:
            raise HostBuildFailure(f"durable evidence drifted: {name}")
        observed_rows.append(observed)
    observed_by_name = {row["name"]: row for row in observed_rows}
    result = _validate_prepared_result(
        bundle / "builder-image-result.json", run_id
    )
    proposal = result["bindingProposal"]
    source = result.get("source")
    if (
        type(source) is not dict
        or manifest.get("source")
        != {
            "commit": source.get("commit"),
            "tree": source.get("tree"),
            "lockSha256": source.get("lockSha256"),
        }
        or manifest.get("imageId") != proposal.get("imageId")
        or manifest.get("bindingProposalCanonicalSha256")
        != _canonical_json_sha(proposal)
        or observed_by_name["builder-image.tar"]["sha256"]
        != proposal["savedArchiveSha256"]
        or observed_by_name["builder-image.tar"]["sizeBytes"]
        != proposal["savedArchiveSizeBytes"]
        or observed_by_name["buildx.log"]["sha256"]
        != proposal["buildxLogSha256"]
        or observed_by_name["buildx.log"]["sizeBytes"]
        != proposal["buildxLogSizeBytes"]
        or observed_by_name["buildx-metadata.json"]["sha256"]
        != proposal["buildxMetadataSha256"]
        or observed_by_name["builder-image-inspect.json"]["sha256"]
        != proposal["imageInspectSha256"]
        or observed_by_name[BUILD_CONTEXT_NAME]["sha256"]
        != result["buildContext"]["sha256"]
        or observed_by_name[BUILD_CONTEXT_NAME]["sizeBytes"]
        != result["buildContext"]["sizeBytes"]
    ):
        raise HostBuildFailure("durable manifest lineage differs from result")
    if _observe_build_context_tar(bundle / BUILD_CONTEXT_NAME) != result[
        "buildContext"
    ]:
        raise HostBuildFailure("durable builder build context drifted")
    with _open_regular_readonly(
        bundle / "builder-image-inspect.json", "durable image inspect evidence"
    ) as stream:
        inspect_value = _decode_json(stream.read(), "durable image inspect evidence")
    _validate_built_image_inspect(inspect_value, proposal["imageId"], source)
    with _open_regular_readonly(
        bundle / "buildx-metadata.json", "durable buildx metadata"
    ) as stream:
        metadata_value = _decode_json(stream.read(), "durable buildx metadata")
    if (
        type(metadata_value) is not dict
        or metadata_value.get("containerimage.config.digest")
        != proposal["imageId"]
    ):
        raise HostBuildFailure("durable buildx metadata is malformed")
    qualification_row = manifest.get("durableQualification")
    if (
        type(qualification_row) is not dict
        or set(qualification_row)
        != {"qualificationId", "resultSha256", "mountIdentity"}
    ):
        raise HostBuildFailure("durable qualification binding is malformed")
    qualification = _validate_durable_qualification(
        qualification_row["qualificationId"]
    )
    if (
        qualification_row.get("resultSha256")
        != qualification["resultSha256"]
        or qualification_row.get("mountIdentity") != mount_identity
        or qualification["mountIdentity"] != mount_identity
    ):
        raise HostBuildFailure("durable qualification binding drifted")
    preflight = _validate_retention_preflight(
        bundle / DURABLE_PREFLIGHT_NAME,
        run_id,
        qualification,
        mount_identity,
    )
    if preflight["file"] != observed_by_name[DURABLE_PREFLIGHT_NAME]:
        raise HostBuildFailure("durable retention preflight record drifted")
    tar_identity = _validate_saved_image_tar(
        bundle / "builder-image.tar", proposal["imageId"]
    )
    if tar_identity != result["archiveProvenance"]["tarIdentity"]:
        raise HostBuildFailure("durable image tar identity differs from result")
    validated = {
        "bundle": bundle,
        "manifest": manifest,
        "manifestSha256": _sha(manifest_path),
        "manifestCanonicalSha256": manifest["manifestCanonicalSha256"],
        "proposal": proposal,
        "qualification": qualification,
        "result": result,
        "resultSha256": _sha(bundle / "builder-image-result.json"),
        "retained": True,
    }
    if not require_receipt:
        return validated
    receipt_path = bundle / RETENTION_RECEIPT_NAME
    receipt = _strict_json(receipt_path)
    if (
        set(receipt)
        != {
            "schema",
            "runId",
            "writtenAtUtc",
            "status",
            "retained",
            "reReadable",
            "imageId",
            "manifestSha256",
            "manifestCanonicalSha256",
            "builderImageResultSha256",
            "bindingProposalCanonicalSha256",
            "buildContextSha256",
            "buildContextSizeBytes",
            "durableQualificationResultSha256",
            "source",
            "receiptCanonicalSha256",
        }
        or receipt.get("schema")
        != "verisilo-r1-diag-durable-builder-retention-receipt/v1"
        or receipt.get("runId") != run_id
        or receipt.get("status")
        != "durably-retained-reread-verified-awaiting-source-lock-binding"
        or receipt.get("retained") is not True
        or receipt.get("reReadable") is not True
        or type(receipt.get("writtenAtUtc")) is not str
        or receipt.get("imageId") != validated["proposal"]["imageId"]
        or receipt.get("manifestSha256") != validated["manifestSha256"]
        or receipt.get("manifestCanonicalSha256")
        != validated["manifestCanonicalSha256"]
        or receipt.get("builderImageResultSha256")
        != validated["resultSha256"]
        or receipt.get("bindingProposalCanonicalSha256")
        != validated["manifest"]["bindingProposalCanonicalSha256"]
        or receipt.get("buildContextSha256")
        != validated["result"]["buildContext"]["sha256"]
        or receipt.get("buildContextSizeBytes")
        != validated["result"]["buildContext"]["sizeBytes"]
        or receipt.get("durableQualificationResultSha256")
        != validated["qualification"]["resultSha256"]
        or receipt.get("source") != validated["manifest"]["source"]
        or receipt.get("receiptCanonicalSha256")
        != _retention_receipt_canonical_sha(receipt)
    ):
        raise HostBuildFailure("durable retention receipt is malformed or drifted")
    validated.update(
        {
            "receipt": receipt,
            "receiptSha256": _sha(receipt_path),
            "receiptCanonicalSha256": receipt["receiptCanonicalSha256"],
            "reReadable": True,
        }
    )
    return validated


def _durable_lock_evidence(bundle: dict) -> dict:
    source = bundle["manifest"]["source"]
    return {
        "bindingProposalCanonicalSha256": bundle["manifest"][
            "bindingProposalCanonicalSha256"
        ],
        "buildContextSha256": bundle["result"]["buildContext"]["sha256"],
        "buildContextSizeBytes": bundle["result"]["buildContext"][
            "sizeBytes"
        ],
        "builderImageResultSha256": bundle["resultSha256"],
        "durableManifestCanonicalSha256": bundle[
            "manifestCanonicalSha256"
        ],
        "durableManifestSha256": bundle["manifestSha256"],
        "durableQualificationId": bundle["qualification"]["qualificationId"],
        "durableQualificationResultSha256": bundle["qualification"][
            "resultSha256"
        ],
        "reReadable": True,
        "retained": True,
        "retentionReceiptCanonicalSha256": bundle[
            "receiptCanonicalSha256"
        ],
        "retentionReceiptSha256": bundle["receiptSha256"],
        "runId": bundle["manifest"]["runId"],
        "sourceCommit": source["commit"],
        "sourceLockSha256": source["lockSha256"],
        "sourceTree": source["tree"],
    }


def _validate_bound_durable_evidence(lock: dict, bundle: dict) -> dict:
    binding = lock["buildBinding"].get("builderImageBinding")
    evidence = lock.get("builderImagePreparationEvidence")
    _validate_binding_proposal(binding)
    _validate_durable_evidence_record(evidence)
    _validate_build_context_tar(
        bundle["bundle"] / BUILD_CONTEXT_NAME,
        lock["buildBinding"]["recipe"],
    )
    if binding != bundle["proposal"]:
        raise HostBuildFailure("durable proposal differs from v2 lock")
    if (
        evidence != _durable_lock_evidence(bundle)
    ):
        raise HostBuildFailure("durable evidence differs from v2 lock")
    return evidence


def prepare_image(args: argparse.Namespace) -> int:
    root, inputs, owner = _run_root(args.run_root, args.run_id)
    try:
        qualification = _validate_durable_qualification(args.qualification_id)
        _validate_input_names(inputs)
        source, locked = _validate_verisilo(inputs / "verisilo", binding_state="unbound")
        other = _validate_upstream(inputs, locked["lock"])
        _reject_historical_preparation_run_id(args.run_id, locked["lock"])
        _reserve_durable_bundle(args.run_id, qualification)
        layout = _prepare_layout(root, output_names=())
        provenance = layout["provenance"]
        metadata = provenance / "buildx-metadata.json"
        build_log = provenance / "buildx.log"
        recipe_dir = inputs / "verisilo" / RECIPE_REL
        build_context_path = provenance / BUILD_CONTEXT_NAME
        build_context = _create_build_context(
            recipe_dir, locked["recipe"], build_context_path
        )
        tag = _builder_tag(args.run_id)
        command = [
            *DOCKER,
            "buildx",
            "build",
            "--platform",
            "linux/amd64",
            "--file",
            "Dockerfile",
            "--no-cache",
            "--pull=false",
            "--load",
            "--progress=plain",
            "--metadata-file",
            str(metadata),
            "--label",
            f"io.verisilo.recipe-source-commit={source['commit']}",
            "--label",
            f"io.verisilo.recipe-source-tree={source['tree']}",
            "--label",
            f"io.verisilo.recipe-source-lock-sha256={source['lockSha256']}",
            "--label",
            f"io.verisilo.recipe-dockerfile-sha256={source['dockerfileSha256']}",
            "--tag",
            tag,
            "-",
        ]
        build_exit = _run_logged_with_binary_stdin(
            command,
            recipe_dir,
            build_log,
            build_context_path,
            build_context,
        )
        if build_exit != 0:
            raise HostBuildFailure(f"R1 builder image build failed with exit code {build_exit}")
        image_id = _image_id_from_build_metadata(metadata)
        inspect = _capture(
            [*DOCKER, "image", "inspect", image_id],
            environment=_docker_environment(),
        )
        inspect_value = _decode_json(inspect.encode("utf-8"), "docker image inspect")
        inspect_json = _validate_built_image_inspect(
            inspect_value, image_id, source
        )
        inspect_path = provenance / "builder-image-inspect.json"
        _write_bytes_exclusive_fsync(
            inspect_path, (inspect + "\n").encode("utf-8")
        )
        archive = provenance / "builder-image.tar"
        save_log = provenance / "docker-save.log"
        save_exit = _save_binary_stdout(
            _docker_image_save_command(image_id), archive, save_log, root
        )
        if save_exit != 0:
            raise HostBuildFailure("builder image save failed")
        archive_details = _archive_provenance(archive)
        archive_details["tarIdentity"] = _validate_saved_image_tar(
            archive, image_id
        )
        proposal = {
            "imageId": image_id,
            "savedArchiveSha256": _sha(archive),
            "savedArchiveSizeBytes": archive.stat().st_size,
            "recipeSourceCommit": source["commit"],
            "recipeSourceTree": source["tree"],
            "recipeSourceLockSha256": source["lockSha256"],
            "dockerfileSha256": source["dockerfileSha256"],
            "baseIndexDigest": EXPECTED_BASE_INDEX_DIGEST,
            "baseLinuxAmd64ManifestDigest": EXPECTED_BASE_AMD64_MANIFEST_DIGEST,
            "buildxLogSha256": _sha(build_log),
            "buildxLogSizeBytes": build_log.stat().st_size,
            "buildxMetadataSha256": _sha(metadata),
            "imageInspectSha256": _sha(inspect_path),
            "hostToolingSha256": _tooling_sha_from_build_context(
                build_context
            ),
        }
        _validate_binding_proposal(proposal)
        _write_json_exclusive(
            provenance / "builder-image-result.json",
            {
                "recordType": "verisilo-r1-diag-builder-image-result/v3",
                "runId": args.run_id,
                "startedAtUtc": owner["createdAtUtc"],
                "completedAtUtc": _utc_now(),
                "owner": owner,
                "source": source,
                "upstream": other,
                "archiveProvenance": archive_details,
                "buildContext": build_context,
                "bindingProposal": proposal,
                "status": "prepared-awaiting-durable-retention",
            },
        )
        _retain_durable_bundle(provenance, args.run_id, qualification)
        return 0
    except Exception as exc:
        provenance = root / "provenance"
        if provenance.is_dir():
            _write_json_exclusive(
                provenance / "builder-image-failure.json",
                {"recordType": "verisilo-r1-diag-builder-image-failure/v2", "runId": args.run_id, "reason": str(exc), "failedAtUtc": _utc_now()},
            )
        raise


def prepare_bound_image(args: argparse.Namespace) -> int:
    root, inputs, owner = _run_root(args.run_root, args.run_id)
    _validate_input_names(inputs)
    source, locked = _validate_verisilo(inputs / "verisilo", binding_state="bound")
    _validate_upstream(inputs, locked["lock"])
    durable = _validate_durable_bundle(args.source_run_id)
    durable_evidence = _validate_bound_durable_evidence(locked["lock"], durable)
    layout = _prepare_layout(root, output_names=())
    provenance = layout["provenance"]
    proposal = durable["proposal"]
    rehydration = _ensure_bound_image(
        proposal,
        durable["bundle"] / "builder-image.tar",
        provenance / "docker-load.log",
        root,
    )
    _write_json_exclusive(
        provenance / "builder-image-result.json",
        {
            "recordType": "verisilo-r1-diag-bound-image-preparation/v3",
            "runId": args.run_id,
            "sourceRunId": args.source_run_id,
            "owner": owner,
            "source": source,
            "bindingProposal": proposal,
            "durableEvidence": durable_evidence,
            "rehydration": rehydration,
            "retained": True,
            "status": "prepared-from-durable-builder-binding",
        },
    )
    return 0


def _validate_bound_binding(lock: dict, prepared: dict, run_id: str) -> dict:
    _validate_operational_lineage(lock, "bound")
    binding = lock["buildBinding"].get("builderImageBinding")
    evidence = lock.get("builderImagePreparationEvidence")
    _validate_binding_proposal(binding)
    _validate_durable_evidence_record(evidence)
    expected_fields = {
        "recordType",
        "runId",
        "sourceRunId",
        "owner",
        "source",
        "bindingProposal",
        "durableEvidence",
        "rehydration",
        "retained",
        "status",
    }
    if (
        set(prepared) != expected_fields
        or prepared.get("recordType")
        != "verisilo-r1-diag-bound-image-preparation/v3"
        or prepared.get("runId") != run_id
        or prepared.get("status") != "prepared-from-durable-builder-binding"
        or prepared.get("retained") is not True
        or prepared.get("sourceRunId") != evidence.get("runId")
        or prepared.get("durableEvidence") != evidence
        or type(prepared.get("owner")) is not dict
        or set(prepared["owner"])
        != {"recordType", "runId", "createdAtUtc", "pid"}
        or prepared["owner"].get("runId") != run_id
        or prepared["owner"].get("recordType")
        != "verisilo-r1-diag-build-owner/v1"
        or type(prepared.get("source")) is not dict
        or set(prepared["source"])
        != {"commit", "tree", "lockPath", "lockSha256", "dockerfileSha256"}
    ):
        raise HostBuildFailure("prepared durable builder image lineage mismatch")
    proposal = prepared.get("bindingProposal")
    if proposal != binding:
        raise HostBuildFailure("prepared builder image proposal differs from v2 lock")
    rehydration = prepared.get("rehydration")
    if (
        type(rehydration) is not dict
        or rehydration.get("action")
        not in {"already-present", "loaded-from-durable-archive"}
        or rehydration.get("exactImageIdVerified") is not True
        or rehydration.get("imageId") != binding["imageId"]
    ):
        raise HostBuildFailure("prepared builder image rehydration is not exact")
    if rehydration["action"] == "already-present":
        if set(rehydration) != {
            "action",
            "exactImageIdVerified",
            "imageId",
        }:
            raise HostBuildFailure("already-present rehydration fields drifted")
    else:
        if (
            set(rehydration)
            != {
                "action",
                "exactImageIdVerified",
                "imageId",
                "loadLogSha256",
                "loadLogSizeBytes",
                "verifiedArchiveSha256",
                "verifiedArchiveSizeBytes",
            }
            or rehydration.get("verifiedArchiveSha256")
            != binding["savedArchiveSha256"]
            or rehydration.get("verifiedArchiveSizeBytes")
            != binding["savedArchiveSizeBytes"]
            or not _matches(
                r"[0-9a-f]{64}", rehydration.get("loadLogSha256")
            )
            or type(rehydration.get("loadLogSizeBytes")) is not int
            or rehydration["loadLogSizeBytes"] <= 0
        ):
            raise HostBuildFailure("loaded rehydration evidence drifted")
    return binding


def build_engine(args: argparse.Namespace) -> int:
    raw_root = Path(args.run_root)
    try:
        if not raw_root.is_absolute() or raw_root.is_symlink():
            raise HostBuildFailure(
                "build run root must be an absolute non-symlink path"
            )
        root = raw_root.resolve(strict=True)
        runs_root = RUNS_ROOT.resolve(strict=True)
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure("build run root is unavailable") from exc
    if (
        raw_root.absolute() != root
        or root.parent != runs_root
        or not root.is_dir()
        or root.name != args.run_id
    ):
        raise HostBuildFailure("build run root is not exact")
    inputs = root / "inputs"
    owner_path = root / OWNER_NAME
    provenance = root / "provenance"
    if {path.name for path in root.iterdir()} != {
        "inputs",
        OWNER_NAME,
        "provenance",
    }:
        raise HostBuildFailure("prepared build run file set is not exact")
    if not owner_path.is_file() or not provenance.is_dir():
        raise HostBuildFailure("build run is missing owner/provenance")
    owner = _strict_json(owner_path)
    if (
        set(owner) != {"recordType", "runId", "createdAtUtc", "pid"}
        or owner.get("runId") != args.run_id
        or owner.get("recordType") != "verisilo-r1-diag-build-owner/v1"
    ):
        raise HostBuildFailure("build owner record mismatch")
    _validate_input_names(inputs)
    prepared_path = provenance / "builder-image-result.json"
    prepared = _strict_json(prepared_path)
    source, locked = _validate_verisilo(inputs / "verisilo", binding_state="bound")
    lock = locked["lock"]
    binding = _validate_bound_binding(lock, prepared, args.run_id)
    if prepared.get("source") != source or prepared.get("owner") != owner:
        raise HostBuildFailure("prepared bound image source checkout drifted")
    durable = _validate_durable_bundle(prepared["sourceRunId"])
    _validate_bound_durable_evidence(lock, durable)
    expected_provenance_names = {"builder-image-result.json"}
    if prepared["rehydration"]["action"] == "loaded-from-durable-archive":
        expected_provenance_names.add("docker-load.log")
        load_record = _file_record(provenance / "docker-load.log")
        if (
            load_record["sha256"]
            != prepared["rehydration"]["loadLogSha256"]
            or load_record["sizeBytes"]
            != prepared["rehydration"]["loadLogSizeBytes"]
        ):
            raise HostBuildFailure("prepared Docker load log drifted")
    if {path.name for path in provenance.iterdir()} != expected_provenance_names:
        raise HostBuildFailure("prepared bound-image provenance file set drifted")
    other = _validate_upstream(inputs, lock)
    _require_exact_image_present(binding["imageId"])
    for name in ("build-home", "work", "out"):
        if (root / name).exists():
            raise HostBuildFailure(f"build run already contains {name}")
        (root / name).mkdir()
    wine_prefix = root / "work" / f"{args.run_id}" / ".wine-prefix"
    start = {
        "recordType": "verisilo-r1-diag-build-engine-start/v2",
        "runId": args.run_id,
        "startedAtUtc": _utc_now(),
        "source": source,
        "builderImageBinding": binding,
    }
    _write_json_exclusive(provenance / "build-engine-start.json", start)
    build_date = lock["buildBinding"]["recipe"]["fixedEnvironment"]["MOZ_BUILD_DATE"]
    command = [
        *DOCKER,
        "run",
        "--rm",
        "--pull=never",
        "--read-only",
        "--mount",
        f"type=bind,src={inputs},dst=/inputs,readonly",
        "--mount",
        f"type=bind,src={root / 'build-home'},dst=/build-home",
        "--mount",
        f"type=bind,src={root / 'work'},dst=/work",
        "--mount",
        f"type=bind,src={root / 'out'},dst=/out",
        "--tmpfs",
        "/tmp:rw,nosuid,nodev,exec,mode=1777,size=4g",
        "--env",
        f"VERISILO_BUILDER_IMAGE_ID={binding['imageId']}",
        "--env",
        f"VERISILO_BUILDER_IMAGE_SAVE_SHA256={binding['savedArchiveSha256']}",
        "--env",
        f"VERISILO_SOURCE_COMMIT={source['commit']}",
        "--env",
        f"VERISILO_SOURCE_TREE={source['tree']}",
        "--env",
        f"VERISILO_SOURCE_LOCK_SHA256={source['lockSha256']}",
        "--env",
        f"VERISILO_BASE_IMAGE_INDEX_DIGEST={EXPECTED_BASE_INDEX_DIGEST}",
        "--env",
        f"VERISILO_BASE_AMD64_MANIFEST_DIGEST={EXPECTED_BASE_AMD64_MANIFEST_DIGEST}",
        "--env",
        f"WINEPREFIX={wine_prefix}",
        binding["imageId"],
        "--run-id",
        args.run_id,
        "--moz-build-date",
        build_date,
    ]
    container_log = provenance / "container.log"
    container_exit = _run_logged(command, root, container_log)
    strict_result = None
    for candidate in (root / "out" / args.run_id / "build-result.json", root / "out" / args.run_id / "build-failure.json"):
        if candidate.is_file():
            strict_result = {"path": candidate.relative_to(root).as_posix(), "sha256": _sha(candidate), "sizeBytes": candidate.stat().st_size}
            break
    host_provenance = {
        "recordType": "verisilo-r1-diag-build-host-provenance/v2",
        "runId": args.run_id,
        "source": source,
        "upstream": other,
        "builderImageBinding": binding,
        "container": {"command": command, "exitCode": container_exit, "logSha256": _sha(container_log), "logSizeBytes": container_log.stat().st_size, "readOnlyRoot": True, "inputMountReadOnly": True, "winePrefix": str(wine_prefix), "driverInjection": False},
        "strictDriverResult": strict_result,
        "status": "container-passed" if container_exit == 0 else "container-failed",
        "completedAtUtc": _utc_now(),
    }
    _write_json_exclusive(provenance / "host-provenance.json", host_provenance)
    return container_exit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in (
        "stage-durable-root-qualification",
        "verify-durable-root-qualification",
        "prepare-image",
        "prepare-bound-image",
        "build-engine",
    ):
        commands.add_parser(name)
    stage = commands.choices["stage-durable-root-qualification"]
    stage.add_argument("--qualification-id", required=True)
    verify = commands.choices["verify-durable-root-qualification"]
    verify.add_argument("--qualification-id", required=True)
    prepare = commands.choices["prepare-image"]
    prepare.add_argument("--run-id", required=True)
    prepare.add_argument("--run-root", required=True)
    prepare.add_argument("--qualification-id", required=True)
    bound = commands.choices["prepare-bound-image"]
    bound.add_argument("--run-id", required=True)
    bound.add_argument("--run-root", required=True)
    bound.add_argument("--source-run-id", required=True)
    engine = commands.choices["build-engine"]
    engine.add_argument("--run-id", required=True)
    engine.add_argument("--run-root", required=True)
    args = parser.parse_args()
    try:
        if args.command == "stage-durable-root-qualification":
            return stage_durable_root_qualification(args)
        if args.command == "verify-durable-root-qualification":
            return verify_durable_root_qualification(args)
        if args.command == "prepare-image":
            return prepare_image(args)
        if args.command == "prepare-bound-image":
            return prepare_bound_image(args)
        return build_engine(args)
    except (
        HostBuildFailure,
        OSError,
        ValueError,
        KeyError,
        AttributeError,
        TypeError,
        subprocess.SubprocessError,
    ) as exc:
        print(f"host-build-failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
