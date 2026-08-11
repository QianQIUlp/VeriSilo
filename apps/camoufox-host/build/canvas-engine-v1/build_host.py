#!/usr/bin/env python3
"""Linux host launcher for one pinned Camoufox Canvas engine build.

This launcher never prepares or downloads source inputs. The caller must place
one clean VeriSilo checkout, one clean pinned Camoufox checkout, and the pinned
Firefox archive in the exact run-root layout before invoking it. The launcher
then takes an O_EXCL ownership record, builds the pinned OCI recipe once, saves
and hashes that image, and runs the strict in-container driver once.

No retry, cleanup, browser launch, Git write, push, or input mutation is
performed here. Failed and successful runs retain their logs and provenance.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shlex
import subprocess
import sys
import uuid
from datetime import datetime, timezone
from pathlib import Path


DATA_MOUNT = Path("/mnt/camoufox-build")
RUNS_ROOT = DATA_MOUNT / "runs"
FIREFOX_ARCHIVE_NAME = "firefox-152.0.4.source.tar.xz"
EXPECTED_INPUT_NAMES = {"verisilo", "upstream", FIREFOX_ARCHIVE_NAME}
LOCK_REL = Path(
    "apps/camoufox-host/lock/"
    "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json"
)
RECIPE_REL = Path("apps/camoufox-host/build/canvas-engine-v1")
DOCKERFILE_REL = RECIPE_REL / "Dockerfile"
EXPECTED_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1"
EXPECTED_BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
EXPECTED_BASE_AMD64_MANIFEST_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
EXPECTED_DOCKERFILE_FROM = (
    "FROM ubuntu:24.04@" + EXPECTED_BASE_INDEX_DIGEST
)
OWNER_NAME = ".verisilo-build-owner.json"
RUN_ID_RE = re.compile(r"[a-z0-9][a-z0-9-]{7,63}")
SHA256_RE = re.compile(r"sha256:[0-9a-f]{64}")
DOCKER = ["sudo", "-n", "docker"]


class HostBuildFailure(RuntimeError):
    """Fail-closed host preparation/build boundary."""


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _sha512(path: Path) -> str:
    digest = hashlib.sha512()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _canonical_sha256(value: object) -> str:
    encoded = json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _strict_json(path: Path) -> dict:
    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict:
        result: dict = {}
        for key, value in pairs:
            if key in result:
                raise HostBuildFailure(f"duplicate JSON key in {path.name}: {key}")
            result[key] = value
        return result

    try:
        value = json.loads(
            path.read_bytes().decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                HostBuildFailure(f"invalid JSON number in {path.name}: {token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise HostBuildFailure(f"invalid JSON in {path.name}: {exc}") from exc
    if type(value) is not dict:
        raise HostBuildFailure(f"{path.name} must contain one JSON object")
    return value


def _write_json_exclusive(path: Path, value: dict) -> None:
    payload = (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode("utf-8")
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    descriptor = os.open(path, flags, 0o600)
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
    finally:
        os.close(descriptor)


def _write_json_replace(path: Path, value: dict) -> None:
    payload = (
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"
    ).encode("utf-8")
    temporary = path.with_suffix(path.suffix + ".tmp")
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
    finally:
        os.close(descriptor)
    os.replace(temporary, path)


def _capture(command: list[str], *, cwd: Path | None = None) -> str:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).strip()
        raise HostBuildFailure(
            f"command failed ({completed.returncode}): {shlex.join(command)}: {detail}"
        )
    return completed.stdout.strip()


def _capture_bytes(command: list[str], *, cwd: Path | None = None) -> bytes:
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        check=False,
        capture_output=True,
        timeout=120,
    )
    if completed.returncode != 0:
        detail = (completed.stderr or completed.stdout).decode(
            "utf-8", errors="replace"
        ).strip()
        raise HostBuildFailure(
            f"command failed ({completed.returncode}): {shlex.join(command)}: {detail}"
        )
    return completed.stdout


def _run_logged(
    command: list[str],
    *,
    cwd: Path,
    log_path: Path,
    environment: dict[str, str] | None = None,
) -> int:
    descriptor = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as log:
        header = f"[{_utc_now()}] {shlex.join(command)}\n"
        print(header, end="", flush=True)
        log.write(header)
        log.flush()
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
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
            log.write(line)
            log.flush()
        return process.wait()


def _run_binary_output(
    command: list[str],
    *,
    cwd: Path,
    output_path: Path,
    log_path: Path,
) -> int:
    output_descriptor = os.open(
        output_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600
    )
    log_descriptor = os.open(
        log_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600
    )
    with os.fdopen(output_descriptor, "wb") as output, os.fdopen(
        log_descriptor, "w", encoding="utf-8", newline="\n"
    ) as log:
        header = f"[{_utc_now()}] {shlex.join(command)} > {output_path.name}\n"
        print(header, end="", flush=True)
        log.write(header)
        log.flush()
        process = subprocess.Popen(
            command,
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            stdout=output,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        assert process.stderr is not None
        for line in process.stderr:
            print(line, end="", flush=True)
            log.write(line)
            log.flush()
        returncode = process.wait()
        output.flush()
        os.fsync(output.fileno())
        return returncode


def _path_is_within(path: Path, root: Path) -> bool:
    resolved = path.resolve(strict=True)
    resolved_root = root.resolve(strict=True)
    return resolved == resolved_root or resolved_root in resolved.parents


def _validate_data_mount() -> dict:
    if platform.system() != "Linux" or platform.machine().lower() not in {
        "x86_64",
        "amd64",
    }:
        raise HostBuildFailure("host launcher requires native Linux x86_64")
    if DATA_MOUNT.resolve(strict=True) != DATA_MOUNT or not os.path.ismount(DATA_MOUNT):
        raise HostBuildFailure(f"{DATA_MOUNT} must be an exact independent mount point")
    mount_stat = DATA_MOUNT.stat()
    parent_stat = DATA_MOUNT.parent.stat()
    if mount_stat.st_dev == parent_stat.st_dev:
        raise HostBuildFailure(f"{DATA_MOUNT} is not independent from its parent filesystem")
    if not RUNS_ROOT.is_dir() or RUNS_ROOT.is_symlink():
        raise HostBuildFailure(f"{RUNS_ROOT} must already exist as a real directory")
    return {
        "path": str(DATA_MOUNT),
        "device": mount_stat.st_dev,
        "runsRoot": str(RUNS_ROOT),
    }


def _containerd_root() -> Path:
    dump = _capture(["containerd", "config", "dump"])
    for line in dump.splitlines():
        match = re.fullmatch(r"root\s*=\s*(['\"])([^'\"\n]+)\1\s*", line)
        if match:
            return Path(match.group(2))
        if line.startswith("["):
            break
    raise HostBuildFailure("containerd config dump did not expose its top-level root")


def _validate_container_roots(mount: dict) -> dict:
    docker_info = json.loads(
        _capture([*DOCKER, "info", "--format", "{{json .}}"])
    )
    docker_root_raw = docker_info.get("DockerRootDir")
    if type(docker_root_raw) is not str or not docker_root_raw:
        raise HostBuildFailure("docker info did not return DockerRootDir")
    docker_root = Path(docker_root_raw)
    containerd_root = _containerd_root()
    for name, path in (("Docker", docker_root), ("containerd", containerd_root)):
        if not path.is_dir() or path.is_symlink():
            raise HostBuildFailure(f"{name} root is not a real directory")
        if not _path_is_within(path, DATA_MOUNT):
            raise HostBuildFailure(f"{name} root is not under {DATA_MOUNT}")
        if path.stat().st_dev != mount["device"]:
            raise HostBuildFailure(f"{name} root is not on the build data filesystem")
    return {
        "dockerRoot": str(docker_root.resolve()),
        "containerdRoot": str(containerd_root.resolve()),
        "dockerVersion": json.loads(
            _capture([*DOCKER, "version", "--format", "{{json .}}"])
        ),
        "buildxVersion": _capture([*DOCKER, "buildx", "version"]),
        "containerdVersion": _capture(["containerd", "--version"]),
    }


def _validate_input_layout(run_root: Path, run_id: str) -> dict:
    if not RUN_ID_RE.fullmatch(run_id):
        raise HostBuildFailure("run-id must be 8-64 lowercase letters, digits or hyphens")
    if run_root.resolve(strict=True) != run_root:
        raise HostBuildFailure("run-root must be an existing canonical absolute path")
    if run_root.parent != RUNS_ROOT or run_root.name != run_id or run_root.is_symlink():
        raise HostBuildFailure(f"run-root must be exactly {RUNS_ROOT}/<run-id>")
    inputs = run_root / "inputs"
    if not inputs.is_dir() or inputs.is_symlink():
        raise HostBuildFailure("inputs must be a real directory")
    actual_inputs = {path.name for path in inputs.iterdir()}
    if actual_inputs != EXPECTED_INPUT_NAMES:
        raise HostBuildFailure(
            "inputs must contain exactly verisilo, upstream and the Firefox archive"
        )
    verisilo = inputs / "verisilo"
    upstream = inputs / "upstream"
    firefox = inputs / FIREFOX_ARCHIVE_NAME
    if any(path.is_symlink() for path in (verisilo, upstream, firefox)):
        raise HostBuildFailure("source inputs must not be symlinks")
    if not verisilo.is_dir() or not upstream.is_dir() or not firefox.is_file():
        raise HostBuildFailure("source input types do not match the exact layout")
    return {"verisilo": verisilo, "upstream": upstream, "firefox": firefox}


def _validate_prepare_layout(run_root: Path) -> None:
    names = {path.name for path in run_root.iterdir()}
    if names != {"inputs"}:
        raise HostBuildFailure("prepare-image requires a run-root containing only inputs")
    for name in ("build-home", "work", "out", "provenance", OWNER_NAME):
        if (run_root / name).exists():
            raise HostBuildFailure(f"prepare-image run-root already contains {name}")


def _validate_engine_layout(run_root: Path) -> None:
    names = {path.name for path in run_root.iterdir()}
    if names != {"inputs", "provenance", OWNER_NAME}:
        raise HostBuildFailure(
            "build-engine requires only inputs, provenance and the ownership record"
        )
    provenance = run_root / "provenance"
    if not provenance.is_dir() or provenance.is_symlink():
        raise HostBuildFailure("build-engine provenance must be a real directory")
    if not (provenance / "builder-image-result.json").is_file():
        raise HostBuildFailure("build-engine is missing builder-image-result.json")
    for name in ("build-home", "work", "out"):
        if (run_root / name).exists():
            raise HostBuildFailure(f"build-engine run-root already contains {name}")


def _git(repo: Path, *args: str) -> str:
    return _capture(["git", "-C", str(repo), *args])


def _validate_verisilo(verisilo: Path) -> tuple[dict, dict]:
    status = _git(verisilo, "status", "--porcelain=v1", "--untracked-files=all")
    if status:
        raise HostBuildFailure("VeriSilo input checkout is not clean")
    commit = _git(verisilo, "rev-parse", "HEAD")
    tree = _git(verisilo, "rev-parse", "HEAD^{tree}")
    branch = _git(verisilo, "branch", "--show-current")
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or not re.fullmatch(
        r"[0-9a-f]{40}", tree
    ):
        raise HostBuildFailure("VeriSilo HEAD/tree are not full Git object IDs")
    lock_path = verisilo / LOCK_REL
    lock = _strict_json(lock_path)
    if lock.get("engineRevision") != EXPECTED_ENGINE_REVISION:
        raise HostBuildFailure("source lock engine revision mismatch")
    build = lock.get("buildBinding") or {}
    base = build.get("ociBase") or {}
    if base.get("indexDigest") != EXPECTED_BASE_INDEX_DIGEST:
        raise HostBuildFailure("source lock OCI index digest mismatch")
    if base.get("linuxAmd64ManifestDigest") != EXPECTED_BASE_AMD64_MANIFEST_DIGEST:
        raise HostBuildFailure("source lock OCI amd64 manifest mismatch")
    recipe = build.get("recipe") or {}
    for item in recipe.get("files") or []:
        path = verisilo / item["path"]
        if (
            not path.is_file()
            or path.stat().st_size != item["sizeBytes"]
            or _sha256(path) != item["sha256"]
        ):
            raise HostBuildFailure(f"source lock recipe file mismatch: {item['path']}")
    dockerfile = verisilo / DOCKERFILE_REL
    if dockerfile.read_text(encoding="utf-8").splitlines()[0] != EXPECTED_DOCKERFILE_FROM:
        raise HostBuildFailure("Dockerfile FROM is not the pinned Ubuntu OCI index")
    firefox = lock.get("firefoxSource") or {}
    return (
        {
            "branch": branch,
            "commit": commit,
            "tree": tree,
            "lockPath": LOCK_REL.as_posix(),
            "lockSha256": _sha256(lock_path),
            "dockerfileSha256": _sha256(dockerfile),
        },
        {"lock": lock, "firefox": firefox},
    )


def _validate_other_inputs(inputs: dict, locked: dict) -> dict:
    upstream = inputs["upstream"]
    lock = locked["lock"]
    expected_upstream = lock["upstream"]
    status = _git(
        upstream,
        "status",
        "--short",
        "--untracked-files=all",
        "--ignored=matching",
    )
    commit = _git(upstream, "rev-parse", "HEAD")
    tree = _git(upstream, "rev-parse", "HEAD^{tree}")
    if status or commit != expected_upstream["commit"] or tree != expected_upstream["tree"]:
        raise HostBuildFailure("upstream checkout is not the exact clean locked tree")
    firefox = inputs["firefox"]
    expected_firefox = locked["firefox"]
    if (
        firefox.stat().st_size != expected_firefox["sizeBytes"]
        or _sha512(firefox) != expected_firefox["sha512"]
    ):
        raise HostBuildFailure("Firefox source archive size/SHA-512 mismatch")
    return {
        "upstreamCommit": commit,
        "upstreamTree": tree,
        "firefoxSizeBytes": firefox.stat().st_size,
        "firefoxSha512": expected_firefox["sha512"],
    }


def _take_ownership(run_root: Path, run_id: str) -> dict:
    value = {
        "recordType": "verisilo-camoufox-build-owner/v1",
        "token": str(uuid.uuid4()),
        "runId": run_id,
        "pid": os.getpid(),
        "createdAtUtc": _utc_now(),
    }
    _write_json_exclusive(run_root / OWNER_NAME, value)
    return value


def _load_owner(run_root: Path, run_id: str, token: str) -> dict:
    owner = _strict_json(run_root / OWNER_NAME)
    if owner.get("runId") != run_id or owner.get("token") != token:
        raise HostBuildFailure("build-engine owner token/run-id mismatch")
    return owner


def _create_output_layout(run_root: Path, names: tuple[str, ...]) -> dict[str, Path]:
    result: dict[str, Path] = {}
    for name in names:
        path = run_root / name
        path.mkdir(mode=0o700, exist_ok=False)
        result[name] = path
    return result


def _image_inspect(tag: str) -> tuple[dict, dict]:
    values = json.loads(_capture([*DOCKER, "image", "inspect", tag]))
    if type(values) is not list or len(values) != 1 or type(values[0]) is not dict:
        raise HostBuildFailure("docker image inspect did not return exactly one image")
    raw = values[0]
    image_id = raw.get("Id")
    if not isinstance(image_id, str) or not SHA256_RE.fullmatch(image_id):
        raise HostBuildFailure("built OCI image has no immutable sha256 image ID")
    if raw.get("Os") != "linux" or raw.get("Architecture") != "amd64":
        raise HostBuildFailure("built OCI image is not linux/amd64")
    labels = ((raw.get("Config") or {}).get("Labels") or {})
    if labels.get("org.opencontainers.image.base.digest") != EXPECTED_BASE_INDEX_DIGEST:
        raise HostBuildFailure("built image base digest label mismatch")
    if (
        labels.get("io.verisilo.base.linux-amd64-manifest")
        != EXPECTED_BASE_AMD64_MANIFEST_DIGEST
    ):
        raise HostBuildFailure("built image amd64 manifest label mismatch")
    summary = {
        "id": image_id,
        "os": raw["Os"],
        "architecture": raw["Architecture"],
        "created": raw.get("Created"),
        "repoTags": raw.get("RepoTags") or [],
        "labels": labels,
    }
    return raw, summary


def prepare_image(args: argparse.Namespace) -> int:
    mount = _validate_data_mount()
    container_roots = _validate_container_roots(mount)
    run_root = Path(args.run_root)
    inputs = _validate_input_layout(run_root, args.run_id)
    _validate_prepare_layout(run_root)
    source, locked = _validate_verisilo(inputs["verisilo"])
    if locked["lock"]["buildBinding"].get("builderImageBinding") is not None:
        raise HostBuildFailure("prepare-image requires an unbound builderImageBinding")
    other_inputs = _validate_other_inputs(inputs, locked)
    owner = _take_ownership(run_root, args.run_id)

    # Recheck absence after the atomic ownership boundary before creating provenance.
    for name in ("build-home", "work", "out", "provenance"):
        if (run_root / name).exists():
            raise HostBuildFailure(f"run layout changed after ownership: {name}")
    layout = _create_output_layout(run_root, ("provenance",))
    provenance_dir = layout["provenance"]
    started = _utc_now()
    result: dict = {
        "recordType": "verisilo-camoufox-builder-image-result/v1",
        "runId": args.run_id,
        "startedAtUtc": started,
        "owner": owner,
        "dataMount": mount,
        "containerRoots": container_roots,
        "recipeSource": source,
        "otherInputs": other_inputs,
        "status": "prepare-image-started",
    }

    recipe_dir = inputs["verisilo"] / RECIPE_REL
    dockerfile = inputs["verisilo"] / DOCKERFILE_REL
    tag = f"verisilo-camoufox-canvas-builder:{args.run_id}"
    metadata = provenance_dir / "buildx-metadata.json"
    build_log = provenance_dir / "buildx.log"
    build_command = [
        *DOCKER,
        "buildx",
        "build",
        "--platform",
        "linux/amd64",
        "--file",
        str(dockerfile),
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
        str(recipe_dir),
    ]
    build_exit = _run_logged(
        build_command,
        cwd=recipe_dir,
        log_path=build_log,
        environment=dict(os.environ),
    )
    result["buildx"] = {
        "exitCode": build_exit,
        "logSha256": _sha256(build_log),
        "logSizeBytes": build_log.stat().st_size,
        "metadataSha256": _sha256(metadata) if metadata.is_file() else None,
    }
    if build_exit != 0:
        result["status"] = "builder-image-build-failed"
        result["completedAtUtc"] = _utc_now()
        _write_json_exclusive(provenance_dir / "builder-image-result.json", result)
        return build_exit
    if not metadata.is_file():
        raise HostBuildFailure("successful buildx run did not write its metadata file")

    inspect_raw, inspect_summary = _image_inspect(tag)
    inspect_path = provenance_dir / "builder-image-inspect.json"
    _write_json_exclusive(inspect_path, inspect_raw)
    image_tar = provenance_dir / "builder-image.tar"
    save_log = provenance_dir / "docker-save.log"
    save_exit = _run_binary_output(
        [*DOCKER, "image", "save", tag],
        cwd=run_root,
        output_path=image_tar,
        log_path=save_log,
    )
    if save_exit != 0 or not image_tar.is_file():
        result["status"] = "builder-image-save-failed"
        result["completedAtUtc"] = _utc_now()
        result["image"] = inspect_summary
        _write_json_exclusive(provenance_dir / "builder-image-result.json", result)
        return save_exit or 1
    image_save_sha = _sha256(image_tar)
    result["image"] = {
        **inspect_summary,
        "inspectSha256": _sha256(inspect_path),
        "savedArchiveSha256": image_save_sha,
        "savedArchiveSizeBytes": image_tar.stat().st_size,
    }
    result["bindingProposal"] = {
        "imageId": inspect_summary["id"],
        "savedArchiveSha256": image_save_sha,
        "savedArchiveSizeBytes": image_tar.stat().st_size,
        "recipeSourceCommit": source["commit"],
        "recipeSourceTree": source["tree"],
        "recipeSourceLockSha256": source["lockSha256"],
        "dockerfileSha256": source["dockerfileSha256"],
        "baseIndexDigest": EXPECTED_BASE_INDEX_DIGEST,
        "baseLinuxAmd64ManifestDigest": EXPECTED_BASE_AMD64_MANIFEST_DIGEST,
        "buildxLogSha256": result["buildx"]["logSha256"],
        "buildxLogSizeBytes": result["buildx"]["logSizeBytes"],
        "buildxMetadataSha256": result["buildx"]["metadataSha256"],
        "imageInspectSha256": result["image"]["inspectSha256"],
        "hostToolingSha256": _canonical_sha256(container_roots),
    }
    result["status"] = "prepared-awaiting-source-lock-binding"
    result["completedAtUtc"] = _utc_now()
    _write_json_exclusive(provenance_dir / "builder-image-result.json", result)
    return 0


def _verify_committed_builder_binding(lock: dict, prepared: dict) -> dict:
    binding = (lock.get("buildBinding") or {}).get("builderImageBinding")
    if type(binding) is not dict:
        raise HostBuildFailure(
            "build-engine requires buildBinding.builderImageBinding in the clean source lock"
        )
    proposal = prepared.get("bindingProposal")
    if type(proposal) is not dict or prepared.get("status") != (
        "prepared-awaiting-source-lock-binding"
    ):
        raise HostBuildFailure("builder-image-result is not a successful prepare-image result")
    expected_required = {
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
    required = set(
        (lock.get("buildBinding") or {}).get(
            "builderImageBindingRequiredFields", []
        )
    )
    if required != expected_required:
        raise HostBuildFailure("source lock builder-image field contract mismatch")
    if set(binding) != required:
        raise HostBuildFailure("committed builderImageBinding field set is not exact")
    for key in required:
        if binding[key] != proposal[key]:
            raise HostBuildFailure(f"committed builderImageBinding mismatch: {key}")
    return binding


def _verify_historical_recipe_source(verisilo: Path, binding: dict) -> dict:
    commit = binding["recipeSourceCommit"]
    expected_tree = binding["recipeSourceTree"]
    actual_tree = _git(verisilo, "rev-parse", f"{commit}^{{tree}}")
    if actual_tree != expected_tree:
        raise HostBuildFailure("historical recipe source tree differs from the lock")

    lock_blob = _capture_bytes(
        ["git", "-C", str(verisilo), "show", f"{commit}:{LOCK_REL.as_posix()}"]
    )
    lock_sha256 = hashlib.sha256(lock_blob).hexdigest()
    if lock_sha256 != binding["recipeSourceLockSha256"]:
        raise HostBuildFailure("historical recipe source lock differs from the binding")

    dockerfile_blob = _capture_bytes(
        [
            "git",
            "-C",
            str(verisilo),
            "show",
            f"{commit}:{DOCKERFILE_REL.as_posix()}",
        ]
    )
    dockerfile_sha256 = hashlib.sha256(dockerfile_blob).hexdigest()
    if dockerfile_sha256 != binding["dockerfileSha256"]:
        raise HostBuildFailure("historical recipe Dockerfile differs from the binding")

    return {
        "commit": commit,
        "tree": actual_tree,
        "sourceLockSha256": lock_sha256,
        "dockerfileSha256": dockerfile_sha256,
    }


def _verify_prepared_image_evidence(
    provenance_dir: Path, binding: dict, container_roots: dict
) -> dict:
    expected_files = {
        "buildxLog": (
            provenance_dir / "buildx.log",
            binding["buildxLogSha256"],
            binding["buildxLogSizeBytes"],
        ),
        "buildxMetadata": (
            provenance_dir / "buildx-metadata.json",
            binding["buildxMetadataSha256"],
            None,
        ),
        "imageInspect": (
            provenance_dir / "builder-image-inspect.json",
            binding["imageInspectSha256"],
            None,
        ),
    }
    verified_files: dict[str, dict] = {}
    for name, (path, expected_sha256, expected_size) in expected_files.items():
        if not path.is_file() or path.is_symlink():
            raise HostBuildFailure(f"frozen builder evidence is missing: {path.name}")
        actual_size = path.stat().st_size
        if expected_size is not None and actual_size != expected_size:
            raise HostBuildFailure(f"frozen builder evidence size mismatch: {path.name}")
        actual_sha256 = _sha256(path)
        if actual_sha256 != expected_sha256:
            raise HostBuildFailure(f"frozen builder evidence digest mismatch: {path.name}")
        verified_files[name] = {
            "sha256": actual_sha256,
            "sizeBytes": actual_size,
        }

    tooling_sha256 = _canonical_sha256(container_roots)
    if tooling_sha256 != binding["hostToolingSha256"]:
        raise HostBuildFailure("current host tooling differs from the builder image binding")
    return {
        "files": verified_files,
        "hostToolingSha256": tooling_sha256,
    }


def build_engine(args: argparse.Namespace) -> int:
    mount = _validate_data_mount()
    container_roots = _validate_container_roots(mount)
    run_root = Path(args.run_root)
    inputs = _validate_input_layout(run_root, args.run_id)
    _validate_engine_layout(run_root)
    owner = _load_owner(run_root, args.run_id, args.owner_token)
    source, locked = _validate_verisilo(inputs["verisilo"])
    other_inputs = _validate_other_inputs(inputs, locked)
    provenance_dir = run_root / "provenance"
    prepared = _strict_json(provenance_dir / "builder-image-result.json")
    binding = _verify_committed_builder_binding(locked["lock"], prepared)
    historical_recipe = _verify_historical_recipe_source(inputs["verisilo"], binding)
    prepared_evidence = _verify_prepared_image_evidence(
        provenance_dir, binding, container_roots
    )

    image_tar = provenance_dir / "builder-image.tar"
    if (
        not image_tar.is_file()
        or image_tar.stat().st_size != binding["savedArchiveSizeBytes"]
        or _sha256(image_tar) != binding["savedArchiveSha256"]
    ):
        raise HostBuildFailure("saved builder image archive no longer matches the lock")
    _, inspect_summary = _image_inspect(binding["imageId"])
    if inspect_summary["id"] != binding["imageId"]:
        raise HostBuildFailure("existing builder image ID differs from the lock")
    expected_labels = {
        "io.verisilo.recipe-source-commit": binding["recipeSourceCommit"],
        "io.verisilo.recipe-source-tree": binding["recipeSourceTree"],
        "io.verisilo.recipe-source-lock-sha256": binding[
            "recipeSourceLockSha256"
        ],
        "io.verisilo.recipe-dockerfile-sha256": binding["dockerfileSha256"],
    }
    for key, expected in expected_labels.items():
        if inspect_summary["labels"].get(key) != expected:
            raise HostBuildFailure(f"existing builder image label mismatch: {key}")

    phase_owner = {
        "recordType": "verisilo-camoufox-build-engine-start/v1",
        "runId": args.run_id,
        "ownerToken": args.owner_token,
        "startedAtUtc": _utc_now(),
        "sourceCommit": source["commit"],
        "sourceTree": source["tree"],
        "sourceLockSha256": source["lockSha256"],
    }
    _write_json_exclusive(provenance_dir / "build-engine-start.json", phase_owner)
    layout = _create_output_layout(run_root, ("build-home", "work", "out"))
    provenance: dict = {
        "recordType": "verisilo-camoufox-build-host-provenance/v1",
        "runId": args.run_id,
        "startedAtUtc": phase_owner["startedAtUtc"],
        "owner": owner,
        "dataMount": mount,
        "containerRoots": container_roots,
        "source": source,
        "otherInputs": other_inputs,
        "committedBuilderImageBinding": binding,
        "verifiedHistoricalRecipeSource": historical_recipe,
        "verifiedPreparedImageEvidence": prepared_evidence,
        "image": inspect_summary,
        "status": "build-engine-started",
    }
    provenance_path = provenance_dir / "host-provenance.json"
    _write_json_exclusive(provenance_path, provenance)

    build_date = locked["lock"]["buildBinding"]["recipe"]["fixedEnvironment"][
        "MOZ_BUILD_DATE"
    ]
    container_log = provenance_dir / "container.log"
    container_command = [
        *DOCKER,
        "run",
        "--rm",
        "--read-only",
        "--mount",
        f"type=bind,src={run_root / 'inputs'},dst=/inputs,readonly",
        "--mount",
        f"type=bind,src={layout['build-home']},dst=/build-home",
        "--mount",
        f"type=bind,src={layout['work']},dst=/work",
        "--mount",
        f"type=bind,src={layout['out']},dst=/out",
        "--tmpfs",
        "/tmp:rw,nosuid,nodev,mode=1777,size=4g",
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
        binding["imageId"],
        "--run-id",
        args.run_id,
        "--moz-build-date",
        build_date,
    ]
    container_exit = _run_logged(
        container_command,
        cwd=run_root,
        log_path=container_log,
        environment=dict(os.environ),
    )
    provenance["container"] = {
        "exitCode": container_exit,
        "logSha256": _sha256(container_log),
        "logSizeBytes": container_log.stat().st_size,
        "readOnlyRoot": True,
        "inputsReadOnly": True,
        "tmpfsTmp": True,
    }
    strict_result_candidates = [
        layout["out"] / args.run_id / "build-result.json",
        layout["out"] / args.run_id / "build-failure.json",
    ]
    strict_results = [path for path in strict_result_candidates if path.is_file()]
    if len(strict_results) == 1:
        strict_result = strict_results[0]
        provenance["strictDriverResult"] = {
            "path": strict_result.relative_to(run_root).as_posix(),
            "sha256": _sha256(strict_result),
            "sizeBytes": strict_result.stat().st_size,
        }
    else:
        provenance["strictDriverResult"] = None
    provenance["status"] = (
        "container-passed" if container_exit == 0 else "container-failed"
    )
    provenance["completedAtUtc"] = _utc_now()
    _write_json_replace(provenance_path, provenance)
    return container_exit


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare = commands.add_parser("prepare-image")
    prepare.add_argument("--run-id", required=True)
    prepare.add_argument("--run-root", required=True)
    engine = commands.add_parser("build-engine")
    engine.add_argument("--run-id", required=True)
    engine.add_argument("--run-root", required=True)
    engine.add_argument("--owner-token", required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare-image":
            return prepare_image(args)
        return build_engine(args)
    except (HostBuildFailure, OSError, ValueError, KeyError) as exc:
        print(f"host-build-failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
