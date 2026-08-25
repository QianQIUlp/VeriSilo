#!/usr/bin/env python3
"""One-shot Linux host launcher for the Formal R1 Windows-target build."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tarfile
from datetime import datetime, timezone
from pathlib import Path


FIREFOX_ARCHIVE_NAME = "firefox-152.0.4.source.tar.xz"
LOCK_REL = Path(
    "apps/camoufox-host/lock/"
    "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-source.json"
)
HOST_TOOL_REL = Path("apps/camoufox-host/build/r1-formal-v1/build_host.py")
ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005"]
RUN_ID_RE = re.compile(r"[a-z0-9][a-z0-9-]{7,63}")
IMAGE_ID_RE = re.compile(r"sha256:[0-9a-f]{64}")
BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
BASE_AMD64_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
CONTEXT_NAMES = ("Dockerfile", "strict_build.py")


class HostBuildFailure(RuntimeError):
    pass


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
        value: dict = {}
        for key, item in pairs:
            if key in value:
                raise HostBuildFailure(f"duplicate JSON key: {key}")
            value[key] = item
        return value

    try:
        value = json.loads(
            path.read_bytes().decode("utf-8"),
            object_pairs_hook=reject_duplicates,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise HostBuildFailure(f"invalid JSON: {path}") from exc
    if type(value) is not dict:
        raise HostBuildFailure(f"JSON root must be an object: {path}")
    return value


def _write_json_exclusive(path: Path, value: dict) -> None:
    data = (json.dumps(value, indent=2, sort_keys=True) + "\n").encode("utf-8")
    try:
        with path.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
    except FileExistsError as exc:
        raise HostBuildFailure(f"refusing to overwrite evidence: {path}") from exc


def _capture(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    if result.returncode:
        raise HostBuildFailure(
            f"command failed ({result.returncode}): {' '.join(command)}\n"
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def _git(repo: Path, *arguments: str) -> str:
    return _capture(
        ["git", "-c", f"safe.directory={repo}", "-C", str(repo), *arguments],
        repo,
    )


def _file_record(path: Path) -> dict:
    return {"name": path.name, "sha256": _sha(path), "sizeBytes": path.stat().st_size}


def _validate_run_root(path: str, run_id: str) -> tuple[Path, Path]:
    if RUN_ID_RE.fullmatch(run_id) is None:
        raise HostBuildFailure("run-id must be 8-64 lowercase letters, digits or hyphens")
    raw = Path(path)
    try:
        if not raw.is_absolute() or raw.is_symlink():
            raise HostBuildFailure("run root must be an absolute non-symlink path")
        root = raw.resolve(strict=True)
    except HostBuildFailure:
        raise
    except OSError as exc:
        raise HostBuildFailure("run root is unavailable") from exc
    if raw.absolute() != root or root.name != run_id or not root.is_dir():
        raise HostBuildFailure("run root identity is not exact")
    if {item.name for item in root.iterdir()} != {"inputs"}:
        raise HostBuildFailure("fresh run root must contain only inputs")
    inputs = root / "inputs"
    if not inputs.is_dir() or inputs.is_symlink():
        raise HostBuildFailure("inputs must be a real directory")
    expected = {"verisilo", "upstream", FIREFOX_ARCHIVE_NAME}
    if {item.name for item in inputs.iterdir()} != expected:
        raise HostBuildFailure("input names are not exact")
    if any(item.is_symlink() for item in inputs.iterdir()):
        raise HostBuildFailure("input symlinks are not allowed")
    if not (inputs / "verisilo").is_dir() or not (inputs / "upstream").is_dir():
        raise HostBuildFailure("source inputs must be directories")
    if not (inputs / FIREFOX_ARCHIVE_NAME).is_file():
        raise HostBuildFailure("Firefox source input must be a regular file")
    return root, inputs


def _ordered_upstream_patches(upstream: Path) -> list[str]:
    paths = [
        item.relative_to(upstream).as_posix()
        for item in (upstream / "patches").rglob("*.patch")
        if item.is_file()
    ]
    paths.sort(key=lambda value: Path(value).name)
    return [p for p in paths if "roverfox" not in Path(p).parts] + [
        p for p in paths if "roverfox" in Path(p).parts
    ]


def _validate_inputs(inputs: Path) -> tuple[dict, dict]:
    verisilo = inputs / "verisilo"
    upstream = inputs / "upstream"
    archive = inputs / FIREFOX_ARCHIVE_NAME
    if _git(verisilo, "status", "--short", "--untracked-files=all", "--ignored=matching"):
        raise HostBuildFailure("VeriSilo input checkout is not completely clean")
    source = {
        "commit": _git(verisilo, "rev-parse", "HEAD"),
        "tree": _git(verisilo, "rev-parse", "HEAD^{tree}"),
        "lockPath": LOCK_REL.as_posix(),
    }
    lock_path = verisilo / LOCK_REL
    lock = _strict_json(lock_path)
    source["lockSha256"] = _sha(lock_path)
    if (
        lock.get("schema") != "verisilo-r1-formal-source-binding/v1"
        or lock.get("buildMode") != "formal"
        or lock.get("diagnosticOnly") is not False
        or lock.get("completeAppliedPatchOrder") != ORDER
        or [item.get("id") for item in lock.get("completePatchSeries", [])] != ORDER
    ):
        raise HostBuildFailure("Formal source-lock identity/order is not exact")
    if "9000" in json.dumps(lock["completePatchSeries"], sort_keys=True):
        raise HostBuildFailure("Formal source lock contains 9000 diagnostics")
    for item in lock["completePatchSeries"]:
        path = verisilo / item["path"]
        if (
            not path.is_file()
            or path.stat().st_size != item["sizeBytes"]
            or _sha(path) != item["sha256"]
            or item.get("diagnosticOnly") is not False
        ):
            raise HostBuildFailure(f"Formal patch binding differs: {item['id']}")
        if path.read_bytes().splitlines()[:1] == [b"# VERISILO-DIAGNOSTIC-MARKER: v1"]:
            raise HostBuildFailure("Formal patch carries a diagnostic marker")
    recipe = lock.get("buildBinding", {}).get("recipe", {})
    if [Path(item.get("path", "")).name for item in recipe.get("files", [])] != list(CONTEXT_NAMES):
        raise HostBuildFailure("Formal builder context membership is not exact")
    for item in recipe["files"]:
        path = verisilo / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"] or _sha(path) != item["sha256"]:
            raise HostBuildFailure(f"Formal recipe binding differs: {item['path']}")
    host_tool = lock.get("buildBinding", {}).get("hostTool")
    if (
        type(host_tool) is not dict
        or host_tool.get("path") != HOST_TOOL_REL.as_posix()
        or _sha(Path(__file__)) != host_tool.get("sha256")
        or Path(__file__).stat().st_size != host_tool.get("sizeBytes")
    ):
        raise HostBuildFailure("Formal host tool binding is not exact")
    expected_from = "FROM ubuntu:24.04@" + BASE_INDEX_DIGEST
    dockerfile = verisilo / recipe["files"][0]["path"]
    if dockerfile.read_text(encoding="utf-8").splitlines()[0] != expected_from:
        raise HostBuildFailure("Formal Dockerfile base digest is not exact")

    expected_upstream = lock["upstream"]
    actual_upstream = {
        "commit": _git(upstream, "rev-parse", "HEAD"),
        "tree": _git(upstream, "rev-parse", "HEAD^{tree}"),
        "tagCommit": _git(upstream, "rev-parse", f"refs/tags/{expected_upstream['tag']}^{{commit}}"),
        "status": _git(upstream, "status", "--short", "--untracked-files=all", "--ignored=matching"),
    }
    if (
        actual_upstream["commit"] != expected_upstream["commit"]
        or actual_upstream["tree"] != expected_upstream["tree"]
        or actual_upstream["tagCommit"] != expected_upstream["commit"]
        or actual_upstream["status"]
    ):
        raise HostBuildFailure("upstream checkout differs from the Formal source lock")
    upstream_rows = lock["sourceInputs"]["upstreamPatches"]
    if _ordered_upstream_patches(upstream) != [item["path"] for item in upstream_rows]:
        raise HostBuildFailure("upstream patch order differs from the Formal source lock")
    for item in upstream_rows + lock["sourceInputs"]["recipeFiles"]:
        path = upstream / item["path"]
        if not path.is_file() or path.stat().st_size != item["sizeBytes"] or _sha(path) != item["sha256"]:
            raise HostBuildFailure(f"upstream input binding differs: {item['path']}")
    firefox = lock["firefoxSource"]
    if archive.stat().st_size != firefox["sizeBytes"] or _sha(archive, "sha512") != firefox["sha512"]:
        raise HostBuildFailure("Firefox source archive differs from the Formal source lock")
    source["upstream"] = {
        "commit": actual_upstream["commit"],
        "tree": actual_upstream["tree"],
        "tag": expected_upstream["tag"],
    }
    source["firefoxArchive"] = {
        "name": archive.name,
        "sizeBytes": archive.stat().st_size,
        "sha512": firefox["sha512"],
    }
    return lock, source


def _validate_host(root: Path, lock: dict) -> tuple[str, dict]:
    if sys.platform != "linux" or platform.machine().lower() not in {"x86_64", "amd64"}:
        raise HostBuildFailure("host must be Linux x86_64")
    docker = shutil.which("docker")
    if docker is None:
        raise HostBuildFailure("docker is unavailable")
    gate = lock["buildBinding"]["resourceGate"]
    free = shutil.disk_usage(root).free
    swap = 0
    for line in Path("/proc/meminfo").read_text(encoding="ascii").splitlines():
        if line.startswith("SwapTotal:"):
            swap = int(line.split()[1]) * 1024
            break
    resources = {"freeBytes": free, "swapBytes": swap, "logicalCpu": os.cpu_count() or 0}
    if (
        free < gate["minimumFreeBytes"]
        or swap < gate["minimumSwapBytes"]
        or resources["logicalCpu"] < gate["minimumLogicalCpu"]
    ):
        raise HostBuildFailure("host resources are below the frozen minimum")
    return str(Path(docker).resolve()), resources


def _create_context(verisilo: Path, lock: dict, target: Path) -> dict:
    if target.exists():
        raise HostBuildFailure("refusing to overwrite builder context")
    rows = lock["buildBinding"]["recipe"]["files"]
    with target.open("xb") as raw:
        with tarfile.open(fileobj=raw, mode="w", format=tarfile.GNU_FORMAT) as bundle:
            for name, row in zip(CONTEXT_NAMES, rows, strict=True):
                data = (verisilo / row["path"]).read_bytes()
                info = tarfile.TarInfo(name)
                info.size = len(data)
                info.mode = 0o755 if name.endswith(".py") else 0o644
                info.mtime = info.uid = info.gid = 0
                info.uname = info.gname = ""
                bundle.addfile(info, io.BytesIO(data))
    return {**_file_record(target), "members": list(CONTEXT_NAMES)}


def _docker_env() -> dict[str, str]:
    return {
        "HOME": str(Path.home()),
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "TZ": "Etc/UTC",
    }


def _run_logged(command: list[str], cwd: Path, log: Path, env: dict[str, str], stdin: Path | None = None) -> int:
    mode = "xb"
    with log.open(mode) as output:
        input_stream = stdin.open("rb") if stdin else None
        try:
            process = subprocess.Popen(
                command,
                cwd=cwd,
                env=env,
                stdin=input_stream,
                stdout=output,
                stderr=subprocess.STDOUT,
            )
            return process.wait()
        finally:
            if input_stream:
                input_stream.close()


def _inspect_image(
    docker: str, image_id: str, root: Path, path: Path, source: dict
) -> dict:
    value = json.loads(_capture([docker, "image", "inspect", image_id], root, _docker_env()))
    if type(value) is not list or len(value) != 1 or value[0].get("Id") != image_id:
        raise HostBuildFailure("built image inspect identity is not exact")
    labels = value[0].get("Config", {}).get("Labels", {})
    if (
        labels.get("io.verisilo.recipe") != "camoufox-152.0.4-beta.28-r1-formal-v1"
        or labels.get("org.opencontainers.image.base.digest") != BASE_INDEX_DIGEST
        or labels.get("io.verisilo.base.linux-amd64-manifest") != BASE_AMD64_DIGEST
        or labels.get("io.verisilo.source.commit") != source["commit"]
        or labels.get("io.verisilo.source.tree") != source["tree"]
        or labels.get("io.verisilo.source-lock.sha256") != source["lockSha256"]
    ):
        raise HostBuildFailure("built image labels differ from the Formal recipe")
    _write_json_exclusive(path, value)
    return _file_record(path)


def _validate_strict_result(path: Path, run_id: str) -> dict:
    result = _strict_json(path)
    if (
        result.get("recordType") != "verisilo-camoufox-r1-formal-build-run/v1"
        or result.get("runId") != run_id
        or result.get("buildMode") != "formal"
        or result.get("diagnosticOnly") is not False
        or result.get("formalSource") is not True
        or result.get("formalR1Passed") is not False
        or result.get("browserLaunches") != 0
        or result.get("windowsRuntimeObserved") is not False
        or result.get("runtimeVerified") is not False
        or result.get("completeAppliedPatchOrder") != ORDER
        or result.get("claims", {}).get("compiled") is not True
    ):
        raise HostBuildFailure("strict build result claim boundary is not exact")
    return result


def execute(args: argparse.Namespace) -> int:
    started = _utc_now()
    root, inputs = _validate_run_root(args.run_root, args.run_id)
    lock, source = _validate_inputs(inputs)
    docker, resources = _validate_host(root, lock)
    layout = {}
    for name in ("provenance", "build-home", "work", "out"):
        path = root / name
        path.mkdir()
        layout[name] = path
    provenance = layout["provenance"]
    context_path = provenance / "builder-context.tar"
    context = _create_context(inputs / "verisilo", lock, context_path)
    metadata = provenance / "buildx-metadata.json"
    buildx_log = provenance / "buildx.log"
    tag = f"verisilo-camoufox-r1-formal-builder:{args.run_id}"
    build_command = [
        docker, "buildx", "build", "--pull=false", "--no-cache", "--load", "--progress=plain",
        "--metadata-file", str(metadata), "--tag", tag,
        "--label", f"io.verisilo.source.commit={source['commit']}",
        "--label", f"io.verisilo.source.tree={source['tree']}",
        "--label", f"io.verisilo.source-lock.sha256={source['lockSha256']}",
        "-f", "Dockerfile", "-",
    ]
    build_exit = _run_logged(build_command, root, buildx_log, _docker_env(), context_path)
    if build_exit != 0:
        raise HostBuildFailure(f"builder image build failed with exit code {build_exit}")
    image_id = _strict_json(metadata).get("containerimage.config.digest")
    if type(image_id) is not str or IMAGE_ID_RE.fullmatch(image_id) is None:
        raise HostBuildFailure("buildx metadata returned no immutable image ID")
    inspect_path = provenance / "builder-image-inspect.json"
    inspect = _inspect_image(docker, image_id, root, inspect_path, source)

    run_command = [
        docker, "run", "--rm", "--pull=never", "--read-only",
        "--mount", f"type=bind,src={inputs},dst=/inputs,readonly",
        "--mount", f"type=bind,src={layout['build-home']},dst=/build-home",
        "--mount", f"type=bind,src={layout['work']},dst=/work",
        "--mount", f"type=bind,src={layout['out']},dst=/out",
        "--tmpfs", "/tmp:rw,nosuid,nodev,exec,mode=1777,size=4g",
        "--env", f"VERISILO_BUILDER_IMAGE_ID={image_id}",
        "--env", f"VERISILO_SOURCE_COMMIT={source['commit']}",
        "--env", f"VERISILO_SOURCE_TREE={source['tree']}",
        "--env", f"VERISILO_SOURCE_LOCK_SHA256={source['lockSha256']}",
        "--env", f"VERISILO_BASE_IMAGE_INDEX_DIGEST={BASE_INDEX_DIGEST}",
        "--env", f"VERISILO_BASE_AMD64_MANIFEST_DIGEST={BASE_AMD64_DIGEST}",
        "--env", f"WINEPREFIX=/work/{args.run_id}/.wine-prefix",
        image_id, "--run-id", args.run_id,
        "--moz-build-date", lock["buildBinding"]["recipe"]["fixedEnvironment"]["MOZ_BUILD_DATE"],
    ]
    container_log = provenance / "container.log"
    container_exit = _run_logged(run_command, root, container_log, _docker_env())
    strict_dir = layout["out"] / args.run_id
    strict_path = strict_dir / ("build-result.json" if container_exit == 0 else "build-failure.json")
    if not strict_path.is_file():
        raise HostBuildFailure("container produced no strict result record")
    if container_exit == 0:
        _validate_strict_result(strict_path, args.run_id)
    host_result = {
        "recordType": "verisilo-r1-formal-build-host-provenance/v1",
        "runId": args.run_id,
        "status": "container-passed" if container_exit == 0 else "container-failed",
        "startedAtUtc": started,
        "completedAtUtc": _utc_now(),
        "source": source,
        "resourcesAtStart": resources,
        "builder": {
            "baseIndexDigest": BASE_INDEX_DIGEST,
            "baseLinuxAmd64ManifestDigest": BASE_AMD64_DIGEST,
            "buildCommand": build_command,
            "context": context,
            "buildxLog": _file_record(buildx_log),
            "buildxMetadata": _file_record(metadata),
            "imageId": image_id,
            "imageInspect": inspect,
        },
        "container": {
            "command": run_command,
            "exitCode": container_exit,
            "log": _file_record(container_log),
            "readOnlyRoot": True,
            "inputMountReadOnly": True,
            "driverInjection": False,
            "pullPolicy": "never",
        },
        "strictDriverRecord": {
            "path": strict_path.relative_to(root).as_posix(),
            "sha256": _sha(strict_path),
            "sizeBytes": strict_path.stat().st_size,
        },
        "claims": {
            "browserLaunches": 0,
            "formalR1Passed": False,
            "windowsRuntimeObserved": False,
            "runtimeVerified": False,
        },
    }
    _write_json_exclusive(provenance / "host-provenance.json", host_result)
    if container_exit:
        raise HostBuildFailure(f"Formal engine build failed with exit code {container_exit}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--run-root", required=True)
    args = parser.parse_args()
    try:
        return execute(args)
    except HostBuildFailure as exc:
        print(f"formal-host-build-failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
