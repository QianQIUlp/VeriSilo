#!/usr/bin/env python3
"""VeriSilo FP2 cross-realm consistency runner.

The browser path is deliberately separate from the FP1 probe and public Host
wire protocol.  The parent owns one loopback HTTP server for the complete
A1 -> A2 -> B1 sequence.  Each session is a fresh child Host process using
the existing Camoufox asset/Artifact validation and Windows Job lifecycle.

This module also contains the pure comparator used by the no-browser tests.
It never turns ``verified:false`` into a positive product claim.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import csv
import hashlib
import importlib
import importlib.metadata
import inspect
import json
import os
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import uuid
from collections import Counter
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Lock, Thread
from typing import Any, Optional
from urllib.parse import parse_qs, unquote, urlparse

REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
BUNDLE_DIR = REPO_ROOT / "tests" / "fingerprint-probe" / "fp2"
PROBE_MANIFEST_PATH = BUNDLE_DIR / "probe-bundle-manifest.json"
APPLICABILITY_PATH = BUNDLE_DIR / "applicability-ledger.json"
RELATION_PATH = BUNDLE_DIR / "relation-matrix.json"
NO_BROWSER_TEST_PATH = HOST_DIR / "test_fp2_cross_realm.py"
ARTIFACT_DIR = REPO_ROOT / "tests" / "fixtures" / "camoufox"
ARTIFACT_A_PATH = ARTIFACT_DIR / "identity-win-canvas-v1-a.json"
ARTIFACT_B_PATH = ARTIFACT_DIR / "identity-win-canvas-v1-b.json"
ASSET_LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-windows-x86_64.json"
TREE_MANIFEST_PATH = REPO_ROOT / "tests" / "fixtures" / "camoufox" / "browser-tree-manifest-verisilo-canvas-v1-windows.json"
BROWSER_ROOT = REPO_ROOT / "artifacts" / "camoufox-fp1" / "windows-candidate-20260818T061456Z-e571f6c" / "extracted-browser"
ARCHIVE_PATH = REPO_ROOT / "artifacts" / "camoufox-fp1" / "windows-candidate-20260818T061456Z-e571f6c" / "out" / "canvas-close-engine-20260816t144711z-e571f6c" / "camoufox-152.0.4-beta.28-win.x86_64.zip"
EXECUTABLE_PATH = BROWSER_ROOT / "camoufox.exe"
FP2_EVIDENCE_ROOT = REPO_ROOT / "artifacts" / "camoufox-fp2"
LEGACY_CLAIM_PATH = FP2_EVIDENCE_ROOT / "fp2-v1-one-shot-claim.json"
GENERATION2_CLAIM_PATH = FP2_EVIDENCE_ROOT / "fp2-v2-one-shot-claim.json"
GLOBAL_CLAIM_PATH = FP2_EVIDENCE_ROOT / "fp2-v3-one-shot-claim.json"
GLOBAL_LOCK_NAME = "fp2-v1-browser-global.lock"
RUNTIME_INTERPRETER_RELATIVE = Path("apps/camoufox-host/.venv/Scripts/python.exe")
EXPECTED_RUNTIME_PYTHON_VERSION = "3.12.13"
EXPECTED_RUNTIME_IMPLEMENTATION = "CPython"
EXPECTED_RUNTIME_DEPENDENCY_VERSIONS = {
    "camoufox": "0.5.4",
    "playwright": "1.60.0",
    "browserforge": "1.2.4",
}
RUNTIME_PREFLIGHT_WATCHDOG_SECONDS = 30
RUNTIME_PREFLIGHT_SCHEMA = "verisilo-camoufox-fp2-runtime-preflight/v1"
RUNTIME_PREFLIGHT_CHILD_SCHEMA = "verisilo-camoufox-fp2-runtime-preflight-child/v1"
EXECUTION_GENERATION = 3
PREVIOUS_BLOCKED_CLAIM_SHA256 = "e77204a09d9dfdbdf7d6c3b00a96114f477fd5b93d01c7fa6a7fd3dd71b28402"
PREVIOUS_BLOCKED_RUN_ID = "fp2-20260820T121344Z-470b08fdb9"
PREVIOUS_BLOCKED_CLASSIFICATION = "pre-browser-runtime-dependency-block"
PREVIOUS_BLOCKED_REASON = (
    "original FP2 contract consumed the one-shot claim before exact child runtime dependency closure"
)
GENERATION2_CLAIM_SHA256 = "bcf9170cb26e46a35664ebad3cd8b39a2ec93928e597b21b84037e8cc6f22b67"
GENERATION2_RUN_ID = "fp2-20260821T053550Z-9f98de991d"
GENERATION2_REPORT_SHA256 = "d0dd41195134686527567586734929d7a13ed54ab647247d6e8f1d253095352b"
GENERATION2_CLASSIFICATION = "harness-http-capture-failure"
GENERATION2_REASON = "generation-2 formal execution failed before any valid realm observation because the HTTP evidence handler crashed"

TASK_VERSION = "fp2-v3"
REPORT_SCHEMA = "verisilo-camoufox-fp2-cross-realm-run/v3"
CLAIM_SCHEMA = "verisilo-camoufox-fp2-one-shot-claim/v3"
ADJUDICATION_SCHEMA = "verisilo-camoufox-fp2-offline-adjudication/v1"
CANONICAL_REALMS = (
    "top-window",
    "same-origin-iframe",
    "cross-origin-iframe",
    "dedicated-worker",
    "shared-worker",
    "service-worker",
)
WINDOW_REALMS = CANONICAL_REALMS[:3]
WORKER_REALMS = CANONICAL_REALMS[3:]
SESSION_LABELS = ("A1", "A2", "B1")
EXPECTED_STATIC_DIFF = (
    "audio:seed",
    "canvas:seed",
    "fonts",
    "fonts:spacing_seed",
    "navigator.hardwareConcurrency",
    "screen.availHeight",
    "screen.availTop",
    "screen.availWidth",
    "screen.height",
    "screen.width",
    "window.history.length",
    "window.screenX",
    "window.screenY",
)
EXPECTED_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1"
EXPECTED_RELEASE = "v152.0.4-beta.28"
EXPECTED_PLATFORM = "windows-x86_64"
EXPECTED_SOURCE_COMMIT = "e571f6c0b2cea90955b929a4ff04ad54007778fa"
EXPECTED_ARCHIVE_SHA256 = "148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94"
EXPECTED_ARCHIVE_SIZE = 493100709
EXPECTED_EXECUTABLE_SHA256 = "172f51387bc61e331446883e5499c67611aea5fd81091f68df26b166c9687bf1"
EXPECTED_ASSET_LOCK_SHA256 = "ce05302d317ec562b096eba52e806ed20302d99d472229640c5eea840d7f98ac"
EXPECTED_TREE_MANIFEST_SHA256 = "3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9"
EXPECTED_TREE_CANONICAL_SHA256 = "42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1"
BASELINE_HEAD = "c1032a4bc926c308fa9cf7883fbfc834e27e09b0"
BASELINE_TREE = "368ab4282e13ca7a640f772c80fc703007ceb692"
PRIMARY_HOST = "127.0.0.1"
SECONDARY_HOST = "localhost"
DEFAULT_RUN_PORT = 18192
TARGET_PROCESS_IMAGES = ("camoufox.exe", "verisilo-camoufox-supervisor.exe")
POWERSHELL_PROCESS_ENUMERATION_SCRIPT = (
    "$ErrorActionPreference = 'Stop'; "
    "$names = @('camoufox', 'verisilo-camoufox-supervisor'); "
    "$payload = [ordered]@{ processes = @( "
    "Get-Process -ErrorAction Stop | "
    "Where-Object { $names -contains $_.ProcessName } | "
    "ForEach-Object { [ordered]@{ imageName = ($_.ProcessName + '.exe'); pid = [int]$_.Id } } "
    ") }; "
    "$payload | ConvertTo-Json -Compress"
)
BROWSER_OPERATION_DEADLINE_SECONDS = 3
REALM_STAGE_DEADLINE_SECONDS = 15
SESSION_WATCHDOG_SECONDS = 60
PARENT_WATCHDOG_SECONDS = 120
HOST_CLOSE_CONTEXT_SECONDS = 10
HOST_CLOSE_PROCESS_TREE_SECONDS = 8
HEX64 = re.compile(r"^[0-9a-f]{64}$")
ABSOLUTE_PATH = re.compile(r"(?:^[A-Za-z]:[\\/]|^/|\\\\)")
SECRET_WORD = re.compile(
    r"(?:\bpassword\b|\bpasswd\b|\btoken\b|\bsecret\b|\bauthorization\b|\bbearer\b|\bapi[_-]?key\b|\bprivate[_-]?key\b)",
    re.IGNORECASE,
)

if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

import host_v1 as host_module


class FP2Failure(RuntimeError):
    """A fail-closed, user-facing FP2 result."""

    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(detail or code)
        self.code = code
        self.detail = detail or code


def fail(code: str, detail: str = "") -> None:
    raise FP2Failure(code, detail)


def require(condition: bool, code: str, detail: str = "") -> None:
    if not condition:
        fail(code, detail)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def canonical_json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def strict_json_bytes(raw: bytes, label: str) -> Any:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                fail("invalid_json", f"{label}: duplicate key {key}")
            result[key] = value
        return result

    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=reject_duplicates)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        fail("invalid_json", f"{label}: {type(exc).__name__}")


def strict_json(path: Path, label: Optional[str] = None) -> Any:
    return strict_json_bytes(path.read_bytes(), label or path.as_posix())


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes((json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def copy_json(path: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(path.read_bytes())


def relative_repo_path(path: Path) -> str:
    return path.resolve().relative_to(REPO_ROOT.resolve()).as_posix()


def child_environment() -> dict[str, str]:
    """Construct the exact environment shared by preflight and browser children."""
    environment = os.environ.copy()
    environment["PYTHONUNBUFFERED"] = "1"
    environment["PYTHONPATH"] = str(HOST_DIR)
    return environment


def resolve_runtime_interpreter() -> Path:
    """Resolve the repository-owned FP1 runtime used for every FP2 child."""
    require(os.name == "nt", "runtime_native_windows_required", "FP2 runtime preflight requires native Windows")
    path = (REPO_ROOT / RUNTIME_INTERPRETER_RELATIVE).resolve()
    require(path.is_file(), "runtime_interpreter_missing", RUNTIME_INTERPRETER_RELATIVE.as_posix())
    require(path.suffix.lower() == ".exe", "runtime_interpreter_invalid", RUNTIME_INTERPRETER_RELATIVE.as_posix())
    try:
        path.relative_to(REPO_ROOT.resolve())
    except ValueError:
        fail("runtime_interpreter_outside_repo", RUNTIME_INTERPRETER_RELATIVE.as_posix())
    return path


def runtime_invocation_descriptor(interpreter: Path) -> dict[str, Any]:
    """Describe the non-secret child launch contract without recording absolute argv."""
    return {
        "interpreterRelativePath": relative_repo_path(interpreter),
        "scriptRelativePath": relative_repo_path(Path(__file__).resolve()),
        "cwdRelativePath": ".",
        "pythonPathRelative": [relative_repo_path(HOST_DIR)],
        "environmentOverrides": {"PYTHONUNBUFFERED": "1", "PYTHONPATH": relative_repo_path(HOST_DIR)},
        "entrypoints": {
            "preflight": "--runtime-preflight-child",
            "session": "--child-session",
        },
        "sessionStopBoundary": "before AsyncNewBrowser(...)",
    }


def runtime_dependency_closure_sha256(closure: dict[str, Any]) -> str:
    return sha256_bytes(canonical_json_bytes(closure))


def resolve_browser_launch_dependencies() -> dict[str, Any]:
    """Resolve exactly the imports used immediately before the browser spawn call."""
    from camoufox import AsyncNewBrowser, DefaultAddons
    from camoufox.utils import launch_options
    from playwright.async_api import async_playwright
    from run_spike import firefox_user_prefs_for_config, normalize_camou_config_env

    return {
        "AsyncNewBrowser": AsyncNewBrowser,
        "DefaultAddons": DefaultAddons,
        "launch_options": launch_options,
        "async_playwright": async_playwright,
        "firefox_user_prefs_for_config": firefox_user_prefs_for_config,
        "normalize_camou_config_env": normalize_camou_config_env,
    }


def runtime_dependency_snapshot() -> dict[str, Any]:
    """Import and resolve all project/runtime dependencies without launching a browser."""
    external = {}
    for module_name, distribution_name in (
        ("camoufox", "camoufox"),
        ("playwright", "playwright"),
        ("browserforge", "browserforge"),
    ):
        try:
            importlib.import_module(module_name)
            version = importlib.metadata.version(distribution_name)
        except (ImportError, importlib.metadata.PackageNotFoundError) as exc:
            fail("runtime_dependency_missing", module_name)
        except Exception as exc:  # noqa: BLE001 - dependency bootstrap is fail-closed
            fail("runtime_dependency_resolution_failed", module_name)
        expected = EXPECTED_RUNTIME_DEPENDENCY_VERSIONS[module_name]
        require(version == expected, "runtime_dependency_version_mismatch", module_name)
        external[module_name] = {"available": True, "version": version}

    project_modules = {}
    for module_name in ("host_v1", "browser_asset", "host_platform", "run_spike"):
        try:
            importlib.import_module(module_name)
        except ImportError as exc:
            fail("runtime_host_import_missing", module_name)
        except Exception as exc:  # noqa: BLE001 - project bootstrap is fail-closed
            fail("runtime_host_import_failed", module_name)
        project_modules[module_name] = {"available": True}

    dependencies = resolve_browser_launch_dependencies()
    require(callable(dependencies["AsyncNewBrowser"]), "runtime_browser_spawn_boundary_unavailable", "AsyncNewBrowser")
    require(hasattr(dependencies["DefaultAddons"], "UBO"), "runtime_browser_spawn_boundary_unavailable", "DefaultAddons.UBO")
    for name in (
        "launch_options",
        "async_playwright",
        "firefox_user_prefs_for_config",
        "normalize_camou_config_env",
    ):
        require(callable(dependencies[name]), "runtime_browser_spawn_boundary_unavailable", name)
    for name in ("AsyncNewBrowser", "launch_options", "async_playwright"):
        require(inspect.signature(dependencies[name]), "runtime_browser_spawn_boundary_unavailable", name)

    return {
        "external": external,
        "project": project_modules,
        "browserSpawnBoundary": {
            "ready": True,
            "browserLaunchCalled": False,
            "nextCall": "AsyncNewBrowser(playwright, from_options=opts, persistent_context=True)",
        },
    }


def safe_nonce_hash(nonce: str) -> str:
    return f"sha256:{sha256_bytes(nonce.encode('ascii'))}"


def hash_value(value: Any) -> str:
    return f"sha256:{sha256_bytes(canonical_json_bytes(value))}"


def load_strict(path: Path, expected_schema: str) -> dict[str, Any]:
    value = strict_json(path)
    require(type(value) is dict, "invalid_json_shape", path.as_posix())
    require(value.get("schema") == expected_schema, "schema_mismatch", path.as_posix())
    return value


def contains_string(value: Any, expected: str) -> bool:
    if isinstance(value, str):
        return value == expected
    if isinstance(value, dict):
        return any(contains_string(item, expected) for item in value.values())
    if isinstance(value, list):
        return any(contains_string(item, expected) for item in value)
    return False


def load_applicability(path: Path = APPLICABILITY_PATH) -> dict[str, Any]:
    ledger = load_strict(path, "verisilo-camoufox-fp2-applicability/v1")
    surface_order = ledger.get("surfaceOrder")
    realms = ledger.get("realms")
    allowed = set(ledger.get("classificationValues", []))
    require(
        type(surface_order) is list and all(type(item) is str for item in surface_order),
        "applicability_invalid",
        "surfaceOrder",
    )
    require(set(realms or {}) == set(CANONICAL_REALMS), "applicability_invalid", "realm set")
    require(allowed == {"required", "conditional-if-api-present", "not-applicable"}, "applicability_invalid", "classification values")
    for realm in CANONICAL_REALMS:
        item = realms[realm]
        require(type(item) is dict, "applicability_invalid", realm)
        statuses = item.get("surfaceStatus")
        require(type(statuses) is dict and set(statuses) == set(surface_order), "applicability_invalid", f"{realm}.surfaceStatus")
        require(all(value in allowed for value in statuses.values()), "applicability_invalid", f"{realm}.status value")
        require(item.get("kind") in {"window", "worker"}, "applicability_invalid", f"{realm}.kind")
    return ledger


def load_relation_matrix(path: Path = RELATION_PATH) -> dict[str, Any]:
    matrix = load_strict(path, "verisilo-camoufox-fp2-relations/v1")
    mapping = matrix.get("artifactDiffMapping")
    require(type(mapping) is dict and set(mapping) == set(EXPECTED_STATIC_DIFF), "relation_matrix_invalid", "artifact diff mapping")
    for key in EXPECTED_STATIC_DIFF:
        item = mapping[key]
        require(type(item) is dict, "relation_matrix_invalid", key)
        require(type(item.get("realms")) is list and item["realms"], "relation_matrix_invalid", f"{key}.realms")
        require(type(item.get("observation")) is str and type(item.get("relation")) is str, "relation_matrix_invalid", key)
    headers = matrix.get("headers")
    require(type(headers) is dict, "relation_matrix_invalid", "headers")
    require(headers.get("identityHeaders") == ["user-agent", "accept-language", "accept-encoding", "dnt", "sec-gpc"], "relation_matrix_invalid", "headers.identityHeaders")
    return matrix


def load_probe_manifest(path: Path = PROBE_MANIFEST_PATH) -> tuple[dict[str, Any], str]:
    manifest = load_strict(path, "verisilo-camoufox-fp2-probe-bundle/v1")
    files = manifest.get("files")
    require(type(files) is list and files, "probe_manifest_invalid", "files")
    seen: set[str] = set()
    for item in files:
        require(type(item) is dict and set(item) == {"path", "sha256", "size"}, "probe_manifest_invalid", "file entry")
        rel = item["path"]
        require(type(rel) is str and rel and rel not in seen and ".." not in Path(rel).parts and not Path(rel).is_absolute(), "probe_manifest_invalid", rel)
        seen.add(rel)
        require(type(item["sha256"]) is str and HEX64.fullmatch(item["sha256"]), "probe_manifest_invalid", f"{rel}.sha256")
        require(type(item["size"]) is int and item["size"] >= 0, "probe_manifest_invalid", f"{rel}.size")
        file_path = BUNDLE_DIR / rel
        require(file_path.is_file(), "probe_bundle_file_missing", rel)
        require(file_path.stat().st_size == item["size"], "probe_bundle_hash_mismatch", rel)
        require(sha256_file(file_path) == item["sha256"], "probe_bundle_hash_mismatch", rel)
    expected = {
        "controlled.html",
        "controlled.js",
        "dedicated-worker.js",
        "frame.html",
        "frame.js",
        "realm-common.js",
        "service-worker.js",
        "shared-worker.js",
        "top.html",
        "top.js",
    }
    require(seen == expected, "probe_manifest_invalid", "bundle file set")
    return manifest, sha256_file(path)


def load_artifact(path: Path) -> tuple[dict[str, Any], dict[str, Any]]:
    raw = path.read_bytes()
    digest = sha256_bytes(raw)
    sidecar_path = path.with_name(path.name + ".sha256")
    require(sidecar_path.is_file(), "artifact_sidecar_missing", path.as_posix())
    sidecar = sidecar_path.read_text(encoding="utf-8")
    require(sidecar == f"{digest}  {path.name}\n", "artifact_sidecar_mismatch", path.as_posix())
    artifact = strict_json_bytes(raw, path.as_posix())
    require(type(artifact) is dict and type(artifact.get("resolvedConfig")) is dict, "artifact_invalid", path.as_posix())
    require(len(artifact["resolvedConfig"]) == 47, "artifact_invalid", f"{path.name}: resolvedConfig key count")
    return artifact, {"path": relative_repo_path(path), "size": len(raw), "sha256": digest}


def config_diff(a: dict[str, Any], b: dict[str, Any]) -> list[str]:
    return sorted(key for key in set(a) | set(b) if a.get(key, object()) != b.get(key, object()))


def build_static_diff(a: dict[str, Any], b: dict[str, Any], relation: dict[str, Any]) -> dict[str, Any]:
    diff = config_diff(a["resolvedConfig"], b["resolvedConfig"])
    require(diff == list(EXPECTED_STATIC_DIFF), "artifact_baseline_drift", json.dumps(diff))
    mapping = relation["artifactDiffMapping"]
    entries = []
    for key in diff:
        entries.append(
            {
                "key": key,
                "aValueSha256": hash_value(a["resolvedConfig"][key]),
                "bValueSha256": hash_value(b["resolvedConfig"][key]),
                "realms": mapping[key]["realms"],
                "observation": mapping[key]["observation"],
                "relation": mapping[key]["relation"],
            }
        )
    return {
        "schema": "verisilo-camoufox-fp2-static-ab-diff/v1",
        "keys": diff,
        "entries": entries,
        "aArtifactSha256": None,
        "bArtifactSha256": None,
    }


def git_command(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        fail("git_preflight_failed", args[0] if args else "git")
    return result.stdout.strip()


def git_preflight() -> dict[str, Any]:
    status = git_command("status", "--porcelain")
    require(status == "", "baseline_worktree_dirty", "tracked worktree is not clean")
    branch = git_command("branch", "--show-current")
    require(branch == "codex/camoufox-m3-engine-adapter", "baseline_branch_mismatch", branch)
    head = git_command("rev-parse", "HEAD")
    tree = git_command("rev-parse", "HEAD^{tree}")
    require(tree, "baseline_tree_missing")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", BASELINE_HEAD, head],
        cwd=REPO_ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    require(ancestor.returncode == 0, "baseline_ancestry_mismatch", head)
    accepted = subprocess.run(
        ["git", "merge-base", "--is-ancestor", "e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd", head],
        cwd=REPO_ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    require(accepted.returncode == 0, "accepted_ancestor_missing", head)
    upstream = git_command("rev-list", "--left-right", "--count", "@{upstream}...HEAD")
    behind, ahead = (int(part) for part in upstream.split())
    return {
        "branch": branch,
        "head": head,
        "tree": tree,
        "baselineHead": BASELINE_HEAD,
        "baselineTree": BASELINE_TREE,
        "baselineHeadMatches": head == BASELINE_HEAD,
        "baselineTreeMatches": tree == BASELINE_TREE,
        "upstream": {"behind": behind, "ahead": ahead},
        "trackedWorktreeClean": True,
    }


def validate_candidate_static() -> dict[str, Any]:
    require(os.name == "nt", "native_windows_required", "FP2 requires native Windows")
    require(ASSET_LOCK_PATH.is_file(), "candidate_lock_missing")
    require(sha256_file(ASSET_LOCK_PATH) == EXPECTED_ASSET_LOCK_SHA256, "candidate_lock_hash_mismatch")
    require(ARCHIVE_PATH.is_file(), "candidate_archive_missing")
    require(sha256_file(ARCHIVE_PATH) == EXPECTED_ARCHIVE_SHA256, "candidate_archive_hash_mismatch")
    require(ARCHIVE_PATH.stat().st_size == EXPECTED_ARCHIVE_SIZE, "candidate_archive_size_mismatch")
    require(EXECUTABLE_PATH.is_file(), "candidate_executable_missing")
    require(sha256_file(EXECUTABLE_PATH) == EXPECTED_EXECUTABLE_SHA256, "candidate_executable_hash_mismatch")
    require(TREE_MANIFEST_PATH.is_file(), "candidate_tree_manifest_missing")
    require(sha256_file(TREE_MANIFEST_PATH) == EXPECTED_TREE_MANIFEST_SHA256, "candidate_tree_manifest_hash_mismatch")
    require(BROWSER_ROOT.is_dir(), "candidate_browser_root_missing")
    from browser_asset import load_asset_lock, verify_self_built_browser_root

    try:
        lock = load_asset_lock(
            ASSET_LOCK_PATH,
            expected_release=EXPECTED_RELEASE,
            expected_platform=EXPECTED_PLATFORM,
        )
    except Exception as exc:  # noqa: BLE001 - candidate mismatch is blocked
        fail("candidate_lock_invalid", type(exc).__name__)
    require(lock.get("engineRevision") == EXPECTED_ENGINE_REVISION, "candidate_engine_revision_mismatch")
    require(lock.get("sha256") == EXPECTED_ARCHIVE_SHA256 and lock.get("sizeBytes") == EXPECTED_ARCHIVE_SIZE, "candidate_lock_binding_mismatch")
    require(lock.get("verified") is False and lock.get("evidenceClass") == "compiled-not-runtime-verified", "candidate_evidence_semantics_mismatch")
    require(contains_string(lock, EXPECTED_SOURCE_COMMIT), "candidate_source_binding_mismatch")
    try:
        receipt = verify_self_built_browser_root(
            lock,
            BROWSER_ROOT,
            repo_root=REPO_ROOT,
            tree_manifest_path=TREE_MANIFEST_PATH,
            verify_tree_contents=True,
        )[1]
    except Exception as exc:  # noqa: BLE001 - candidate mismatch is blocked
        fail("candidate_binding_invalid", type(exc).__name__)
    require(receipt.get("fileCount") == 503 and receipt.get("totalBytes") == 981205753, "candidate_tree_shape_mismatch")
    require(receipt.get("treeManifestCanonicalSha256") == EXPECTED_TREE_CANONICAL_SHA256, "candidate_tree_canonical_mismatch")
    require((receipt.get("tree") or {}).get("manifestSha256") == EXPECTED_TREE_CANONICAL_SHA256, "candidate_tree_canonical_mismatch")
    return {
        "engineRevision": EXPECTED_ENGINE_REVISION,
        "sourceCommit": EXPECTED_SOURCE_COMMIT,
        "archive": {"path": relative_repo_path(ARCHIVE_PATH), "sha256": EXPECTED_ARCHIVE_SHA256, "size": EXPECTED_ARCHIVE_SIZE},
        "executable": {"path": relative_repo_path(EXECUTABLE_PATH), "sha256": EXPECTED_EXECUTABLE_SHA256, "size": EXECUTABLE_PATH.stat().st_size},
        "assetLock": {"path": relative_repo_path(ASSET_LOCK_PATH), "rawSha256": EXPECTED_ASSET_LOCK_SHA256},
        "treeManifest": {"path": relative_repo_path(TREE_MANIFEST_PATH), "rawSha256": EXPECTED_TREE_MANIFEST_SHA256, "canonicalSha256": EXPECTED_TREE_CANONICAL_SHA256, "fileCount": 503, "totalBytes": 981205753},
        "browserRoot": relative_repo_path(BROWSER_ROOT),
    }


def surface_status(ledger: dict[str, Any], realm: str, surface: str) -> str:
    return ledger["realms"][realm]["surfaceStatus"][surface]


def surface_capability(result: dict[str, Any], surface: str) -> tuple[bool, str]:
    capabilities = result.get("capabilities") or {}
    capability = capabilities.get(surface)
    if surface == "privacySignals":
        if not isinstance(capability, dict):
            return False, "privacy_capability_shape_missing"
        dnt = capability.get("doNotTrack") or {}
        gpc = capability.get("globalPrivacyControl") or {}
        if not isinstance(dnt, dict) or not isinstance(gpc, dict):
            return False, "privacy_capability_shape_invalid"
        return bool(dnt.get("apiPresent") and gpc.get("apiPresent")), "privacy_api_missing"
    if surface == "maxTouchPoints":
        if not isinstance(capability, dict):
            return False, "max_touch_capability_shape_missing"
        return bool(capability.get("apiPresent")), str(capability.get("reason") or "max_touch_api_missing")
    if not isinstance(capability, dict):
        return False, "capability_shape_missing"
    return bool(capability.get("apiPresent")), str(capability.get("reason") or "api_missing")


def surface_value(result: dict[str, Any], surface: str) -> Any:
    values = {
        "navigator": result.get("navigator"),
        "localeTimezone": result.get("locale"),
        "screenDpr": {"screen": result.get("screen"), "devicePixelRatio": result.get("devicePixelRatio")},
        "geometry": result.get("geometry"),
        "history": result.get("historyLength"),
        "canvas": result.get("canvas"),
        "audio": result.get("audio"),
        "webgl": result.get("webgl"),
        "webgl2": result.get("webgl2"),
        "fonts": result.get("fonts"),
        "voices": result.get("voices"),
        "mediaDevices": result.get("mediaDevices"),
        "privacySignals": result.get("privacySignals"),
        "maxTouchPoints": result.get("maxTouchPoints"),
        "httpHeaders": result.get("requestHeaders"),
        "workerCanvas": result.get("workerCanvas"),
    }
    return values.get(surface)


def normalize_voice_projection(voices: Any) -> list[dict[str, Any]]:
    if not isinstance(voices, list):
        return []
    normalized = []
    for voice in voices:
        if not isinstance(voice, dict):
            normalized.append({})
            continue
        normalized.append(
            {
                "name": voice.get("name"),
                "lang": voice.get("lang"),
                "voiceURI": voice.get("voiceURI"),
                "isDefault": voice.get("isDefault"),
                "isLocalService": voice.get("localService", voice.get("isLocalService")),
            }
        )
    return normalized


def expected_voice_projection(config: dict[str, Any]) -> list[dict[str, Any]]:
    values = config.get("voices")
    if not isinstance(values, list):
        return []
    return [
        {
            "name": value.get("name"),
            "lang": value.get("lang"),
            "voiceURI": value.get("voiceUri", value.get("voiceURI")),
            "isDefault": value.get("isDefault", value.get("default")),
            "isLocalService": value.get("isLocalService", value.get("localService")),
        }
        for value in values
        if isinstance(value, dict)
    ]


def validate_png_surface(label: str, realm: str, result: dict[str, Any], status: str) -> None:
    if status == "not-applicable":
        return
    present, reason = surface_capability(result, "canvas" if result.get("kind") == "window" else "workerCanvas")
    canvas = result.get("canvas") if result.get("kind") == "window" else result.get("workerCanvas")
    if status == "required" and not present:
        fail("realm_capability_missing", f"{label}.{realm}.canvas:{reason}")
    if status == "conditional-if-api-present" and not present:
        return
    require(isinstance(canvas, dict), "conditional_surface_uncompared", f"{label}.{realm}.canvas")
    if canvas.get("resultPresent") is False:
        fail("conditional_surface_uncompared", f"{label}.{realm}.workerCanvas")
    required_hashes = ["rawHash", "rawRgbaHash", "decodedPngPixelsHash", "pngBytesHash"]
    if result.get("kind") == "window":
        required_hashes.extend(["dataUrlHash", "exportHash"])
    else:
        require(canvas.get("exportHash"), "conditional_surface_uncompared", f"{label}.{realm}.workerCanvas.exportHash")
    for key in required_hashes:
        require(type(canvas.get(key)) is str and canvas[key].startswith("sha256:") and HEX64.fullmatch(canvas[key][7:]), "png_invalid", f"{label}.{realm}.{key}")
    require(canvas.get("rawHash") == canvas.get("rawRgbaHash"), "canvas_internal_mismatch", f"{label}.{realm}.raw")
    png = canvas.get("png")
    require(isinstance(png, dict), "png_invalid", f"{label}.{realm}.png")
    require(
        png.get("signatureValid") is True
        and png.get("decodeValid") is True
        and png.get("width") == 240
        and png.get("height") == 120
        and png.get("mimeType") == "image/png",
        "png_invalid",
        f"{label}.{realm}.png contract",
    )


def validate_header_coherence(label: str, realm: str, result: dict[str, Any], artifact: dict[str, Any]) -> None:
    headers = result.get("requestHeaders")
    navigator_value = result.get("navigator") or {}
    require(isinstance(headers, dict), "header_observation_missing", f"{label}.{realm}")
    identity = headers.get("identityHeaders")
    require(isinstance(identity, dict), "header_observation_missing", f"{label}.{realm}.identityHeaders")
    for key in ("user-agent", "accept-language", "accept-encoding", "dnt", "sec-gpc"):
        require(key in identity, "header_observation_missing", f"{label}.{realm}.{key}")
    require(identity["user-agent"] == navigator_value.get("userAgent"), "header_js_mismatch", f"{label}.{realm}.User-Agent")
    language = str(navigator_value.get("language") or "")
    languages = [str(value) for value in navigator_value.get("languages", [])]
    require(language and languages and languages[0].casefold() == language.casefold(), "accept_language_mismatch", f"{label}.{realm}.navigator")
    header_tokens = [part.split(";", 1)[0].strip() for part in str(identity["accept-language"] or "").split(",") if part.strip()]
    require(header_tokens == languages, "accept_language_mismatch", f"{label}.{realm}.token order")
    config = artifact["resolvedConfig"]
    require(identity["accept-encoding"] == config["headers.Accept-Encoding"], "accept_encoding_mismatch", f"{label}.{realm}")
    configured_dnt = config.get("navigator.doNotTrack")
    if configured_dnt is not None:
        require(identity["dnt"] == str(configured_dnt), "dnt_mapping_mismatch", f"{label}.{realm}")
    configured_gpc = config.get("navigator.globalPrivacyControl")
    if configured_gpc is not None:
        expected = "1" if configured_gpc is True else "0"
        require(identity["sec-gpc"] == expected, "gpc_mapping_mismatch", f"{label}.{realm}")
    policy = headers.get("requestPolicy")
    require(
        policy == {"method": "GET", "cache": "no-store", "credentials": "omit"},
        "header_request_policy_mismatch",
        f"{label}.{realm}",
    )


def validate_realm_result(label: str, realm: str, result: dict[str, Any], artifact: dict[str, Any], ledger: dict[str, Any]) -> dict[str, Any]:
    require(isinstance(result, dict), "realm_result_missing", f"{label}.{realm}")
    require(result.get("realm") == realm, "realm_label_mismatch", f"{label}.{realm}")
    kind = result.get("kind")
    require(kind == ledger["realms"][realm]["kind"], "realm_kind_mismatch", f"{label}.{realm}")
    require(isinstance(result.get("navigator"), dict), "realm_result_missing", f"{label}.{realm}.navigator")
    require(isinstance(result.get("locale"), dict), "realm_result_missing", f"{label}.{realm}.locale")
    for surface in ledger["surfaceOrder"]:
        if surface in {"registrationState", "storage"}:
            continue
        status = surface_status(ledger, realm, surface)
        if status == "not-applicable":
            continue
        present, reason = surface_capability(result, surface)
        if status == "required" and not present:
            fail("realm_capability_missing", f"{label}.{realm}.{surface}:{reason}")
        if status == "conditional-if-api-present" and not present:
            continue
        value = surface_value(result, surface)
        if value is None:
            fail("conditional_surface_uncompared", f"{label}.{realm}.{surface}")
    navigator_value = result.get("navigator") or {}
    config = artifact["resolvedConfig"]
    require(navigator_value.get("userAgent") == config["navigator.userAgent"], "navigator_config_mismatch", f"{label}.{realm}.userAgent")
    require(navigator_value.get("platform") == config["navigator.platform"], "navigator_config_mismatch", f"{label}.{realm}.platform")
    require(navigator_value.get("hardwareConcurrency") == config["navigator.hardwareConcurrency"], "navigator_config_mismatch", f"{label}.{realm}.hardwareConcurrency")
    expected_locale = artifact.get("policy", {}).get("locale") or "-".join(filter(None, [config.get("locale:language"), config.get("locale:region")]))
    require(str(navigator_value.get("language", "")).casefold() == str(expected_locale).casefold(), "locale_mismatch", f"{label}.{realm}.language")
    require((result.get("locale") or {}).get("timeZone") == config["timezone"], "timezone_mismatch", f"{label}.{realm}")
    require(isinstance((result.get("locale") or {}).get("utcOffsetMinutes"), int), "utc_offset_missing", f"{label}.{realm}")
    dnt = config.get("navigator.doNotTrack")
    if dnt is not None and surface_status(ledger, realm, "privacySignals") == "required":
        require(navigator_value.get("doNotTrack") == dnt, "dnt_mapping_mismatch", f"{label}.{realm}.navigator")
    gpc = config.get("navigator.globalPrivacyControl")
    if gpc is not None and surface_status(ledger, realm, "privacySignals") == "required":
        require(navigator_value.get("globalPrivacyControl") is gpc, "gpc_mapping_mismatch", f"{label}.{realm}.navigator")
    if kind == "window":
        screen_value = result.get("screen") or {}
        for field in ("width", "height", "availWidth", "availHeight", "availTop", "availLeft", "colorDepth", "pixelDepth"):
            require(screen_value.get(field) == config[f"screen.{field}"], "screen_config_mismatch", f"{label}.{realm}.screen.{field}")
        if realm == "top-window":
            geometry = result.get("geometry") or {}
            for field in ("outerWidth", "outerHeight", "screenX", "screenY"):
                require(geometry.get(field) == config[f"window.{field}"], "geometry_config_mismatch", f"{label}.{realm}.{field}")
            require(result.get("historyLength") == config["window.history.length"], "history_config_mismatch", label)
        if surface_status(ledger, realm, "fonts") != "not-applicable":
            fonts = result.get("fonts") or {}
            families = [item.get("family") for item in fonts.get("injectedFonts", []) if isinstance(item, dict)]
            require(families == config["fonts"], "font_config_mismatch", f"{label}.{realm}")
        if surface_status(ledger, realm, "voices") in {"required", "conditional-if-api-present"} and surface_capability(result, "voices")[0]:
            require(normalize_voice_projection((result.get("voices") or {}).get("voices")) == expected_voice_projection(config), "voice_config_mismatch", f"{label}.{realm}")
        if surface_status(ledger, realm, "mediaDevices") in {"required", "conditional-if-api-present"} and surface_capability(result, "mediaDevices")[0]:
            counts = (result.get("mediaDevices") or {}).get("counts")
            expected_counts = {
                "audioinput": config["mediaDevices:micros"],
                "videoinput": config["mediaDevices:webcams"],
                "audiooutput": config["mediaDevices:speakers"],
            }
            if config["mediaDevices:enabled"] is False:
                expected_counts = {key: 0 for key in expected_counts}
            require(counts == expected_counts, "media_config_mismatch", f"{label}.{realm}")
    # Registration and Profile storage are returned by the top-level
    # orchestrator, not by the individual Window/Worker realm object.
    # Their applicability remains required in the ledger and is checked by
    # validate_session_result below.
    validate_png_surface(label, realm, result, surface_status(ledger, realm, "canvas" if kind == "window" else "workerCanvas"))
    if kind == "window" and surface_status(ledger, realm, "audio") == "required":
        audio = result.get("audio") or {}
        audio_hash = audio.get("audioHash")
        require(
            isinstance(audio_hash, str)
            and audio_hash.startswith("sha256:")
            and HEX64.fullmatch(audio_hash[7:]),
            "audio_observation_missing",
            f"{label}.{realm}",
        )
    validate_header_coherence(label, realm, result, artifact)
    return capability_shape(result, realm, ledger)


def capability_shape(result: dict[str, Any], realm: str, ledger: dict[str, Any]) -> dict[str, Any]:
    shape: dict[str, Any] = {}
    for surface in ledger["surfaceOrder"]:
        status = surface_status(ledger, realm, surface)
        if status == "not-applicable":
            shape[surface] = "not-applicable"
            continue
        present, _reason = surface_capability(result, surface)
        shape[surface] = {"status": status, "apiPresent": present}
    return shape


def identity_projection(result: dict[str, Any], realm: str, ledger: dict[str, Any]) -> dict[str, Any]:
    projection: dict[str, Any] = {}
    for surface in ("navigator", "localeTimezone", "screenDpr", "canvas", "audio", "webgl", "webgl2", "fonts", "voices", "mediaDevices", "privacySignals", "maxTouchPoints", "httpHeaders", "workerCanvas"):
        status = surface_status(ledger, realm, surface)
        if status == "not-applicable":
            continue
        present, _reason = surface_capability(result, surface)
        if status == "conditional-if-api-present" and not present:
            continue
        value = surface_value(result, surface)
        if value is not None:
            if surface == "navigator":
                # Navigator is a cross-realm hard-field surface. Privacy
                # signals are tracked separately because Worker exposure is
                # conditional and must not turn an unavailable API into a
                # false identity mismatch.
                value = {
                    field: value.get(field)
                    for field in (
                        "userAgent",
                        "platform",
                        "hardwareConcurrency",
                        "language",
                        "languages",
                    )
                    if field in value
                }
            projection[surface] = value
    return projection


def context_projection(result: dict[str, Any], realm: str, ledger: dict[str, Any]) -> dict[str, Any]:
    result_value: dict[str, Any] = {}
    for surface in ("geometry", "history"):
        if surface_status(ledger, realm, surface) == "not-applicable":
            continue
        present, _reason = surface_capability(result, surface)
        if present:
            result_value[surface] = surface_value(result, surface)
    return result_value


def compare_projection(label: str, left: dict[str, Any], right: dict[str, Any], code: str) -> None:
    if left != right:
        differing = sorted(set(left) | set(right))
        first = next((key for key in differing if left.get(key) != right.get(key)), "unknown")
        fail(code, f"{label}.{first}")


def cross_realm_identity_projection(result: dict[str, Any], realm: str, other_realm: str, ledger: dict[str, Any]) -> dict[str, Any]:
    projection = identity_projection(result, realm, ledger)
    if (realm in WINDOW_REALMS) == (other_realm in WINDOW_REALMS):
        return projection
    # Window-only rendering surfaces and Worker-only conditional surfaces are
    # not implicitly comparable across execution families. The shared subset
    # is the hard navigator/locale/header relationship plus privacy signals
    # and touch points when the Worker API is present.
    return {
        key: projection[key]
        for key in ("navigator", "localeTimezone", "privacySignals", "maxTouchPoints", "httpHeaders")
        if key in projection
    }


def validate_session_result(
    label: str,
    raw_result: dict[str, Any],
    artifact: dict[str, Any],
    ledger: dict[str, Any],
    captures: list[dict[str, Any]],
    probe_manifest: Optional[dict[str, Any]] = None,
    probe_manifest_sha256: Optional[str] = None,
) -> dict[str, Any]:
    realms = raw_result.get("realms")
    require(isinstance(realms, dict) and set(realms) == set(CANONICAL_REALMS), "realm_matrix_incomplete", label)
    require(raw_result.get("realmOrder") == list(CANONICAL_REALMS), "realm_order_mismatch", label)
    if probe_manifest is not None:
        require(raw_result.get("bundleManifestSha256") == probe_manifest_sha256, "probe_bundle_hash_mismatch", label)
        require(raw_result.get("bundleFiles") == probe_manifest.get("files"), "probe_bundle_hash_mismatch", f"{label}.files")
    shapes = {}
    for realm in CANONICAL_REALMS:
        shapes[realm] = validate_realm_result(label, realm, realms[realm], artifact, ledger)
    for first in CANONICAL_REALMS:
        for second in CANONICAL_REALMS:
            if first >= second:
                continue
            left = cross_realm_identity_projection(realms[first], first, second, ledger)
            right = cross_realm_identity_projection(realms[second], second, first, ledger)
            shared = {key for key in left if key in right}
            compare_projection(f"{label}:{first}:{second}", {key: left[key] for key in shared}, {key: right[key] for key in shared}, "cross_realm_identity_mismatch")
    labels = [capture.get("realm") for capture in captures]
    require(labels == list(CANONICAL_REALMS), "header_capture_matrix_incomplete", label)
    for realm, capture in zip(CANONICAL_REALMS, captures):
        observed_headers = realms[realm]["requestHeaders"].get("identityHeaders")
        require(capture.get("identityHeaders") == observed_headers, "header_capture_js_mismatch", f"{label}.{realm}")
    service_worker = raw_result.get("serviceWorker")
    require(isinstance(service_worker, dict), "service_worker_state_missing", label)
    require(service_worker.get("scriptURLPath") == "/fp2/service-worker.js", "service_worker_script_url_mismatch", label)
    require(service_worker.get("scopePath") == "/fp2/", "service_worker_scope_mismatch", label)
    require(service_worker.get("activeState") == "activated", "service_worker_not_activated", label)
    require(service_worker.get("scriptSha256") == f"sha256:{sha256_file(BUNDLE_DIR / 'service-worker.js')}", "service_worker_script_hash_mismatch", label)
    storage = raw_result.get("storage")
    require(isinstance(storage, dict), "storage_observation_missing", label)
    require(isinstance(storage.get("cookie"), dict) and isinstance(storage.get("localStorage"), dict), "storage_observation_missing", label)
    return {
        "label": label,
        "realmCount": len(realms),
        "realmOrder": list(CANONICAL_REALMS),
        "capabilityShape": shapes,
        "identityProjection": {realm: identity_projection(realms[realm], realm, ledger) for realm in CANONICAL_REALMS},
        "contextProjection": {realm: context_projection(realms[realm], realm, ledger) for realm in CANONICAL_REALMS},
        "serviceWorker": {
            "existedBefore": service_worker.get("existedBefore"),
            "scriptURLPath": service_worker.get("scriptURLPath"),
            "scriptSha256": service_worker.get("scriptSha256"),
            "scopePath": service_worker.get("scopePath"),
            "activeState": service_worker.get("activeState"),
            "topController": service_worker.get("topController"),
            "controlledPage": service_worker.get("controlledPage"),
        },
        "storage": {
            "boot": storage.get("boot"),
            "cookiePresentBefore": storage.get("cookie", {}).get("presentBefore"),
            "cookiePresentAfter": storage.get("cookie", {}).get("presentAfter"),
            "cookieValueSha256": storage.get("cookie", {}).get("valueSha256"),
            "localStoragePresentBefore": storage.get("localStorage", {}).get("presentBefore"),
            "localStoragePresentAfter": storage.get("localStorage", {}).get("presentAfter"),
            "localStorageValueSha256": storage.get("localStorage", {}).get("valueSha256"),
        },
    }


def compare_session_pair(
    label: str,
    first: dict[str, Any],
    second: dict[str, Any],
    same_artifact: bool = True,
) -> dict[str, Any]:
    if same_artifact:
        require(first["artifactSha256"] == second["artifactSha256"], "a1_a2_artifact_mismatch", label)
        require(first["configuredIdentityDigest"] == second["configuredIdentityDigest"], "a1_a2_config_digest_mismatch", label)
    require(first["realmOrder"] == list(CANONICAL_REALMS) and second["realmOrder"] == list(CANONICAL_REALMS), "realm_order_mismatch", label)
    require(first["capabilityShape"] == second["capabilityShape"], "realm_capability_shape_drift", label)
    for realm in CANONICAL_REALMS:
        compare_projection(f"{label}.{realm}", first["identityProjection"][realm], second["identityProjection"][realm], "a1_a2_identity_mismatch" if same_artifact else "ab_common_identity_mismatch")
        if same_artifact:
            require(first["contextProjection"][realm] == second["contextProjection"][realm], "a1_a2_context_drift", realm)
    require(first["serviceWorker"]["scriptSha256"] == second["serviceWorker"]["scriptSha256"], "service_worker_script_hash_mismatch", label)
    require(first["serviceWorker"]["scopePath"] == second["serviceWorker"]["scopePath"], "service_worker_scope_mismatch", label)
    require(first["serviceWorker"]["activeState"] == second["serviceWorker"]["activeState"] == "activated", "service_worker_state_drift", label)
    return {"sameArtifact": same_artifact, "capabilityShapeStable": True, "identityStable": True}


def _field(result: dict[str, Any], path: str) -> Any:
    value: Any = result
    for part in path.split("."):
        if not isinstance(value, dict):
            return None
        value = value.get(part)
    return value


def canvas_raw_family(result: dict[str, Any]) -> tuple[Any, Any, Any]:
    canvas = result.get("canvas") or result.get("workerCanvas") or {}
    return canvas.get("rawHash"), canvas.get("rawRgbaHash"), canvas.get("decodedPngPixelsHash")


def validate_full_b_canvas_raw_relation(raw_a: dict[str, Any], raw_b: dict[str, Any], realm: str) -> None:
    raw_family_a = canvas_raw_family(raw_a)
    raw_family_b = canvas_raw_family(raw_b)
    if raw_family_a == raw_family_b:
        return
    font_changed = _field(raw_a, "fonts.injectedFonts") != _field(raw_b, "fonts.injectedFonts")
    width_changed = _field(raw_a, "fonts.fontUniverseWidths") != _field(raw_b, "fonts.fontUniverseWidths")
    require(font_changed or width_changed, "full_b_canvas_unexplained_raw_drift", realm)
    require((raw_family_a[0] != raw_family_a[1]) is False and (raw_family_b[0] != raw_family_b[1]) is False, "canvas_internal_mismatch", realm)


def compare_ab(
    a1: dict[str, Any],
    a2: dict[str, Any],
    b1: dict[str, Any],
    artifact_a: dict[str, Any],
    artifact_b: dict[str, Any],
    ledger: dict[str, Any],
    relation: dict[str, Any],
) -> dict[str, Any]:
    require(b1["artifactSha256"] != a1["artifactSha256"], "ab_artifact_not_distinct")
    require(b1["storage"]["boot"] == {"before": 0, "after": 1}, "b_profile_boot_not_fresh")
    require(b1["storage"]["cookiePresentBefore"] is False and b1["storage"]["localStoragePresentBefore"] is False, "b_profile_inherits_storage")
    compare_session_pair("A1/A2", a1, a2, same_artifact=True)
    require(a1["capabilityShape"] == b1["capabilityShape"], "ab_capability_shape_drift")
    mapping = relation["artifactDiffMapping"]
    changed = set(EXPECTED_STATIC_DIFF)
    for realm in CANONICAL_REALMS:
        a = a1["rawRealms"][realm]
        b = b1["rawRealms"][realm]
        status = ledger["realms"][realm]["surfaceStatus"]
        for key, entry in mapping.items():
            if realm not in entry["realms"]:
                continue
            observation = entry["observation"]
            if observation == "audio.audioHash":
                require(_field(a, "audio.audioHash") != _field(b, "audio.audioHash"), "ab_expected_difference_missing", f"{realm}.audio")
            elif observation == "navigator.hardwareConcurrency":
                require(_field(a, "navigator.hardwareConcurrency") != _field(b, "navigator.hardwareConcurrency"), "ab_expected_difference_missing", f"{realm}.hardwareConcurrency")
            elif observation == "historyLength":
                require(a.get("historyLength") != b.get("historyLength"), "ab_expected_difference_missing", f"{realm}.history")
            elif observation == "geometry.screenX":
                require(_field(a, "geometry.screenX") != _field(b, "geometry.screenX"), "ab_expected_difference_missing", f"{realm}.screenX")
            elif observation == "geometry.screenY":
                require(_field(a, "geometry.screenY") != _field(b, "geometry.screenY"), "ab_expected_difference_missing", f"{realm}.screenY")
            elif observation.startswith("screen."):
                field = observation.split(".", 1)[1]
                require(_field(a, f"screen.{field}") != _field(b, f"screen.{field}"), "ab_expected_difference_missing", f"{realm}.{observation}")
            elif observation == "fonts.injectedFonts":
                require(_field(a, "fonts.injectedFonts") != _field(b, "fonts.injectedFonts"), "ab_expected_difference_missing", f"{realm}.fonts")
            elif observation == "fonts.fontUniverseWidths":
                require(_field(a, "fonts.fontUniverseWidths") != _field(b, "fonts.fontUniverseWidths"), "ab_expected_difference_missing", f"{realm}.fontWidths")
            elif observation == "canvas export surface when applicable":
                canvas_surface = "canvas" if a.get("kind") == "window" else "workerCanvas"
                canvas_status = status.get(canvas_surface)
                present_a, _reason_a = surface_capability(a, canvas_surface)
                present_b, _reason_b = surface_capability(b, canvas_surface)
                if canvas_status == "conditional-if-api-present" and not present_a:
                    continue
                require(present_a and present_b, "conditional_surface_uncompared", f"{realm}.canvas")
                canvas_a = a.get(canvas_surface) or {}
                canvas_b = b.get(canvas_surface) or {}
                require(canvas_a.get("exportHash") != canvas_b.get("exportHash"), "ab_expected_difference_missing", f"{realm}.canvas.export")
        if realm in WINDOW_REALMS:
            validate_full_b_canvas_raw_relation(a, b, realm)
    for realm in CANONICAL_REALMS:
        a_projection = copy.deepcopy(a1["identityProjection"][realm])
        b_projection = copy.deepcopy(b1["identityProjection"][realm])
        for key, entry in mapping.items():
            if realm not in entry["realms"]:
                continue
            observation = entry["observation"]
            if observation.startswith("screen."):
                field = observation.split(".", 1)[1]
                for projection in (a_projection, b_projection):
                    screen = projection.get("screenDpr")
                    if isinstance(screen, dict) and isinstance(screen.get("screen"), dict):
                        screen["screen"].pop(field, None)
            elif observation == "navigator.hardwareConcurrency":
                for projection in (a_projection, b_projection):
                    navigator = projection.get("navigator")
                    if isinstance(navigator, dict):
                        navigator.pop("hardwareConcurrency", None)
            elif observation == "audio.audioHash":
                for projection in (a_projection, b_projection):
                    audio = projection.get("audio")
                    if isinstance(audio, dict):
                        audio.pop("audioHash", None)
            elif observation in {"fonts.injectedFonts", "fonts.fontUniverseWidths"}:
                field = observation.split(".", 1)[1]
                for projection in (a_projection, b_projection):
                    fonts = projection.get("fonts")
                    if isinstance(fonts, dict):
                        fonts.pop(field, None)
            elif observation == "canvas export surface when applicable":
                for projection in (a_projection, b_projection):
                    for surface_name in ("canvas", "workerCanvas"):
                        canvas = projection.get(surface_name)
                        if isinstance(canvas, dict):
                            for field in ("dataUrlHash", "exportHash", "pngBytesHash"):
                                canvas.pop(field, None)
        compare_projection(f"A/B.{realm}", a_projection, b_projection, "ab_common_identity_mismatch")
    return {
        "staticDiffKeys": sorted(changed),
        "expectedDifferencesChecked": True,
        "commonFieldsStable": True,
        "fullBCanvasRule": "font/spacing evidence permits raw family change; export remains seed-related",
    }


def _header_value(headers: Any, name: str) -> Optional[str]:
    value = headers.get(name)
    return None if value is None else str(value)


def _redacted_context_header(value: Optional[str]) -> dict[str, Any]:
    return {
        "present": value is not None,
        "sha256": None if value is None else f"sha256:{sha256_bytes(value.encode('utf-8'))}",
    }


class _FP2RequestHandler(BaseHTTPRequestHandler):
    server: "_FP2HTTPServer"

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path == "/fp2/header-observation":
            self._handle_header_observation(parse_qs(parsed.query, keep_blank_values=True))
            return
        if parsed.path.startswith("/fp2/"):
            self._serve_bundle(parsed.path.removeprefix("/fp2/"))
            return
        self.send_response(404)
        self.end_headers()

    def _serve_bundle(self, relative: str) -> None:
        owner = self.server.owner
        if relative not in owner.bundle_files:
            self.send_response(404)
            self.end_headers()
            return
        path = owner.bundle_dir / relative
        try:
            body = path.read_bytes()
        except OSError:
            self.send_response(404)
            self.end_headers()
            return
        content_type = {
            ".html": "text/html; charset=utf-8",
            ".js": "text/javascript; charset=utf-8",
            ".json": "application/json; charset=utf-8",
        }.get(path.suffix, "application/octet-stream")
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        if path.name == "service-worker.js":
            self.send_header("Service-Worker-Allowed", "/fp2/")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _handle_header_observation(self, query: dict[str, list[str]]) -> None:
        owner = self.server.owner
        realm = query.get("realm", [""])[0]
        nonce = query.get("nonce", [""])[0]
        try:
            capture = owner.record_header_request(
                method=self.command,
                path=urlparse(self.path).path,
                headers=self.headers,
                realm=realm,
                nonce=nonce,
            )
        except FP2Failure as exc:
            body = json.dumps({"ok": False, "error": exc.code}).encode("utf-8")
            self.send_response(409)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        response = {
            "ok": True,
            "realm": realm,
            "identityHeaders": capture["identityHeaders"],
            "contextHeaders": capture["contextHeaders"],
        }
        body = json.dumps(response, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_args: Any) -> None:  # noqa: N802
        return


class _FP2HTTPServer(ThreadingHTTPServer):
    def __init__(self, owner: "FP2HTTPServer", host: str, port: int) -> None:
        self.owner = owner
        if ":" in host:
            self.address_family = socket.AF_INET6
            address = (host, port, 0, 0)
        else:
            self.address_family = socket.AF_INET
            address = (host, port)
        super().__init__(address, _FP2RequestHandler)
        self.daemon_threads = True
        self.allow_reuse_address = False


class FP2HTTPServer:
    """One continuous primary/secondary loopback server for A1/A2/B1."""

    def __init__(self, bundle_dir: Path, port: int, bundle_files: set[str]) -> None:
        self.bundle_dir = bundle_dir
        self.bundle_files = bundle_files
        self._lock = Lock()
        self.active_label: Optional[str] = None
        self.active_nonce: Optional[str] = None
        self.captures: list[dict[str, Any]] = []
        try:
            self.httpd = _FP2HTTPServer(self, PRIMARY_HOST, port)
        except OSError as exc:
            fail("loopback_capability_unavailable", type(exc).__name__)
        self.thread = Thread(target=self.httpd.serve_forever, name="fp2-loopback", daemon=True)
        self.thread.start()
        self.port = int(self.httpd.server_address[1])
        require(self.port == port, "run_port_changed", f"{port}->{self.port}")
        self.secondary_httpd: Optional[_FP2HTTPServer] = None
        self.secondary_thread: Optional[Thread] = None
        try:
            localhost_addresses = socket.getaddrinfo(SECONDARY_HOST, port, type=socket.SOCK_STREAM)
        except OSError as exc:
            self.close()
            fail("loopback_capability_unavailable", f"localhost resolution: {type(exc).__name__}")
        has_ipv6_localhost = any(item[0] == socket.AF_INET6 for item in localhost_addresses)
        if has_ipv6_localhost:
            try:
                self.secondary_httpd = _FP2HTTPServer(self, "::1", port)
            except OSError as exc:
                self.close()
                fail("loopback_capability_unavailable", f"localhost IPv6: {type(exc).__name__}")
            self.secondary_thread = Thread(target=self.secondary_httpd.serve_forever, name="fp2-loopback-localhost", daemon=True)
            self.secondary_thread.start()

    @property
    def primary_origin(self) -> str:
        return f"http://{PRIMARY_HOST}:{self.port}"

    @property
    def secondary_origin(self) -> str:
        return f"http://{SECONDARY_HOST}:{self.port}"

    def begin_session(self, label: str, nonce: str) -> None:
        require(label in SESSION_LABELS and len(nonce) >= 16, "session_protocol_invalid", label)
        with self._lock:
            self.active_label = label
            self.active_nonce = nonce
            self.captures = []

    def record_header_request(self, *, method: str, path: str, headers: Any, realm: str, nonce: str) -> dict[str, Any]:
        with self._lock:
            require(self.active_label is not None and self.active_nonce is not None, "header_request_without_session")
            require(nonce == self.active_nonce, "cross_origin_nonce_mismatch", realm)
            require(realm in CANONICAL_REALMS, "header_realm_invalid", realm)
            require(path == "/fp2/header-observation", "header_path_mismatch", path)
            require(not any(item.get("realm") == realm for item in self.captures), "duplicate_header_observation", realm)
            identity_names = ("User-Agent", "Accept-Language", "Accept-Encoding", "DNT", "Sec-GPC")
            context_names = ("Origin", "Referer", "Sec-Fetch-Site", "Sec-Fetch-Mode", "Sec-Fetch-Dest", "Accept")
            identity = {name.lower(): _header_value(headers, name) for name in identity_names}
            context = {name.lower(): _redacted_context_header(_header_value(headers, name)) for name in context_names}
            capture = {
                "realm": realm,
                "nonceSha256": safe_nonce_hash(nonce),
                "method": method,
                "path": path.removeprefix("/"),
                "identityHeaders": identity,
                "contextHeaders": context,
                "cookiePresent": _header_value(headers, "Cookie") is not None,
                "customRealmHeaderMatches": _header_value(headers, "X-FP2-Realm") == realm,
                "customNonceHeaderMatches": _header_value(headers, "X-FP2-Nonce") == nonce,
            }
            require(capture["method"] == "GET", "header_method_mismatch", realm)
            require(capture["customRealmHeaderMatches"] and capture["customNonceHeaderMatches"], "header_protocol_mismatch", realm)
            require(capture["cookiePresent"] is False, "header_cookie_present", realm)
            self.captures.append(capture)
            return capture

    def take_captures(self) -> list[dict[str, Any]]:
        with self._lock:
            return copy.deepcopy(self.captures)

    def close(self) -> None:
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)
        if self.secondary_httpd is not None and self.secondary_thread is not None:
            self.secondary_httpd.shutdown()
            self.secondary_httpd.server_close()
            self.secondary_thread.join(timeout=5)


def _enumerate_tasklist_processes() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for image_name in TARGET_PROCESS_IMAGES:
        result = subprocess.run(
            ["tasklist.exe", "/FI", f"IMAGENAME eq {image_name}", "/FO", "CSV", "/NH"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            check=False,
        )
        if result.returncode != 0:
            fail("process_scan_backend_unavailable", "tasklist.exe")
        for row in csv.reader(result.stdout.splitlines()):
            if not row or row[0].startswith("INFO:") or row[0].lower() != image_name:
                continue
            try:
                pid = int(row[1])
            except (IndexError, ValueError) as exc:
                fail("process_scan_backend_invalid", "tasklist.exe")
                raise AssertionError from exc
            rows.append({"imageName": row[0], "pid": pid})
    return rows


def _enumerate_powershell_processes() -> list[dict[str, Any]]:
    failures: list[str] = []
    for executable in ("powershell.exe", "pwsh.exe"):
        try:
            result = subprocess.run(
                [executable, "-NoLogo", "-NoProfile", "-NonInteractive", "-Command", POWERSHELL_PROCESS_ENUMERATION_SCRIPT],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                check=False,
            )
        except OSError:
            failures.append(executable)
            continue
        if result.returncode != 0:
            failures.append(executable)
            continue
        raw = result.stdout.strip()
        if not raw:
            return []
        try:
            payload = json.loads(raw)
        except json.JSONDecodeError:
            failures.append(executable)
            continue
        if not isinstance(payload, dict) or not isinstance(payload.get("processes"), list):
            failures.append(executable)
            continue
        rows: list[dict[str, Any]] = []
        for item in payload["processes"]:
            if not isinstance(item, dict) or not isinstance(item.get("imageName"), str) or not isinstance(item.get("pid"), int):
                failures.append(executable)
                break
            rows.append({"imageName": item["imageName"], "pid": item["pid"]})
        else:
            return rows
    fail("process_scan_backend_unavailable", ",".join(failures) or "powershell")


def target_processes() -> list[dict[str, Any]]:
    if os.name != "nt":
        return []
    failures: list[str] = []
    try:
        return _enumerate_tasklist_processes()
    except FP2Failure as exc:
        failures.append(exc.code)
    try:
        return _enumerate_powershell_processes()
    except FP2Failure as exc:
        failures.append(exc.code)
    fail("process_cleanliness_unverifiable", ",".join(failures))


def require_no_target_processes(stage: str) -> None:
    processes = target_processes()
    require(processes == [], "target_processes_present", stage)


def require_no_preflight_locks(stage: str) -> None:
    require(not (FP2_EVIDENCE_ROOT / GLOBAL_LOCK_NAME).exists(), "runtime_preflight_lock_residual", stage)
    require(
        not any(item.suffix.lower() in {".lock", ".lck"} for item in FP2_EVIDENCE_ROOT.glob("fp2-runtime-preflight-*/**/*") if item.is_file()),
        "runtime_preflight_lock_residual",
        stage,
    )


def profile_lock_available(profile_root: Path, profile_id: str) -> dict[str, bool]:
    from host_platform import ProfileLock, probe_supervisor_lock

    path = profile_root / f"{profile_id}.lock"
    require(path.is_file(), "profile_lock_missing", profile_id)
    lock = None
    try:
        lock = ProfileLock.acquire(path)
        profile_available = True
    except OSError:
        profile_available = False
    finally:
        if lock is not None:
            lock.release()
    supervisor_available = probe_supervisor_lock(path)
    require(profile_available and supervisor_available, "profile_lock_unavailable", profile_id)
    return {"profileByteAvailable": True, "supervisorByteAvailable": True}


class FP2ManagedHost(host_module.CamoufoxHost):
    """A fresh internal Host instance used by one child session.

    It reuses the existing Host artifact, asset, Job Object, profile-lock and
    bounded-close implementation, while replacing only the FP1 page probe
    stage with the independently versioned FP2 bundle.
    """

    def __init__(
        self,
        *,
        artifact_root: Path,
        profile_root: Path,
        state_root: Path,
        tree_manifest: Path,
        asset_lock: Path,
        browser_root: Path,
        primary_origin: str,
        nonce: str,
        bundle_manifest_sha256: str,
        ledger: dict[str, Any],
        expected_boot: tuple[int, int],
    ) -> None:
        self.fp2_primary_origin = primary_origin
        self.fp2_nonce = nonce
        self.fp2_bundle_manifest_sha256 = bundle_manifest_sha256
        self.fp2_ledger = ledger
        self.fp2_expected_boot = expected_boot
        self.fp2_result: Optional[dict[str, Any]] = None
        self.fp2_stages: list[dict[str, Any]] = []
        super().__init__(
            artifact_root=artifact_root,
            profile_root=profile_root,
            state_root=state_root,
            tree_manifest=tree_manifest,
            display=None,
            probe_port=urlparse(primary_origin).port or 0,
            asset_lock=asset_lock,
            browser_root=browser_root,
        )

    def _stage_start(self, name: str) -> float:
        started = time.perf_counter()
        self.fp2_stages.append({"stage": name, "status": "started"})
        return started

    def _stage_finish(self, name: str, started: float, status: str) -> None:
        for item in reversed(self.fp2_stages):
            if item["stage"] == name and item["status"] == "started":
                item["status"] = status
                item["elapsedSeconds"] = round(time.perf_counter() - started, 3)
                return

    async def _await_supervisor_metadata(self, session: dict[str, Any]) -> dict[str, Any]:
        supervisor_path = session["sessionDir"] / "supervisor.json"
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if supervisor_path.exists():
                try:
                    candidate = json.loads(supervisor_path.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError):
                    candidate = None
                if (
                    isinstance(candidate, dict)
                    and isinstance(candidate.get("supervisorPid"), int)
                    and isinstance(candidate.get("childPid"), int)
                    and (
                        not host_module.IS_WINDOWS
                        or (
                            candidate.get("jobName") == session.get("expectedJobName")
                            and isinstance(candidate.get("supervisorCreationTime100ns"), int)
                            and isinstance(candidate.get("childCreationTime100ns"), int)
                            and candidate.get("jobKillOnClose") is True
                            and candidate.get("jobAssignmentVerified") is True
                            and candidate.get("processHandleEvidence") is True
                        )
                    )
                ):
                    return candidate
            await asyncio.sleep(0.1)
        fail("supervisor_metadata_missing", "FP2 supervisor metadata deadline")

    async def _launch_browser(self, session: dict[str, Any], artifact: dict[str, Any]) -> None:
        from functools import partial

        dependencies = resolve_browser_launch_dependencies()
        AsyncNewBrowser = dependencies["AsyncNewBrowser"]
        DefaultAddons = dependencies["DefaultAddons"]
        launch_options = dependencies["launch_options"]
        firefox_user_prefs_for_config = dependencies["firefox_user_prefs_for_config"]
        normalize_camou_config_env = dependencies["normalize_camou_config_env"]

        policy = artifact["policy"]
        window = tuple(policy["window"])
        disk_config = copy.deepcopy(artifact["resolvedConfig"])
        disk_digest = host_module.configured_identity_digest(disk_config)
        session["probePort"] = urlparse(self.fp2_primary_origin).port
        session["server"] = None
        session["launchAttempted"] = True
        os.environ["VERISILO_REAL_EXE"] = str(self.executable)
        os.environ["VERISILO_EXIT_FILE"] = str(session["exitFile"])
        os.environ["VERISILO_SUPERVISOR_FILE"] = str(session["sessionDir"] / "supervisor.json")
        if host_module.IS_WINDOWS:
            os.environ["VERISILO_PROFILE_LOCK_PATH"] = str(self.profile_root / f"{session['profileId']}.lock")
            session["expectedJobName"] = f"Local\\VeriSiloCamoufox-{session['sessionId']}"
            os.environ["VERISILO_JOB_NAME"] = session["expectedJobName"]

        launch_started = time.perf_counter()
        stage = self._stage_start("launch_options")
        opts = await asyncio.get_running_loop().run_in_executor(
            None,
            partial(
                launch_options,
                config=copy.deepcopy(disk_config),
                os=policy["targetOs"],
                window=window,
                locale=policy["locale"],
                ff_version=policy["ffVersion"],
                headless=False,
                executable_path=str(self.executable),
                user_data_dir=str(session["profileDir"]),
                virtual_display=None,
                firefox_user_prefs=firefox_user_prefs_for_config(disk_config),
                exclude_addons=[DefaultAddons.UBO],
                i_know_what_im_doing=True,
            ),
        )
        try:
            sent_config, diff, opts["env"] = normalize_camou_config_env(opts["env"], disk_config)
        except Exception as exc:
            self._stage_finish("launch_options", stage, "error")
            raise host_module.ProtocolError("config_mutation", type(exc).__name__) from exc
        sent_digest = host_module.configured_identity_digest(sent_config)
        if sent_digest != disk_digest or diff["added"] or diff["removed"] or diff["changed"]:
            self._stage_finish("launch_options", stage, "error")
            raise host_module.ProtocolError("config_mutation", "normalized config differs from Artifact")
        session["configuredIdentityDigest"] = disk_digest
        self._stage_finish("launch_options", stage, "success")
        opts["executable_path"] = str(host_module.SUPERVISOR)

        stage = self._stage_start("launch_persistent_context")
        ctx = await AsyncNewBrowser(self.playwright, from_options=opts, persistent_context=True)
        session["ctx"] = ctx
        if host_module.DownloadGuard.tripped:
            await ctx.close()
            self._stage_finish("launch_persistent_context", stage, "error")
            raise host_module.ProtocolError("webdl_attempted", "unpinned download attempted during launch")
        self._stage_finish("launch_persistent_context", stage, "success")

        stage = self._stage_start("supervisor_job_bind")
        supervisor_meta = await self._await_supervisor_metadata(session)
        session["supervisorMeta"] = supervisor_meta
        session["pid"] = supervisor_meta["supervisorPid"]
        session["childPid"] = supervisor_meta.get("childPid")
        session["managedIdentities"] = host_module.managed_identities(session)
        if host_module.IS_WINDOWS:
            try:
                session["jobHandle"] = host_module.JobHandle.open(supervisor_meta["jobName"])
            except OSError as exc:
                self._stage_finish("supervisor_job_bind", stage, "error")
                raise host_module.ProtocolError("job_unavailable", type(exc).__name__) from exc
        self._stage_finish("supervisor_job_bind", stage, "success")

        stage = self._stage_start("new_page")
        page = await ctx.new_page()
        session["page"] = page
        self._stage_finish("new_page", stage, "success")

        nonce_query = self.fp2_nonce
        top_url = f"{self.fp2_primary_origin}/fp2/top.html?nonce={nonce_query}"
        stage = self._stage_start("goto")
        await page.goto(top_url, wait_until="domcontentloaded", timeout=REALM_STAGE_DEADLINE_SECONDS * 1000)
        self._stage_finish("goto", stage, "success")

        fonts = artifact["resolvedConfig"]["fonts"]
        input_payload = {
            "fonts": fonts,
            "fontInputSha256": hash_value(fonts),
            "bundleManifestSha256": self.fp2_bundle_manifest_sha256,
            "bundleFiles": load_probe_manifest()[0]["files"],
        }
        stage = self._stage_start("realm_matrix")
        await asyncio.wait_for(
            page.evaluate("(input) => window.__fp2ProvideInput(input)", input_payload),
            timeout=REALM_STAGE_DEADLINE_SECONDS,
        )
        await page.wait_for_function(
            "window.__fp2Result !== undefined || window.__fp2Error !== undefined",
            timeout=REALM_STAGE_DEADLINE_SECONDS * 1000,
        )
        probe_error = await page.evaluate("window.__fp2Error")
        if probe_error is not None:
            self._stage_finish("realm_matrix", stage, "error")
            raise host_module.ProtocolError("realm_probe_failed", str(probe_error.get("name", "Error")))
        result = await asyncio.wait_for(page.evaluate("window.__fp2GetResult()"), timeout=REALM_STAGE_DEADLINE_SECONDS)
        require(isinstance(result, dict), "realm_result_missing", session["sessionId"])
        self._stage_finish("realm_matrix", stage, "success")
        session["probeSeconds"] = round(time.perf_counter() - launch_started, 3)
        session["spawnSeconds"] = round(time.perf_counter() - launch_started, 3)
        storage = result.get("storage") or {}
        boot = storage.get("boot") or {}
        require(tuple([boot.get("before"), boot.get("after")]) == self.fp2_expected_boot, "profile_boot_mismatch", session["profileId"])
        session["bootCountBefore"] = boot.get("before")
        session["bootCountAfter"] = boot.get("after")
        session["cookieEvidence"] = {
            "cookieInApi": storage.get("cookie", {}).get("presentAfter") is True,
            "cookieOnPage": storage.get("cookie", {}).get("presentAfter") is True,
            "cookieValueLooksManaged": bool(storage.get("cookie", {}).get("valueSha256")),
        }
        session["fontMode"] = policy.get("fontMode", "inherit")
        session["observedWebsiteDigest"] = sha256_bytes(canonical_json_bytes(result.get("realms", {})))
        self.fp2_result = result
        session["fp2Stages"] = self.fp2_stages
        session["fp2BundleManifestSha256"] = self.fp2_bundle_manifest_sha256
        session["fp2NonceSha256"] = safe_nonce_hash(self.fp2_nonce)
        write_json(session["sessionDir"] / "fp2-observation.json", sanitize_browser_result(result))
        write_json(session["sessionDir"] / "fp2-stages.json", {"stages": self.fp2_stages})
        session["state"] = "running"
        session["stopMonitor"] = asyncio.Event()
        session["monitorTask"] = asyncio.create_task(self._monitor_session(session))
        host_module.write_session_state(session)


def emit_child_event(event: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(event, ensure_ascii=True, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def validate_close_receipt(close: dict[str, Any], label: str) -> None:
    process_tree = close.get("processTreeExit")
    job = process_tree.get("job") if isinstance(process_tree, dict) else None
    close_outcome = close.get("closeOutcome")
    require(
        close.get("state") == "exited"
        and close.get("exitStatus") == 0
        and close.get("exitFileObserved") is True
        and isinstance(process_tree, dict)
        and process_tree.get("exited") is True
        and process_tree.get("remaining") == []
        and isinstance(job, dict)
        and job.get("activeProcessCount") == 0
        and isinstance(close_outcome, dict)
        and close_outcome.get("status") == "success"
        and (close_outcome.get("forcedJobCleanup") or {}).get("status") == "not_needed",
        "lifecycle_unclean",
        label,
    )


def validate_realm_key_set(realms: Any, label: str) -> None:
    """Validate the ordered realm labels before converting a wire result to a dict."""
    require(isinstance(realms, list), "realm_matrix_incomplete", label)
    labels = [item.get("realm") if isinstance(item, dict) else None for item in realms]
    require(len(labels) == len(CANONICAL_REALMS), "realm_matrix_incomplete", label)
    require(len(set(labels)) == len(labels), "duplicate_realm", label)
    require(set(labels) == set(CANONICAL_REALMS), "realm_matrix_incomplete", label)


def validate_hash_sidecar(path: Path, sidecar: Path) -> None:
    require(path.is_file() and sidecar.is_file(), "evidence_reference_missing", path.name)
    expected = f"{sha256_file(path)}  {path.name}\n"
    require(sidecar.read_text(encoding="ascii") == expected, "evidence_integrity_mismatch", path.name)


def validate_file_reference(base: Path, reference: dict[str, Any]) -> None:
    relative = reference.get("path")
    require(isinstance(relative, str) and relative and ".." not in Path(relative).parts and not Path(relative).is_absolute(), "evidence_reference_invalid", str(relative))
    path = base / Path(relative)
    require(path.is_file(), "evidence_reference_missing", relative)
    require(reference.get("size") == path.stat().st_size, "evidence_reference_hash_mismatch", relative)
    require(reference.get("sha256") == sha256_file(path), "evidence_reference_hash_mismatch", relative)


def validate_bound_file_hash(path: Path, expected_sha256: str, failure_code: str) -> None:
    require(path.is_file(), failure_code, path.name)
    require(sha256_file(path) == expected_sha256, failure_code, path.name)


def child_result_summary(
    label: str,
    host: Optional[FP2ManagedHost],
    launch: Optional[dict[str, Any]],
    close: Optional[dict[str, Any]],
    lock_receipt: Optional[dict[str, bool]],
    failure: Optional[dict[str, str]],
    raw_result_path: Path,
) -> dict[str, Any]:
    session = host.session if host is not None else None
    return {
        "schema": "verisilo-camoufox-fp2-child-session/v1",
        "label": label,
        "status": "passed" if failure is None and close is not None else "failed",
        "verified": False,
        "hostPid": os.getpid(),
        "sessionId": None if session is None else session.get("sessionId"),
        "artifactId": None if session is None else session.get("artifactId"),
        "artifactSha256": None if session is None else session.get("artifactFileSha256"),
        "profileId": None if session is None else session.get("profileId"),
        "configuredIdentityDigest": None if launch is None else launch.get("configuredIdentityDigest"),
        "boot": None if launch is None else [launch.get("bootCountBefore"), launch.get("bootCountAfter")],
        "probePort": None if launch is None else launch.get("probePort"),
        "bundleManifestSha256": None if host is None else host.fp2_bundle_manifest_sha256,
        "nonceSha256": None if host is None else safe_nonce_hash(host.fp2_nonce),
        "rawResultPath": raw_result_path.name,
        "lifecycle": None
        if close is None
        else {
            "state": close.get("state"),
            "exitStatus": close.get("exitStatus"),
            "exitFileObserved": close.get("exitFileObserved"),
            "processTreeExited": (close.get("processTreeExit") or {}).get("exited"),
            "remainingCount": len((close.get("processTreeExit") or {}).get("remaining", [])),
            "jobActiveProcessCount": ((close.get("processTreeExit") or {}).get("job") or {}).get("activeProcessCount"),
            "closeOutcome": (close.get("closeOutcome") or {}).get("status"),
            "forcedJobCleanup": ((close.get("closeOutcome") or {}).get("forcedJobCleanup") or {}).get("status"),
            "closeSeconds": close.get("closeSeconds"),
        },
        "profileLease": lock_receipt,
        "failure": failure,
    }


async def run_child_session(args: argparse.Namespace) -> int:
    label = args.label
    raw_result_path = Path(args.raw_result).resolve()
    child_result_path = Path(args.child_result).resolve()
    ledger = load_applicability(APPLICABILITY_PATH)
    host: Optional[FP2ManagedHost] = None
    launch: Optional[dict[str, Any]] = None
    close: Optional[dict[str, Any]] = None
    lock_receipt: Optional[dict[str, bool]] = None
    failure: Optional[dict[str, str]] = None
    try:
        os.environ["VERISILO_CAMOUFOX_CACHE_DIR"] = str(Path(args.cache_root).resolve())
        host = FP2ManagedHost(
            artifact_root=Path(args.artifact_root).resolve(),
            profile_root=Path(args.profile_root).resolve(),
            state_root=Path(args.state_root).resolve(),
            tree_manifest=Path(args.tree_manifest).resolve(),
            asset_lock=Path(args.asset_lock).resolve(),
            browser_root=Path(args.browser_root).resolve(),
            primary_origin=args.primary_origin,
            nonce=args.nonce,
            bundle_manifest_sha256=args.bundle_manifest_sha256,
            ledger=ledger,
            expected_boot=(int(args.expected_boot_before), int(args.expected_boot_after)),
        )
        emit_child_event({"event": "hello", "label": label, "status": "ready", "verified": False})
        from playwright.async_api import async_playwright

        async with async_playwright() as playwright:
            host.set_playwright(playwright)
            artifact_id = args.artifact_id
            artifact_sha = args.artifact_sha256
            launch = await asyncio.wait_for(
                host.launch(artifact_id, args.profile_id, artifact_sha),
                timeout=SESSION_WATCHDOG_SECONDS,
            )
            emit_child_event({"event": "launch", "label": label, "status": launch.get("state"), "sessionId": launch.get("sessionId")})
            require(launch.get("state") == "running", "child_launch_failed", label)
            raw_result_path.parent.mkdir(parents=True, exist_ok=True)
            write_json(raw_result_path, host.fp2_result)
            require(isinstance(host.fp2_result, dict), "realm_result_missing", label)
            artifact_for_validation = strict_json(Path(args.artifact_root) / f"{artifact_id}.json", label)
            for realm in CANONICAL_REALMS:
                validate_realm_result(label, realm, host.fp2_result["realms"][realm], artifact_for_validation, ledger)
            close = await asyncio.wait_for(host.close(launch["sessionId"]), timeout=SESSION_WATCHDOG_SECONDS)
            emit_child_event({"event": "close", "label": label, "status": close.get("state"), "exitStatus": close.get("exitStatus")})
            validate_close_receipt(close, label)
            lock_receipt = profile_lock_available(Path(args.profile_root).resolve(), args.profile_id)
            require_no_target_processes(f"after {label}")
            emit_child_event({"event": "shutdown", "label": label, "status": "clean", "verified": False})
    except FP2Failure as exc:
        failure = {"code": exc.code, "detail": exc.detail}
    except host_module.ProtocolError as exc:
        failure = {"code": exc.code, "detail": type(exc).__name__}
    except asyncio.TimeoutError:
        failure = {"code": "session_watchdog_timeout", "detail": label}
    except (Exception, SystemExit) as exc:  # noqa: BLE001 - child report is fail-closed
        failure = {"code": "child_session_failed", "detail": type(exc).__name__}
    finally:
        if host is not None and host.session is not None and host.session.get("state") in {"starting", "running", "closing"}:
            try:
                await asyncio.wait_for(host.close(host.session["sessionId"]), timeout=SESSION_WATCHDOG_SECONDS)
            except Exception:  # noqa: BLE001 - parent will treat a nonzero child as failed
                pass
        summary = child_result_summary(label, host, launch, close, lock_receipt, failure, raw_result_path)
        write_json(child_result_path, summary)
    return 0 if failure is None else 1


def sanitize_browser_result(value: Any) -> Any:
    """Remove run-only/context details before evidence is written."""
    if isinstance(value, list):
        return [sanitize_browser_result(item) for item in value]
    if not isinstance(value, dict):
        return value
    result = {key: sanitize_browser_result(item) for key, item in value.items()}
    if "contextHeaders" in result and isinstance(result["contextHeaders"], dict):
        result["contextHeaders"] = {
            key: _redacted_context_header(None)
            if not isinstance(item, dict)
            else item
            for key, item in result["contextHeaders"].items()
        }
    if "injectedFonts" in result and isinstance(result["injectedFonts"], list):
        result["injectedFonts"] = [
            {
                "familySha256": hash_value(item.get("family")),
                "available": item.get("available"),
            }
            for item in result["injectedFonts"]
            if isinstance(item, dict)
        ]
    if "fontNegativeControls" in result and isinstance(result["fontNegativeControls"], dict):
        result["fontNegativeControls"] = {
            f"familyHash:{hash_value(key)}": value for key, value in result["fontNegativeControls"].items()
        }
    if "voices" in result and isinstance(result["voices"], list):
        result["voices"] = [
            {
                "nameSha256": hash_value(item.get("name")),
                "lang": item.get("lang"),
                "voiceUriSha256": hash_value(item.get("voiceURI")),
                "localService": item.get("localService"),
                "isDefault": item.get("isDefault"),
            }
            for item in result["voices"]
            if isinstance(item, dict)
        ]
    return result


def ensure_sanitized(value: Any, label: str, forbidden_values: tuple[str, ...] = ()) -> None:
    if isinstance(value, str):
        require(not ABSOLUTE_PATH.match(value), "secret_path_sentinel_leak", label)
        require(not SECRET_WORD.search(value), "secret_path_sentinel_leak", label)
        for forbidden in forbidden_values:
            require(forbidden not in value, "secret_path_sentinel_leak", label)
    elif isinstance(value, dict):
        for key, item in value.items():
            ensure_sanitized(str(key), label, forbidden_values)
            ensure_sanitized(item, label, forbidden_values)
    elif isinstance(value, list):
        for item in value:
            ensure_sanitized(item, label, forbidden_values)


def phase_file_entry(path: Path, run_dir: Path, evidence_class: str) -> dict[str, Any]:
    return {
        "path": path.relative_to(run_dir).as_posix(),
        "size": path.stat().st_size,
        "sha256": sha256_file(path),
        "evidenceClass": evidence_class,
    }


def write_report_sidecar(report_path: Path) -> str:
    digest = sha256_file(report_path)
    report_path.with_name("run-report.sha256").write_text(f"{digest}  {report_path.name}\n", encoding="ascii")
    return digest


def atomic_create(path: Path, payload: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY
    flags |= getattr(os, "O_BINARY", 0)
    try:
        fd = os.open(path, flags, 0o600)
    except FileExistsError:
        fail("one_shot_claim_already_exists", path.name)
    try:
        view = memoryview(payload)
        while view:
            written = os.write(fd, view)
            view = view[written:]
        os.fsync(fd)
    finally:
        os.close(fd)


def run_no_browser_tests() -> dict[str, Any]:
    require(NO_BROWSER_TEST_PATH.is_file(), "no_browser_test_missing")
    result = subprocess.run(
        [sys.executable, str(NO_BROWSER_TEST_PATH)],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    combined_output = "\n".join(part for part in (result.stdout.strip(), result.stderr.strip()) if part)
    summary = combined_output.splitlines()[-1] if combined_output else ""
    require(result.returncode == 0, "no_browser_tests_failed", summary or type(result).__name__)
    return {
        "command": "python apps/camoufox-host/test_fp2_cross_realm.py",
        "exitCode": result.returncode,
        "summary": summary,
        "testFileSha256": sha256_file(NO_BROWSER_TEST_PATH),
    }


def assert_port_free(port: int) -> None:
    require(1024 <= port <= 65535, "invalid_run_port", str(port))
    probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        probe.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
    except (AttributeError, OSError):
        pass
    try:
        probe.bind((PRIMARY_HOST, port))
    except OSError as exc:
        fail("loopback_port_in_use", type(exc).__name__)
    finally:
        probe.close()


def previous_blocked_attempt() -> dict[str, Any]:
    require(LEGACY_CLAIM_PATH.is_file(), "previous_blocked_claim_missing", LEGACY_CLAIM_PATH.as_posix())
    require(sha256_file(LEGACY_CLAIM_PATH) == PREVIOUS_BLOCKED_CLAIM_SHA256, "previous_blocked_claim_hash_mismatch", LEGACY_CLAIM_PATH.name)
    claim = strict_json(LEGACY_CLAIM_PATH, LEGACY_CLAIM_PATH.as_posix())
    require(isinstance(claim, dict) and claim.get("runId") == PREVIOUS_BLOCKED_RUN_ID, "previous_blocked_claim_mismatch", "runId")
    run_evidence_path = claim.get("runEvidencePath")
    require(isinstance(run_evidence_path, str) and ".." not in Path(run_evidence_path).parts and not Path(run_evidence_path).is_absolute(), "previous_blocked_claim_mismatch", "runEvidencePath")
    previous_report_path = REPO_ROOT / Path(run_evidence_path) / "run-report.json"
    require(previous_report_path.is_file(), "previous_blocked_report_missing", previous_report_path.name)
    previous_report = strict_json(previous_report_path, previous_report_path.as_posix())
    require(previous_report.get("status") == "blocked" and previous_report.get("verified") is False, "previous_blocked_report_mismatch")
    matrix = previous_report.get("matrix")
    require(isinstance(matrix, list) and len(matrix) == 1 and matrix[0].get("label") == "A1", "previous_blocked_observation_mismatch")
    require((matrix[0].get("files") or {}).get("rawRealms") is None, "previous_blocked_observation_mismatch")
    return {
        "claimPath": relative_repo_path(LEGACY_CLAIM_PATH),
        "claimSha256": PREVIOUS_BLOCKED_CLAIM_SHA256,
        "run": PREVIOUS_BLOCKED_RUN_ID,
        "browserObservations": 0,
        "classification": PREVIOUS_BLOCKED_CLASSIFICATION,
        "reasonForReauthorization": PREVIOUS_BLOCKED_REASON,
    }


def previous_generation2_attempt() -> dict[str, Any]:
    require(GENERATION2_CLAIM_PATH.is_file(), "previous_generation2_claim_missing", GENERATION2_CLAIM_PATH.as_posix())
    require(sha256_file(GENERATION2_CLAIM_PATH) == GENERATION2_CLAIM_SHA256, "previous_generation2_claim_hash_mismatch", GENERATION2_CLAIM_PATH.name)
    claim = strict_json(GENERATION2_CLAIM_PATH, GENERATION2_CLAIM_PATH.as_posix())
    require(isinstance(claim, dict) and claim.get("runId") == GENERATION2_RUN_ID and claim.get("executionGeneration") == 2, "previous_generation2_claim_mismatch", "identity")
    run_evidence_path = claim.get("runEvidencePath")
    require(isinstance(run_evidence_path, str) and ".." not in Path(run_evidence_path).parts and not Path(run_evidence_path).is_absolute(), "previous_generation2_claim_mismatch", "runEvidencePath")
    previous_report_path = REPO_ROOT / Path(run_evidence_path) / "run-report.json"
    require(previous_report_path.is_file(), "previous_generation2_report_missing", previous_report_path.name)
    require(sha256_file(previous_report_path) == GENERATION2_REPORT_SHA256, "previous_generation2_report_hash_mismatch", previous_report_path.name)
    validate_hash_sidecar(previous_report_path, previous_report_path.with_name("run-report.sha256"))
    previous_report = strict_json(previous_report_path, previous_report_path.as_posix())
    require(previous_report.get("status") == "failed" and previous_report.get("verified") is False, "previous_generation2_report_mismatch")
    matrix = previous_report.get("matrix")
    require(isinstance(matrix, list) and len(matrix) == 1 and matrix[0].get("label") == "A1", "previous_generation2_observation_mismatch")
    require(matrix[0].get("hostPid") is not None and matrix[0].get("headerCaptureCount") == 0, "previous_generation2_observation_mismatch")
    require((matrix[0].get("files") or {}).get("rawRealms") is None, "previous_generation2_observation_mismatch")
    require((previous_report.get("failure") or {}).get("code") == "realm_probe_failed", "previous_generation2_report_mismatch")
    return {
        "claimPath": relative_repo_path(GENERATION2_CLAIM_PATH),
        "claimSha256": GENERATION2_CLAIM_SHA256,
        "run": GENERATION2_RUN_ID,
        "browserLaunched": True,
        "browserObservations": 0,
        "validRealmObservations": 0,
        "headerCaptureCount": 0,
        "classification": GENERATION2_CLASSIFICATION,
        "reasonForReauthorization": GENERATION2_REASON,
    }


def previous_execution_attempts() -> dict[str, dict[str, Any]]:
    return {
        "generation1": previous_blocked_attempt(),
        "generation2": previous_generation2_attempt(),
    }


def previous_attempt_parts(previous: dict[str, Any]) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    if "generation1" in previous and "generation2" in previous:
        require(isinstance(previous["generation1"], dict) and isinstance(previous["generation2"], dict), "previous_attempts_invalid")
        return previous["generation1"], previous
    return previous, {"generation1": previous}


def sanitized_runtime_log(raw: bytes) -> str:
    text = raw.decode("utf-8", errors="replace")
    text = ABSOLUTE_PATH.sub("<redacted-path>", text)
    if SECRET_WORD.search(text):
        return "<redacted-runtime-log>\n"
    return text


def run_runtime_preflight(
    *,
    interpreter: Path,
    port: int,
    previous: dict[str, Any],
    git: dict[str, Any],
) -> dict[str, Any]:
    """Run the exact child bootstrap path and stop immediately before browser spawn."""
    previous_blocked, previous_attempts = previous_attempt_parts(previous)
    preflight_id = f"fp2-runtime-preflight-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{uuid.uuid4().hex[:10]}"
    preflight_dir = FP2_EVIDENCE_ROOT / preflight_id
    preflight_dir.mkdir(parents=True, exist_ok=False)
    child_result_path = preflight_dir / "runtime-preflight-child.json"
    stdout_path = preflight_dir / "child-stdout.log"
    stderr_path = preflight_dir / "child-stderr.log"
    failure: Optional[dict[str, str]] = None
    exit_code: Optional[int] = None
    child_result: dict[str, Any] = {}
    try:
        require_no_target_processes("before runtime preflight")
        require_no_preflight_locks("before runtime preflight")
        assert_port_free(port)
        command = runtime_preflight_child_command(interpreter=interpreter, preflight_result=child_result_path, selected_port=port)
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(
                command,
                cwd=REPO_ROOT,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                env=child_environment(),
            )
            try:
                exit_code = process.wait(timeout=RUNTIME_PREFLIGHT_WATCHDOG_SECONDS)
            except subprocess.TimeoutExpired:
                terminate_child_process(process)
                failure = {"code": "runtime_preflight_timeout", "detail": "child"}
                exit_code = process.returncode if process.returncode is not None else -1
        if child_result_path.is_file():
            child_result = strict_json(child_result_path, child_result_path.as_posix())
        if failure is None and exit_code != 0:
            child_failure = child_result.get("failure") if isinstance(child_result, dict) else None
            failure = child_failure if isinstance(child_failure, dict) else {"code": "runtime_preflight_child_failed", "detail": "child"}
        if failure is None:
            runtime_binding = validate_runtime_preflight_result(child_result, interpreter)
        else:
            runtime_binding = None
    except FP2Failure as exc:
        failure = {"code": exc.code, "detail": exc.detail}
        runtime_binding = None
    except Exception as exc:  # noqa: BLE001 - preflight is fail-closed
        failure = {"code": "runtime_preflight_failed", "detail": type(exc).__name__}
        runtime_binding = None
    finally:
        stdout_path.write_text(sanitized_runtime_log(stdout_path.read_bytes()) if stdout_path.is_file() else "", encoding="utf-8")
        stderr_path.write_text(sanitized_runtime_log(stderr_path.read_bytes()) if stderr_path.is_file() else "", encoding="utf-8")
        try:
            require_no_target_processes("after runtime preflight")
            require_no_preflight_locks("after runtime preflight")
            assert_port_free(port)
            clean = True
        except FP2Failure as exc:
            clean = False
            failure = failure or {"code": exc.code, "detail": exc.detail}

    synthetic_finalization: Optional[dict[str, Any]] = None
    if failure is None and runtime_binding is not None and clean:
        try:
            synthetic_finalization = synthetic_report_finalization_test()
        except FP2Failure as exc:
            failure = {"code": exc.code, "detail": exc.detail}
        except Exception as exc:  # noqa: BLE001 - finalization is fail-closed
            failure = {"code": "synthetic_finalization_failed", "detail": type(exc).__name__}
    status = "passed" if failure is None and runtime_binding is not None and synthetic_finalization is not None else "blocked"
    receipt: dict[str, Any] = {
        "schema": RUNTIME_PREFLIGHT_SCHEMA,
        "taskVersion": TASK_VERSION,
        "executionGeneration": EXECUTION_GENERATION,
        "git": {
            "branch": git["branch"],
            "head": git["head"],
            "tree": git["tree"],
            "trackedWorktreeClean": git["trackedWorktreeClean"],
        },
        "status": status,
        "verified": False,
        "previousBlockedAttempt": previous_blocked,
        "previousAttempts": previous_attempts,
        "selectedPort": port,
        "runtimeBinding": runtime_binding,
        "syntheticFinalization": synthetic_finalization,
        "childExitCode": exit_code,
        "childResult": child_result if isinstance(child_result, dict) else {},
        "clean": clean,
        "claimCreated": False,
        "claimCreationAllowed": status == "passed" and clean,
        "browserLaunchCalled": False,
        "browserProcessCreated": False,
        "profileCreated": False,
        "lockFilesCreated": False,
        "runnerSha256": sha256_file(Path(__file__).resolve()),
        "receiptPath": relative_repo_path(preflight_dir / "runtime-preflight-receipt.json"),
        "byteClosurePath": relative_repo_path(preflight_dir / "byte-closure-receipt.json"),
        "files": {
            "childResult": phase_file_entry(child_result_path, preflight_dir, "sanitized-runtime-child") if child_result_path.is_file() else None,
            "childStdout": phase_file_entry(stdout_path, preflight_dir, "sanitized-runtime-log"),
            "childStderr": phase_file_entry(stderr_path, preflight_dir, "sanitized-runtime-log"),
        },
        "failure": safe_failure(failure),
    }
    ensure_sanitized(receipt, "runtime-preflight")
    receipt_path = preflight_dir / "runtime-preflight-receipt.json"
    write_json(receipt_path, receipt)
    receipt_sha256 = write_sha256_sidecar(receipt_path, "runtime-preflight-receipt.sha256")
    closure_path, closure_sha256 = write_byte_closure(preflight_dir)
    return {
        "schema": RUNTIME_PREFLIGHT_SCHEMA,
        "status": status,
        "verified": False,
        "claimCreationAllowed": status == "passed" and clean,
        "receiptPath": relative_repo_path(receipt_path),
        "receiptSha256": receipt_sha256,
        "byteClosurePath": relative_repo_path(closure_path),
        "byteClosureSha256": closure_sha256,
        "runtimeBinding": runtime_binding,
        "syntheticFinalization": synthetic_finalization,
        "previousBlockedAttempt": previous_blocked,
        "previousAttempts": previous_attempts,
        "preflightDirectory": relative_repo_path(preflight_dir),
    }


def create_claim(
    *,
    run_id: str,
    run_dir: Path,
    port: int,
    git: dict[str, Any],
    candidate: dict[str, Any],
    artifacts: dict[str, dict[str, Any]],
    probe_manifest_sha256: str,
    applicability_sha256: str,
    relation_sha256: str,
    static_diff_sha256: str,
    no_browser_test_sha256: str,
    runtime_preflight: dict[str, Any],
    previous_blocked_attempt: dict[str, Any],
) -> tuple[dict[str, Any], str]:
    require_runtime_preflight_for_claim(runtime_preflight)
    previous_blocked, previous_attempts = previous_attempt_parts(previous_blocked_attempt)
    runner_sha256 = sha256_file(Path(__file__).resolve())
    claim = {
        "schema": CLAIM_SCHEMA,
        "taskVersion": TASK_VERSION,
        "executionGeneration": EXECUTION_GENERATION,
        "scope": [TASK_VERSION, EXPECTED_ARCHIVE_SHA256, "fp2-probe-bundle", "relation-matrix"],
        "runId": run_id,
        "createdAtUtc": utc_now(),
        "git": {
            "branch": git["branch"],
            "head": git["head"],
            "tree": git["tree"],
            "upstream": git["upstream"],
            "baselineHead": BASELINE_HEAD,
            "baselineTree": BASELINE_TREE,
        },
        "candidate": candidate,
        "artifacts": artifacts,
        "probeBundleManifestSha256": probe_manifest_sha256,
        "applicabilityLedgerSha256": applicability_sha256,
        "relationMatrixSha256": relation_sha256,
        "staticAbDiffSha256": static_diff_sha256,
        "comparatorSha256": runner_sha256,
        "runnerSha256": runner_sha256,
        "noBrowserTestFileSha256": no_browser_test_sha256,
        "previousBlockedAttempt": previous_blocked,
        "previousAttempts": previous_attempts,
        "runtime": runtime_preflight["runtimeBinding"],
        "runtimePreflight": {
            "receiptPath": runtime_preflight["receiptPath"],
            "receiptSha256": runtime_preflight["receiptSha256"],
            "dependencyClosureSha256": runtime_preflight["runtimeBinding"]["dependencyClosureSha256"],
            "childInvocationSha256": runtime_preflight["runtimeBinding"]["childInvocationSha256"],
            "syntheticFinalization": runtime_preflight.get("syntheticFinalization"),
        },
        "loopback": {
            "selectedPort": port,
            "primaryOrigin": f"http://{PRIMARY_HOST}:{port}",
            "secondaryOrigin": f"http://{SECONDARY_HOST}:{port}",
        },
        "deadlinesSeconds": {
            "browserOperation": BROWSER_OPERATION_DEADLINE_SECONDS,
            "realmStage": REALM_STAGE_DEADLINE_SECONDS,
            "sessionWatchdog": SESSION_WATCHDOG_SECONDS,
            "parentWatchdog": PARENT_WATCHDOG_SECONDS,
            "hostCloseContext": HOST_CLOSE_CONTEXT_SECONDS,
            "hostCloseProcessTree": HOST_CLOSE_PROCESS_TREE_SECONDS,
        },
        "claimPath": relative_repo_path(GLOBAL_CLAIM_PATH),
        "runEvidencePath": relative_repo_path(run_dir),
        "verified": False,
    }
    payload = (json.dumps(claim, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    atomic_create(GLOBAL_CLAIM_PATH, payload)
    claim_hash = sha256_bytes(payload)
    copy_path = run_dir / "one-shot-claim.json"
    copy_path.write_bytes(payload)
    return claim, claim_hash


def require_runtime_preflight_for_claim(runtime_preflight: dict[str, Any]) -> None:
    require(runtime_preflight.get("status") == "passed", "runtime_preflight_required")
    require(runtime_preflight.get("claimCreationAllowed") is True, "runtime_preflight_required")
    binding = runtime_preflight.get("runtimeBinding")
    require(isinstance(binding, dict), "runtime_preflight_required")
    validate_runtime_binding(binding)
    receipt_path = runtime_preflight.get("receiptPath")
    receipt_sha256 = runtime_preflight.get("receiptSha256")
    require(isinstance(receipt_path, str) and ".." not in Path(receipt_path).parts and not Path(receipt_path).is_absolute(), "runtime_preflight_receipt_invalid", "path")
    path = REPO_ROOT / Path(receipt_path)
    require(path.is_file(), "runtime_preflight_receipt_missing", receipt_path)
    require(receipt_sha256 == sha256_file(path), "runtime_preflight_receipt_mismatch", receipt_path)


def child_command(
    *,
    interpreter: Path,
    label: str,
    run_dir: Path,
    artifact_root: Path,
    profile_root: Path,
    state_root: Path,
    cache_root: Path,
    artifact_id: str,
    artifact_sha256: str,
    profile_id: str,
    nonce: str,
    port: int,
    bundle_manifest_sha256: str,
    expected_boot: tuple[int, int],
    child_result: Path,
    raw_result: Path,
) -> list[str]:
    return [
        str(interpreter),
        str(Path(__file__).resolve()),
        "--child-session",
        "--label",
        label,
        "--artifact-root",
        str(artifact_root),
        "--profile-root",
        str(profile_root),
        "--state-root",
        str(state_root),
        "--cache-root",
        str(cache_root),
        "--artifact-id",
        artifact_id,
        "--artifact-sha256",
        artifact_sha256,
        "--profile-id",
        profile_id,
        "--asset-lock",
        str(ASSET_LOCK_PATH),
        "--browser-root",
        str(BROWSER_ROOT),
        "--tree-manifest",
        str(TREE_MANIFEST_PATH),
        "--primary-origin",
        f"http://{PRIMARY_HOST}:{port}",
        "--nonce",
        nonce,
        "--bundle-manifest-sha256",
        bundle_manifest_sha256,
        "--expected-boot-before",
        str(expected_boot[0]),
        "--expected-boot-after",
        str(expected_boot[1]),
        "--child-result",
        str(child_result),
        "--raw-result",
        str(raw_result),
    ]


def runtime_preflight_child_command(*, interpreter: Path, preflight_result: Path, selected_port: int) -> list[str]:
    return [
        str(interpreter),
        str(Path(__file__).resolve()),
        "--runtime-preflight-child",
        "--expected-interpreter",
        str(interpreter),
        "--preflight-result",
        str(preflight_result),
        "--run-port",
        str(selected_port),
    ]


def terminate_child_process(process: subprocess.Popen[Any]) -> None:
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(
            ["taskkill.exe", "/PID", str(process.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    else:
        process.kill()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass


def comparison_item(
    *,
    label: str,
    artifact_sha256: str,
    configured_identity_digest: str,
    validated: dict[str, Any],
    raw_result: dict[str, Any],
    child: dict[str, Any],
) -> dict[str, Any]:
    item = copy.deepcopy(validated)
    item.update(
        {
            "label": label,
            "artifactSha256": artifact_sha256,
            "configuredIdentityDigest": configured_identity_digest,
            "rawRealms": raw_result["realms"],
            "hostPid": child.get("hostPid"),
            "sessionId": child.get("sessionId"),
            "profileId": child.get("profileId"),
            "boot": child.get("boot"),
        }
    )
    return item


def run_one_phase(
    *,
    interpreter: Path,
    label: str,
    run_dir: Path,
    server: FP2HTTPServer,
    artifact: dict[str, Any],
    artifact_sha256: str,
    artifact_id: str,
    profile_id: str,
    profile_root: Path,
    artifact_root: Path,
    cache_root: Path,
    runtime_state_root: Path,
    ledger: dict[str, Any],
    probe_manifest: dict[str, Any],
    bundle_manifest_sha256: str,
    port: int,
    expected_boot: tuple[int, int],
) -> dict[str, Any]:
    phase_dir = run_dir / label
    phase_dir.mkdir(parents=True, exist_ok=False)
    state_root = runtime_state_root / label
    child_result_path = phase_dir / "child-result.json"
    raw_result_path = phase_dir / "raw-realms.json"
    stdout_path = phase_dir / "child-stdout.log"
    stderr_path = phase_dir / "child-stderr.log"
    nonce = secrets.token_urlsafe(24)
    server.begin_session(label, nonce)
    command = child_command(
        interpreter=interpreter,
        label=label,
        run_dir=run_dir,
        artifact_root=artifact_root,
        profile_root=profile_root,
        state_root=state_root,
        cache_root=cache_root,
        artifact_id=artifact_id,
        artifact_sha256=artifact_sha256,
        profile_id=profile_id,
        nonce=nonce,
        port=port,
        bundle_manifest_sha256=bundle_manifest_sha256,
        expected_boot=expected_boot,
        child_result=child_result_path,
        raw_result=raw_result_path,
    )
    env = child_environment()
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        process = subprocess.Popen(
            command,
            cwd=REPO_ROOT,
            stdin=subprocess.DEVNULL,
            stdout=stdout,
            stderr=stderr,
            env=env,
        )
        try:
            exit_code = process.wait(timeout=SESSION_WATCHDOG_SECONDS)
        except subprocess.TimeoutExpired:
            terminate_child_process(process)
            exit_code = process.returncode if process.returncode is not None else -1
            child_timeout = {"code": "session_watchdog_timeout", "detail": label}
        else:
            child_timeout = None
    captures = server.take_captures()
    child = strict_json(child_result_path, f"{label}.child-result") if child_result_path.is_file() else {
        "status": "failed",
        "failure": {"code": "child_result_missing", "detail": label},
    }
    if child_timeout is not None:
        child["status"] = "failed"
        child["failure"] = child_timeout
    raw_result = strict_json(raw_result_path, f"{label}.raw-realms") if raw_result_path.is_file() else None
    phase_failure: Optional[dict[str, str]] = None
    validated: Optional[dict[str, Any]] = None
    comparison: Optional[dict[str, Any]] = None
    if exit_code != 0 or child.get("status") != "passed" or not isinstance(raw_result, dict):
        failure_value = child.get("failure") if isinstance(child.get("failure"), dict) else {"code": "child_session_failed", "detail": label}
        phase_failure = {"code": str(failure_value.get("code", "child_session_failed")), "detail": str(failure_value.get("detail", label))}
    else:
        try:
            validated = validate_session_result(
                label,
                raw_result,
                artifact,
                ledger,
                captures,
                probe_manifest=probe_manifest,
                probe_manifest_sha256=bundle_manifest_sha256,
            )
            comparison = comparison_item(
                label=label,
                artifact_sha256=artifact_sha256,
                configured_identity_digest=str(child.get("configuredIdentityDigest")),
                validated=validated,
                raw_result=raw_result,
                child=child,
            )
        except FP2Failure as exc:
            phase_failure = {"code": exc.code, "detail": exc.detail}
    if target_processes():
        phase_failure = phase_failure or {"code": "lifecycle_residual_process", "detail": label}
    if phase_failure is None:
        lock_receipt = child.get("profileLease")
        require(isinstance(lock_receipt, dict) and lock_receipt.get("profileByteAvailable") is True and lock_receipt.get("supervisorByteAvailable") is True, "profile_lock_unavailable", label)
    sanitized_result_path = phase_dir / "realm-observations.json"
    header_path = phase_dir / "request-header-captures.json"
    lifecycle_path = phase_dir / "lifecycle-receipt.json"
    if isinstance(raw_result, dict):
        write_json(sanitized_result_path, sanitize_browser_result(raw_result))
    write_json(header_path, captures)
    write_json(lifecycle_path, child)
    record: dict[str, Any] = {
        "label": label,
        "status": "passed" if phase_failure is None else "failed",
        "artifactId": artifact_id,
        "artifactSha256": artifact_sha256,
        "profileId": profile_id,
        "hostPid": child.get("hostPid"),
        "sessionId": child.get("sessionId"),
        "configuredIdentityDigest": child.get("configuredIdentityDigest"),
        "boot": child.get("boot"),
        "probePort": port,
        "nonceSha256": child.get("nonceSha256"),
        "bundleManifestSha256": child.get("bundleManifestSha256"),
        "headerCaptureCount": len(captures),
        "lifecycle": child.get("lifecycle"),
        "validation": {
            "requiredRealms": len(CANONICAL_REALMS),
            "crossRealmIdentity": phase_failure is None,
            "headerCoherence": phase_failure is None,
            "serviceWorker": phase_failure is None,
        },
        "failure": phase_failure,
        "files": {
            "childResult": phase_file_entry(child_result_path, run_dir, "raw-child-summary") if child_result_path.is_file() else None,
            "rawRealms": phase_file_entry(raw_result_path, run_dir, "raw-realm-observation") if raw_result_path.is_file() else None,
            "realmObservations": phase_file_entry(sanitized_result_path, run_dir, "sanitized-realm-observation") if sanitized_result_path.is_file() else None,
            "requestHeaders": phase_file_entry(header_path, run_dir, "sanitized-header-capture"),
            "lifecycle": phase_file_entry(lifecycle_path, run_dir, "sanitized-lifecycle"),
            "childStdout": phase_file_entry(stdout_path, run_dir, "raw-child-protocol"),
            "childStderr": phase_file_entry(stderr_path, run_dir, "raw-child-stderr"),
        },
        "comparison": comparison,
    }
    ensure_sanitized(record["files"], f"{label}.report.files")
    record["comparison"] = {"validated": phase_failure is None}
    return {"record": record, "comparison": comparison, "rawResult": raw_result, "captures": captures}


def public_phase_file_entries(run_dir: Path) -> list[dict[str, Any]]:
    entries = []
    for path in sorted(item for item in run_dir.rglob("*") if item.is_file()):
        if path.name in {"run-report.json", "run-report.sha256", "final-offline-adjudication.json", "byte-closure-receipt.json", "byte-closure-receipt.sha256"}:
            continue
        rel = path.relative_to(run_dir).as_posix()
        evidence_class = "sanitized" if path.name in {"realm-observations.json", "request-header-captures.json", "lifecycle-receipt.json", "no-browser-tests.json"} else "raw"
        entries.append({"path": rel, "size": path.stat().st_size, "sha256": sha256_file(path), "evidenceClass": evidence_class})
    return entries


def tracked_reference(path: Path, evidence_class: str = "frozen-input") -> dict[str, Any]:
    return {
        "path": relative_repo_path(path),
        "size": path.stat().st_size,
        "sha256": sha256_file(path),
        "evidenceClass": evidence_class,
    }


def validate_storage_sequence(comparisons: dict[str, dict[str, Any]]) -> None:
    a1 = comparisons["A1"]["storage"]
    a2 = comparisons["A2"]["storage"]
    b1 = comparisons["B1"]["storage"]
    require(a1["boot"] == {"before": 0, "after": 1}, "a1_boot_invalid")
    require(a2["boot"] == {"before": 1, "after": 2}, "a2_boot_invalid")
    require(b1["boot"] == {"before": 0, "after": 1}, "b1_boot_invalid")
    require(a1["cookiePresentBefore"] is False and a1["localStoragePresentBefore"] is False, "a1_profile_not_fresh")
    require(a2["cookiePresentBefore"] is True and a2["localStoragePresentBefore"] is True, "a2_storage_not_continuous")
    require(a1["cookieValueSha256"] == a2["cookieValueSha256"] and a1["localStorageValueSha256"] == a2["localStorageValueSha256"], "a1_a2_storage_hash_drift")
    require(b1["cookiePresentBefore"] is False and b1["localStoragePresentBefore"] is False, "b1_profile_inherits_a_storage")
    require(a1["cookieValueSha256"] != b1["cookieValueSha256"] or a1["localStorageValueSha256"] != b1["localStorageValueSha256"], "ab_storage_hash_not_separate")


def validate_service_worker_sequence(comparisons: dict[str, dict[str, Any]]) -> None:
    a1 = comparisons["A1"]["serviceWorker"]
    a2 = comparisons["A2"]["serviceWorker"]
    b1 = comparisons["B1"]["serviceWorker"]
    require(a1["existedBefore"] is False and a2["existedBefore"] is True and b1["existedBefore"] is False, "service_worker_profile_state_mismatch")
    for item in (a1, a2, b1):
        require(item["scriptURLPath"] == "/fp2/service-worker.js" and item["scopePath"] == "/fp2/" and item["activeState"] == "activated", "service_worker_registration_invalid")
        require(item["topController"] is True or item["controlledPage"] is True, "service_worker_control_missing")
    require(a1["scriptSha256"] == a2["scriptSha256"] == b1["scriptSha256"], "service_worker_script_drift")


def build_report(
    *,
    run_id: str,
    run_dir: Path,
    git: dict[str, Any],
    candidate: dict[str, Any],
    artifact_infos: dict[str, dict[str, Any]],
    claim_hash: str,
    claim: dict[str, Any],
    runtime_preflight: dict[str, Any],
    previous_blocked_attempt: dict[str, Any],
    probe_manifest: dict[str, Any],
    probe_manifest_sha256: str,
    applicability_sha256: str,
    relation_sha256: str,
    static_diff: dict[str, Any],
    static_diff_sha256: str,
    no_browser: dict[str, Any],
    phase_records: list[dict[str, Any]],
    comparisons: Optional[dict[str, Any]],
    conclusion: str,
    failure: Optional[dict[str, str]],
    server_closed: bool,
    global_lock_released: bool,
) -> dict[str, Any]:
    previous_blocked, previous_attempts = previous_attempt_parts(previous_blocked_attempt)
    phase_public = []
    for record in phase_records:
        item = copy.deepcopy(record)
        item.pop("comparison", None)
        phase_public.append(item)
    report = {
        "schema": REPORT_SCHEMA,
        "taskVersion": TASK_VERSION,
        "executionGeneration": EXECUTION_GENERATION,
        "runId": run_id,
        "generatedAtUtc": utc_now(),
        "status": conclusion,
        "verified": False,
        "claims": {
            "fp2Accepted": False,
            "fp3Open": False,
            "managedIdentityVerified": False,
            "originalFp1EvidenceModified": False,
        },
        "git": {
            "branch": git["branch"],
            "startingHead": BASELINE_HEAD,
            "startingTree": BASELINE_TREE,
            "implementationCommit": git["head"],
            "implementationTree": git["tree"],
            "upstream": git["upstream"],
            "trackedWorktreeCleanAtClaim": git["trackedWorktreeClean"],
        },
        "candidate": candidate,
        "artifacts": artifact_infos,
        "previousBlockedAttempt": previous_blocked,
        "previousAttempts": previous_attempts,
        "runtimePreflight": {
            "receiptPath": runtime_preflight["receiptPath"],
            "receiptSha256": runtime_preflight["receiptSha256"],
            "dependencyClosureSha256": runtime_preflight["runtimeBinding"]["dependencyClosureSha256"],
            "childInvocationSha256": runtime_preflight["runtimeBinding"]["childInvocationSha256"],
        },
        "probeBundle": {
            "manifestPath": relative_repo_path(PROBE_MANIFEST_PATH),
            "manifestSha256": probe_manifest_sha256,
            "fileCount": len(probe_manifest["files"]),
        },
        "applicabilityLedger": {"path": relative_repo_path(APPLICABILITY_PATH), "sha256": applicability_sha256},
        "relationMatrix": {"path": relative_repo_path(RELATION_PATH), "sha256": relation_sha256},
        "staticAbDiff": {
            "path": "static-ab-diff.json",
            "sha256": static_diff_sha256,
            "keys": static_diff["keys"],
        },
        "loopback": claim["loopback"],
        "deadlinesSeconds": claim["deadlinesSeconds"],
        "matrix": phase_public,
        "comparisons": comparisons or {"notCompleted": True},
        "noBrowserRegression": no_browser,
        "evidence": {
            "claim": {"path": relative_repo_path(GLOBAL_CLAIM_PATH), "sha256": claim_hash, "evidenceClass": "one-shot-claim"},
            "runEvidenceRoot": relative_repo_path(run_dir),
            "files": public_phase_file_entries(run_dir),
            "reportReferencedFiles": [],
            "serverClosed": server_closed,
            "globalBrowserLockReleased": global_lock_released,
        },
        "scope": {
            "fp2Only": True,
            "fp1OriginalProbeModified": False,
            "fp1EvidenceModified": False,
            "fp3Touched": False,
            "m3WiTouched": False,
            "standardSiloTouched": False,
            "publicHostProtocolChanged": False,
            "browserPatchChanged": False,
            "timeoutChanged": False,
        },
        "failure": failure,
        "backlog": [
            "FP3 network identity / Geo / WebRTC",
            "broader Worker APIs",
            "TLS/QUIC",
            "real sites",
            "cross-machine replay",
            "clean M3-WI",
            "product integration",
            "release/signing",
        ],
    }
    return report


def write_offline_adjudication(run_dir: Path, report_sha256: str, conclusion: str, checks: dict[str, Any]) -> Path:
    path = run_dir / "final-offline-adjudication.json"
    write_json(
        path,
        {
            "schema": ADJUDICATION_SCHEMA,
            "runReportSha256": report_sha256,
            "status": conclusion,
            "verified": False,
            "checks": checks,
        },
    )
    return path


def write_byte_closure(run_dir: Path) -> tuple[Path, str]:
    path = run_dir / "byte-closure-receipt.json"
    entries = []
    for item in sorted(run_dir.rglob("*")):
        if not item.is_file() or item == path:
            continue
        entries.append({"path": item.relative_to(run_dir).as_posix(), "size": item.stat().st_size, "sha256": sha256_file(item)})
    write_json(path, {"schema": "verisilo-camoufox-fp2-byte-closure/v1", "files": entries, "verified": False})
    digest = sha256_file(path)
    path.with_name("byte-closure-receipt.sha256").write_text(f"{digest}  {path.name}\n", encoding="ascii")
    return path, digest


def finalize_report_artifacts(
    *,
    run_dir: Path,
    report: dict[str, Any],
    conclusion: str,
    checks: dict[str, Any],
) -> dict[str, Any]:
    """Write and close a report without relying on browser execution side effects."""
    ensure_sanitized(report, "run-report")
    report_path = run_dir / "run-report.json"
    write_json(report_path, report)
    report_sha256 = write_report_sidecar(report_path)
    validate_hash_sidecar(report_path, run_dir / "run-report.sha256")
    adjudication_path = write_offline_adjudication(run_dir, report_sha256, conclusion, checks)
    adjudication_sha256 = write_sha256_sidecar(adjudication_path, "final-offline-adjudication.sha256")
    closure_path, closure_sha256 = write_byte_closure(run_dir)
    return {
        "reportPath": report_path,
        "reportSha256": report_sha256,
        "adjudicationPath": adjudication_path,
        "adjudicationSha256": adjudication_sha256,
        "closurePath": closure_path,
        "closureSha256": closure_sha256,
    }


def synthetic_report_finalization_test() -> dict[str, Any]:
    """Exercise success/failed/blocked finalization with no claim or browser."""
    require_no_target_processes("before synthetic finalization")
    assert_port_free(DEFAULT_RUN_PORT)
    results: dict[str, Any] = {}
    with tempfile.TemporaryDirectory(prefix="fp2-finalization-") as folder:
        root = Path(folder)
        for conclusion in ("execution-passed-awaiting-main-brain-gate", "failed", "blocked"):
            run_dir = root / conclusion.replace("/", "-")
            run_dir.mkdir()
            report = {
                "schema": REPORT_SCHEMA,
                "taskVersion": TASK_VERSION,
                "status": conclusion,
                "verified": False,
                "claims": {"fp2Accepted": False, "fp3Open": False, "managedIdentityVerified": False},
                "failure": None if conclusion == "execution-passed-awaiting-main-brain-gate" else {"code": "synthetic", "detail": conclusion},
            }
            artifacts = finalize_report_artifacts(
                run_dir=run_dir,
                report=report,
                conclusion=conclusion,
                checks={"synthetic": True, "verified": False},
            )
            require((artifacts["reportPath"]).is_file(), "synthetic_finalization_failed", conclusion)
            require((artifacts["adjudicationPath"]).is_file(), "synthetic_finalization_failed", conclusion)
            require((artifacts["closurePath"]).is_file(), "synthetic_finalization_failed", conclusion)
            results[conclusion] = {
                "reportSha256": artifacts["reportSha256"],
                "adjudicationSha256": artifacts["adjudicationSha256"],
                "closureSha256": artifacts["closureSha256"],
            }
    require_no_target_processes("after synthetic finalization")
    assert_port_free(DEFAULT_RUN_PORT)
    return {"status": "passed", "verified": False, "cases": results, "claimCreated": False, "browserLaunchCalled": False}


BLOCKED_FAILURE_CODES = {
    "baseline_worktree_dirty",
    "baseline_branch_mismatch",
    "baseline_ancestry_mismatch",
    "accepted_ancestor_missing",
    "baseline_tree_missing",
    "native_windows_required",
    "candidate_lock_missing",
    "candidate_lock_hash_mismatch",
    "candidate_archive_missing",
    "candidate_archive_hash_mismatch",
    "candidate_archive_size_mismatch",
    "candidate_executable_missing",
    "candidate_executable_hash_mismatch",
    "candidate_tree_manifest_missing",
    "candidate_tree_manifest_hash_mismatch",
    "candidate_browser_root_missing",
    "candidate_tree_shape_mismatch",
    "candidate_tree_canonical_mismatch",
    "candidate_engine_revision_mismatch",
    "candidate_lock_binding_mismatch",
    "candidate_evidence_semantics_mismatch",
    "candidate_source_binding_mismatch",
    "candidate_lock_invalid",
    "candidate_binding_invalid",
    "artifact_sidecar_missing",
    "artifact_sidecar_mismatch",
    "artifact_baseline_drift",
    "probe_manifest_invalid",
    "probe_manifest_file_missing",
    "probe_bundle_file_missing",
    "probe_bundle_hash_mismatch",
    "no_browser_test_missing",
    "no_browser_tests_failed",
    "invalid_run_port",
    "loopback_port_in_use",
    "loopback_capability_unavailable",
    "run_port_changed",
    "one_shot_claim_already_exists",
    "parent_metadata_unavailable",
    "timeout_budget_unfrozen",
    "process_scan_failed",
    "process_cleanliness_unverifiable",
    "process_scan_backend_unavailable",
    "process_scan_backend_invalid",
    "target_processes_present",
    "runtime_native_windows_required",
    "runtime_interpreter_missing",
    "runtime_interpreter_invalid",
    "runtime_interpreter_outside_repo",
    "runtime_interpreter_mismatch",
    "runtime_interpreter_hash_mismatch",
    "runtime_python_version_mismatch",
    "runtime_implementation_mismatch",
    "runtime_module_path_mismatch",
    "runtime_dependency_missing",
    "runtime_dependency_resolution_failed",
    "runtime_dependency_version_mismatch",
    "runtime_host_import_missing",
    "runtime_host_import_failed",
    "runtime_browser_spawn_boundary_unavailable",
    "runtime_preflight_receipt_invalid",
    "runtime_preflight_receipt_missing",
    "runtime_preflight_receipt_mismatch",
    "runtime_preflight_child_failed",
    "runtime_preflight_timeout",
    "runtime_preflight_failed",
    "runtime_preflight_arguments_missing",
    "runtime_dependency_closure_mismatch",
    "runtime_child_invocation_invalid",
    "runtime_child_invocation_mismatch",
    "runtime_environment_changed",
    "previous_blocked_claim_missing",
    "previous_blocked_claim_hash_mismatch",
    "previous_blocked_claim_mismatch",
    "previous_blocked_report_missing",
    "previous_blocked_report_mismatch",
    "previous_blocked_observation_mismatch",
    "previous_generation2_claim_missing",
    "previous_generation2_claim_hash_mismatch",
    "previous_generation2_claim_mismatch",
    "previous_generation2_report_missing",
    "previous_generation2_report_hash_mismatch",
    "previous_generation2_report_mismatch",
    "previous_generation2_observation_mismatch",
    "previous_attempts_invalid",
    "synthetic_finalization_failed",
    "runtime_preflight_process_residual",
    "runtime_preflight_port_residual",
}


def conclusion_for_failure(failure: Optional[dict[str, str]], *, preclaim: bool = False) -> str:
    if failure is None:
        return "execution-passed-awaiting-main-brain-gate"
    if preclaim or failure.get("code") in BLOCKED_FAILURE_CODES:
        return "blocked"
    return "failed"


def safe_failure(failure: Optional[dict[str, str]]) -> Optional[dict[str, str]]:
    if failure is None:
        return None
    detail = str(failure.get("detail", ""))
    detail = ABSOLUTE_PATH.sub("<redacted-path>", detail)
    return {"code": str(failure.get("code", "runner_failure")), "detail": detail[:256]}


def runtime_preflight_child(args: argparse.Namespace) -> int:
    result: dict[str, Any] = {
        "schema": RUNTIME_PREFLIGHT_CHILD_SCHEMA,
        "status": "blocked",
        "verified": False,
    }
    try:
        expected_interpreter = Path(args.expected_interpreter).resolve()
        actual_interpreter = Path(sys.executable).resolve()
        require(actual_interpreter == expected_interpreter, "runtime_interpreter_mismatch", RUNTIME_INTERPRETER_RELATIVE.as_posix())
        actual_version = ".".join(str(part) for part in sys.version_info[:3])
        require(actual_version == EXPECTED_RUNTIME_PYTHON_VERSION, "runtime_python_version_mismatch", actual_version)
        require(sys.implementation.name == "cpython", "runtime_implementation_mismatch", sys.implementation.name)
        host_dir = HOST_DIR.resolve()
        require(
            any(Path(item).resolve() == host_dir for item in sys.path if item),
            "runtime_module_path_mismatch",
            RUNTIME_INTERPRETER_RELATIVE.as_posix(),
        )
        dependencies = runtime_dependency_snapshot()
        invocation = runtime_invocation_descriptor(actual_interpreter)
        closure = {
            "interpreterSha256": sha256_file(actual_interpreter),
            "pythonVersion": actual_version,
            "implementation": EXPECTED_RUNTIME_IMPLEMENTATION,
            "dependencies": dependencies,
            "childInvocation": invocation,
        }
        runtime_binding = {
            "interpreterRelativePath": invocation["interpreterRelativePath"],
            "interpreterSha256": closure["interpreterSha256"],
            "pythonVersion": actual_version,
            "implementation": EXPECTED_RUNTIME_IMPLEMENTATION,
            "dependencyClosureSha256": runtime_dependency_closure_sha256(closure),
            "childInvocationSha256": sha256_bytes(canonical_json_bytes(invocation)),
        }
        result.update(
            {
                "status": "passed",
                "runtimeBinding": runtime_binding,
                "dependencyClosure": closure,
                "browserSpawnBoundary": dependencies["browserSpawnBoundary"],
                "claimCreationAllowed": True,
            }
        )
    except FP2Failure as exc:
        result["failure"] = {"code": exc.code, "detail": safe_failure({"code": exc.code, "detail": exc.detail})["detail"]}
    except Exception as exc:  # noqa: BLE001 - runtime bootstrap is fail-closed
        result["failure"] = {"code": "runtime_preflight_child_failed", "detail": type(exc).__name__}
    result_path = Path(args.preflight_result).resolve()
    write_json(result_path, result)
    emit_child_event({"event": "runtime-preflight", "status": result["status"], "verified": False})
    return 0 if result["status"] == "passed" else 1


def validate_runtime_preflight_result(result: dict[str, Any], interpreter: Path) -> dict[str, Any]:
    require(result.get("schema") == RUNTIME_PREFLIGHT_CHILD_SCHEMA, "runtime_preflight_receipt_invalid", "schema")
    if result.get("status") != "passed":
        failure = result.get("failure")
        if isinstance(failure, dict) and isinstance(failure.get("code"), str):
            fail(failure["code"], str(failure.get("detail", failure["code"])))
        fail("runtime_preflight_child_failed", str(failure or {}))
    require(result.get("verified") is False, "runtime_preflight_receipt_invalid", "verified")
    binding = result.get("runtimeBinding")
    closure = result.get("dependencyClosure")
    boundary = result.get("browserSpawnBoundary")
    require(isinstance(binding, dict) and isinstance(closure, dict), "runtime_preflight_receipt_invalid", "binding")
    require(isinstance(boundary, dict) and boundary.get("ready") is True and boundary.get("browserLaunchCalled") is False, "runtime_browser_spawn_boundary_unavailable")
    expected_path = relative_repo_path(interpreter)
    require(binding.get("interpreterRelativePath") == expected_path, "runtime_interpreter_mismatch", expected_path)
    require(binding.get("interpreterSha256") == sha256_file(interpreter), "runtime_interpreter_hash_mismatch", expected_path)
    require(binding.get("pythonVersion") == EXPECTED_RUNTIME_PYTHON_VERSION, "runtime_python_version_mismatch", str(binding.get("pythonVersion")))
    require(binding.get("implementation") == EXPECTED_RUNTIME_IMPLEMENTATION, "runtime_implementation_mismatch", str(binding.get("implementation")))
    require(binding.get("dependencyClosureSha256") == runtime_dependency_closure_sha256(closure), "runtime_dependency_closure_mismatch")
    invocation = closure.get("childInvocation")
    require(isinstance(invocation, dict), "runtime_child_invocation_invalid")
    require(binding.get("childInvocationSha256") == sha256_bytes(canonical_json_bytes(invocation)), "runtime_child_invocation_mismatch")
    require(invocation == runtime_invocation_descriptor(interpreter), "runtime_child_invocation_mismatch")
    return binding


def validate_runtime_binding(binding: dict[str, Any]) -> None:
    interpreter = resolve_runtime_interpreter()
    require(binding.get("interpreterRelativePath") == relative_repo_path(interpreter), "runtime_environment_changed", "interpreter path")
    require(binding.get("interpreterSha256") == sha256_file(interpreter), "runtime_environment_changed", "interpreter hash")
    require(binding.get("pythonVersion") == EXPECTED_RUNTIME_PYTHON_VERSION, "runtime_environment_changed", "python version")
    require(binding.get("implementation") == EXPECTED_RUNTIME_IMPLEMENTATION, "runtime_environment_changed", "implementation")
    require(binding.get("childInvocationSha256") == sha256_bytes(canonical_json_bytes(runtime_invocation_descriptor(interpreter))), "runtime_environment_changed", "child invocation")


def write_sha256_sidecar(path: Path, sidecar_name: Optional[str] = None) -> str:
    digest = sha256_file(path)
    sidecar = path.with_name(sidecar_name or f"{path.name}.sha256")
    sidecar.write_text(f"{digest}  {path.name}\n", encoding="ascii")
    return digest


def write_failure_phase_record(
    *,
    run_dir: Path,
    label: str,
    artifact_id: str,
    artifact_sha256: str,
    profile_id: str,
    port: int,
    failure: dict[str, str],
) -> dict[str, Any]:
    phase_dir = run_dir / label
    phase_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_path = phase_dir / "lifecycle-receipt.json"
    write_json(
        lifecycle_path,
        {
            "schema": "verisilo-camoufox-fp2-phase-failure/v1",
            "label": label,
            "status": "failed",
            "verified": False,
            "failure": failure,
        },
    )
    return {
        "label": label,
        "status": "failed",
        "artifactId": artifact_id,
        "artifactSha256": artifact_sha256,
        "profileId": profile_id,
        "probePort": port,
        "headerCaptureCount": 0,
        "validation": {"requiredRealms": len(CANONICAL_REALMS), "crossRealmIdentity": False, "headerCoherence": False, "serviceWorker": False},
        "failure": failure,
        "files": {
            "childResult": None,
            "rawRealms": None,
            "realmObservations": None,
            "requestHeaders": None,
            "lifecycle": phase_file_entry(lifecycle_path, run_dir, "sanitized-lifecycle"),
            "childStdout": None,
            "childStderr": None,
        },
    }


def tracked_evidence_references(
    probe_manifest: dict[str, Any],
    runtime_preflight: Optional[dict[str, Any]] = None,
) -> list[dict[str, Any]]:
    references = [
        tracked_reference(LEGACY_CLAIM_PATH, "previous-blocked-claim"),
        tracked_reference(ARTIFACT_A_PATH, "artifact-input"),
        tracked_reference(ARTIFACT_A_PATH.with_name(ARTIFACT_A_PATH.name + ".sha256"), "artifact-sidecar"),
        tracked_reference(ARTIFACT_B_PATH, "artifact-input"),
        tracked_reference(ARTIFACT_B_PATH.with_name(ARTIFACT_B_PATH.name + ".sha256"), "artifact-sidecar"),
        tracked_reference(ASSET_LOCK_PATH, "candidate-asset-lock"),
        tracked_reference(TREE_MANIFEST_PATH, "candidate-tree-manifest"),
        tracked_reference(APPLICABILITY_PATH, "applicability-ledger"),
        tracked_reference(RELATION_PATH, "relation-matrix"),
        tracked_reference(PROBE_MANIFEST_PATH, "probe-bundle-manifest"),
        tracked_reference(Path(__file__).resolve(), "runner"),
        tracked_reference(NO_BROWSER_TEST_PATH, "no-browser-tests"),
    ]
    for item in probe_manifest["files"]:
        references.append(tracked_reference(BUNDLE_DIR / item["path"], "probe-bundle-file"))
    if runtime_preflight is not None:
        for raw_path, evidence_class in (
            (runtime_preflight["receiptPath"], "runtime-preflight-receipt"),
            (runtime_preflight["receiptPath"].replace(".json", ".sha256"), "runtime-preflight-sidecar"),
            (runtime_preflight["byteClosurePath"], "runtime-preflight-byte-closure"),
            (runtime_preflight["byteClosurePath"].replace(".json", ".sha256"), "runtime-preflight-byte-closure-sidecar"),
        ):
            references.append(tracked_reference(REPO_ROOT / Path(raw_path), evidence_class))
    return references


def update_report_references(report: dict[str, Any], references: list[dict[str, Any]]) -> None:
    report["evidence"]["reportReferencedFiles"] = references


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--child-session", action="store_true")
    parser.add_argument("--runtime-preflight-child", action="store_true")
    parser.add_argument("--execute-browser-matrix", action="store_true", help="Run generation-3 A1 -> A2 -> B1 after preflight; default is no-browser preflight only.")
    parser.add_argument("--run-port", type=int, default=DEFAULT_RUN_PORT)
    parser.add_argument("--label")
    parser.add_argument("--artifact-root")
    parser.add_argument("--profile-root")
    parser.add_argument("--state-root")
    parser.add_argument("--cache-root")
    parser.add_argument("--artifact-id")
    parser.add_argument("--artifact-sha256")
    parser.add_argument("--profile-id")
    parser.add_argument("--asset-lock")
    parser.add_argument("--browser-root")
    parser.add_argument("--tree-manifest")
    parser.add_argument("--primary-origin")
    parser.add_argument("--nonce")
    parser.add_argument("--bundle-manifest-sha256")
    parser.add_argument("--expected-boot-before", type=int)
    parser.add_argument("--expected-boot-after", type=int)
    parser.add_argument("--child-result")
    parser.add_argument("--raw-result")
    parser.add_argument("--expected-interpreter")
    parser.add_argument("--preflight-result")
    return parser


def require_child_args(args: argparse.Namespace) -> None:
    required = (
        "label",
        "artifact_root",
        "profile_root",
        "state_root",
        "cache_root",
        "artifact_id",
        "artifact_sha256",
        "profile_id",
        "asset_lock",
        "browser_root",
        "tree_manifest",
        "primary_origin",
        "nonce",
        "bundle_manifest_sha256",
        "expected_boot_before",
        "expected_boot_after",
        "child_result",
        "raw_result",
    )
    for name in required:
        require(getattr(args, name) is not None, "child_arguments_missing", name)


def require_runtime_preflight_args(args: argparse.Namespace) -> None:
    for name in ("expected_interpreter", "preflight_result"):
        require(getattr(args, name) is not None, "runtime_preflight_arguments_missing", name)


def orchestrate(args: argparse.Namespace) -> int:
    """Run generation-3 preflight; browser execution requires an explicit flag."""

    try:
        git = git_preflight()
        ledger = load_applicability()
        relation = load_relation_matrix()
        probe_manifest, probe_manifest_sha256 = load_probe_manifest()
        artifact_a, artifact_a_info = load_artifact(ARTIFACT_A_PATH)
        artifact_b, artifact_b_info = load_artifact(ARTIFACT_B_PATH)
        static_diff = build_static_diff(artifact_a, artifact_b, relation)
        static_diff["aArtifactSha256"] = artifact_a_info["sha256"]
        static_diff["bArtifactSha256"] = artifact_b_info["sha256"]
        candidate = validate_candidate_static()
        require_no_target_processes("before FP2 claim")
        assert_port_free(args.run_port)
        no_browser = run_no_browser_tests()
        previous = previous_execution_attempts()
        require(not GLOBAL_CLAIM_PATH.exists(), "one_shot_claim_already_exists", GLOBAL_CLAIM_PATH.name)
        interpreter = resolve_runtime_interpreter()
        runtime_preflight = run_runtime_preflight(interpreter=interpreter, port=args.run_port, previous=previous, git=git)
        require(runtime_preflight.get("status") == "passed", "runtime_preflight_required")
        require(runtime_preflight.get("claimCreationAllowed") is True, "runtime_preflight_required")
        require_no_target_processes("after runtime preflight closure")
        assert_port_free(args.run_port)
    except FP2Failure as exc:
        print(conclusion_for_failure({"code": exc.code, "detail": exc.detail}, preclaim=True))
        return 1

    if not args.execute_browser_matrix:
        print("runtime-preflight-closure-passed-awaiting-generation-3-authorization")
        return 0

    run_id = f"fp2-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{uuid.uuid4().hex[:10]}"
    run_dir = FP2_EVIDENCE_ROOT / run_id
    runtime_root: Optional[Path] = None
    server: Optional[FP2HTTPServer] = None
    claim: Optional[dict[str, Any]] = None
    claim_hash: Optional[str] = None
    claim_created = False
    server_closed = True
    phase_records: list[dict[str, Any]] = []
    comparisons: dict[str, dict[str, Any]] = {}
    failure: Optional[dict[str, str]] = None
    conclusion = "failed"
    global_lock_released = True
    try:
        run_dir.mkdir(parents=True, exist_ok=False)
        write_json(run_dir / "static-ab-diff.json", static_diff)
        copy_json(PROBE_MANIFEST_PATH, run_dir / "probe-bundle-manifest.json")
        copy_json(APPLICABILITY_PATH, run_dir / "applicability-ledger.json")
        copy_json(RELATION_PATH, run_dir / "relation-matrix.json")
        write_json(run_dir / "no-browser-tests.json", no_browser)
        claim, claim_hash = create_claim(
            run_id=run_id,
            run_dir=run_dir,
            port=args.run_port,
            git=git,
            candidate=candidate,
            artifacts={"A": artifact_a_info, "B": artifact_b_info},
            probe_manifest_sha256=probe_manifest_sha256,
            applicability_sha256=sha256_file(APPLICABILITY_PATH),
            relation_sha256=sha256_file(RELATION_PATH),
            static_diff_sha256=sha256_file(run_dir / "static-ab-diff.json"),
            no_browser_test_sha256=no_browser["testFileSha256"],
            runtime_preflight=runtime_preflight,
            previous_blocked_attempt=previous,
        )
        claim_created = True
        write_sha256_sidecar(run_dir / "one-shot-claim.json", "one-shot-claim.sha256")
        try:
            validate_runtime_binding(runtime_preflight["runtimeBinding"])
        except FP2Failure as exc:
            failure = {"code": exc.code, "detail": exc.detail}
        if failure is None:
            runtime_root = Path(tempfile.mkdtemp(prefix="verisilo-fp2-"))
            server = FP2HTTPServer(BUNDLE_DIR, args.run_port, {item["path"] for item in probe_manifest["files"]})
            server_closed = False
        if failure is None:
            profile_root = runtime_root / "profiles"
            cache_root = runtime_root / "cache"
            phase_specs = (
                ("A1", artifact_a, artifact_a_info, "identity-win-canvas-v1-a", "fp2-a", (0, 1)),
                ("A2", artifact_a, artifact_a_info, "identity-win-canvas-v1-a", "fp2-a", (1, 2)),
                ("B1", artifact_b, artifact_b_info, "identity-win-canvas-v1-b", "fp2-b", (0, 1)),
            )
            for label, artifact, artifact_info, artifact_id, profile_id, expected_boot in phase_specs:
                try:
                    phase = run_one_phase(
                        interpreter=interpreter,
                        label=label,
                        run_dir=run_dir,
                        server=server,
                        artifact=artifact,
                        artifact_sha256=artifact_info["sha256"],
                        artifact_id=artifact_id,
                        profile_id=profile_id,
                        profile_root=profile_root,
                        artifact_root=ARTIFACT_DIR,
                        cache_root=cache_root,
                        runtime_state_root=runtime_root / "state",
                        ledger=ledger,
                        probe_manifest=probe_manifest,
                        bundle_manifest_sha256=probe_manifest_sha256,
                        port=args.run_port,
                        expected_boot=expected_boot,
                    )
                except FP2Failure as exc:
                    phase_failure = {"code": exc.code, "detail": exc.detail}
                    phase_records.append(
                        write_failure_phase_record(
                            run_dir=run_dir,
                            label=label,
                            artifact_id=artifact_id,
                            artifact_sha256=artifact_info["sha256"],
                            profile_id=profile_id,
                            port=args.run_port,
                            failure=phase_failure,
                        )
                    )
                    failure = phase_failure
                    break
                phase_records.append(phase["record"])
                if phase["record"]["status"] != "passed" or phase["comparison"] is None:
                    failure = phase["record"].get("failure") or {"code": "phase_failed", "detail": label}
                    break
                comparisons[label] = phase["comparison"]
        if failure is None and set(comparisons) == set(SESSION_LABELS):
            validate_storage_sequence(comparisons)
            validate_service_worker_sequence(comparisons)
            a1_a2 = compare_session_pair("A1/A2", comparisons["A1"], comparisons["A2"], same_artifact=True)
            ab = compare_ab(comparisons["A1"], comparisons["A2"], comparisons["B1"], artifact_a, artifact_b, ledger, relation)
            comparisons_summary: dict[str, Any] = {
                "A1/A2": a1_a2,
                "A/B": ab,
                "storage": {"bootAndContinuity": True, "profileIsolation": True},
                "serviceWorker": {"registrationReplay": True, "profileIsolation": True},
            }
        else:
            comparisons_summary = None
    except FP2Failure as exc:
        failure = {"code": exc.code, "detail": exc.detail}
        comparisons_summary = None
    except Exception as exc:  # noqa: BLE001 - final evidence is fail-closed
        failure = {"code": "runner_exception", "detail": type(exc).__name__}
        comparisons_summary = None
    finally:
        if server is not None:
            try:
                server.close()
                server_closed = not server.thread.is_alive()
                if not server_closed and failure is None:
                    failure = {"code": "loopback_server_unclean", "detail": "server thread remained alive"}
                assert_port_free(args.run_port)
            except FP2Failure as exc:
                server_closed = False
                failure = failure or {"code": exc.code, "detail": exc.detail}
            except Exception as exc:  # noqa: BLE001
                server_closed = False
                failure = failure or {"code": "loopback_server_unclean", "detail": type(exc).__name__}
        if failure is None:
            try:
                require_no_target_processes("after FP2 matrix")
            except FP2Failure as exc:
                failure = {"code": exc.code, "detail": exc.detail}
        conclusion = conclusion_for_failure(failure)

    if runtime_root is not None:
        try:
            if not target_processes():
                shutil.rmtree(runtime_root)
        except Exception as exc:  # noqa: BLE001 - evidence records cleanup failure
            failure = failure or {"code": "runtime_cleanup_failed", "detail": type(exc).__name__}
            conclusion = conclusion_for_failure(failure)

    if claim_created and claim is not None and claim_hash is not None:
        report = build_report(
            run_id=run_id,
            run_dir=run_dir,
            git=git,
            candidate=candidate,
            artifact_infos={"A": artifact_a_info, "B": artifact_b_info},
            claim_hash=claim_hash,
            claim=claim,
            runtime_preflight=runtime_preflight,
            previous_blocked_attempt=previous,
            probe_manifest=probe_manifest,
            probe_manifest_sha256=probe_manifest_sha256,
            applicability_sha256=sha256_file(APPLICABILITY_PATH),
            relation_sha256=sha256_file(RELATION_PATH),
            static_diff=static_diff,
            static_diff_sha256=sha256_file(run_dir / "static-ab-diff.json"),
            no_browser=no_browser,
            phase_records=phase_records,
            comparisons=comparisons_summary if failure is None else None,
            conclusion=conclusion,
            failure=safe_failure(failure),
            server_closed=server_closed,
            global_lock_released=global_lock_released,
        )
        update_report_references(report, tracked_evidence_references(probe_manifest, runtime_preflight))
        finalize_report_artifacts(
            run_dir=run_dir,
            report=report,
            conclusion=conclusion,
            checks={
                "claimHash": claim_hash,
                "staticDiffKeysExact": static_diff["keys"] == list(EXPECTED_STATIC_DIFF),
                "requiredRealmCount": len(CANONICAL_REALMS),
                "phaseCount": len(phase_records),
                "allRequiredPhasesCompleted": set(item["label"] for item in phase_records) == set(SESSION_LABELS),
                "runtimePreflightPassed": runtime_preflight.get("status") == "passed",
                "serverClosed": server_closed,
                "verified": False,
            },
        )

    print(conclusion)
    return 0 if conclusion == "execution-passed-awaiting-main-brain-gate" else 1


def main() -> int:
    args = build_parser().parse_args()
    if args.runtime_preflight_child:
        require_runtime_preflight_args(args)
        return runtime_preflight_child(args)
    if args.child_session:
        require_child_args(args)
        return asyncio.run(run_child_session(args))
    return orchestrate(args)


if __name__ == "__main__":
    raise SystemExit(main())
