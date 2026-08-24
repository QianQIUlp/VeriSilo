#!/usr/bin/env python3
"""FP2-R1 Voices V1-V4 diagnostic-only execution package.

The default command performs no browser launch.  Browser execution is behind
the explicit ``--execute-browser-diagnostic`` switch and writes to a separate
one-shot namespace that can never satisfy an FP2 or Formal R1 claim.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import contextlib
import hashlib
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
import zipfile
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from typing import Any

import fp2_cross_realm as fp2
import host_v1 as host_module
from browser_tree import TREE_MANIFEST_SCHEMA, TreeIntegrityError, verify_tree


REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
SOURCE_LOCK_PATH = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
)
PATCH_9000_PATH = (
    HOST_DIR
    / "patches"
    / "camoufox"
    / "v152.0.4-beta.28-r1-diag"
    / "9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch"
)
BUILD_EVIDENCE_DIR = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-r1-diag-build"
    / "r1diag-engine-20260823t1542z"
)
ARCHIVE_PATH = BUILD_EVIDENCE_DIR / "camoufox-152.0.4-beta.28-win.x86_64.zip"
EXTRACTION_MANIFEST_PATH = BUILD_EVIDENCE_DIR / "windows-extraction-tree.json"
EXTRACTED_BROWSER_ROOT = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-fp2-r1-diag"
    / "runtime"
    / "r1diag-engine-20260823t1542z"
    / "extracted-browser"
)
BASE_ARTIFACT_PATH = (
    REPO_ROOT / "tests" / "fixtures" / "camoufox" / "identity-win-canvas-v1-a.json"
)
GEN5_RAW_REALMS_PATH = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-fp2"
    / "fp2-20260822T065118Z-18fdbee7a8"
    / "A1"
    / "raw-realms.json"
)
EVIDENCE_ROOT = REPO_ROOT / "artifacts" / "camoufox-fp2-r1-diag"
CLAIM_PATH = EVIDENCE_ROOT / "fp2-r1-diag-v1-one-shot-claim.json"
PLAYWRIGHT_DRIVER_DIR = (
    HOST_DIR / ".venv" / "Lib" / "site-packages" / "playwright" / "driver"
)
PLAYWRIGHT_NODE_PATH = PLAYWRIGHT_DRIVER_DIR / "node.exe"
PLAYWRIGHT_CORE_BUNDLE_PATH = PLAYWRIGHT_DRIVER_DIR / "package" / "lib" / "coreBundle.js"
PLAYWRIGHT_TRANSPORT_PATH = (
    HOST_DIR / ".venv" / "Lib" / "site-packages" / "playwright" / "_impl" / "_transport.py"
)
PLAYWRIGHT_DRIVER_PY_PATH = PLAYWRIGHT_TRANSPORT_PATH.with_name("_driver.py")
WINDOWS_SUPERVISOR_SOURCE_PATH = (
    HOST_DIR / "windows-supervisor" / "src" / "main.rs"
)

EXPECTED_LOCK_SHA256 = "6b93a2425cbf8c54c542a8d134a051d51be39f32239150d2f7ae515b2f00186b"
EXPECTED_LOCK_SIZE = 50158
EXPECTED_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-r1-diag-v2"
EXPECTED_RUN_ID = "r1diag-engine-20260823t1542z"
EXPECTED_ARCHIVE_SHA256 = "241b656945260963ff66b4fcff8ded313bd1b45f066b000b726f950b08a8ae3d"
EXPECTED_ARCHIVE_SIZE = 493471385
EXPECTED_EXE_SHA256 = "9fef022fea062f22e4916e4c125c913931eefe8afe522d3930089ed3393dbfd5"
EXPECTED_BUILD_ID = "20260811045234"
EXPECTED_SOURCE_STAMP = "e39c605adc0fc049a165d7fe4a3f6517b761edf7"
EXPECTED_TREE_SHA256 = "d65b168849b4df8f1fde52e8627e834e3d0b85b4c4e7befb5b179a8440211e06"
EXPECTED_TREE_SIZE = 95902
EXPECTED_BASE_ARTIFACT_SHA256 = "e273ca6376c9f4984a3bd7d78885771d3d5c712881da49691f67c2a44a8684bb"
EXPECTED_GEN5_RAW_REALMS_SHA256 = "ebf10af98b0074b1a48ba0da1bc45788e7cb410bbf9e94714529c84cfc13d9c8"
EXPECTED_PATCH_9000_SHA256 = "1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8"
EXPECTED_SUPERVISOR_SHA256 = "d12204d76ecebed681f601a95e47f29a75bc67a879b6106fb3c1f38579054a98"
EXPECTED_SUPERVISOR_SIZE = 185856
EXPECTED_PATCH_ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "9000"]
EXPECTED_RUNTIME_PYTHON = "3.12.13"
EXPECTED_RUNTIME_PACKAGES = {"camoufox": "0.5.4", "playwright": "1.60.0", "browserforge": "1.2.4"}
DEFAULT_PORT = 18193
SESSION_WATCHDOG_SECONDS = 60
REALM_DEADLINE_SECONDS = 15
PARENT_WATCHDOG_SECONDS = 150

READINESS_SCHEMA = "verisilo-fp2-r1-diag-readiness/v1"
CLAIM_SCHEMA = "verisilo-fp2-r1-diag-one-shot-claim/v1"
CHILD_AUTH_SCHEMA = "verisilo-fp2-r1-diag-child-authorization/v1"
REPORT_SCHEMA = "verisilo-fp2-r1-diag-run/v1"
TIMELINE_SCHEMA = "verisilo-fp2-r1-diag-timeline/v1"
DECISION_SCHEMA = "verisilo-fp2-r1-diag-v1-v4-decision/v1"
DIAGNOSTIC_MARKER = "# VERISILO-DIAGNOSTIC-MARKER: v1"
CHILD_TOKEN_ENV = "VERISILO_R1_DIAG_CHILD_TOKEN"

EVENT_NAMES = {
    "E1_mvoices_parsed",
    "E2a_sapi_init_begin",
    "E2b_sapi_init_end",
    "E3a_managed_batch_begin",
    "E3b_managed_batch_end",
    "E4_sendinit_snapshot",
    "E5_send_voice_added",
    "E6_recv_initial_voices",
    "E6_recv_add_voice",
    "E7_getvoices",
    "OVERFLOW",
}
PLAYWRIGHT_VSIDIAG_RE = re.compile(
    r"^pw:browser \[pid=(?P<pid>[1-9][0-9]*)\]\[err\] (?P<line>VSIDIAG .+)$"
)


class DiagnosticError(RuntimeError):
    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}" if detail else code)
        self.code = code
        self.detail = detail


def require(condition: bool, code: str, detail: str = "") -> None:
    if not condition:
        raise DiagnosticError(code, detail)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False, allow_nan=False
    ).encode("utf-8")


def strict_json_bytes(raw: bytes, label: str) -> dict[str, Any]:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise DiagnosticError("duplicate_json_key", f"{label}:{key}")
            result[key] = value
        return result

    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                DiagnosticError("invalid_json_number", f"{label}:{token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise DiagnosticError("invalid_json", f"{label}:{type(exc).__name__}") from exc
    require(type(value) is dict, "invalid_json_shape", label)
    return value


def strict_json(path: Path) -> dict[str, Any]:
    require(path.is_file(), "evidence_missing", path.as_posix())
    return strict_json_bytes(path.read_bytes(), path.name)


def write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")


def file_receipt(path: Path) -> dict[str, Any]:
    return {"name": path.name, "sha256": sha256_file(path), "sizeBytes": path.stat().st_size}


def diagnostic_child_environment(token: str) -> dict[str, str]:
    environment = fp2.child_environment()
    environment.update(
        {
            CHILD_TOKEN_ENV: token,
            "DEBUG": "pw:browser",
            "DEBUG_COLORS": "0",
            "DEBUG_HIDE_DATE": "1",
        }
    )
    environment.pop("DEBUG_FILE", None)
    return environment


def native_supervisor_receipt() -> dict[str, Any]:
    path = host_module.SUPERVISOR.resolve()
    require(path.is_file(), "native_supervisor_missing")
    try:
        host_module.ensure_no_reparse_points(path)
    except OSError as exc:
        raise DiagnosticError("native_supervisor_reparse_rejected", str(exc)) from exc
    receipt = file_receipt(path)
    require(
        receipt["sha256"] == EXPECTED_SUPERVISOR_SHA256
        and receipt["sizeBytes"] == EXPECTED_SUPERVISOR_SIZE,
        "native_supervisor_binding_mismatch",
    )
    return receipt


def _manifest_file_entries(manifest: dict[str, Any]) -> list[dict[str, Any]]:
    entries = manifest.get("entries")
    require(type(entries) is list, "extraction_manifest_invalid", "entries")
    files: list[dict[str, Any]] = []
    seen: set[str] = set()
    for index, entry in enumerate(entries):
        require(type(entry) is dict, "extraction_manifest_invalid", str(index))
        require(entry.get("type") in {"file", "directory"}, "extraction_manifest_invalid", str(index))
        expected_keys = {"path", "sha256", "sizeBytes", "type"} if entry["type"] == "file" else {"path", "type"}
        require(set(entry) == expected_keys, "extraction_manifest_invalid", str(index))
        path = entry["path"]
        require(type(path) is str and path and "\\" not in path, "extraction_manifest_invalid", str(index))
        parts = path.rstrip("/").split("/")
        require(not path.startswith("/") and all(part not in {"", ".", ".."} for part in parts), "extraction_manifest_unsafe_path", path)
        key = path.rstrip("/").casefold()
        require(key not in seen, "extraction_manifest_case_collision", path)
        seen.add(key)
        if entry["type"] == "file":
            require(type(entry["sizeBytes"]) is int and entry["sizeBytes"] >= 0, "extraction_manifest_invalid", path)
            require(type(entry["sha256"]) is str and re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]) is not None, "extraction_manifest_invalid", path)
            files.append(entry)
    require(len(entries) == 514, "extraction_manifest_shape_mismatch", "entries")
    require(len(files) == 503, "extraction_manifest_shape_mismatch", "files")
    require(sum(item["sizeBytes"] for item in files) == 982403785, "extraction_manifest_shape_mismatch", "bytes")
    return files


def _voice_hash12(uri: str) -> str:
    return hashlib.sha256(uri.encode("utf-8")).hexdigest()[:12]


def reference_voice_hashes() -> dict[str, set[str]]:
    require(sha256_file(BASE_ARTIFACT_PATH) == EXPECTED_BASE_ARTIFACT_SHA256, "base_artifact_hash_mismatch")
    artifact = strict_json(BASE_ARTIFACT_PATH)
    voices = artifact.get("resolvedConfig", {}).get("voices")
    require(type(voices) is list and len(voices) == 53, "managed_voice_reference_invalid")
    managed = {_voice_hash12(item["voiceUri"]) for item in voices}
    require(len(managed) == 53, "managed_voice_hash_collision")

    require(sha256_file(GEN5_RAW_REALMS_PATH) == EXPECTED_GEN5_RAW_REALMS_SHA256, "gen5_voice_reference_hash_mismatch")
    raw = strict_json(GEN5_RAW_REALMS_PATH)
    native_voices = raw.get("realms", {}).get("top-window", {}).get("voices", {}).get("voices")
    require(type(native_voices) is list and len(native_voices) == 5, "native_voice_reference_invalid")
    native = {_voice_hash12(item["voiceURI"]) for item in native_voices}
    require(len(native) == 5 and managed.isdisjoint(native), "voice_reference_hash_collision")
    return {"managed": managed, "knownNative": native}


def verify_playwright_capture_bridge() -> dict[str, Any]:
    paths = (
        PLAYWRIGHT_NODE_PATH,
        PLAYWRIGHT_CORE_BUNDLE_PATH,
        PLAYWRIGHT_TRANSPORT_PATH,
        PLAYWRIGHT_DRIVER_PY_PATH,
        WINDOWS_SUPERVISOR_SOURCE_PATH,
    )
    require(all(path.is_file() for path in paths), "playwright_capture_bridge_missing")
    core = PLAYWRIGHT_CORE_BUNDLE_PATH.read_text(encoding="utf-8")
    transport = PLAYWRIGHT_TRANSPORT_PATH.read_text(encoding="utf-8")
    driver = PLAYWRIGHT_DRIVER_PY_PATH.read_text(encoding="utf-8")
    supervisor = WINDOWS_SUPERVISOR_SOURCE_PATH.read_text(encoding="utf-8")
    require(
        'this.logName = "browser";' in core
        and 'options2.log(`[pid=${spawnedProcess.pid}][err] ` + data);' in core
        and "stderr=_get_stderr_fileno()" in transport
        and "env = os.environ.copy()" in driver,
        "playwright_capture_bridge_semantics_mismatch",
    )
    require(
        "startup.std_error = stdio_handles[2];" in supervisor
        and "CreateProcessW(" in supervisor
        and "CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT" in supervisor,
        "supervisor_capture_bridge_semantics_mismatch",
    )
    environment = diagnostic_child_environment("capture-probe-token")
    environment.pop(CHILD_TOKEN_ENV)
    script = (
        "require('./apps/camoufox-host/.venv/Lib/site-packages/playwright/driver/"
        "package/lib/utilsBundle').debug('pw:browser')"
        "('[pid=123][err] VSIDIAG sample')"
    )
    completed = subprocess.run(
        [str(PLAYWRIGHT_NODE_PATH), "-e", script],
        cwd=REPO_ROOT,
        env=environment,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    require(
        completed.returncode == 0
        and completed.stdout == ""
        and completed.stderr.strip() == "pw:browser [pid=123][err] VSIDIAG sample",
        "playwright_capture_bridge_probe_failed",
    )
    return {
        "mode": "playwright-pw-browser-stderr-v1",
        "node": file_receipt(PLAYWRIGHT_NODE_PATH),
        "coreBundle": file_receipt(PLAYWRIGHT_CORE_BUNDLE_PATH),
        "pythonTransport": file_receipt(PLAYWRIGHT_TRANSPORT_PATH),
        "pythonDriver": file_receipt(PLAYWRIGHT_DRIVER_PY_PATH),
        "supervisorSource": file_receipt(WINDOWS_SUPERVISOR_SOURCE_PATH),
        "probe": "passed-no-browser",
    }


def verify_readiness(*, hash_archive: bool = True) -> dict[str, Any]:
    require(os.name == "nt", "native_windows_required")
    raw_lock = SOURCE_LOCK_PATH.read_bytes()
    require(len(raw_lock) == EXPECTED_LOCK_SIZE, "source_lock_size_mismatch")
    require(sha256_bytes(raw_lock) == EXPECTED_LOCK_SHA256, "source_lock_hash_mismatch")
    lock = strict_json_bytes(raw_lock, SOURCE_LOCK_PATH.name)
    require(lock.get("schema") == "verisilo-r1-diag-source-binding/v2", "source_lock_schema_mismatch")
    require(lock.get("engineRevision") == EXPECTED_ENGINE_REVISION, "engine_revision_mismatch")
    require(lock.get("status") == "diagnostic-engine-build-provenance-closed", "source_lock_status_mismatch")
    require(lock.get("diagnosticOnly") is True and lock.get("formalEligible") is False, "diagnostic_classification_mismatch")
    require(lock.get("verified") is False and lock.get("browserLaunches") == 0, "runtime_claim_boundary_mismatch")
    binding_root = lock.get("buildBinding")
    require(type(binding_root) is dict and binding_root.get("status") == "diagnostic-engine-build-provenance-closed", "build_binding_status_mismatch")
    require(lock.get("pendingAtBuildHost") == [], "build_binding_pending")
    binding = binding_root.get("binaryBinding")
    require(type(binding) is dict, "binary_binding_missing")
    require(binding.get("runId") == EXPECTED_RUN_ID and binding.get("buildMode") == "diagnostic", "binary_binding_identity_mismatch")
    require(binding.get("engineRevision") == EXPECTED_ENGINE_REVISION, "binary_binding_identity_mismatch")
    require(binding.get("completeAppliedPatchOrder") == EXPECTED_PATCH_ORDER, "patch_order_mismatch")
    claims = binding.get("claims")
    require(claims == {"compiled": True, "diagnosticOnly": True, "formalEligible": False, "verified": False, "windowsRuntimeObserved": False}, "binary_claims_mismatch")
    gate = binding.get("diagnosticGateResult")
    require(type(gate) is dict and gate.get("ok") is True and gate.get("diagnosticOnly") is True and gate.get("formalEligible") is False, "diagnostic_gate_mismatch")
    require(gate.get("purpose") == "fp2-r1-voices-v1-v4-discrimination", "diagnostic_gate_purpose_mismatch")
    host = binding.get("hostProvenance")
    require(type(host) is dict and host.get("status") == "container-passed" and host.get("containerExitCode") == 0, "host_provenance_mismatch")
    archive = binding.get("archive")
    require(type(archive) is dict, "archive_binding_missing")
    require(archive.get("sha256") == EXPECTED_ARCHIVE_SHA256 and archive.get("sizeBytes") == EXPECTED_ARCHIVE_SIZE, "archive_binding_mismatch")
    require(archive.get("camoufoxExeSha256") == EXPECTED_EXE_SHA256, "executable_binding_mismatch")
    require(archive.get("buildId") == EXPECTED_BUILD_ID and archive.get("sourceStamp") == EXPECTED_SOURCE_STAMP, "bundle_metadata_binding_mismatch")
    require(archive.get("entryCount") == 514 and archive.get("fileCount") == 503 and archive.get("totalUncompressedFileBytes") == 982403785, "archive_shape_mismatch")
    tree_binding = archive.get("treeManifest")
    require(type(tree_binding) is dict and tree_binding.get("sha256") == EXPECTED_TREE_SHA256 and tree_binding.get("sizeBytes") == EXPECTED_TREE_SIZE, "tree_binding_mismatch")

    evidence_fields = {
        "buildResult": "build-result.json",
        "hostProvenance": "host-provenance.json",
        "preparedBuilderBinding": "builder-image-result.json",
        "buildEngineStart": "build-engine-start.json",
        "diagnosticGateResult": "diagnostic-gate-result.json",
        "buildLog": "build.log",
        "containerLog": "container.log",
    }
    for field, name in evidence_fields.items():
        expected = binding.get(field)
        path = BUILD_EVIDENCE_DIR / name
        require(type(expected) is dict and path.is_file(), "build_evidence_missing", name)
        require(expected.get("sha256") == sha256_file(path) and expected.get("sizeBytes") == path.stat().st_size, "build_evidence_mismatch", name)

    require(ARCHIVE_PATH.is_file() and ARCHIVE_PATH.stat().st_size == EXPECTED_ARCHIVE_SIZE, "archive_missing")
    if hash_archive:
        require(sha256_file(ARCHIVE_PATH) == EXPECTED_ARCHIVE_SHA256, "archive_hash_mismatch")
    require(EXTRACTION_MANIFEST_PATH.is_file() and file_receipt(EXTRACTION_MANIFEST_PATH) == {"name": EXTRACTION_MANIFEST_PATH.name, "sha256": EXPECTED_TREE_SHA256, "sizeBytes": EXPECTED_TREE_SIZE}, "extraction_manifest_binding_mismatch")
    manifest = strict_json(EXTRACTION_MANIFEST_PATH)
    require(manifest.get("schema") == "verisilo-windows-extraction-tree/v1", "extraction_manifest_schema_mismatch")
    files = _manifest_file_entries(manifest)
    by_path = {item["path"]: item for item in files}
    for name, digest in archive.get("requiredMemberSha256", {}).items():
        require(by_path.get(name, {}).get("sha256") == digest, "required_member_binding_mismatch", name)
    require(by_path["camoufox.exe"]["sha256"] == EXPECTED_EXE_SHA256, "executable_binding_mismatch")

    require(sha256_file(PATCH_9000_PATH) == EXPECTED_PATCH_9000_SHA256, "patch_9000_hash_mismatch")
    require(PATCH_9000_PATH.read_text(encoding="utf-8").splitlines()[0] == DIAGNOSTIC_MARKER, "patch_9000_marker_mismatch")
    hashes = reference_voice_hashes()
    capture_bridge = verify_playwright_capture_bridge()
    supervisor_receipt = native_supervisor_receipt()
    runtime = runtime_preflight()
    return {
        "schema": READINESS_SCHEMA,
        "status": "execution-package-ready-no-browser",
        "buildProvenanceStatus": "passed",
        "discriminatorContract": "actual-9000-amendment-v1",
        "diagnosticOnly": True,
        "formalEligible": False,
        "browserLaunches": 0,
        "verified": False,
        "sourceLock": file_receipt(SOURCE_LOCK_PATH),
        "runId": EXPECTED_RUN_ID,
        "engineRevision": EXPECTED_ENGINE_REVISION,
        "archive": {"sha256": EXPECTED_ARCHIVE_SHA256, "sizeBytes": EXPECTED_ARCHIVE_SIZE},
        "executableSha256": EXPECTED_EXE_SHA256,
        "treeManifest": file_receipt(EXTRACTION_MANIFEST_PATH),
        "baseArtifact": file_receipt(BASE_ARTIFACT_PATH),
        "historicalNativeReference": file_receipt(GEN5_RAW_REALMS_PATH),
        "managedVoiceHashCount": len(hashes["managed"]),
        "knownNativeVoiceHashCount": len(hashes["knownNative"]),
        "captureBridge": capture_bridge,
        "nativeSupervisor": supervisor_receipt,
        "nativeSupervisorClassification": "host-local-observed-support-asset",
        "runtime": runtime,
        "sourceAdjudication": {
            "V1": "source-refuted-as-written",
            "V2": "source-refuted-as-written",
            "T1": "content-mirror-incremental-delivery-positive-signature",
            "V3": "unexplained-content-local-transition-only",
            "V4": "source-seam-suspicion-only",
        },
        "nextActionRequiresBrowserAuthorization": True,
        "claims": {
            "fp2R1Accepted": False,
            "formalR1": False,
            "gpcRuntimeVerified": False,
            "voicesFixed": False,
            "remediationSuccess": False,
        },
    }


def _zip_member_path(name: str) -> str:
    normalized = name.replace("\\", "/")
    require(normalized == name and normalized and not normalized.startswith("/"), "unsafe_archive_path", name)
    parts = normalized.rstrip("/").split("/")
    require(all(part not in {"", ".", ".."} for part in parts) and ":" not in parts[0], "unsafe_archive_path", name)
    return normalized


def verify_extracted_browser(root: Path) -> dict[str, Any]:
    manifest = strict_json(EXTRACTION_MANIFEST_PATH)
    files = _manifest_file_entries(manifest)
    projected = {
        "schema": TREE_MANIFEST_SCHEMA,
        "treeRootLabel": root.name,
        "fileCount": len(files),
        "totalBytes": sum(item["sizeBytes"] for item in files),
        "entries": [
            {"path": item["path"], "size": item["sizeBytes"], "sha256": item["sha256"]}
            for item in files
        ],
    }
    try:
        verify_tree(root, projected)
    except TreeIntegrityError as exc:
        raise DiagnosticError("extracted_browser_tree_mismatch", str(exc)) from exc
    ini = (root / "application.ini").read_text(encoding="utf-8")
    build_id = re.search(r"^BuildID=(.+)$", ini, re.MULTILINE)
    source_stamp = re.search(r"^SourceStamp=(.+)$", ini, re.MULTILINE)
    require(build_id is not None and build_id.group(1).strip() == EXPECTED_BUILD_ID, "build_id_mismatch")
    require(source_stamp is not None and source_stamp.group(1).strip() == EXPECTED_SOURCE_STAMP, "source_stamp_mismatch")
    require(sha256_file(root / "camoufox.exe") == EXPECTED_EXE_SHA256, "executable_hash_mismatch")
    return {"fileCount": len(files), "totalBytes": sum(item["sizeBytes"] for item in files), "executableSha256": EXPECTED_EXE_SHA256}


def ensure_extracted_browser(*, readiness_verified: bool = False) -> dict[str, Any]:
    if not readiness_verified:
        verify_readiness(hash_archive=True)
    if EXTRACTED_BROWSER_ROOT.exists():
        return verify_extracted_browser(EXTRACTED_BROWSER_ROOT)
    EXTRACTED_BROWSER_ROOT.parent.mkdir(parents=True, exist_ok=True)
    manifest = strict_json(EXTRACTION_MANIFEST_PATH)
    entries = manifest["entries"]
    expected_names = {entry["path"] + ("/" if entry["type"] == "directory" and not entry["path"].endswith("/") else "") for entry in entries}
    with zipfile.ZipFile(ARCHIVE_PATH) as archive:
        infos = archive.infolist()
        names = {_zip_member_path(info.filename) for info in infos}
        require(len(names) == len(infos), "archive_duplicate_member")
        normalized_expected = {name.rstrip("/") for name in expected_names}
        require({name.rstrip("/") for name in names} == normalized_expected, "archive_manifest_member_mismatch")
        for info in infos:
            mode = (info.external_attr >> 16) & 0xF000
            require(mode not in {0xA000, 0x6000}, "archive_link_rejected", info.filename)
        with tempfile.TemporaryDirectory(prefix="extracting-", dir=EXTRACTED_BROWSER_ROOT.parent) as temp_name:
            temp_root = Path(temp_name)
            for info in infos:
                name = _zip_member_path(info.filename)
                target = temp_root.joinpath(*name.rstrip("/").split("/"))
                resolved_parent = target.parent.resolve()
                require(temp_root.resolve() == resolved_parent or temp_root.resolve() in resolved_parent.parents, "unsafe_archive_path", name)
                if info.is_dir() or name.endswith("/"):
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(info) as source, target.open("xb") as destination:
                    shutil.copyfileobj(source, destination, length=1024 * 1024)
            result = verify_extracted_browser(temp_root)
            temp_root.rename(EXTRACTED_BROWSER_ROOT)
    return result


def _validate_event(value: dict[str, Any], line_index: int) -> None:
    name = value.get("e")
    require(name in EVENT_NAMES, "vsidiag_unknown_event", str(name))
    if name == "OVERFLOW":
        require(set(value) == {"e"}, "vsidiag_invalid_fields", str(line_index))
        return
    fields = {
        "E1_mvoices_parsed": {"n"},
        "E2a_sapi_init_begin": set(),
        "E2b_sapi_init_end": set(),
        "E3a_managed_batch_begin": {"n"},
        "E3b_managed_batch_end": set(),
        "E4_sendinit_snapshot": {"n"},
        "E5_send_voice_added": {"h"},
        "E6_recv_initial_voices": {"n"},
        "E6_recv_add_voice": {"h"},
        "E7_getvoices": {"ctx", "n", "cache", "first"},
    }[name]
    require(set(value) == {"e", "proc", "seq", *fields}, "vsidiag_invalid_fields", str(line_index))
    expected_proc = "P" if name.startswith(("E1_", "E2", "E3", "E4_", "E5_")) else "C"
    require(value["proc"] == expected_proc, "vsidiag_invalid_process", str(line_index))
    require(type(value["seq"]) is int and 0 <= value["seq"] < 512, "vsidiag_invalid_sequence", str(line_index))
    if "n" in fields:
        valid_n = value["n"] is None if name == "E1_mvoices_parsed" else False
        valid_n = valid_n or (type(value["n"]) is int and value["n"] >= 0)
        require(valid_n, "vsidiag_invalid_count", str(line_index))
    if "h" in fields:
        require(type(value["h"]) is str and re.fullmatch(r"[0-9a-f]{12}", value["h"]) is not None, "vsidiag_invalid_uri_hash", str(line_index))
    if name == "E7_getvoices":
        require(type(value["ctx"]) is int and value["ctx"] >= 0, "vsidiag_invalid_context", str(line_index))
        require(type(value["cache"]) is int and value["cache"] >= 0, "vsidiag_invalid_cache_count", str(line_index))
        require(type(value["first"]) is int and value["first"] in {0, 1}, "vsidiag_invalid_first", str(line_index))


def parse_diagnostic_log(text: str) -> dict[str, Any]:
    events: list[dict[str, Any]] = []
    sequences: dict[str, set[int]] = {"P": set(), "C": set()}
    transport_pids: set[int] = set()
    for line_index, raw_line in enumerate(text.splitlines()):
        wrapped = PLAYWRIGHT_VSIDIAG_RE.fullmatch(raw_line)
        if wrapped is not None:
            diagnostic_line = wrapped.group("line")
            require(len(diagnostic_line.encode("utf-8")) <= 240, "vsidiag_line_too_long", str(line_index))
            value = strict_json_bytes(diagnostic_line[len("VSIDIAG ") :].encode("utf-8"), f"VSIDIAG:{line_index}")
            _validate_event(value, line_index)
            transport_pids.add(int(wrapped.group("pid")))
            if value["e"] != "OVERFLOW":
                proc = value["proc"]
                require(value["seq"] not in sequences[proc], "vsidiag_sequence_duplicate", f"{proc}:{value['seq']}")
                sequences[proc].add(value["seq"])
            events.append(value)
        elif "VSIDIAG " in raw_line:
            raise DiagnosticError("vsidiag_transport_invalid", str(line_index))
    require(not any(item["e"] == "OVERFLOW" for item in events), "vsidiag_overflow")
    for proc, values in sequences.items():
        if values:
            require(sorted(values) == list(range(len(values))), "vsidiag_sequence_gap", proc)
    require(len(transport_pids) == 1 and bool(events), "vsidiag_transport_unbound")
    return {
        "schema": TIMELINE_SCHEMA,
        "captureMode": "playwright-pw-browser-stderr-v1",
        "transportPids": sorted(transport_pids),
        "events": events,
    }


def _single_event(events: list[dict[str, Any]], name: str, *, required: bool = True) -> dict[str, Any] | None:
    selected = [item for item in events if item["e"] == name]
    if required:
        require(len(selected) == 1, "vsidiag_event_cardinality", name)
    else:
        require(len(selected) <= 1, "vsidiag_event_cardinality", name)
    return selected[0] if selected else None


def _top_observation(observation: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    configured = observation.get("configuredIdentityDigest")
    expected = observation.get("expectedConfiguredIdentityDigest")
    require(type(configured) is str and re.fullmatch(r"[0-9a-f]{64}", configured) is not None, "config_delivery_unproven")
    require(configured == expected, "config_delivery_unproven")
    require(observation.get("singleTopObjectSchedule") is True, "topology_schedule_unproven")
    pair = observation.get("top")
    require(type(pair) is dict and pair.get("waitReason") == "bounded-delay", "voice_observation_invalid")
    first_at = pair.get("firstAtMonotonicMs")
    second_at = pair.get("secondAtMonotonicMs")
    require(type(first_at) in {int, float} and type(second_at) in {int, float} and second_at > first_at, "voice_observation_invalid")
    inventories: list[dict[str, Any]] = []
    expected_keys = {"count", "uriHashes"}
    for label in ("first", "second"):
        value = pair.get(label)
        require(type(value) is dict and set(value) == expected_keys, "voice_observation_invalid", label)
        require(type(value["count"]) is int and value["count"] >= 0, "voice_observation_invalid", label)
        hashes = value["uriHashes"]
        require(type(hashes) is list and len(hashes) == value["count"] and len(set(hashes)) == len(hashes), "voice_observation_invalid", label)
        require(all(type(item) is str and re.fullmatch(r"[0-9a-f]{12}", item) is not None for item in hashes), "voice_observation_invalid", label)
        inventories.append(value)
    return inventories[0], inventories[1]


def classify_v1_v4(
    timeline: dict[str, Any],
    observation: dict[str, Any],
    *,
    managed_hashes: set[str],
    known_native_hashes: set[str],
) -> dict[str, Any]:
    events = timeline.get("events")
    require(type(events) is list, "timeline_invalid")
    first_observed, second_observed = _top_observation(observation)

    parent = sorted((item for item in events if item.get("proc") == "P"), key=lambda item: item["seq"])
    e1 = _single_event(parent, "E1_mvoices_parsed", required=False)
    e3a = _single_event(parent, "E3a_managed_batch_begin", required=False)
    e3b = _single_event(parent, "E3b_managed_batch_end", required=False)
    e2a = _single_event(parent, "E2a_sapi_init_begin")
    e2b = _single_event(parent, "E2b_sapi_init_end")
    e4 = _single_event(parent, "E4_sendinit_snapshot")

    result: dict[str, dict[str, Any]] = {
        "V1": {"status": "source-refuted-as-written", "evidence": ["E4 calls GetInstance; the managed batch completes before GetInstance returns"]},
        "V2": {"status": "source-refuted-as-written", "evidence": ["pinned SAPI Init enumerates and registers voices synchronously before the managed batch"]},
        "V3": {"status": "not-observed", "evidence": []},
        "V4": {"status": "not-observed", "evidence": []},
    }
    managed_count = 53
    if e1 is None:
        require(e3a is None and e3b is None, "managed_seam_log_inconsistent")
        require(e2a["seq"] < e2b["seq"] < e4["seq"], "pinned_source_order_contradicted")
        result["V4"] = {"status": "suspicion", "evidence": ["E4 executed but E1/E3 did not; source audit required"]}
    elif e1.get("n") is None:
        require(e3a is None and e3b is None, "managed_seam_log_inconsistent")
        require(e2a["seq"] < e2b["seq"] < e1["seq"] < e4["seq"], "pinned_source_order_contradicted")
        result["V4"] = {"status": "suspicion", "evidence": ["E1 parsed managed voices as null despite exact config delivery"]}
    else:
        require(e1.get("n") == managed_count, "managed_voice_count_mismatch")
        require((e3a is None) == (e3b is None), "managed_seam_log_inconsistent")
        if e3a is None:
            require(e2a["seq"] < e2b["seq"] < e1["seq"] < e4["seq"], "pinned_source_order_contradicted")
            result["V4"] = {"status": "suspicion", "evidence": ["E1 parsed 53 voices but the managed batch seam did not execute"]}
        else:
            require(e3a.get("n") == managed_count, "managed_batch_invalid")
            require(e2a["seq"] < e2b["seq"] < e1["seq"] < e3a["seq"] < e3b["seq"] < e4["seq"], "pinned_source_order_contradicted")
            late_native = [item for item in parent if item["e"] == "E5_send_voice_added" and item.get("h") in known_native_hashes and item["seq"] > e3b["seq"]]
            require(not late_native, "pinned_source_order_contradicted", "late native E5")

    all_e7 = [item for item in events if item["e"] == "E7_getvoices"]
    require(len(all_e7) == 2, "e7_producer_ambiguity")
    all_e7.sort(key=lambda item: item["seq"])
    first, second = all_e7
    require(first.get("ctx") == second.get("ctx") and first.get("first") == 1 and second.get("first") == 0, "top_e7_object_schedule_mismatch")
    require(first["seq"] < second["seq"], "top_e7_sequence_mismatch")
    require(first["cache"] == first["n"] == first_observed["count"], "first_observation_log_mismatch")
    require(second["cache"] == second["n"] == second_observed["count"], "second_observation_log_mismatch")
    between_delivery = [item for item in events if item.get("proc") == "C" and item["e"].startswith("E6_") and first["seq"] < item["seq"] < second["seq"]]
    managed_between = {item["h"] for item in between_delivery if item["e"] == "E6_recv_add_voice" and item["h"] in managed_hashes}
    full_snapshot_between = any(item["e"] == "E6_recv_initial_voices" and item["n"] == second["n"] for item in between_delivery)
    first_hashes = set(first_observed["uriHashes"])
    second_hashes = set(second_observed["uriHashes"])
    new_managed = (second_hashes - first_hashes) & managed_hashes
    managed_delta = len(second_hashes & managed_hashes) - len(first_hashes & managed_hashes)
    temporal_supported = (
        second["n"] > first["n"]
        and e4["n"] == second["n"] == len(known_native_hashes | managed_hashes)
        and first_hashes == known_native_hashes
        and second_hashes == known_native_hashes | managed_hashes
        and managed_delta == managed_count
        and (full_snapshot_between or new_managed <= managed_between)
    )
    temporal = {
        "status": "supported" if temporal_supported else "not-observed",
        "evidence": [] if not temporal_supported else [
            "same content mirror gained managed voices between the two top-object queries",
            f"count={first['n']}->{second['n']}",
            f"managedDelta={managed_delta}",
        ],
    }
    if first["n"] != second["n"] and not between_delivery:
        result["V3"] = {
            "status": "inconclusive",
            "evidence": ["content-local transition was not explained by an E6 delivery; mVoiceCache is not proven causal"],
        }

    supported = ["T1"] if temporal_supported else []
    conclusion = "temporal-incremental-delivery-supported" if temporal_supported else "inconclusive"
    if result["V4"]["status"] == "suspicion":
        conclusion = "source-seam-suspicion"
    elif result["V3"]["status"] == "inconclusive":
        conclusion = "unexplained-content-local-transition"
    return {
        "schema": DECISION_SCHEMA,
        "diagnosticOnly": True,
        "formalEligible": False,
        "supported": supported,
        "axes": result,
        "actualCompensation": {"T1_contentMirrorIncrementalDelivery": temporal},
        "conclusion": conclusion,
        "exhaustiveExclusionClaim": False,
        "limits": [
            "V1 as written and V2 are source-refuted for the pinned FF152 image; T1 is a distinct content-side compensation",
            "a count change without E6 does not prove mVoiceCache causality",
            "proc:C has no PID; top-only execution and contiguous sequence are operational attribution constraints",
            "no FP2, Formal R1, GPC runtime, or voices-remediation claim is produced",
        ],
    }


DIAG_HTML = b"<!doctype html><meta charset=utf-8><title>VeriSilo FP2-R1 diagnostic</title><body></body>\n"


class _DiagnosticHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        if self.path.split("?", 1)[0] != "/diag.html":
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(DIAG_HTML)))
        self.end_headers()
        self.wfile.write(DIAG_HTML)

    def log_message(self, _format: str, *args: object) -> None:
        return


class DiagnosticServer:
    def __init__(self, port: int) -> None:
        self.server = ThreadingHTTPServer(("127.0.0.1", port), _DiagnosticHandler)
        self.thread = Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
        require(not self.thread.is_alive(), "diagnostic_server_unclean")


VOICE_PAIR_JS = r"""
async () => {
  const snapshot = () => speechSynthesis.getVoices().map((voice) => voice.voiceURI);
  const firstAt = performance.now();
  const first = snapshot();
  await new Promise((resolve) => setTimeout(resolve, 3000));
  const secondAt = performance.now();
  const second = snapshot();
  return {firstAt, secondAt, waitReason: "bounded-delay", first, second};
}
"""


def _voice_inventory_summary(value: object) -> dict[str, Any]:
    require(type(value) is list, "voice_observation_invalid")
    require(all(type(item) is str for item in value), "voice_observation_invalid")
    hashes = {_voice_hash12(item) for item in value}
    require(len(hashes) == len(value), "voice_observation_invalid", "duplicate URI")
    return {"count": len(value), "uriHashes": sorted(hashes)}


def _voice_pair_summary(value: object) -> dict[str, Any]:
    require(type(value) is dict, "voice_observation_invalid")
    return {
        "firstAtMonotonicMs": value.get("firstAt"),
        "secondAtMonotonicMs": value.get("secondAt"),
        "waitReason": value.get("waitReason"),
        "first": _voice_inventory_summary(value.get("first")),
        "second": _voice_inventory_summary(value.get("second")),
    }


class R1DiagnosticHost(fp2.FP2ManagedHost):
    """Dedicated bridge from historical Artifact A config to the diag binary."""

    diag_result: dict[str, Any] | None = None

    def _prepare(self) -> None:
        self.artifact_root.mkdir(parents=True, exist_ok=True)
        self.profile_root.mkdir(parents=True, exist_ok=True)
        self.state_root.mkdir(parents=True, exist_ok=True)
        host_module.ensure_no_reparse_points(self.artifact_root)
        host_module.ensure_no_reparse_points(self.profile_root)
        host_module.ensure_no_reparse_points(self.state_root)
        require(self.asset_lock_arg == SOURCE_LOCK_PATH.resolve(), "diagnostic_source_lock_path_mismatch")
        require(self.browser_root_arg == EXTRACTED_BROWSER_ROOT.resolve(), "diagnostic_browser_root_path_mismatch")
        self.lock = {
            "schema": "verisilo-r1-diag-runtime-view/v1",
            "release": "v152.0.4-beta.28",
            "sha256": EXPECTED_ARCHIVE_SHA256,
            "sizeBytes": EXPECTED_ARCHIVE_SIZE,
            "engineRevision": EXPECTED_ENGINE_REVISION,
            "diagnosticOnly": True,
            "formalEligible": False,
        }
        self.executable = self.browser_root_arg / "camoufox.exe"
        cache_root = Path(os.environ.get("VERISILO_CAMOUFOX_CACHE_DIR", str(host_module.XDG_CACHE_DIR)))
        host_module.configure_camoufox_cache(cache_root)
        if not host_module.SUPERVISOR.exists():
            raise SystemExit(f"missing native supervisor: {host_module.SUPERVISOR}")
        host_module.install_download_guard()
        host_module.DownloadGuard.reset()

    def _verify_browser_binding_for_launch(self, artifact: dict) -> None:
        require(artifact.get("artifactId") == "identity-win-canvas-v1-a", "diagnostic_artifact_identity_mismatch")
        binding = artifact.get("browserBinding") or {}
        require(binding.get("archiveSha256") == fp2.EXPECTED_ARCHIVE_SHA256, "historical_artifact_binding_mismatch")
        require(file_receipt(SOURCE_LOCK_PATH) == {"name": SOURCE_LOCK_PATH.name, "sha256": EXPECTED_LOCK_SHA256, "sizeBytes": EXPECTED_LOCK_SIZE}, "source_lock_binding_drift")
        require(file_receipt(EXTRACTION_MANIFEST_PATH) == {"name": EXTRACTION_MANIFEST_PATH.name, "sha256": EXPECTED_TREE_SHA256, "sizeBytes": EXPECTED_TREE_SIZE}, "tree_manifest_binding_drift")
        native_supervisor_receipt()
        verify_extracted_browser(self.browser_root_arg)

    async def _launch_browser(self, session: dict[str, Any], artifact: dict[str, Any]) -> None:
        from functools import partial

        dependencies = fp2.resolve_browser_launch_dependencies()
        AsyncNewBrowser = dependencies["AsyncNewBrowser"]
        DefaultAddons = dependencies["DefaultAddons"]
        launch_options = dependencies["launch_options"]
        firefox_user_prefs_for_config = dependencies["firefox_user_prefs_for_config"]
        normalize_camou_config_env = dependencies["normalize_camou_config_env"]
        policy = artifact["policy"]
        disk_config = copy.deepcopy(artifact["resolvedConfig"])
        disk_digest = host_module.configured_identity_digest(disk_config)
        session["probePort"] = int(self.fp2_primary_origin.rsplit(":", 1)[1])
        session["server"] = None
        session["launchAttempted"] = True
        os.environ["VERISILO_REAL_EXE"] = str(self.executable)
        os.environ["VERISILO_EXIT_FILE"] = str(session["exitFile"])
        os.environ["VERISILO_SUPERVISOR_FILE"] = str(session["sessionDir"] / "supervisor.json")
        os.environ["VERISILO_PROFILE_LOCK_PATH"] = str(self.profile_root / f"{session['profileId']}.lock")
        session["expectedJobName"] = f"Local\\VeriSiloCamoufox-{session['sessionId']}"
        os.environ["VERISILO_JOB_NAME"] = session["expectedJobName"]

        started = time.perf_counter()
        opts = await asyncio.get_running_loop().run_in_executor(
            None,
            partial(
                launch_options,
                config=copy.deepcopy(disk_config),
                os=policy["targetOs"],
                window=tuple(policy["window"]),
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
        sent, diff, opts["env"] = normalize_camou_config_env(opts["env"], disk_config)
        require(host_module.configured_identity_digest(sent) == disk_digest and not any(diff.values()), "config_mutation")
        opts["executable_path"] = str(host_module.SUPERVISOR)
        session["browserSpawnCalled"] = True
        ctx = await AsyncNewBrowser(self.playwright, from_options=opts, persistent_context=True)
        session["ctx"] = ctx
        require(not host_module.DownloadGuard.tripped, "webdl_attempted")
        supervisor = await self._await_supervisor_metadata(session)
        session["supervisorMeta"] = supervisor
        session["pid"] = supervisor["supervisorPid"]
        session["childPid"] = supervisor["childPid"]
        session["managedIdentities"] = host_module.managed_identities(session)
        session["jobHandle"] = host_module.JobHandle.open(supervisor["jobName"])
        page = await ctx.new_page()
        session["page"] = page
        await page.goto(f"{self.fp2_primary_origin}/diag.html", wait_until="domcontentloaded", timeout=REALM_DEADLINE_SECONDS * 1000)
        top_raw = await asyncio.wait_for(page.evaluate(VOICE_PAIR_JS), timeout=REALM_DEADLINE_SECONDS)
        result = {
            "schema": "verisilo-fp2-r1-diag-browser-observation/v1",
            "diagnosticOnly": True,
            "formalEligible": False,
            "configuredIdentityDigest": disk_digest,
            "expectedConfiguredIdentityDigest": artifact["configuredIdentityDigest"],
            "singleTopObjectSchedule": True,
            "top": _voice_pair_summary(top_raw),
        }
        self.diag_result = result
        session["configuredIdentityDigest"] = disk_digest
        session["observedWebsiteDigest"] = sha256_bytes(canonical_bytes(result))
        session["spawnSeconds"] = round(time.perf_counter() - started, 3)
        session["probeSeconds"] = session["spawnSeconds"]
        session["fontMode"] = policy.get("fontMode", "inherit")
        session["state"] = "running"
        session["stopMonitor"] = asyncio.Event()
        session["monitorTask"] = asyncio.create_task(self._monitor_session(session))
        host_module.write_session_state(session)


def runtime_preflight() -> dict[str, Any]:
    interpreter = fp2.resolve_runtime_interpreter()
    script = (
        "import importlib.metadata,json,platform;"
        "print(json.dumps({'implementation':platform.python_implementation(),"
        "'version':platform.python_version(),'packages':{n:importlib.metadata.version(n) "
        "for n in ('camoufox','playwright','browserforge')}}))"
    )
    completed = subprocess.run(
        [str(interpreter), "-c", script],
        cwd=REPO_ROOT,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    require(completed.returncode == 0, "runtime_preflight_failed", completed.stderr[:160])
    value = strict_json_bytes(completed.stdout.strip().encode("utf-8"), "runtime-preflight")
    require(value.get("implementation") == "CPython" and value.get("version") == EXPECTED_RUNTIME_PYTHON, "runtime_version_mismatch")
    require(value.get("packages") == EXPECTED_RUNTIME_PACKAGES, "runtime_dependency_mismatch")
    return {
        "interpreter": {
            "relativePath": interpreter.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_file(interpreter),
            "sizeBytes": interpreter.stat().st_size,
        },
        **value,
        "browserLaunchCalled": False,
    }


def git_preflight() -> dict[str, Any]:
    require(fp2.git_command("status", "--porcelain", "--untracked-files=no") == "", "tracked_worktree_not_clean")
    head = fp2.git_command("rev-parse", "HEAD")
    tree = fp2.git_command("rev-parse", "HEAD^{tree}")
    upstream = fp2.git_command("rev-parse", "@{upstream}")
    require(head == upstream, "branch_not_pushed", f"{head}/{upstream}")
    return {"branch": fp2.git_command("branch", "--show-current"), "head": head, "tree": tree, "upstream": upstream, "trackedWorktreeClean": True}


def _write_exclusive_json(
    path: Path, value: object, *, exists_code: str = "one_shot_claim_already_exists"
) -> str:
    payload = json.dumps(value, ensure_ascii=False, indent=2).encode("utf-8") + b"\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    except FileExistsError as exc:
        raise DiagnosticError(exists_code, path.name) from exc
    with os.fdopen(descriptor, "wb") as handle:
        handle.write(payload)
        handle.flush()
        os.fsync(handle.fileno())
    return sha256_bytes(payload)


def _browser_launch_evidence(
    child: dict[str, Any], *, child_started: bool
) -> tuple[int | None, str]:
    value = child.get("browserSpawnCalled")
    if child.get("schema") == "verisilo-fp2-r1-diag-child/v1" and type(value) is bool:
        return int(value), "child-result"
    if child_started:
        return None, "unknown-after-child-start"
    return 0, "parent-pre-spawn"


def _post_run_process_cleanliness(
    run_id: str,
) -> tuple[bool | None, dict[str, str] | None]:
    try:
        processes = fp2.target_processes()
    except fp2.FP2Failure as exc:
        return None, {"code": exc.code, "detail": exc.detail}
    except Exception as exc:  # noqa: BLE001 - report must survive scanner failure
        return None, {
            "code": "process_cleanliness_unverifiable",
            "detail": type(exc).__name__,
        }
    if processes:
        return False, {"code": "diagnostic_residual_processes", "detail": run_id}
    return True, None


def _terminate_child_bounded(process: subprocess.Popen[Any]) -> bool:
    if process.poll() is not None:
        return True
    if os.name == "nt":
        try:
            subprocess.run(
                ["taskkill.exe", "/PID", str(process.pid), "/T", "/F"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
                timeout=10,
            )
        except (OSError, subprocess.TimeoutExpired):
            pass
    if process.poll() is None:
        with contextlib.suppress(OSError):
            process.kill()
    with contextlib.suppress(OSError, subprocess.TimeoutExpired):
        process.wait(timeout=10)
    return process.poll() is not None


def _consume_child_authorization(args: argparse.Namespace) -> None:
    require(
        type(args.child_run_id) is str
        and re.fullmatch(r"fp2-r1-diag-\d{8}T\d{6}Z-[0-9a-f]{10}", args.child_run_id) is not None,
        "child_authorization_invalid",
        "runId",
    )
    run_dir = (EVIDENCE_ROOT / args.child_run_id).resolve()
    authorization_path = Path(args.child_authorization).resolve()
    require(authorization_path == run_dir / "child-authorization.json", "child_authorization_invalid", "path")
    authorization = strict_json(authorization_path)
    require(authorization.get("schema") == CHILD_AUTH_SCHEMA and authorization.get("runId") == args.child_run_id, "child_authorization_invalid", "identity")
    require(authorization.get("runnerSha256") == sha256_file(Path(__file__)), "child_authorization_invalid", "runner")
    token = os.environ.pop(CHILD_TOKEN_ENV, "")
    require(type(token) is str and secrets.compare_digest(sha256_bytes(token.encode("utf-8")), authorization.get("tokenSha256", "")), "child_authorization_invalid", "token")
    require(CLAIM_PATH.is_file() and (run_dir / "one-shot-claim.json").is_file(), "child_authorization_invalid", "claim-missing")
    global_claim = CLAIM_PATH.read_bytes()
    claim_copy = (run_dir / "one-shot-claim.json").read_bytes()
    require(global_claim == claim_copy and sha256_bytes(global_claim) == authorization.get("claimSha256"), "child_authorization_invalid", "claim")
    claim = strict_json_bytes(global_claim, CLAIM_PATH.name)
    require(claim.get("runId") == args.child_run_id and claim.get("diagnosticOnly") is True and claim.get("formalEligible") is False, "child_authorization_invalid", "claim-boundary")
    require((claim.get("runner") or {}).get("sha256") == authorization.get("runnerSha256"), "child_authorization_invalid", "claim-runner")
    supervisor = native_supervisor_receipt()
    require(
        claim.get("nativeSupervisor") == supervisor
        and authorization.get("supervisorSha256") == supervisor["sha256"],
        "child_authorization_invalid",
        "supervisor",
    )
    capture_bridge = verify_playwright_capture_bridge()
    require(
        claim.get("captureBridge") == capture_bridge
        and authorization.get("captureBridgeSha256")
        == sha256_bytes(canonical_bytes(capture_bridge)),
        "child_authorization_invalid",
        "capture-bridge",
    )
    runtime_interpreter = (claim.get("runtime") or {}).get("interpreter") or {}
    actual_interpreter = Path(sys.executable).resolve()
    require(
        runtime_interpreter
        == {
            "relativePath": actual_interpreter.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_file(actual_interpreter),
            "sizeBytes": actual_interpreter.stat().st_size,
        }
        and authorization.get("runtimeInterpreterSha256")
        == runtime_interpreter.get("sha256"),
        "child_authorization_invalid",
        "runtime-interpreter",
    )
    expected = authorization.get("childArguments")
    actual = {
        "primaryOrigin": args.primary_origin,
        "profileRoot": str(Path(args.profile_root).resolve()),
        "stateRoot": str(Path(args.state_root).resolve()),
        "cacheRoot": str(Path(args.cache_root).resolve()),
        "observation": str(Path(args.observation).resolve()),
        "childResult": str(Path(args.child_result).resolve()),
    }
    require(expected == actual, "child_authorization_invalid", "arguments")
    _write_exclusive_json(
        run_dir / "child-authorization-consumed.json",
        {
            "schema": "verisilo-fp2-r1-diag-child-authorization-consumed/v1",
            "runId": args.child_run_id,
            "authorizationSha256": sha256_file(authorization_path),
            "consumerPid": os.getpid(),
        },
        exists_code="child_authorization_already_consumed",
    )


async def run_child(args: argparse.Namespace) -> int:
    _consume_child_authorization(args)
    require(
        os.environ.get("DEBUG") == "pw:browser"
        and os.environ.get("DEBUG_COLORS") == "0"
        and os.environ.get("DEBUG_HIDE_DATE") == "1"
        and "DEBUG_FILE" not in os.environ,
        "playwright_capture_environment_mismatch",
    )
    observation_path = Path(args.observation).resolve()
    result_path = Path(args.child_result).resolve()
    host: R1DiagnosticHost | None = None
    launch: dict[str, Any] | None = None
    close: dict[str, Any] | None = None
    failure: dict[str, str] | None = None
    try:
        os.environ["VERISILO_CAMOUFOX_CACHE_DIR"] = str(Path(args.cache_root).resolve())
        host = R1DiagnosticHost(
            artifact_root=BASE_ARTIFACT_PATH.parent,
            profile_root=Path(args.profile_root).resolve(),
            state_root=Path(args.state_root).resolve(),
            tree_manifest=EXTRACTION_MANIFEST_PATH,
            asset_lock=SOURCE_LOCK_PATH,
            browser_root=EXTRACTED_BROWSER_ROOT,
            primary_origin=args.primary_origin,
            nonce=secrets.token_urlsafe(24),
            bundle_manifest_sha256=sha256_file(Path(__file__)),
            ledger=fp2.load_applicability(),
            expected_boot=(0, 1),
        )
        from playwright.async_api import async_playwright

        async with async_playwright() as playwright:
            host.set_playwright(playwright)
            launch = await asyncio.wait_for(
                host.launch("identity-win-canvas-v1-a", "fp2-r1-diag-v1", EXPECTED_BASE_ARTIFACT_SHA256),
                timeout=SESSION_WATCHDOG_SECONDS,
            )
            require(launch.get("state") == "running" and host.diag_result is not None, "diagnostic_launch_failed")
            write_json(observation_path, host.diag_result)
            close = await asyncio.wait_for(host.close(launch["sessionId"]), timeout=SESSION_WATCHDOG_SECONDS)
            fp2.validate_close_receipt(close, "R1-DIAG")
            fp2.require_no_target_processes("after R1 diagnostic child")
    except (DiagnosticError, fp2.FP2Failure) as exc:
        failure = {"code": exc.code, "detail": exc.detail}
    except host_module.ProtocolError as exc:
        failure = fp2.protocol_error_failure(exc)
    except asyncio.TimeoutError:
        failure = {"code": "diagnostic_session_watchdog_timeout", "detail": "R1-DIAG"}
    except (Exception, SystemExit) as exc:  # noqa: BLE001 - child evidence is fail closed
        failure = {"code": "diagnostic_child_failed", "detail": type(exc).__name__}
    finally:
        if host is not None and host.session is not None and host.session.get("state") in {"starting", "running", "closing"}:
            with contextlib.suppress(Exception):
                await asyncio.wait_for(host.close(host.session["sessionId"]), timeout=SESSION_WATCHDOG_SECONDS)
        write_json(
            result_path,
            {
                "schema": "verisilo-fp2-r1-diag-child/v1",
                "status": "completed" if failure is None and close is not None else "failed",
                "diagnosticOnly": True,
                "formalEligible": False,
                "verified": False,
                "browserSpawnCalled": bool(host is not None and host.session is not None and host.session.get("browserSpawnCalled")),
                "launch": None if launch is None else {"state": launch.get("state"), "sessionId": launch.get("sessionId"), "configuredIdentityDigest": launch.get("configuredIdentityDigest")},
                "close": None if close is None else {"state": close.get("state"), "exitStatus": close.get("exitStatus"), "processTreeExited": (close.get("processTreeExit") or {}).get("exited"), "closeOutcome": (close.get("closeOutcome") or {}).get("status")},
                "failure": failure,
            },
        )
    return 0 if failure is None else 1


def execute_browser_diagnostic(port: int) -> int:
    readiness = verify_readiness(hash_archive=True)
    extraction = ensure_extracted_browser(readiness_verified=True)
    runtime = readiness["runtime"]
    git = git_preflight()
    fp2.require_no_target_processes("before R1 diagnostic claim")
    fp2.assert_port_free(port)
    require(not CLAIM_PATH.exists(), "one_shot_claim_already_exists", CLAIM_PATH.name)
    run_id = f"fp2-r1-diag-{datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{uuid.uuid4().hex[:10]}"
    run_dir = EVIDENCE_ROOT / run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    child_result_path = run_dir / "child-result.json"
    observation_path = run_dir / "voice-observation.json"
    stdout_path = run_dir / "child-stdout.log"
    stderr_path = run_dir / "child-stderr.log"
    claim = {
        "schema": CLAIM_SCHEMA,
        "runId": run_id,
        "createdAtUtc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "diagnosticOnly": True,
        "formalEligible": False,
        "verified": False,
        "browserLaunches": 0,
        "oneShot": True,
        "purpose": "fp2-r1-voices-actual-9000-causal-discrimination",
        "contract": "actual-9000-amendment-v1",
        "captureMode": "playwright-pw-browser-stderr-v1",
        "captureBridge": readiness["captureBridge"],
        "nativeSupervisor": readiness["nativeSupervisor"],
        "nativeSupervisorClassification": readiness["nativeSupervisorClassification"],
        "git": git,
        "sourceLock": readiness["sourceLock"],
        "binaryRunId": EXPECTED_RUN_ID,
        "archive": readiness["archive"],
        "executableSha256": EXPECTED_EXE_SHA256,
        "historicalArtifactBridge": {"artifact": readiness["baseArtifact"], "classification": "historical-artifact-config-on-diagnostic-derived-engine", "artifactReplayClaim": False},
        "runner": file_receipt(Path(__file__)),
        "runtime": runtime,
        "port": port,
        "claims": {"fp2R1Accepted": False, "formalR1": False, "voicesFixed": False, "gpcRuntimeVerified": False, "remediationSuccess": False},
    }
    claim_sha = _write_exclusive_json(CLAIM_PATH, claim)
    write_json(run_dir / "one-shot-claim.json", claim)
    exit_code = -1
    failure: dict[str, str] | None = None
    decision: dict[str, Any] | None = None
    child: dict[str, Any] = {}
    child_started = False
    child_exit_confirmed: bool | None = None
    server: DiagnosticServer | None = None
    runtime_root: Path | None = None
    process_clean: bool | None = None
    cleanup_failure: dict[str, str] | None = None
    try:
        server = DiagnosticServer(port)
        runtime_root = Path(tempfile.mkdtemp(prefix="verisilo-fp2-r1-diag-"))
        child_arguments = {
            "primaryOrigin": f"http://127.0.0.1:{port}",
            "profileRoot": str((runtime_root / "profiles").resolve()),
            "stateRoot": str((runtime_root / "state").resolve()),
            "cacheRoot": str((EVIDENCE_ROOT / "runtime-cache").resolve()),
            "observation": str(observation_path.resolve()),
            "childResult": str(child_result_path.resolve()),
        }
        child_token = secrets.token_urlsafe(32)
        authorization_path = run_dir / "child-authorization.json"
        _write_exclusive_json(
            authorization_path,
            {
                "schema": CHILD_AUTH_SCHEMA,
                "runId": run_id,
                "claimSha256": claim_sha,
                "tokenSha256": sha256_bytes(child_token.encode("utf-8")),
                "runnerSha256": sha256_file(Path(__file__)),
                "supervisorSha256": readiness["nativeSupervisor"]["sha256"],
                "captureBridgeSha256": sha256_bytes(
                    canonical_bytes(readiness["captureBridge"])
                ),
                "runtimeInterpreterSha256": runtime["interpreter"]["sha256"],
                "childArguments": child_arguments,
            },
            exists_code="child_authorization_already_exists",
        )
        command = [
            str(REPO_ROOT / fp2.RUNTIME_INTERPRETER_RELATIVE),
            str(Path(__file__).resolve()),
            "--child-session",
            "--child-run-id",
            run_id,
            "--child-authorization",
            str(authorization_path),
            "--primary-origin",
            child_arguments["primaryOrigin"],
            "--profile-root",
            child_arguments["profileRoot"],
            "--state-root",
            child_arguments["stateRoot"],
            "--cache-root",
            child_arguments["cacheRoot"],
            "--observation",
            child_arguments["observation"],
            "--child-result",
            child_arguments["childResult"],
        ]
        child_env = diagnostic_child_environment(child_token)
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            process = subprocess.Popen(command, cwd=REPO_ROOT, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, env=child_env)
            child_started = True
            try:
                exit_code = process.wait(timeout=PARENT_WATCHDOG_SECONDS)
                child_exit_confirmed = True
            except subprocess.TimeoutExpired:
                child_exit_confirmed = _terminate_child_bounded(process)
                exit_code = process.returncode if process.returncode is not None else -1
                failure = {
                    "code": (
                        "diagnostic_session_watchdog_timeout"
                        if child_exit_confirmed
                        else "child_process_termination_unconfirmed"
                    ),
                    "detail": run_id,
                }
        child = strict_json(child_result_path) if child_result_path.is_file() else {}
        if failure is None and (exit_code != 0 or child.get("status") != "completed"):
            failure = child.get("failure") or {"code": "diagnostic_child_failed", "detail": str(exit_code)}
        if failure is None:
            observation = strict_json(observation_path)
            timeline = parse_diagnostic_log(
                stderr_path.read_text(encoding="utf-8", errors="strict")
            )
            refs = reference_voice_hashes()
            decision = classify_v1_v4(timeline, observation, managed_hashes=refs["managed"], known_native_hashes=refs["knownNative"])
            write_json(run_dir / "vsidiag-timeline.json", timeline)
            write_json(run_dir / "v1-v4-decision.json", decision)
    except DiagnosticError as exc:
        failure = {"code": exc.code, "detail": exc.detail}
    except Exception as exc:  # noqa: BLE001 - parent report is fail closed
        failure = {"code": "diagnostic_parent_failed", "detail": type(exc).__name__}
    finally:
        if server is not None:
            try:
                server.close()
            except Exception as exc:  # noqa: BLE001 - cleanup is part of evidence
                if failure is None:
                    failure = {"code": "diagnostic_server_cleanup_failed", "detail": type(exc).__name__}
        process_clean, cleanup_failure = _post_run_process_cleanliness(run_id)
        if child_exit_confirmed is False:
            process_clean = False
            cleanup_failure = {
                "code": "child_process_termination_unconfirmed",
                "detail": run_id,
            }
        if failure is None and cleanup_failure is not None:
            failure = cleanup_failure
        if runtime_root is not None and process_clean is True:
            shutil.rmtree(runtime_root, ignore_errors=True)

    browser_launches, browser_launch_evidence = _browser_launch_evidence(
        child, child_started=child_started
    )
    report = {
        "schema": REPORT_SCHEMA,
        "runId": run_id,
        "status": "evidence-captured" if failure is None else "failed",
        "conclusion": None if failure is not None or decision is None else decision["conclusion"],
        "diagnosticOnly": True,
        "formalEligible": False,
        "verified": False,
        "browserLaunches": browser_launches,
        "claimSha256": claim_sha,
        "readiness": readiness,
        "extraction": extraction,
        "runtime": runtime,
        "exitCode": exit_code,
        "childProcessExitConfirmed": child_exit_confirmed,
        "processClean": process_clean,
        "cleanupFailure": cleanup_failure,
        "browserLaunchEvidence": browser_launch_evidence,
        "decision": decision,
        "failure": failure,
        "claims": {"fp2R1Accepted": False, "formalR1": False, "voicesFixed": False, "gpcRuntimeVerified": False, "remediationSuccess": False},
    }
    write_json(run_dir / "run-report.json", report)
    for path in run_dir.iterdir():
        if path.is_file() and path.suffix in {".json", ".log"}:
            (path.with_name(path.name + ".sha256")).write_text(f"{sha256_file(path)}  {path.name}\n", encoding="ascii", newline="\n")
    print("diagnostic-evidence-captured-awaiting-main-brain-gate" if failure is None else "diagnostic-run-failed")
    return 0 if failure is None else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-readiness", action="store_true")
    parser.add_argument("--materialize-browser", action="store_true")
    parser.add_argument("--execute-browser-diagnostic", action="store_true")
    parser.add_argument("--run-port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--child-session", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--child-run-id", help=argparse.SUPPRESS)
    parser.add_argument("--child-authorization", help=argparse.SUPPRESS)
    parser.add_argument("--primary-origin", help=argparse.SUPPRESS)
    parser.add_argument("--profile-root", help=argparse.SUPPRESS)
    parser.add_argument("--state-root", help=argparse.SUPPRESS)
    parser.add_argument("--cache-root", help=argparse.SUPPRESS)
    parser.add_argument("--observation", help=argparse.SUPPRESS)
    parser.add_argument("--child-result", help=argparse.SUPPRESS)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    modes = sum(bool(value) for value in (args.check_readiness, args.materialize_browser, args.execute_browser_diagnostic, args.child_session))
    require(modes <= 1, "execution_mode_conflict")
    if args.child_session:
        for name in (
            "child_run_id",
            "child_authorization",
            "primary_origin",
            "profile_root",
            "state_root",
            "cache_root",
            "observation",
            "child_result",
        ):
            require(getattr(args, name) is not None, "child_arguments_missing", name)
        return asyncio.run(run_child(args))
    if args.execute_browser_diagnostic:
        return execute_browser_diagnostic(args.run_port)
    if args.materialize_browser:
        result = ensure_extracted_browser()
        print(json.dumps({"status": "execution-package-ready-no-browser", "browserLaunches": 0, "extraction": result}, sort_keys=True))
        return 0
    result = verify_readiness(hash_archive=True)
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (DiagnosticError, fp2.FP2Failure) as exc:
        print(json.dumps({"status": "blocked", "code": exc.code, "detail": exc.detail}), file=sys.stderr)
        raise SystemExit(1)
