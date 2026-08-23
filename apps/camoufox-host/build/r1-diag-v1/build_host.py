#!/usr/bin/env python3
"""Linux host launcher for the independent R1 diagnostic build family.

The launcher has separate image-preparation and engine-build phases.  It never
injects a driver into a container: the engine phase invokes only the bound
image's own ENTRYPOINT and records the exact command, mounts, environment
metadata, image binding, and retained logs.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


DATA_MOUNT = Path("/mnt/camoufox-build")
RUNS_ROOT = DATA_MOUNT / "runs"
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
DOCKER = ["sudo", "-n", "docker"]
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


def _utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _sha(path: Path, algorithm: str = "sha256") -> str:
    digest = hashlib.new(algorithm)
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _strict_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_bytes().decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise HostBuildFailure(f"invalid JSON: {path}") from exc
    if type(value) is not dict:
        raise HostBuildFailure(f"JSON object required: {path}")
    return value


def _write_json_exclusive(path: Path, value: dict) -> None:
    if path.exists():
        raise HostBuildFailure(f"refusing to overwrite provenance: {path}")
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n",
        encoding="utf-8",
        newline="\n",
    )


def _write_json_replace(path: Path, value: dict) -> None:
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
        raise HostBuildFailure((completed.stderr or completed.stdout).strip())
    return completed.stdout.strip()


def _git(repo: Path, *arguments: str) -> str:
    return _capture(["git", "-C", str(repo), *arguments])


def _run_logged(command: list[str], cwd: Path, log_path: Path) -> int:
    with log_path.open("w", encoding="utf-8", newline="\n") as stream:
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
        )
        assert process.stdout is not None
        for line in process.stdout:
            stream.write(line)
            stream.flush()
        returncode = process.wait()
        stream.write(f"[{_utc_now()}] exit={returncode}\n")
        return returncode


def _owner(run_id: str) -> dict:
    if not RUN_ID_RE.fullmatch(run_id):
        raise HostBuildFailure("run-id is not exact")
    return {
        "recordType": "verisilo-r1-diag-build-owner/v1",
        "runId": run_id,
        "createdAtUtc": _utc_now(),
        "pid": os.getpid(),
    }


def _run_root(path: str, run_id: str) -> tuple[Path, Path, dict]:
    root = Path(path).resolve()
    if root.parent != RUNS_ROOT.resolve():
        raise HostBuildFailure("run root must be one direct child of the R1 runs root")
    if root.name != run_id or not root.is_dir() or root.is_symlink():
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
    ):
        raise HostBuildFailure("R1 v2 source lock identity is not exact")
    binding = lock.get("buildBinding")
    if type(binding) is not dict:
        raise HostBuildFailure("R1 v2 lock has no buildBinding object")
    actual_binding = binding.get("builderImageBinding")
    if binding_state not in {"unbound", "bound"}:
        raise HostBuildFailure("R1 binding-state selector is invalid")
    if binding_state == "bound" and type(actual_binding) is not dict:
        raise HostBuildFailure("R1 v2 lock has no bound builder image")
    if binding_state == "unbound" and actual_binding is not None:
        raise HostBuildFailure("prepare-image requires builderImageBinding=null")
    if actual_binding is not None and set(actual_binding) != REQUIRED_BINDING_FIELDS:
        raise HostBuildFailure("R1 builder image binding field set is not exact")
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


def _tooling_sha(lock: dict, checkout: Path) -> str:
    rows = []
    for item in lock["buildBinding"]["recipe"]["files"]:
        path = checkout / item["path"]
        rows.append({"path": item["path"], "sha256": _sha(path), "sizeBytes": path.stat().st_size})
    encoded = json.dumps(rows, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def prepare_image(args: argparse.Namespace) -> int:
    root, inputs, owner = _run_root(args.run_root, args.run_id)
    try:
        _validate_input_names(inputs)
        source, locked = _validate_verisilo(inputs / "verisilo", binding_state="unbound")
        other = _validate_upstream(inputs, locked["lock"])
        layout = _prepare_layout(root, output_names=())
        provenance = layout["provenance"]
        metadata = provenance / "buildx-metadata.json"
        build_log = provenance / "buildx.log"
        recipe_dir = inputs / "verisilo" / RECIPE_REL
        tag = _builder_tag(args.run_id)
        command = [
            *DOCKER,
            "buildx",
            "build",
            "--platform",
            "linux/amd64",
            "--file",
            str(recipe_dir / "Dockerfile"),
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
        build_exit = _run_logged(command, recipe_dir, build_log)
        if build_exit != 0:
            raise HostBuildFailure(f"R1 builder image build failed with exit code {build_exit}")
        inspect = _capture([*DOCKER, "image", "inspect", tag])
        inspect_path = provenance / "builder-image-inspect.json"
        inspect_path.write_text(inspect + "\n", encoding="utf-8", newline="\n")
        inspect_json = json.loads(inspect)[0]
        image_id = inspect_json.get("Id")
        if not isinstance(image_id, str) or not image_id.startswith("sha256:"):
            raise HostBuildFailure("docker inspect returned no immutable image ID")
        archive = provenance / "builder-image.tar"
        save_log = provenance / "docker-save.log"
        save_exit = _run_logged([*DOCKER, "image", "save", tag, "-o", str(archive)], root, save_log)
        if save_exit != 0 or not archive.is_file():
            raise HostBuildFailure("builder image save failed")
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
            "hostToolingSha256": _tooling_sha(locked["lock"], inputs / "verisilo"),
        }
        _write_json_exclusive(
            provenance / "builder-image-result.json",
            {
                "recordType": "verisilo-r1-diag-builder-image-result/v2",
                "runId": args.run_id,
                "startedAtUtc": owner["createdAtUtc"],
                "completedAtUtc": _utc_now(),
                "owner": owner,
                "source": source,
                "upstream": other,
                "bindingProposal": proposal,
                "status": "prepared-awaiting-source-lock-binding",
            },
        )
        return 0
    except Exception as exc:
        provenance = root / "provenance"
        if provenance.is_dir():
            _write_json_exclusive(
                provenance / "builder-image-failure.json",
                {"recordType": "verisilo-r1-diag-builder-image-failure/v2", "runId": args.run_id, "reason": str(exc), "failedAtUtc": _utc_now()},
            )
        raise


def _validate_prepared_result(path: Path, expected_run_id: str) -> dict:
    record = _strict_json(path)
    if record.get("status") != "prepared-awaiting-source-lock-binding" or record.get("runId") != expected_run_id:
        raise HostBuildFailure("builder image result is not an accepted Phase B record")
    proposal = record.get("bindingProposal")
    if type(proposal) is not dict or set(proposal) != REQUIRED_BINDING_FIELDS:
        raise HostBuildFailure("builder image binding proposal fields are not exact")
    return record


def prepare_bound_image(args: argparse.Namespace) -> int:
    root, inputs, owner = _run_root(args.run_root, args.run_id)
    _validate_input_names(inputs)
    source, locked = _validate_verisilo(inputs / "verisilo", binding_state="bound")
    _validate_upstream(inputs, locked["lock"])
    source_root = Path(args.source_run_root).resolve()
    source_result = _validate_prepared_result(source_root / "provenance" / "builder-image-result.json", args.source_run_id)
    layout = _prepare_layout(root, output_names=())
    provenance = layout["provenance"]
    for name in ("buildx.log", "buildx-metadata.json", "builder-image-inspect.json", "builder-image.tar"):
        source_path = source_root / "provenance" / name
        if not source_path.is_file():
            raise HostBuildFailure(f"Phase B evidence missing: {name}")
        shutil.copy2(source_path, provenance / name)
    proposal = source_result["bindingProposal"]
    observed = {
        "savedArchiveSha256": _sha(provenance / "builder-image.tar"),
        "savedArchiveSizeBytes": (provenance / "builder-image.tar").stat().st_size,
        "buildxLogSha256": _sha(provenance / "buildx.log"),
        "buildxLogSizeBytes": (provenance / "buildx.log").stat().st_size,
        "buildxMetadataSha256": _sha(provenance / "buildx-metadata.json"),
        "imageInspectSha256": _sha(provenance / "builder-image-inspect.json"),
    }
    for key, value in observed.items():
        if proposal.get(key) != value:
            raise HostBuildFailure(f"copied Phase B evidence differs from binding proposal: {key}")
    _write_json_exclusive(
        provenance / "builder-image-result.json",
        {
            "recordType": "verisilo-r1-diag-bound-image-preparation/v2",
            "runId": args.run_id,
            "sourceRunId": args.source_run_id,
            "owner": owner,
            "source": source,
            "bindingProposal": proposal,
            "status": "prepared-from-frozen-builder-binding",
        },
    )
    return 0


def _validate_bound_binding(lock: dict, prepared: dict, run_id: str) -> dict:
    binding = lock["buildBinding"].get("builderImageBinding")
    if type(binding) is not dict:
        raise HostBuildFailure("R1 v2 lock is not bound to a builder image")
    if prepared.get("runId") != run_id or prepared.get("status") not in {"prepared-awaiting-source-lock-binding", "prepared-from-frozen-builder-binding"}:
        raise HostBuildFailure("prepared builder image result lineage mismatch")
    proposal = prepared.get("bindingProposal")
    if proposal != binding:
        raise HostBuildFailure("prepared builder image proposal differs from v2 lock")
    return binding


def build_engine(args: argparse.Namespace) -> int:
    root = Path(args.run_root).resolve()
    if not root.is_dir() or root.name != args.run_id:
        raise HostBuildFailure("build run root is not exact")
    inputs = root / "inputs"
    owner_path = root / OWNER_NAME
    provenance = root / "provenance"
    if not owner_path.is_file() or not provenance.is_dir():
        raise HostBuildFailure("build run is missing owner/provenance")
    owner = _strict_json(owner_path)
    if owner.get("runId") != args.run_id or owner.get("recordType") != "verisilo-r1-diag-build-owner/v1":
        raise HostBuildFailure("build owner record mismatch")
    prepared_path = provenance / "builder-image-result.json"
    prepared = _strict_json(prepared_path)
    source, locked = _validate_verisilo(inputs / "verisilo", binding_state="bound")
    lock = locked["lock"]
    binding = _validate_bound_binding(lock, prepared, args.run_id)
    other = _validate_upstream(inputs, lock)
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
    for name in ("prepare-image", "prepare-bound-image", "build-engine"):
        commands.add_parser(name)
    prepare = commands.choices["prepare-image"]
    prepare.add_argument("--run-id", required=True)
    prepare.add_argument("--run-root", required=True)
    bound = commands.choices["prepare-bound-image"]
    bound.add_argument("--run-id", required=True)
    bound.add_argument("--run-root", required=True)
    bound.add_argument("--source-run-id", required=True)
    bound.add_argument("--source-run-root", required=True)
    engine = commands.choices["build-engine"]
    engine.add_argument("--run-id", required=True)
    engine.add_argument("--run-root", required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare-image":
            return prepare_image(args)
        if args.command == "prepare-bound-image":
            return prepare_bound_image(args)
        return build_engine(args)
    except (HostBuildFailure, OSError, ValueError, KeyError) as exc:
        print(f"host-build-failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
