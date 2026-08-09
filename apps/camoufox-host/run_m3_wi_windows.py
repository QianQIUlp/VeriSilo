#!/usr/bin/env python3
"""Run the frozen M3-WI native-Windows real Host/desktop Gate.

This runner is intentionally offline and test-only.  It verifies the exact
clean Git revision and pinned inputs, runs the ignored Rust test that owns the
real RuntimeManager -> Host -> Camoufox path, then runs the required regression
matrix and writes gitignored report/sidecar receipts.  It never installs,
downloads, signs, or publishes anything.
"""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import importlib.metadata
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time
import uuid
from ctypes import wintypes
from pathlib import Path
from typing import Any

from browser_tree import load_tree_manifest, verify_tree


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parents[1]
START_CHECKPOINT = "aefa294e85e6fd05a0c8749ab1afacf6ab06becb"
EXPECTED_BRANCH = "codex/camoufox-m3-engine-adapter"
EXPECTED_ARCHIVE_SHA256 = (
    "386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d"
)
EXPECTED_ARCHIVE_SIZE = 492_370_020
EXPECTED_ARTIFACT_SHA256 = (
    "a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f"
)
EXPECTED_TREE_CANONICAL_SHA256 = (
    "1c749534d139b7efcb425faf03de9cfe1d59004034a1fe1c5ba423b86239c37b"
)
EXPECTED_RELEASE = "v152.0.4-beta.28"
RUNS_ROOT = REPO_ROOT / "artifacts" / "camoufox-m3-wi-windows-gate" / "runs"
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"
ARCHIVE = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-m0"
    / "camoufox-152.0.4-beta.28-win.x86_64.zip"
)
BROWSER_ROOT = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-m0"
    / "browser"
    / "camoufox-152.0.4-beta.28-win-x86_64"
)
TREE_MANIFEST = FIXTURES / "browser-tree-manifest-windows.json"
ARTIFACT = FIXTURES / "identity-win-a.json"
ARTIFACT_SIDECAR = FIXTURES / "identity-win-a.json.sha256"
ASSET_LOCK = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-windows-x86_64.json"
UV_LOCK = HOST_DIR / "uv.lock"
HOST_SOURCE = HOST_DIR / "host_v1.py"
PROTECTED_PATHS = [
    "apps/camoufox-host/identity_policy.py",
    "apps/camoufox-host/host_v1.py",
    "tests/fixtures/camoufox/browser-tree-manifest.json",
    "tests/fixtures/camoufox/browser-tree-manifest-windows.json",
    "tests/fixtures/camoufox/evidence-manifest.json",
    "tests/fixtures/camoufox/evidence-manifest-windows.json",
    "tests/fixtures/camoufox/identity-a.json",
    "tests/fixtures/camoufox/identity-a.json.sha256",
    "tests/fixtures/camoufox/identity-b.json",
    "tests/fixtures/camoufox/identity-b.json.sha256",
    "tests/fixtures/camoufox/identity-c.json",
    "tests/fixtures/camoufox/identity-c.json.sha256",
    "tests/fixtures/camoufox/identity-win-a.json",
    "tests/fixtures/camoufox/identity-win-a.json.sha256",
    "tests/fixtures/camoufox/identity-win-b.json",
    "tests/fixtures/camoufox/identity-win-b.json.sha256",
    "tests/fixtures/camoufox/identity-win-c.json",
    "tests/fixtures/camoufox/identity-win-c.json.sha256",
]


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def run(
    command: list[str],
    *,
    cwd: Path = REPO_ROOT,
    env: dict[str, str] | None = None,
    timeout: float = 1800,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    print("$ " + subprocess.list2cmdline(command), flush=True)
    completed = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    if completed.stdout:
        print(completed.stdout, end="" if completed.stdout.endswith("\n") else "\n")
    if check and completed.returncode != 0:
        raise RuntimeError(
            f"command failed with exit code {completed.returncode}: "
            + subprocess.list2cmdline(command)
        )
    return completed


def git(*args: str, check: bool = True) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        text=True,
        encoding="utf-8",
        errors="strict",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if check and completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or f"git {' '.join(args)} failed")
    return completed.stdout.strip()


