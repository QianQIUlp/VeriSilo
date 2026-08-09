#!/usr/bin/env python3
"""Finalize an already-completed R2 soak without starting another browser.

The R2 Rust test owns the only real ten-cycle soak.  This command is used only
when that soak has passed but receipt normalization or a later matrix command
needs to finish.  It never launches the real Rust test or a browser.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import sys
import time
from pathlib import Path
from typing import Any

from run_m3_wi_r2_windows import (
    ARTIFACT,
    EXPECTED_ARTIFACT_SHA256,
    EXPECTED_BRANCH,
    EXPECTED_RELEASE,
    HOST_DIR,
    HOST_TEST_SOURCE,
    REPO_ROOT,
    R2_SCHEMA,
    RUNS_ROOT,
    SECRET_SENTINEL_MARKERS,
    START_CHECKPOINT,
    failure_history_receipt,
    locked_python,
    parse_close_context_regression,
    process_secret_receipt,
    schema_receipt,
    validate_r2_runtime,
    write_cycle_receipts,
)
from run_m3_wi_windows import (
    command_receipt,
    fixed_input_preflight,
    git,
    parse_engine_verify,
    parse_sqlite_retry_regression,
    parse_windows_host_regression,
    run,
    sha256_file,
    target_processes,
    version,
    windows_session,
)


def normalize_runtime_diagnostics(runtime: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    """Keep the last per-Host bounded block and preserve the raw evidence."""
    normalized = json.loads(json.dumps(runtime))
    normalization: dict[str, Any] = {
        "status": "passed",
        "strategy": "last-response-write-start-block-per-cycle",
        "cycleBlocks": [],
    }
    for cycle in normalized.get("cycles", []):
        entries = cycle.get("stageDiagnostics")
        if not isinstance(entries, list):
            raise RuntimeError("R2 raw runtime evidence has no diagnostics list")
        starts = [
            index
            for index, entry in enumerate(entries)
            if isinstance(entry, dict)
            and entry.get("stage") == "response write"
            and entry.get("event") == "start"
        ]
        if not starts:
            raise RuntimeError("R2 raw runtime evidence has no Host block boundary")
        start = starts[-1]
        block = entries[start:]
        if len(block) > 20:
            raise RuntimeError("one Host diagnostic block exceeds the bounded 20-event limit")
        cycle["stageDiagnostics"] = block
        normalization["cycleBlocks"].append(
            {"cycle": cycle.get("cycle"), "rawCount": len(entries), "normalizedCount": len(block)}
        )
    return normalized, normalization


def clean_receipt_start() -> tuple[str, str, str, str]:
    if os.name != "nt" or platform.system() != "Windows":
        raise RuntimeError("R2 finalization requires native Windows")
    if Path.cwd().resolve() != REPO_ROOT.resolve():
        raise RuntimeError(f"cwd must be the original checkout {REPO_ROOT}")
    if git("branch", "--show-current") != EXPECTED_BRANCH:
        raise RuntimeError("unexpected R2 finalization branch")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before R2 finalization")
    revision = git("rev-parse", "HEAD")
    tree = git("show", "-s", "--format=%T", revision)
    if git("write-tree") != tree:
        raise RuntimeError("R2 finalization requires a clean receipt tree")
    session_id, session_name, connect_state = windows_session()
    if session_id == 0 or connect_state != 0 or session_name.lower() == "services":
        raise RuntimeError("R2 finalization requires an interactive console/RDP session")
    if target_processes():
        raise RuntimeError("target process exists before R2 finalization")
    return revision, tree, session_name, str(session_id)


def run_finalization(run_id: str) -> int:
    finalization_revision, finalization_tree, session_name, session_id = clean_receipt_start()
    run_dir = RUNS_ROOT / run_id
    raw_runtime_path = run_dir / "r2-runtime-evidence.json"
    report_path = run_dir / "r2-report.json"
    if not run_dir.is_dir() or not raw_runtime_path.is_file():
        raise RuntimeError("R2 run directory/raw runtime evidence is missing")
    if report_path.exists():
        raise RuntimeError("R2 report already exists; finalization is single-use")
    raw_runtime_bytes = raw_runtime_path.read_bytes()
    raw_runtime = json.loads(raw_runtime_bytes)
    soak_revision = raw_runtime.get("codeGitRevision")
    soak_tree = raw_runtime.get("codeTreeHash")
    if not isinstance(soak_revision, str) or not isinstance(soak_tree, str):
        raise RuntimeError("R2 raw runtime evidence is not bound to a soak receipt")
    if raw_runtime.get("status") != "passed" or raw_runtime.get("cycleCount") != 10:
        raise RuntimeError("R2 raw runtime evidence is not the completed ten-cycle pass")
    normalized_runtime, normalization = normalize_runtime_diagnostics(raw_runtime)
    validate_r2_runtime(normalized_runtime, soak_revision, soak_tree)
    normalized_path = run_dir / "r2-runtime-evidence-normalized.json"
    normalized_bytes = (json.dumps(normalized_runtime, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    normalized_path.write_bytes(normalized_bytes)

    fixed_inputs = fixed_input_preflight()
    if fixed_inputs["artifact"]["rawFileSha256"] != EXPECTED_ARTIFACT_SHA256:
        raise RuntimeError("fixed Artifact SHA changed")
    if fixed_inputs["release"] != EXPECTED_RELEASE:
        raise RuntimeError("fixed Camoufox release changed")
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
        "relativePath": R2_SCHEMA.relative_to(REPO_ROOT).as_posix(),
        "sha256": sha256_file(R2_SCHEMA),
    }
    history = failure_history_receipt()
    history["postSoakSetupFailure"] = {
        "runId": "run-1786289175-03eaf231",
        "status": "preserved-zero-cycle-runner-setup-failure",
        "realBrowserStarted": False,
    }

    commands: list[dict[str, Any]] = [
        {
            "label": "M3-WI-R2 unique real ten-cycle soak (completed before finalization)",
            "exitCode": 0,
            "outputSha256": hashlib.sha256(raw_runtime_bytes).hexdigest(),
            "counts": {"realCycles": 10},
        }
    ]
    engine_verify = run([tools["pnpm"], "engine:verify"], timeout=120)
    commands.append(command_receipt("pnpm engine:verify", engine_verify))
    parse_engine_verify(engine_verify.stdout or "")

    sqlite_temp = run_dir / "r2-final-sqlite-temp"
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
        raise RuntimeError("SQLite R2 finalization regression left temporary residue")
    sqlite_temp.rmdir()
    commands.append(command_receipt("Host SQLite normal/verbatim read-only regression", sqlite))
    sqlite_receipt = parse_sqlite_retry_regression(sqlite)

    close_regression = run(
        [str(python_path), str(HOST_TEST_SOURCE), "--close-context-regression"],
        cwd=HOST_DIR,
        timeout=120,
    )
    commands.append(command_receipt("Host fake-context typed close/ownership regression", close_regression))
    close_receipt = parse_close_context_regression(close_regression)

    # Keep the run-owned cache under a short repo path.  The browser seed
    # contains a nested DLL path and Windows' legacy path handling otherwise
    # turns a long run receipt path into WinError 3 during copytree.
    host_root = REPO_ROOT / "artifacts" / (
        f"r2-host-regression-{run_id.rsplit('-', 1)[-1]}-{int(time.time())}-{finalization_revision[-8:]}"
    )
    host_runs = host_root / "runs"
    host_temp = host_root / "temp"
    host_cache = host_root / "empty-cache"
    host_runs.mkdir(parents=True, exist_ok=False)
    host_temp.mkdir(parents=True, exist_ok=False)
    host_env = os.environ.copy()
    host_env.update(
        {
            "VERISILO_WINDOWS_HOST_EMPTY_CACHE": str(host_cache),
            "VERISILO_WINDOWS_HOST_RUNS_ROOT": str(host_runs),
            "TEMP": str(host_temp),
            "TMP": str(host_temp),
        }
    )
    host = run([str(python_path), str(HOST_TEST_SOURCE)], cwd=HOST_DIR, env=host_env, timeout=3600)
    host_command = command_receipt("Windows Host fresh empty-cache 10/10", host)
    commands.append(host_command)
    if host_command["counts"].get("windowsHostTests") != 10:
        raise RuntimeError("R2 finalization Host regression did not report 10 PASS tests")
    host_receipt = parse_windows_host_regression(host, host_runs)
    if not host_cache.is_dir():
        raise RuntimeError("R2 finalization Host regression did not seed its cache")
    host_receipt.update(
        {
            "emptyCacheInitiallyAbsent": True,
            "emptyCacheRelativePath": host_cache.relative_to(REPO_ROOT).as_posix(),
            "cacheSeededFromVerifiedArchive": True,
        }
    )
    if target_processes():
        raise RuntimeError("target process remains after R2 finalization Host regression")

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
        ("Artifact 25/25", [str(python_path), "test_identity_artifact.py"], HOST_DIR, 300),
        (
            "R2 schema JSON validation",
            [
                str(python_path),
                "-c",
                "import json; from pathlib import Path; paths=[Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.schema.json'),Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-r2-windows.schema.json')]; values=[json.loads(path.read_text(encoding='utf-8')) for path in paths]; assert all(value.get(chr(36)+'schema') == 'https://json-schema.org/draft/2020-12/schema' for value in values); print('schemaCount='+str(len(values)))",
            ],
            REPO_ROOT,
            120,
        ),
    ]
    for label, command, cwd, timeout in validation_commands:
        completed = run(command, cwd=cwd, timeout=timeout)
        commands.append(command_receipt(label, completed))
        if target_processes():
            raise RuntimeError(f"target process appeared after {label}")

    cycle_receipts = write_cycle_receipts(normalized_runtime, run_dir)
    schema_result = schema_receipt()
    process_secret_result = process_secret_receipt(normalized_runtime, normalized_path)
    if git("status", "--porcelain=v1") or target_processes():
        raise RuntimeError("R2 finalization changed tracked worktree or left a target process")

    report = {
        "schema": "verisilo-camoufox-m3-wi-r2-windows-run-report/v1",
        "status": "passed",
        "runId": run_id,
        "startCheckpoint": START_CHECKPOINT,
        "receiptCommit": soak_revision,
        "receiptTree": soak_tree,
        "codeGitRevision": soak_revision,
        "codeTreeHash": soak_tree,
        "finalizationCommit": finalization_revision,
        "finalizationTree": finalization_tree,
        "branch": EXPECTED_BRANCH,
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
            "sessionId": session_id,
            "python": sys.version.split()[0],
            "uv": tool_versions["uv"],
            "rustc": tool_versions["rustc"],
            "cargo": tool_versions["cargo"],
            "node": tool_versions["node"],
            "pnpm": tool_versions["pnpm"],
        },
        "fixedInputs": fixed_inputs,
        "runtimeEvidencePath": normalized_path.relative_to(REPO_ROOT).as_posix(),
        "runtimeEvidenceSha256": sha256_file(normalized_path),
        "rawRuntimeEvidencePath": raw_runtime_path.relative_to(REPO_ROOT).as_posix(),
        "rawRuntimeEvidenceSha256": hashlib.sha256(raw_runtime_bytes).hexdigest(),
        "diagnosticNormalization": normalization,
        "runtimeEvidence": normalized_runtime,
        "r2RuntimeReceipt": validate_r2_runtime(normalized_runtime, soak_revision, soak_tree),
        "cycleReceipts": cycle_receipts,
        "sqliteRegression": sqlite_receipt,
        "fakeCloseRegression": close_receipt,
        "windowsHostRegression": host_receipt,
        "schemaValidation": schema_result,
        "secretProcessValidation": process_secret_result,
        "commands": commands,
        "failureHistory": history,
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
    report_bytes = (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    report_path.write_bytes(report_bytes)
    report_sha = hashlib.sha256(report_bytes).hexdigest()
    report_sidecar = run_dir / "r2-report.sha256"
    report_sidecar.write_text(f"{report_sha}  r2-report.json\n", encoding="utf-8", newline="\n")
    summary = {
        "schema": "verisilo-camoufox-m3-wi-r2-windows-summary/v1",
        "status": "passed",
        "runId": run_id,
        "codeGitRevision": soak_revision,
        "codeTreeHash": soak_tree,
        "finalizationCommit": finalization_revision,
        "finalizationTree": finalization_tree,
        "reportPath": report_path.relative_to(REPO_ROOT).as_posix(),
        "reportSha256": report_sha,
        "runtimeEvidencePath": normalized_path.relative_to(REPO_ROOT).as_posix(),
        "runtimeEvidenceSha256": sha256_file(normalized_path),
        "rawRuntimeEvidencePath": raw_runtime_path.relative_to(REPO_ROOT).as_posix(),
        "rawRuntimeEvidenceSha256": hashlib.sha256(raw_runtime_bytes).hexdigest(),
        "cycleCount": 10,
        "cycleReceipts": cycle_receipts,
        "hostRegressionCount": 10,
        "fakeCloseCaseCount": 4,
        "realSoakReexecuted": False,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }
    summary_path = run_dir / "r2-summary.json"
    summary_bytes = (json.dumps(summary, indent=2) + "\n").encode("utf-8")
    summary_path.write_bytes(summary_bytes)
    summary_sha = hashlib.sha256(summary_bytes).hexdigest()
    summary_sidecar = run_dir / "r2-summary.sha256"
    summary_sidecar.write_text(f"{summary_sha}  r2-summary.json\n", encoding="utf-8", newline="\n")
    print(f"m3-wi-r2-run-id={run_id}")
    print(f"m3-wi-r2-report={report_path}")
    print(f"m3-wi-r2-report-sha256={report_sha}")
    print(f"m3-wi-r2-summary={summary_path}")
    print(f"m3-wi-r2-summary-sha256={summary_sha}")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    try:
        return run_finalization(args.run_id)
    except Exception as exc:  # noqa: BLE001 - one bounded finalization attempt
        print(f"M3-WI-R2 FINALIZATION FAILED: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
