#!/usr/bin/env python3
"""Run the immutable clean M3-WI Attempt 4 qualification."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
BRANCH = "codex/camoufox-m3-engine-adapter"
ATTEMPT_ROOT = REPO_ROOT / "artifacts/camoufox-m3-wi-clean-attempt-4"
CONTRACT = REPO_ROOT / "docs/camoufox-m3-wi-clean-contract.md"
CONTRACT_SHA256 = "acdc725dbbb1ccb0c39571cea43f6eb7ef3137429f4f8b256ec764f3be20af74"
ARTIFACT = REPO_ROOT / (
    "artifacts/camoufox-fp3-1b-attempt-7/identity-fp3-1b-formal-v3-a.json"
)
ARTIFACT_SHA256 = "8a4cd0d10a0a456678d1f3b4beb1515195d5d171742c4695c2d909132a26e722"
ARTIFACT_SIDECAR = ARTIFACT.with_suffix(".json.sha256")
ARTIFACT_SIDECAR_SHA256 = (
    "e027eb101fa2783adbc697fa8b47a339e7d66bf00170eacdca7b71a8983f8b86"
)
ARTIFACT_ID = "identity-fp3-1b-formal-v3-a"
ASSET_LOCK = REPO_ROOT / (
    "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-runtime-asset-lock.json"
)
ASSET_LOCK_SHA256 = "81e73a69347272d0b770bfa3c9b3eb07449bb165efb0c16948eece2e5a0678ce"
TREE_MANIFEST = REPO_ROOT / (
    "artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-browser-tree-manifest.json"
)
TREE_MANIFEST_SHA256 = "8434ab9925bf0f7d95cc4ff06fe94b7dcf9963a0691f37638469d68cda58ace2"
TREE_MANIFEST_CANONICAL_SHA256 = (
    "68d78d0f414d90545691560858b46ed179ee163b7258306c44f0d850bcde6204"
)
BROWSER_ROOT = REPO_ROOT / "artifacts/camoufox-fp2-formal-r1-attempt-8/browser"
EXECUTABLE = BROWSER_ROOT / "camoufox.exe"
EXECUTABLE_SHA256 = "b147602826db5bf852e5777f56cd56036dc04e8ea8868a8e55f8b08744f142a6"
ARCHIVE_SHA256 = "032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5"
HOST_SOURCE = HOST_DIR / "host_v1.py"
HOST_SOURCE_SHA256 = "b3b313d4cf6d2eaadceaff4320e5a6bb8afb5d39212652b2c51474eb6809aad0"
HOST_ENTRYPOINT = HOST_DIR / "run_m3_wi_clean_host.py"
HOST_ENTRYPOINT_SHA256 = "2015e91bf0902cc6b7276aadb6e8589ca728eb0dc11791a457d0b9744bae5ee8"
FORMAL_INPUT_BINDER = HOST_DIR / "run_fp3_1b_windows.py"
FORMAL_INPUT_BINDER_SHA256 = "73a4fe9b20a95588d8bd03335aeffddf2a93b53cd0ddf9c24301ea99d6437785"
UV_LOCK = HOST_DIR / "uv.lock"
UV_LOCK_SHA256 = "41f63b2c12c3102573266b4d9ac002fbd29f7f95cc3d291b8a41d09e411f8f6f"
PYTHON = HOST_DIR / ".venv/Scripts/python.exe"
PROXY_HOST = "127.0.0.1"
PROXY_PORT = 7897
TEST_NAME = "m3_wi_clean_native_windows_two_cycle_qualification"
FOCUSED_TEST_NAME = "camoufox_host_running_applies_only_profile_binding"
MAX_FAILURE_STDERR_BYTES = 64 * 1024


class Blocked(RuntimeError):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace(
        "+00:00", "Z"
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_file(path: Path, expected_sha256: str) -> None:
    if not path.is_file() or path.is_symlink():
        raise Blocked(f"missing or irregular frozen file: {path}")
    if sha256_file(path) != expected_sha256:
        raise Blocked(f"SHA-256 mismatch: {path}")


def git(*arguments: str) -> str:
    return subprocess.run(
        ["git", *arguments],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    ).stdout.strip()


def relative(path: Path) -> str:
    return path.relative_to(REPO_ROOT).as_posix()


def write_new(path: Path, raw: bytes) -> None:
    with path.open("xb") as stream:
        stream.write(raw)


def write_sidecar(path: Path, digest: str) -> None:
    write_new(
        path.with_suffix(path.suffix + ".sha256"),
        f"{digest}  {path.name}\n".encode("ascii"),
    )


def write_json(path: Path, value: object) -> str:
    raw = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    digest = hashlib.sha256(raw).hexdigest()
    write_new(path, raw)
    write_sidecar(path, digest)
    return digest


def static_preflight() -> tuple[str, str, str, str, str]:
    if os.name != "nt":
        raise Blocked("clean M3-WI requires native Windows")
    if Path(git("rev-parse", "--show-toplevel")).resolve() != REPO_ROOT.resolve():
        raise Blocked("Git root differs from repository root")
    if git("branch", "--show-current") != BRANCH:
        raise Blocked("wrong Git branch")
    if git("status", "--porcelain=v1", "--untracked-files=all"):
        raise Blocked("worktree is not clean")
    revision = git("rev-parse", "HEAD")
    origin_revision = git("rev-parse", f"origin/{BRANCH}")
    if revision != origin_revision:
        raise Blocked("implementation commit is not synchronized with origin")
    tree = git("rev-parse", "HEAD^{tree}")
    for path, digest in (
        (CONTRACT, CONTRACT_SHA256),
        (ARTIFACT, ARTIFACT_SHA256),
        (ARTIFACT_SIDECAR, ARTIFACT_SIDECAR_SHA256),
        (ASSET_LOCK, ASSET_LOCK_SHA256),
        (TREE_MANIFEST, TREE_MANIFEST_SHA256),
        (EXECUTABLE, EXECUTABLE_SHA256),
        (HOST_ENTRYPOINT, HOST_ENTRYPOINT_SHA256),
        (FORMAL_INPUT_BINDER, FORMAL_INPUT_BINDER_SHA256),
        (HOST_SOURCE, HOST_SOURCE_SHA256),
        (UV_LOCK, UV_LOCK_SHA256),
    ):
        require_file(path, digest)
    if ARTIFACT_SIDECAR.read_text(encoding="ascii") != (
        f"{ARTIFACT_SHA256}  {ARTIFACT.name}\n"
    ):
        raise Blocked("Artifact SHA-256 sidecar content mismatch")
    if not PYTHON.is_file() or PYTHON.is_symlink():
        raise Blocked(f"locked venv Python is unavailable: {PYTHON}")
    python_result = subprocess.run(
        [str(PYTHON), "--version"],
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        check=False,
    )
    if python_result.returncode != 0:
        raise Blocked("locked venv Python version probe failed")
    python_version = (python_result.stdout or python_result.stderr).strip()
    cargo = shutil.which("cargo")
    powershell = shutil.which("powershell.exe") or shutil.which("pwsh.exe")
    if cargo is None or powershell is None:
        raise Blocked("cargo or PowerShell is unavailable")
    if ATTEMPT_ROOT.exists():
        raise Blocked(f"immutable attempt already exists: {ATTEMPT_ROOT}")
    return revision, tree, cargo, powershell, python_version


def focused_semantics_preflight(cargo: str) -> list[str]:
    command = [
        cargo,
        "test",
        "--locked",
        "--manifest-path",
        "apps/desktop/src-tauri/Cargo.toml",
        FOCUSED_TEST_NAME,
        "--",
        "--nocapture",
    ]
    result = subprocess.run(
        command,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise Blocked("focused evidence-semantics test failed")
    return command


def proxy_preflight(powershell: str) -> list[str]:
    command = [
        powershell,
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "$ok = Test-NetConnection 127.0.0.1 -Port 7897 -InformationLevel Quiet; "
        "if ($ok) { exit 0 } else { exit 1 }",
    ]
    result = subprocess.run(
        command,
        cwd=REPO_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise Blocked("Test-NetConnection 127.0.0.1 -Port 7897 failed")
    return command


def execute() -> str:
    revision, tree, cargo, powershell, host_python_version = static_preflight()
    focused_command = focused_semantics_preflight(cargo)
    preflight_command = proxy_preflight(powershell)
    ATTEMPT_ROOT.mkdir(parents=False, exist_ok=False)
    native_path = ATTEMPT_ROOT / "native-evidence.json"
    report_path = ATTEMPT_ROOT / "run-report.json"
    command = [
        cargo,
        "test",
        "--locked",
        "--manifest-path",
        "apps/desktop/src-tauri/Cargo.toml",
        TEST_NAME,
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    environment = os.environ.copy()
    environment.update(
        {
            "PYTHONDONTWRITEBYTECODE": "1",
            "VERISILO_M3_WI_PYTHON_PATH": str(PYTHON),
            "VERISILO_M3_WI_ARTIFACT_PATH": str(ARTIFACT),
            "VERISILO_M3_WI_CLEAN_ALLOW_REAL_BROWSER": "1",
            "VERISILO_M3_WI_CLEAN_NATIVE_EVIDENCE_PATH": str(native_path),
            "VERISILO_M3_WI_CLEAN_ARTIFACT_PATH": str(ARTIFACT),
            "VERISILO_M3_WI_CLEAN_ARTIFACT_ID": ARTIFACT_ID,
            "VERISILO_M3_WI_CLEAN_ARTIFACT_SHA256": ARTIFACT_SHA256,
            "VERISILO_M3_WI_CLEAN_PROXY_HOST": PROXY_HOST,
            "VERISILO_M3_WI_CLEAN_PROXY_PORT": str(PROXY_PORT),
            "VERISILO_M3_WI_CLEAN_HOST_SCRIPT": str(HOST_ENTRYPOINT),
            "VERISILO_M3_WI_CLEAN_ASSET_LOCK": str(ASSET_LOCK),
            "VERISILO_M3_WI_CLEAN_BROWSER_ROOT": str(BROWSER_ROOT),
            "VERISILO_M3_WI_CLEAN_TREE_MANIFEST": str(TREE_MANIFEST),
            "VERISILO_M3_WI_CLEAN_TREE_SHA256": TREE_MANIFEST_SHA256,
            "VERISILO_M3_WI_CLEAN_EXPECTED_ASSET_SHA256": ARCHIVE_SHA256,
            "VERISILO_M3_WI_CLEAN_BRANCH": BRANCH,
            "VERISILO_M3_WI_CLEAN_CODE_REVISION": revision,
            "VERISILO_M3_WI_CLEAN_CODE_TREE": tree,
            "VERISILO_M3_WI_CLEAN_CONTRACT_SHA256": CONTRACT_SHA256,
        }
    )
    started_at = utc_now()
    result = subprocess.run(
        command,
        cwd=REPO_ROOT,
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    completed_at = utc_now()
    stderr_path = ATTEMPT_ROOT / "cargo-stderr.txt"
    if result.returncode != 0:
        stderr = result.stderr[-MAX_FAILURE_STDERR_BYTES:]
        if not stderr:
            stderr = b"cargo stderr was empty\n" + result.stdout[-MAX_FAILURE_STDERR_BYTES:]
        write_new(stderr_path, stderr)

    native_evidence: dict[str, object] | None = None
    native_error: str | None = None
    native_sha256: str | None = None
    if native_path.is_file() and not native_path.is_symlink():
        native_sha256 = sha256_file(native_path)
        write_sidecar(native_path, native_sha256)
        try:
            parsed = json.loads(native_path.read_text(encoding="utf-8"))
            if type(parsed) is dict:
                native_evidence = parsed
            else:
                native_error = "native evidence root is not an object"
        except (OSError, UnicodeError, json.JSONDecodeError) as exc:
            native_error = type(exc).__name__
    else:
        native_error = "native evidence was not produced"

    native_status = native_evidence.get("status") if native_evidence else None
    native_verified = native_evidence.get("verified") if native_evidence else None
    checks = {
        "cargoExitZero": result.returncode == 0,
        "nativeEvidenceProduced": native_sha256 is not None,
        "nativeStatusPassed": native_status == "passed",
        "verifiedRemainsFalse": native_verified is False,
    }
    if all(checks.values()):
        status = "passed"
    elif native_status == "failed":
        status = "failed"
    else:
        status = "inconclusive"
    report = {
        "schema": "verisilo-camoufox-m3-wi-clean-run-report/v1",
        "attempt": 4,
        "runId": "clean-m3-wi-attempt-4",
        "startedAtUtc": started_at,
        "completedAtUtc": completed_at,
        "status": status,
        "evidenceClass": (
            "observed-on-this-native-windows-host"
            if status == "passed"
            else "attempted-on-this-native-windows-host"
        ),
        "verified": False,
        "host": {
            "platform": platform.platform(),
            "runnerPython": platform.python_version(),
        },
        "code": {
            "branch": BRANCH,
            "revision": revision,
            "tree": tree,
            "originRevision": revision,
            "runnerSha256": sha256_file(Path(__file__).resolve()),
            "contract": {"path": relative(CONTRACT), "sha256": CONTRACT_SHA256},
        },
        "inputs": {
            "artifact": {
                "id": ARTIFACT_ID,
                "path": relative(ARTIFACT),
                "sha256": ARTIFACT_SHA256,
                "sidecarPath": relative(ARTIFACT_SIDECAR),
                "sidecarSha256": ARTIFACT_SIDECAR_SHA256,
            },
            "engineRevision": "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3",
            "runtimeAssetLock": {
                "path": relative(ASSET_LOCK),
                "sha256": ASSET_LOCK_SHA256,
            },
            "runtimeTree": {
                "path": relative(TREE_MANIFEST),
                "rawSha256": TREE_MANIFEST_SHA256,
                "canonicalSha256": TREE_MANIFEST_CANONICAL_SHA256,
            },
            "executable": {"path": relative(EXECUTABLE), "sha256": EXECUTABLE_SHA256},
            "hostEntrypoint": {
                "path": relative(HOST_ENTRYPOINT),
                "sha256": HOST_ENTRYPOINT_SHA256,
            },
            "formalInputBinder": {
                "path": relative(FORMAL_INPUT_BINDER),
                "sha256": FORMAL_INPUT_BINDER_SHA256,
            },
            "hostSource": {"path": relative(HOST_SOURCE), "sha256": HOST_SOURCE_SHA256},
            "pythonLock": {"path": relative(UV_LOCK), "sha256": UV_LOCK_SHA256},
            "pythonRuntime": {
                "path": str(PYTHON),
                "version": host_python_version,
            },
            "requiredProxy": "socks5://127.0.0.1:7897",
            "proxyPreflight": {
                "command": preflight_command,
                "tcpTestSucceeded": True,
            },
            "focusedSemanticsPreflight": {
                "command": focused_command,
                "exitCode": 0,
            },
        },
        "execution": {"command": command, "cargoExitCode": result.returncode},
        "nativeEvidence": (
            {
                "path": relative(native_path),
                "sha256": native_sha256,
                "status": native_status,
                "verified": native_verified,
            }
            if native_sha256 is not None
            else None
        ),
        "nativeEvidenceReadError": native_error,
        "adjudication": {"checks": checks, "precedence": "failed>inconclusive>passed"},
        "evidenceBoundaries": {
            "packageVerification": (
                "not_requested" if status == "passed" else "unavailable"
            ),
            "hostLaunch": "observed" if status == "passed" else "unavailable",
            "verifiedAdapter": None if status == "passed" else "unavailable",
            "externalNetworkObservation": "not_requested",
            "productionPackageVerified": False,
            "shipped": False,
            "verified": False,
        },
        "limitations": [
            "This result is bounded to this native Windows host and the exact frozen inputs.",
            "The test-only adapter does not provide production package or signer verification.",
            "Exit IP, Geo, Geolocation, DNS, WebRTC, TLS, QUIC and ordinary-site checks were not requested.",
            "This does not claim shipping, release, cross-host replay or verified:true.",
        ],
        "failureDiagnostics": (
            {
                "path": relative(stderr_path),
                "sha256": sha256_file(stderr_path),
                "sizeBytes": stderr_path.stat().st_size,
            }
            if stderr_path.is_file()
            else None
        ),
    }
    write_json(report_path, report)
    print(f"{status}: {relative(report_path)}")
    return status


def main() -> int:
    status = execute()
    return {"passed": 0, "failed": 1, "inconclusive": 2}[status]


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (Blocked, subprocess.CalledProcessError) as exc:
        raise SystemExit(f"clean M3-WI blocked before attempt: {exc}") from exc