def windows_session() -> tuple[int, str, int]:
    session = wintypes.DWORD()
    if not ctypes.windll.kernel32.ProcessIdToSessionId(
        os.getpid(), ctypes.byref(session)
    ):
        raise OSError(ctypes.get_last_error(), "ProcessIdToSessionId failed")
    query = ctypes.windll.wtsapi32.WTSQuerySessionInformationW
    query.argtypes = [
        wintypes.HANDLE,
        wintypes.DWORD,
        ctypes.c_int,
        ctypes.POINTER(ctypes.c_void_p),
        ctypes.POINTER(wintypes.DWORD),
    ]
    query.restype = wintypes.BOOL
    free = ctypes.windll.wtsapi32.WTSFreeMemory
    free.argtypes = [ctypes.c_void_p]
    free.restype = None

    def query_value(info_class: int, *, string: bool) -> str | int:
        buffer = ctypes.c_void_p()
        returned = wintypes.DWORD()
        if not query(None, session.value, info_class, ctypes.byref(buffer), ctypes.byref(returned)):
            raise OSError(ctypes.get_last_error(), "WTSQuerySessionInformationW failed")
        try:
            if string:
                return ctypes.wstring_at(buffer.value)
            if returned.value < ctypes.sizeof(wintypes.DWORD):
                raise RuntimeError("WTS connect-state response was truncated")
            return ctypes.cast(buffer, ctypes.POINTER(wintypes.DWORD)).contents.value
        finally:
            free(buffer)

    # WTSWinStationName=6; WTSConnectState=8; WTSActive=0.
    return (
        session.value,
        str(query_value(6, string=True)),
        int(query_value(8, string=False)),
    )


def require_clean_checkpoint() -> tuple[str, str, str, str]:
    if os.name != "nt" or platform.system() != "Windows":
        raise RuntimeError("M3-WI requires native Windows")
    if Path.cwd().resolve() != REPO_ROOT.resolve():
        raise RuntimeError(f"cwd must be the original checkout {REPO_ROOT}")
    if git("rev-parse", "--git-dir") != ".git":
        raise RuntimeError("M3-WI refuses a linked/Codex worktree")
    branch = git("branch", "--show-current")
    if branch != EXPECTED_BRANCH:
        raise RuntimeError(f"unexpected branch {branch!r}")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before M3-WI")
    revision = git("rev-parse", "HEAD")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", START_CHECKPOINT, revision],
        cwd=REPO_ROOT,
        check=False,
    )
    if ancestor.returncode != 0:
        raise RuntimeError("M3-WI code revision is not a descendant of the frozen checkpoint")
    tree = git("show", "-s", "--format=%T", revision)
    current_session_id, session_name, connect_state = windows_session()
    if current_session_id == 0 or connect_state != 0 or session_name.lower() == "services":
        raise RuntimeError("M3-WI requires an interactive console/RDP session")
    return revision, tree, branch, session_name


def session_id() -> int:
    return windows_session()[0]


def target_processes() -> list[dict[str, Any]]:
    script = (
        "$p=Get-Process -ErrorAction SilentlyContinue | "
        "Where-Object {$_.ProcessName -match '^(camoufox|firefox|verisilo-camoufox-supervisor)$'} | "
        "Select-Object Id,ProcessName; if($p){$p | ConvertTo-Json -Compress}"
    )
    completed = subprocess.run(
        ["powershell.exe", "-NoProfile", "-NonInteractive", "-Command", script],
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"process preflight failed: {completed.stderr.strip()}")
    raw = completed.stdout.strip()
    if not raw:
        return []
    value = json.loads(raw)
    return value if isinstance(value, list) else [value]


def version(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"version command failed: {subprocess.list2cmdline(command)}")
    return completed.stdout.strip().splitlines()[0]


