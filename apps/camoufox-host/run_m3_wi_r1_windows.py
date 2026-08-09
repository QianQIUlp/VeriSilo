#!/usr/bin/env python3
"""Run the bounded native-Windows M3-WI-R1 evidence pass.

R1 is intentionally narrower than the frozen M3-WI Gate: it runs one
five-cycle RuntimeManager -> Host -> fixed Camoufox soak, the Host SQLite
regression, the required fresh-cache Host 10/10 regression, and the relevant
repository matrix.  It never changes tracked fixtures, downloads assets, or
publishes a production/package verification claim.
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
import time
import uuid
from pathlib import Path
from typing import Any

from run_m3_wi_windows import (
    ARTIFACT,
    EXPECTED_ARTIFACT_SHA256,
    EXPECTED_BRANCH,
    EXPECTED_RELEASE,
    HOST_DIR,
    REPO_ROOT,
    TREE_MANIFEST,
    WINDOWS_HOST_TEST,
    command_receipt,
    fixed_input_preflight,
    git,
    live_cookie_sqlite_receipt,
    parse_windows_host_regression,
    parse_sqlite_retry_regression,
    run,
    sha256_file,
    target_processes,
    version,
    windows_session,
)


START_CHECKPOINT = "c2e999cdfc198231801616a67b318cfb3db75c78"
RUNS_ROOT = REPO_ROOT / "artifacts" / "camoufox-m3-wi-windows-gate" / "runs"
R1_TEST_NAME = "m3_wi_windows_r1_runtime_manager_five_cycle_soak"
R1_SCHEMA = REPO_ROOT / "tests/fixtures/camoufox/evidence-manifest-m3-wi-r1-windows.schema.json"
ASSET_LOCK = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-windows-x86_64.json"
UV_LOCK = HOST_DIR / "uv.lock"
HOST_SOURCE = HOST_DIR / "host_v1.py"
HOST_TEST_SOURCE = HOST_DIR / "test_windows_host.py"
EXPECTED_SQLITE_MAX_ATTEMPTS = 6
EXPECTED_SQLITE_DELAY_MS = 200
SECRET_SENTINEL_MARKERS = (
    "M3-WI-VAULT-TOKEN-SENTINEL-DO-NOT-EMIT",
    "M3-WI-PROXY-USERNAME-SENTINEL-DO-NOT-EMIT",
    "M3-WI-PROXY-PASSWORD-SENTINEL-DO-NOT-EMIT",
)


def require_clean_receipt_start() -> tuple[str, str, str, str]:
    if os.name != "nt" or platform.system() != "Windows":
        raise RuntimeError("M3-WI-R1 requires native Windows")
    if Path.cwd().resolve() != REPO_ROOT.resolve():
        raise RuntimeError(f"cwd must be the original checkout {REPO_ROOT}")
    if git("rev-parse", "--git-dir") != ".git":
        raise RuntimeError("M3-WI-R1 refuses a linked/Codex worktree")
    branch = git("branch", "--show-current")
    if branch != EXPECTED_BRANCH:
        raise RuntimeError(f"unexpected branch {branch!r}")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before M3-WI-R1")
    revision = git("rev-parse", "HEAD")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", START_CHECKPOINT, revision],
        cwd=REPO_ROOT,
        check=False,
    )
    if ancestor.returncode != 0:
        raise RuntimeError("R1 receipt commit is not a descendant of c2e999c")
    tree = git("show", "-s", "--format=%T", revision)
    if git("write-tree") != tree:
        raise RuntimeError("R1 receipt commit does not describe the clean worktree tree")
    session_id, session_name, connect_state = windows_session()
    if session_id == 0 or connect_state != 0 or session_name.lower() == "services":
        raise RuntimeError("M3-WI-R1 requires an interactive console/RDP session")
    if target_processes():
        raise RuntimeError("target Camoufox/Firefox/supervisor process exists before R1")
    return revision, tree, branch, session_name


def locked_python(uv_path: str) -> Path:
    completed = run(
        [
            uv_path,
            "run",
            "--frozen",
            "--offline",
            "--project",
            str(HOST_DIR),
            "python",
            "-c",
            "import sys; print(sys.executable)",
        ],
        timeout=120,
    )
    path = Path(completed.stdout.strip().splitlines()[-1]).resolve()
    if not path.is_file():
        raise RuntimeError("uv did not resolve a concrete locked Python interpreter")
    return path


def validate_r1_runtime(value: Any, revision: str, tree: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise RuntimeError("R1 Rust evidence is not an object")
    if (
        value.get("status") != "passed"
        or value.get("codeGitRevision") != revision
        or value.get("codeTreeHash") != tree
        or value.get("cycleCount") != 5
        or value.get("sameProfile") is not True
        or value.get("observedWebsiteDigestStable") is not True
        or value.get("verified") is not False
        or value.get("productionPackageVerified") is not False
        or value.get("secretScan", {}).get("matches") != []
        or value.get("residualProcessCheck", {}).get("aliveOwnedPids") != []
    ):
        raise RuntimeError("R1 RuntimeManager receipt failed its binding/secret/residual checks")
    semantic = value.get("semanticBoundary", {})
    if (
        semantic.get("launchExecutable") != "uv-resolved-locked-python-interpreter"
        or semantic.get("hostEntrypoint") != "apps/camoufox-host/host_v1.py"
        or semantic.get("typedHostArgvRecorded") is not True
        or semantic.get("argvContainsProxyArguments") is not False
        or semantic.get("argvContainsSecrets") is not False
        or semantic.get("verifiedAdapter") is not None
    ):
        raise RuntimeError("R1 RuntimeManager receipt crossed the frozen boundary")
    cycles = value.get("cycles")
    if not isinstance(cycles, list) or len(cycles) != 5:
        raise RuntimeError("R1 receipt must contain exactly five cycle receipts")
    digests: list[str] = []
    profiles: list[str] = []
    host_argvs: list[str] = []
    for index, cycle in enumerate(cycles, start=1):
        if not isinstance(cycle, dict) or cycle.get("cycle") != index:
            raise RuntimeError(f"R1 cycle {index} receipt is malformed")
        if [cycle.get("bootCountBefore"), cycle.get("bootCountAfter")] != [index - 1, index]:
            raise RuntimeError(f"R1 cycle {index} boot continuity is invalid")
        if cycle.get("profileId") is not None:
            profiles.append(str(cycle["profileId"]))
        digests.append(str(cycle.get("observedWebsiteDigest")))
        if (
            cycle.get("cookieApi", {}).get("cookieInApi") is not True
            or cycle.get("pageCookie") is not True
            or cycle.get("sqlite", {}).get("fileExists") is not True
            or cycle.get("sqlite", {}).get("cookieNamePresent") is not True
            or cycle.get("sqlite", {}).get("valuesManaged") is not True
            or not 1 <= cycle.get("sqlite", {}).get("sqliteReadAttempts", 0) <= EXPECTED_SQLITE_MAX_ATTEMPTS
            or cycle.get("sqlite", {}).get("sqliteReadMaxAttempts") != EXPECTED_SQLITE_MAX_ATTEMPTS
            or cycle.get("sqlite", {}).get("sqliteRetryDelayMilliseconds") != EXPECTED_SQLITE_DELAY_MS
            or cycle.get("sqlite", {}).get("sqliteRetryExhausted") is not False
            or "sqliteReadError" in cycle.get("sqlite", {})
        ):
            raise RuntimeError(f"R1 cycle {index} cookie/API/page/SQLite evidence is incomplete")
        close = cycle.get("close", {})
        if (
            close.get("exitStatus") != 0
            or close.get("exitFile") is not True
            or close.get("processTreeExited") is not True
            or close.get("jobActiveProcessCount") != 0
        ):
            raise RuntimeError(f"R1 cycle {index} close receipt is not clean")
        diagnostics = cycle.get("stageDiagnostics")
        if not isinstance(diagnostics, list) or len(diagnostics) > 128:
            raise RuntimeError(f"R1 cycle {index} diagnostics are not bounded")
        required_stages = {
            "browser/context",
            "page",
            "probe",
            "observed collection",
            "response write",
            "close",
        }
        successful = {
            entry.get("stage")
            for entry in diagnostics
            if isinstance(entry, dict)
            and entry.get("kind") == "camoufox-host-stage"
            and entry.get("event") == "success"
            and isinstance(entry.get("durationMs"), int)
            and len(json.dumps(entry, separators=(",", ":"))) <= 512
        }
        if not required_stages.issubset(successful):
            raise RuntimeError(f"R1 cycle {index} is missing a successful stage diagnostic")
        launch_surface = cycle.get("running", {}).get("launchSurface", {})
        arguments = launch_surface.get("arguments", [])
        host_argvs.append(json.dumps(arguments, separators=(",", ":"), ensure_ascii=False))
        if any(
            str(argument).lower().startswith("--proxy")
            or str(argument).lower() == "--no-proxy-server"
            or any(marker in str(argument) for marker in SECRET_SENTINEL_MARKERS)
            for argument in arguments
        ):
            raise RuntimeError(f"R1 cycle {index} Host argv crossed the Direct/secret boundary")
    if len(set(digests)) != 1 or len(set(profiles)) != 1 or len(set(host_argvs)) != 1:
        raise RuntimeError("R1 same-Profile/deterministic Host argv continuity is not stable")
    return {
        "cycleCount": 5,
        "bootCounts": [[index - 1, index] for index in range(1, 6)],
        "observedWebsiteDigest": digests[0],
        "profileId": profiles[0],
        "typedHostArgvStable": True,
        "stageClasses": sorted(
            ["browser/context", "page", "probe", "observed collection", "response write", "close"]
        ),
        "secretMatches": [],
        "aliveOwnedPids": [],
    }


def schema_receipt() -> dict[str, Any]:
    paths = [
        REPO_ROOT / "tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.schema.json",
        R1_SCHEMA,
    ]
    for path in paths:
        value = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(value, dict) or value.get("$schema") != "https://json-schema.org/draft/2020-12/schema":
            raise RuntimeError(f"invalid JSON schema header: {path}")
    return {"schemasParsed": len(paths), "status": "passed"}


def process_secret_receipt(runtime: dict[str, Any], runtime_path: Path) -> dict[str, Any]:
    encoded = runtime_path.read_text(encoding="utf-8")
    matches = [marker for marker in SECRET_SENTINEL_MARKERS if marker in encoded]
    if matches or target_processes():
        raise RuntimeError(f"R1 secret/process boundary failed: {matches}")
    return {
        "status": "passed",
        "secretMatches": matches,
        "targetProcessesAfterRuntime": [],
        "runtimeEvidenceSha256": sha256_file(runtime_path),
    }


def write_cycle_receipts(runtime: dict[str, Any], run_dir: Path) -> list[dict[str, str]]:
    receipts: list[dict[str, str]] = []
    for cycle in runtime.get("cycles", []):
        number = cycle.get("cycle")
        if not isinstance(number, int) or not 1 <= number <= 5:
            raise RuntimeError("R1 cycle receipt has an invalid cycle number")
        path = run_dir / f"r1-cycle-{number}.json"
        payload = (json.dumps(cycle, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
        path.write_bytes(payload)
        digest = hashlib.sha256(payload).hexdigest()
        sidecar = path.with_suffix(".sha256")
        sidecar.write_text(f"{digest}  {path.name}\n", encoding="utf-8", newline="\n")
        receipts.append(
            {
                "cycle": str(number),
                "relativePath": path.relative_to(REPO_ROOT).as_posix(),
                "sha256": digest,
                "sidecarRelativePath": sidecar.relative_to(REPO_ROOT).as_posix(),
            }
        )
    if len(receipts) != 5:
        raise RuntimeError("R1 did not produce five cycle receipts")
    return sorted(receipts, key=lambda receipt: int(receipt["cycle"]))


def run_gate(args: argparse.Namespace) -> int:
    revision, tree, branch, session_name = require_clean_receipt_start()
    fixed_inputs = fixed_input_preflight()
    if fixed_inputs["artifact"]["rawFileSha256"] != EXPECTED_ARTIFACT_SHA256:
        raise RuntimeError("fixed Artifact SHA changed")
    if fixed_inputs["release"] != EXPECTED_RELEASE:
        raise RuntimeError("fixed Camoufox release changed")
    if not R1_SCHEMA.is_file() or not UV_LOCK.is_file() or not HOST_TEST_SOURCE.is_file():
        raise RuntimeError("R1 schema/lock/Host regression source is missing")
    tools: dict[str, str] = {}
    for name in ["uv", "pnpm", "cargo", "rustc", "node"]:
        resolved = shutil.which(name)
        if not resolved:
            raise RuntimeError(f"required executable is unavailable: {name}")
        tools[name] = resolved
    tool_versions = {name: version([path, "--version"]) for name, path in tools.items()}
    python_path = locked_python(tools["uv"])
    fixed_inputs["pythonInterpreter"] = {
        "relativePath": (
            python_path.relative_to(REPO_ROOT).as_posix()
            if python_path.is_relative_to(REPO_ROOT)
            else "external-locked-python"
        ),
        "sha256": sha256_file(python_path),
        "version": version([str(python_path), "--version"]),
        "resolvedBy": "uv run --frozen --offline",
    }
    fixed_inputs["schema"] = {
        "relativePath": R1_SCHEMA.relative_to(REPO_ROOT).as_posix(),
        "sha256": sha256_file(R1_SCHEMA),
    }

    run_id = args.run_id or f"run-{int(time.time())}-{uuid.uuid4().hex[:8]}"
    if not re.fullmatch(r"run-[0-9]+-[0-9a-f]{8}", run_id):
        raise RuntimeError("run-id must match run-<epoch>-<8 hex>")
    run_dir = RUNS_ROOT / run_id
    if run_dir.exists():
        raise RuntimeError(f"run-id already exists: {run_id}")

    commands: list[dict[str, Any]] = []
    engine_verify = run([tools["pnpm"], "engine:verify"], timeout=120)
    commands.append(command_receipt("pnpm engine:verify", engine_verify))

    real_env = os.environ.copy()
    real_env.update(
        {
            "VERISILO_M3_WI_ALLOW_REAL_BROWSER": "1",
            "VERISILO_M3_WI_RUN_ID": run_id,
            "VERISILO_M3_WI_CODE_REVISION": revision,
            "VERISILO_M3_WI_CODE_TREE": tree,
            "VERISILO_M3_WI_BRANCH": branch,
            "VERISILO_M3_WI_PYTHON_PATH": str(python_path),
            "VERISILO_M3_WI_TREE_RAW_SHA256": fixed_inputs["browserTree"]["rawFileSha256"],
            "VERISILO_M3_WI_ARTIFACT_PATH": str(ARTIFACT.resolve()),
        }
    )
    real_command = [
        tools["cargo"],
        "test",
        "--locked",
        "--manifest-path",
        "apps/desktop/src-tauri/Cargo.toml",
        R1_TEST_NAME,
        "--",
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]
    real = run(real_command, env=real_env, timeout=1800)
    commands.append(command_receipt("M3-WI-R1 real five-cycle RuntimeManager/Host/Camoufox", real))
    runtime_path = run_dir / "r1-runtime-evidence.json"
    if not runtime_path.is_file():
        raise RuntimeError("R1 Rust test did not write r1-runtime-evidence.json")
    runtime_evidence = json.loads(runtime_path.read_text(encoding="utf-8"))
    runtime_receipt = validate_r1_runtime(runtime_evidence, revision, tree)
    cycle_receipts = write_cycle_receipts(runtime_evidence, run_dir)
    if target_processes():
        raise RuntimeError("target process remains after R1 real soak")

    sqlite_temp = run_dir / "r1-sqlite-temp"
    sqlite_temp.mkdir(parents=False, exist_ok=False)
    sqlite_env = os.environ.copy()
    sqlite_env.update({"TEMP": str(sqlite_temp), "TMP": str(sqlite_temp)})
    sqlite = run(
        [str(python_path), str(HOST_TEST_SOURCE), "--cookie-sqlite-retry-regression"],
        cwd=HOST_DIR,
        env=sqlite_env,
        timeout=120,
    )
    if any(sqlite_temp.iterdir()):
        raise RuntimeError("SQLite R1 regression left temporary residue")
    sqlite_temp.rmdir()
    commands.append(command_receipt("Host SQLite URI/retry deterministic regression", sqlite))
    sqlite_receipt = parse_sqlite_retry_regression(sqlite)

    host_root = run_dir / "r1-windows-host-regression"
    host_runs = host_root / "runs"
    host_temp = host_root / "temp"
    host_cache = host_root / "empty-cache"
    host_runs.mkdir(parents=True, exist_ok=False)
    host_temp.mkdir(parents=True, exist_ok=False)
    if host_cache.exists():
        raise RuntimeError("R1 fresh Host cache was not absent before test")
    host_env = os.environ.copy()
    host_env.update(
        {
            "VERISILO_WINDOWS_HOST_EMPTY_CACHE": str(host_cache),
            "VERISILO_WINDOWS_HOST_RUNS_ROOT": str(host_runs),
            "TEMP": str(host_temp),
            "TMP": str(host_temp),
        }
    )
    host = run(
        [str(python_path), str(WINDOWS_HOST_TEST)],
        cwd=HOST_DIR,
        env=host_env,
        timeout=3600,
    )
    host_command = command_receipt("Windows Host fresh empty-cache 10/10", host)
    commands.append(host_command)
    if host_command["counts"].get("windowsHostTests") != 10:
        raise RuntimeError("R1 Windows Host regression did not report 10 PASS tests")
    host_receipt = parse_windows_host_regression(host, host_runs)
    if not host_cache.is_dir():
        raise RuntimeError("R1 Windows Host regression did not seed the run-owned cache")
    host_receipt.update(
        {
            "emptyCacheInitiallyAbsent": True,
            "emptyCacheRelativePath": host_cache.relative_to(REPO_ROOT).as_posix(),
            "cacheSeededFromVerifiedArchive": True,
        }
    )
    if target_processes():
        raise RuntimeError("target process remains after R1 Windows Host regression")

    validation_commands = [
        ("pnpm check", [tools["pnpm"], "check"], REPO_ROOT, 600),
        ("pnpm test", [tools["pnpm"], "test"], REPO_ROOT, 900),
        ("pnpm build", [tools["pnpm"], "build"], REPO_ROOT, 900),
        (
            "cargo fmt --check",
            [tools["cargo"], "fmt", "--manifest-path", "apps/desktop/src-tauri/Cargo.toml", "--", "--check"],
            REPO_ROOT,
            300,
        ),
        (
            "cargo test --locked",
            [tools["cargo"], "test", "--locked", "--manifest-path", "apps/desktop/src-tauri/Cargo.toml"],
            REPO_ROOT,
            1800,
        ),
        (
            "cargo clippy --locked -D warnings",
            [tools["cargo"], "clippy", "--locked", "--manifest-path", "apps/desktop/src-tauri/Cargo.toml", "--all-targets", "--", "-D", "warnings"],
            REPO_ROOT,
            1800,
        ),
        (
            "Artifact 25/25",
            [str(python_path), "test_identity_artifact.py"],
            HOST_DIR,
            300,
        ),
        (
            "R1 schema JSON validation",
            [str(python_path), "-c", "import json; from pathlib import Path; paths=[Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.schema.json'),Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-r1-windows.schema.json')]; values=[json.loads(path.read_text(encoding='utf-8')) for path in paths]; assert all(value.get('$schema') == 'https://json-schema.org/draft/2020-12/schema' for value in values); print('schemaCount='+str(len(values)))"],
            REPO_ROOT,
            120,
        ),
    ]
    for label, command, cwd, timeout in validation_commands:
        completed = run(command, cwd=cwd, timeout=timeout)
        commands.append(command_receipt(label, completed))
        if target_processes():
            raise RuntimeError(f"target process appeared after {label}")

    schema_result = schema_receipt()
    process_secret_result = process_secret_receipt(runtime_evidence, runtime_path)
    if git("status", "--porcelain=v1") or target_processes():
        raise RuntimeError("R1 changed tracked worktree or left a target process")

    report = {
        "schema": "verisilo-camoufox-m3-wi-r1-windows-run-report/v1",
        "status": "passed",
        "runId": run_id,
        "startCheckpoint": START_CHECKPOINT,
        "receiptCommit": revision,
        "receiptTree": tree,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "branch": branch,
        "integrationPath": "test-only-real-host",
        "productionPackageVerified": False,
        "productionVerifierFailClosed": True,
        "shipped": False,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
        "environment": {
            "os": platform.platform(),
            "windowsRelease": platform.win32_ver()[0],
            "windowsVersion": platform.win32_ver()[1],
            "architecture": platform.machine(),
            "sessionName": session_name,
            "sessionId": windows_session()[0],
            "python": sys.version.split()[0],
            "uv": tool_versions["uv"],
            "rustc": tool_versions["rustc"],
            "cargo": tool_versions["cargo"],
            "node": tool_versions["node"],
            "pnpm": tool_versions["pnpm"],
        },
        "fixedInputs": fixed_inputs,
        "runtimeEvidencePath": runtime_path.relative_to(REPO_ROOT).as_posix(),
        "runtimeEvidenceSha256": sha256_file(runtime_path),
        "runtimeEvidence": runtime_evidence,
        "r1RuntimeReceipt": runtime_receipt,
        "cycleReceipts": cycle_receipts,
        "sqliteRegression": sqlite_receipt,
        "windowsHostRegression": host_receipt,
        "schemaValidation": schema_result,
        "secretProcessValidation": process_secret_result,
        "commands": commands,
        "failureHistory": {
            "startingGateCommit": START_CHECKPOINT,
            "priorMainBrainFailedGatePreserved": True,
            "m3WiFullGateRerun": False,
            "selectiveRetry": False,
        },
        "residualProcessCheck": {
            "targetProcessesBefore": [],
            "targetProcessesAfter": [],
            "ownedPidsAlive": [],
        },
        "boundaries": {
            "directOnly": True,
            "proxyInjection": False,
            "uiChanged": False,
            "installerChanged": False,
            "signerChanged": False,
            "controlledChromiumChanged": False,
            "realPackageVerified": False,
        },
    }
    report_path = run_dir / "r1-report.json"
    report_bytes = (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    report_path.write_bytes(report_bytes)
    report_sha = hashlib.sha256(report_bytes).hexdigest()
    report_sidecar = run_dir / "r1-report.sha256"
    report_sidecar.write_text(f"{report_sha}  r1-report.json\n", encoding="utf-8", newline="\n")
    summary = {
        "schema": "verisilo-camoufox-m3-wi-r1-windows-summary/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "reportPath": report_path.relative_to(REPO_ROOT).as_posix(),
        "reportSha256": report_sha,
        "runtimeEvidencePath": runtime_path.relative_to(REPO_ROOT).as_posix(),
        "runtimeEvidenceSha256": sha256_file(runtime_path),
        "cycleCount": 5,
        "cycleReceipts": cycle_receipts,
        "hostRegressionCount": 10,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }
    summary_path = run_dir / "r1-summary.json"
    summary_bytes = (json.dumps(summary, indent=2) + "\n").encode("utf-8")
    summary_path.write_bytes(summary_bytes)
    summary_sha = hashlib.sha256(summary_bytes).hexdigest()
    summary_sidecar = run_dir / "r1-summary.sha256"
    summary_sidecar.write_text(f"{summary_sha}  r1-summary.json\n", encoding="utf-8", newline="\n")
    print(f"m3-wi-r1-run-id={run_id}")
    print(f"m3-wi-r1-report={report_path}")
    print(f"m3-wi-r1-report-sha256={report_sha}")
    print(f"m3-wi-r1-summary={summary_path}")
    print(f"m3-wi-r1-summary-sha256={summary_sha}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", help="optional unique run-<epoch>-<8 hex> identifier")
    args = parser.parse_args()
    try:
        return run_gate(args)
    except Exception as exc:  # noqa: BLE001 - one bounded R1 attempt
        print(f"M3-WI-R1 FAILED: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