def fixed_input_preflight() -> dict[str, Any]:
    for path in [
        ARCHIVE,
        BROWSER_ROOT / "camoufox.exe",
        TREE_MANIFEST,
        ARTIFACT,
        ARTIFACT_SIDECAR,
        ASSET_LOCK,
        UV_LOCK,
        HOST_SOURCE,
    ]:
        if not path.exists():
            raise RuntimeError(f"fixed input is missing: {path}")
    archive_sha = sha256_file(ARCHIVE)
    if archive_sha != EXPECTED_ARCHIVE_SHA256 or ARCHIVE.stat().st_size != EXPECTED_ARCHIVE_SIZE:
        raise RuntimeError("pinned Windows archive size/SHA mismatch")
    artifact_sha = sha256_file(ARTIFACT)
    sidecar_sha = ARTIFACT_SIDECAR.read_text(encoding="utf-8").split()[0]
    if artifact_sha != EXPECTED_ARTIFACT_SHA256 or sidecar_sha != artifact_sha:
        raise RuntimeError("Windows Artifact raw bytes/sidecar mismatch")
    tree_raw_sha = sha256_file(TREE_MANIFEST)
    tree_result = verify_tree(BROWSER_ROOT, load_tree_manifest(TREE_MANIFEST))
    if tree_result["manifestSha256"] != EXPECTED_TREE_CANONICAL_SHA256:
        raise RuntimeError("Windows browser tree canonical digest mismatch")
    lock = json.loads(ASSET_LOCK.read_text(encoding="utf-8"))
    if (
        lock.get("release") != EXPECTED_RELEASE
        or lock.get("sha256") != EXPECTED_ARCHIVE_SHA256
        or lock.get("sizeBytes") != EXPECTED_ARCHIVE_SIZE
        or lock.get("digestAgreement") is not True
    ):
        raise RuntimeError("Windows asset lock does not match the frozen input")
    packages = {
        name: importlib.metadata.version(name)
        for name in ["camoufox", "playwright", "browserforge"]
    }
    if packages != {
        "camoufox": "0.5.4",
        "playwright": "1.60.0",
        "browserforge": "1.2.4",
    }:
        raise RuntimeError(f"locked Python package versions changed: {packages}")
    return {
        "archive": {
            "relativePath": ARCHIVE.relative_to(REPO_ROOT).as_posix(),
            "sha256": archive_sha,
            "sizeBytes": ARCHIVE.stat().st_size,
        },
        "assetLock": {
            "relativePath": ASSET_LOCK.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_file(ASSET_LOCK),
        },
        "browserTree": {
            "relativePath": TREE_MANIFEST.relative_to(REPO_ROOT).as_posix(),
            "rawFileSha256": tree_raw_sha,
            "canonicalManifestSha256": tree_result["manifestSha256"],
            "fileCount": tree_result["fileCount"],
            "totalBytes": tree_result["totalBytes"],
        },
        "artifact": {
            "relativePath": ARTIFACT.relative_to(REPO_ROOT).as_posix(),
            "rawFileSha256": artifact_sha,
            "sidecarMatches": True,
        },
        "hostSource": {
            "relativePath": HOST_SOURCE.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_file(HOST_SOURCE),
        },
        "uvLock": {
            "relativePath": UV_LOCK.relative_to(REPO_ROOT).as_posix(),
            "sha256": sha256_file(UV_LOCK),
        },
        "packages": packages,
        "release": EXPECTED_RELEASE,
    }


def protected_preflight(revision: str) -> dict[str, str]:
    changed = subprocess.run(
        ["git", "diff", "--quiet", f"{START_CHECKPOINT}..{revision}", "--", *PROTECTED_PATHS],
        cwd=REPO_ROOT,
        check=False,
    )
    if changed.returncode != 0:
        raise RuntimeError("M3-WI changed a protected Artifact/Host/M0-M2-W evidence input")
    return {
        path: sha256_file(REPO_ROOT / path)
        for path in PROTECTED_PATHS
        if (REPO_ROOT / path).is_file()
    }


def command_receipt(label: str, completed: subprocess.CompletedProcess[str]) -> dict[str, Any]:
    output = completed.stdout or ""
    plain_output = re.sub(r"\x1b\[[0-9;]*m", "", output)
    counts: dict[str, int] = {}
    cargo = re.findall(
        r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored", plain_output
    )
    if cargo:
        counts["passed"] = sum(int(item[0]) for item in cargo)
        counts["failed"] = sum(int(item[1]) for item in cargo)
        counts["ignored"] = sum(int(item[2]) for item in cargo)
    vitest = re.findall(r"Tests\s+(\d+) passed", plain_output)
    if vitest:
        counts["jsTests"] = sum(map(int, vitest))
    python_tests = re.findall(r"Ran (\d+) tests?", plain_output)
    python_tests.extend(re.findall(r"all (\d+) tests passed", plain_output))
    if python_tests:
        counts["pythonTests"] = max(map(int, python_tests))
    return {
        "label": label,
        "exitCode": completed.returncode,
        "outputSha256": hashlib.sha256(output.encode("utf-8")).hexdigest(),
        "counts": counts,
    }


def parse_engine_verify(output: str) -> dict[str, Any]:
    start = output.find("{")
    if start < 0:
        raise RuntimeError("pnpm engine:verify did not emit JSON")
    value, _ = json.JSONDecoder().raw_decode(output[start:])
    if (
        value.get("ok") is not True
        or value.get("trustedSignerCount") != 0
        or value.get("controlledEngineArtifactsBundled") is not False
        or "No release signer certificate pin" not in value.get("externalBlocker", "")
    ):
        raise RuntimeError(f"production verifier no longer has the frozen fail-closed boundary: {value}")
    return value


def run_gate(args: argparse.Namespace) -> int:
    revision, tree, branch, session_name = require_clean_checkpoint()
    if target_processes():
        raise RuntimeError("Camoufox/Firefox/supervisor process exists before M3-WI")
    fixed_inputs = fixed_input_preflight()
    protected_hashes = protected_preflight(revision)
    uv_path = shutil.which("uv")
    if not uv_path:
        raise RuntimeError("uv executable is unavailable")
    locked_python = run(
        [
            "uv", "run", "--frozen", "--offline", "--project",
            str(HOST_DIR), "python", "-c", "import sys; print(sys.executable)",
        ],
        timeout=120,
    ).stdout.strip().splitlines()[-1]
    locked_python_path = Path(locked_python).resolve()
    if not locked_python_path.is_file():
        raise RuntimeError("uv did not resolve a concrete locked Python interpreter")
    fixed_inputs["pythonInterpreter"] = {
        "relativePath": (
            locked_python_path.relative_to(REPO_ROOT).as_posix()
            if locked_python_path.is_relative_to(REPO_ROOT)
            else "external-locked-python"
        ),
        "sha256": sha256_file(locked_python_path),
        "version": version([str(locked_python_path), "--version"]),
        "resolvedBy": "uv run --frozen --offline",
    }

    run_id = args.run_id or f"run-{int(time.time())}-{uuid.uuid4().hex[:8]}"
    if not re.fullmatch(r"run-[0-9]+-[0-9a-f]{8}", run_id):
        raise RuntimeError("run-id must match run-<epoch>-<8 hex>")
    run_dir = RUNS_ROOT / run_id
    if run_dir.exists():
        raise RuntimeError(f"run-id already exists: {run_id}")

    command_results: list[dict[str, Any]] = []
    engine_verify = run(["pnpm", "engine:verify"], timeout=120)
    engine_verify_json = parse_engine_verify(engine_verify.stdout)
    command_results.append(command_receipt("pnpm engine:verify", engine_verify))

    for label, command in [
        (
            "M3-0 Host framing/failure regression",
            [
                "cargo", "test", "--locked", "--manifest-path",
                "apps/desktop/src-tauri/Cargo.toml", "fake_camoufox_host_", "--",
                "--test-threads=1",
            ],
        ),
        (
            "Camoufox Direct-only regression",
            [
                "cargo", "test", "--locked", "--manifest-path",
                "apps/desktop/src-tauri/Cargo.toml",
                "camoufox_runtime_manager_rejects_non_direct_network_before_spawn",
                "--", "--test-threads=1",
            ],
        ),
        (
            "typed fixed probe-port regression",
            [
                "cargo", "test", "--locked", "--manifest-path",
                "apps/desktop/src-tauri/Cargo.toml",
                "camoufox_host_hello_binds_typed_fixed_probe_port",
                "--", "--test-threads=1",
            ],
        ),
    ]:
        completed = run(command, timeout=300)
        command_results.append(command_receipt(label, completed))

    real_command = [
        "cargo",
        "test",
        "--locked",
        "--manifest-path",
        "apps/desktop/src-tauri/Cargo.toml",
        "m3_wi_windows_real_host_runtime_manager_gate",
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    real_env = os.environ.copy()
    real_env.update(
        {
            "VERISILO_M3_WI_ALLOW_REAL_BROWSER": "1",
            "VERISILO_M3_WI_RUN_ID": run_id,
            "VERISILO_M3_WI_CODE_REVISION": revision,
            "VERISILO_M3_WI_CODE_TREE": tree,
            "VERISILO_M3_WI_BRANCH": branch,
            "VERISILO_M3_WI_PYTHON_PATH": str(locked_python_path),
            "VERISILO_M3_WI_TREE_RAW_SHA256": fixed_inputs["browserTree"]["rawFileSha256"],
            "VERISILO_M3_WI_ARTIFACT_PATH": str(ARTIFACT.resolve()),
        }
    )
    real = run(real_command, env=real_env, timeout=1800)
    command_results.append(command_receipt("M3-WI real RuntimeManager/Host/browser", real))
    runtime_path = run_dir / "runtime-evidence.json"
    if not runtime_path.is_file():
        raise RuntimeError("real Rust Gate did not write runtime-evidence.json")
    runtime_evidence = json.loads(runtime_path.read_text(encoding="utf-8"))
    semantic_boundary = runtime_evidence.get("semanticBoundary", {})
    launch_surfaces = [
        runtime_evidence.get("persistence", {}).get("cycle1", {}).get("launchSurface", {}),
        runtime_evidence.get("persistence", {}).get("cycle2", {}).get("launchSurface", {}),
        runtime_evidence.get("activeEof", {}).get("running", {}).get("launchSurface", {}),
        runtime_evidence.get("hostCrash", {}).get("running", {}).get("launchSurface", {}),
        runtime_evidence.get("desktopDrop", {}).get("running", {}).get("launchSurface", {}),
    ]
    if (
        runtime_evidence.get("status") != "passed"
        or runtime_evidence.get("codeGitRevision") != revision
        or runtime_evidence.get("codeTreeHash") != tree
        or runtime_evidence.get("productionPackageVerified") is not False
        or runtime_evidence.get("verified") is not False
        or runtime_evidence.get("secretScan", {}).get("matches") != []
        or runtime_evidence.get("residualProcessCheck", {}).get("aliveOwnedPids") != []
        or semantic_boundary.get("launchExecutable")
        != "uv-resolved-locked-python-interpreter"
        or semantic_boundary.get("hostEntrypoint") != "apps/camoufox-host/host_v1.py"
        or semantic_boundary.get("typedHostArgvRecorded") is not True
        or semantic_boundary.get("argvContainsProxyArguments") is not False
        or semantic_boundary.get("argvContainsSecrets") is not False
        or any(
            surface.get("integrationPath") != "test-only-real-host"
            or surface.get("adapterVersion") != "m3-wi-test-only-real-host"
            or surface.get("transport") != "camoufox-host-jsonl-v1"
            or surface.get("packageVerification") is not None
            or surface.get("shell") is not False
            for surface in launch_surfaces
        )
    ):
        raise RuntimeError("real Rust runtime evidence failed its binding/secret/residual checks")

    # Full required regression matrix runs after the real receipt.  Any code
    # change needed to repair it invalidates this run and must be rerun.
    validation_commands = [
        ("pnpm check", ["pnpm", "check"], REPO_ROOT, 600),
        ("pnpm test", ["pnpm", "test"], REPO_ROOT, 600),
        ("pnpm build", ["pnpm", "build"], REPO_ROOT, 600),
        (
            "cargo fmt --check",
            [
                "cargo", "fmt", "--manifest-path", "apps/desktop/src-tauri/Cargo.toml",
                "--", "--check",
            ],
            REPO_ROOT,
            300,
        ),
        (
            "cargo test --locked",
            [
                "cargo", "test", "--locked", "--manifest-path",
                "apps/desktop/src-tauri/Cargo.toml",
            ],
            REPO_ROOT,
            1200,
        ),
        (
            "cargo clippy --locked -D warnings",
            [
                "cargo", "clippy", "--locked", "--manifest-path",
                "apps/desktop/src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings",
            ],
            REPO_ROOT,
            1200,
        ),
        (
            "Artifact 25/25",
            ["uv", "run", "--frozen", "--offline", "python", "test_identity_artifact.py"],
            HOST_DIR,
            300,
        ),
    ]
    for label, command, cwd, timeout in validation_commands:
        completed = run(command, cwd=cwd, timeout=timeout)
        command_results.append(command_receipt(label, completed))

    if git("rev-parse", "HEAD") != revision or git("status", "--porcelain=v1"):
        raise RuntimeError("tracked code/worktree changed during M3-WI execution")
    residual = target_processes()
    if residual:
        raise RuntimeError(f"target process remains after M3-WI: {residual}")

    environment = {
        "os": platform.platform(),
        "windowsRelease": platform.win32_ver()[0],
        "windowsVersion": platform.win32_ver()[1],
        "architecture": platform.machine(),
        "sessionName": session_name,
        "sessionId": session_id(),
        "python": sys.version.split()[0],
        "uv": version(["uv", "--version"]),
        "rustc": version(["rustc", "--version"]),
        "cargo": version(["cargo", "--version"]),
        "node": version(["node", "--version"]),
        "pnpm": version(["pnpm", "--version"]),
    }
    gates = [
        {"id": number, "status": "passed", "evidence": evidence}
        for number, evidence in [
            (1, "runner baseline + interactive native-Windows session"),
            (2, "archive/tree/Artifact/lock offline fixed-input preflight"),
            (3, "cfg(test) adapter + production verifier signer-pin blocker"),
            (4, "real RuntimeManager/adapter/spawn/host_v1.py/Camoufox path"),
            (5, "real hello/launch/status exact binding"),
            (6, "ObservedWebsiteDigest/media readiness, verified=false"),
            (7, "two RuntimeManager/Host cycles, bootCount 1->2 and cookie persistence"),
            (8, "close/shutdown/exact Host exit/Job activeProcessCount=0"),
            (9, "active EOF, exact Host crash, desktop-control drop"),
            (10, "concurrent Profile rejection + unrelated sentinel survival"),
            (11, "real binding negatives + M3-0 malformed/oversize/timeout/early-exit regression"),
            (12, "Direct success + FixedProxy/PAC/Host proxy argv rejection regression"),
            (13, "six-class sentinel scan across argv/wire/evidence/log surfaces"),
            (14, "hostLaunch observed; generic receipts N/A; verifiedAdapter null"),
            (15, "full JS/Rust/Artifact regression matrix"),
            (16, "report, sidecars, code/tree binding, protected-file hashes"),
        ]
    ]
    report = {
        "schema": "verisilo-camoufox-m3-wi-windows-run-report/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "branch": branch,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": False,
        "productionVerifierFailClosed": True,
        "shipped": False,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
        "environment": environment,
        "fixedInputs": fixed_inputs,
        "productionVerifier": engine_verify_json,
        "runtimeEvidencePath": runtime_path.relative_to(REPO_ROOT).as_posix(),
        "runtimeEvidenceSha256": sha256_file(runtime_path),
        "runtimeEvidence": runtime_evidence,
        "commands": command_results,
        "gateMatrix": gates,
        "protectedAcceptedFiles": {
            "unchangedFromStartCheckpoint": True,
            "sha256": protected_hashes,
        },
        "downloadGuard": {
            "offlineUv": True,
            "hostWebdlGuardFailClosed": True,
            "webdlAttemptObserved": False,
        },
        "residualProcessCheck": {
            "targetProcessesBefore": [],
            "targetProcessesAfter": residual,
            "ownedPidsAlive": runtime_evidence["residualProcessCheck"]["aliveOwnedPids"],
        },
    }
    report_path = run_dir / "report.json"
    report_bytes = (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    report_path.write_bytes(report_bytes)
    report_sha = hashlib.sha256(report_bytes).hexdigest()
    report_sidecar = run_dir / "report.sha256"
    report_sidecar.write_text(f"{report_sha}  report.json\n", encoding="utf-8", newline="\n")
    summary = {
        "schema": "verisilo-camoufox-m3-wi-windows-summary/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "reportPath": report_path.relative_to(REPO_ROOT).as_posix(),
        "reportSha256": report_sha,
        "gateCount": 16,
        "gatesPassed": 16,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": False,
        "shipped": False,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }
    summary_path = run_dir / "summary.json"
    summary_bytes = (json.dumps(summary, indent=2) + "\n").encode("utf-8")
    summary_path.write_bytes(summary_bytes)
    summary_sha = hashlib.sha256(summary_bytes).hexdigest()
    summary_sidecar = run_dir / "summary.sha256"
    summary_sidecar.write_text(f"{summary_sha}  summary.json\n", encoding="utf-8", newline="\n")
    print(f"m3-wi-run-id={run_id}")
    print(f"m3-wi-report={report_path}")
    print(f"m3-wi-report-sha256={report_sha}")
    print(f"m3-wi-summary={summary_path}")
    print(f"m3-wi-summary-sha256={summary_sha}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", help="optional unique run-<epoch>-<8 hex> identifier")
    args = parser.parse_args()
    try:
        return run_gate(args)
    except Exception as exc:  # noqa: BLE001 - Gate runner reports one bounded failure
        print(f"M3-WI FAILED: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
