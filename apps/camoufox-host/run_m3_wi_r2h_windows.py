#!/usr/bin/env python3
"""Run the bounded M3-WI-R2H Host-matrix determinism Gate.

R2H is deliberately independent from the rejected R2 evidence.  It records
all prior R2 Host attempts, then runs one declared alternating acceptance
sequence (persistence, lock-crash, five times) followed by one fresh-cache
10/10 Host matrix.  It never runs the RuntimeManager soak and never changes
production Host or launcher code.
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
    EXPECTED_TREE_CANONICAL_SHA256,
    HOST_DIR,
    REPO_ROOT,
    TREE_MANIFEST,
    WINDOWS_HOST_TEST,
    command_receipt,
    fixed_input_preflight,
    git,
    parse_engine_verify,
    parse_windows_host_regression,
    sha256_file,
    target_processes,
    version,
    windows_session,
)


START_CHECKPOINT = "ecafca9dc22d728087bc5d7de127f14c908293dd"
R2H_SCHEMA = REPO_ROOT / "tests/fixtures/camoufox/evidence-manifest-m3-wi-r2h-windows.schema.json"
R2H_MANIFEST = REPO_ROOT / "tests/fixtures/camoufox/evidence-manifest-m3-wi-r2h-windows.json"
R2H_ROOT = REPO_ROOT / "artifacts/camoufox-m3-wi-r2h-windows-gate"
R2H_RUNS_ROOT = R2H_ROOT / "runs"
OLD_R2_MANIFEST_RELATIVE = "tests/fixtures/camoufox/evidence-manifest-m3-wi-r2-windows.json"
OLD_R2_MANIFEST = REPO_ROOT / OLD_R2_MANIFEST_RELATIVE
ACCEPTANCE_SEQUENCE = (
    "persistence",
    "lock-crash",
    "persistence",
    "lock-crash",
    "persistence",
    "lock-crash",
    "persistence",
    "lock-crash",
    "persistence",
    "lock-crash",
)
SECRET_SENTINELS = (
    "M3-WI-VAULT-TOKEN-SENTINEL-DO-NOT-EMIT",
    "M3-WI-PROXY-USERNAME-SENTINEL-DO-NOT-EMIT",
    "M3-WI-PROXY-PASSWORD-SENTINEL-DO-NOT-EMIT",
    "R2-CLOSE-SECRET-SENTINEL",
)
PROTECTED_PATHS = (
    "apps/camoufox-host/host_v1.py",
    "apps/desktop/src-tauri/src/launcher.rs",
    "apps/desktop/src-tauri/src/engine.rs",
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-windows-x86_64.json",
    "apps/camoufox-host/uv.lock",
    "tests/fixtures/camoufox/browser-tree-manifest-windows.json",
    "tests/fixtures/camoufox/identity-win-a.json",
    "tests/fixtures/camoufox/identity-win-a.json.sha256",
    "tests/fixtures/camoufox/identity-win-b.json",
    "tests/fixtures/camoufox/identity-win-b.json.sha256",
    "tests/fixtures/camoufox/identity-win-c.json",
    "tests/fixtures/camoufox/identity-win-c.json.sha256",
    OLD_R2_MANIFEST_RELATIVE,
)

_ACTIVE_RUN_DIR: Path | None = None
_ACTIVE_CONTEXT: dict[str, Any] = {}


class R2HFailure(RuntimeError):
    def __init__(self, message: str, receipt: dict[str, Any] | None = None) -> None:
        self.receipt = receipt
        super().__init__(message)


def run_command(
    command: list[str],
    *,
    cwd: Path = REPO_ROOT,
    env: dict[str, str] | None = None,
    timeout: float = 1800,
    check: bool = False,
) -> subprocess.CompletedProcess[str]:
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
    if check and completed.returncode != 0:
        raise R2HFailure(
            "command failed: " + subprocess.list2cmdline(command),
            command_receipt(subprocess.list2cmdline(command), completed),
        )
    return completed


def locked_python(uv_path: str) -> Path:
    completed = run_command(
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
    if completed.returncode != 0 or not completed.stdout.strip():
        raise R2HFailure("uv did not resolve the locked Python interpreter")
    path = Path(completed.stdout.strip().splitlines()[-1]).resolve()
    if not path.is_file():
        raise R2HFailure("uv resolved a missing Python interpreter")
    return path


def require_clean_start() -> tuple[str, str, str, str]:
    if os.name != "nt" or platform.system() != "Windows":
        raise R2HFailure("M3-WI-R2H requires native Windows")
    if Path.cwd().resolve() != REPO_ROOT.resolve():
        raise R2HFailure(f"cwd must be the original checkout {REPO_ROOT}")
    if git("rev-parse", "--git-dir") != ".git":
        raise R2HFailure("R2H refuses a linked/Codex worktree")
    branch = git("branch", "--show-current")
    if branch != EXPECTED_BRANCH:
        raise R2HFailure(f"unexpected branch {branch!r}")
    if git("status", "--porcelain=v1"):
        raise R2HFailure("worktree must be clean before R2H")
    revision = git("rev-parse", "HEAD")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", START_CHECKPOINT, revision],
        cwd=REPO_ROOT,
        check=False,
    )
    if ancestor.returncode != 0:
        raise R2HFailure("R2H revision is not a descendant of the rejected R2 checkpoint")
    tree = git("show", "-s", "--format=%T", revision)
    if git("write-tree") != tree:
        raise R2HFailure("R2H requires the clean committed tree")
    for path in PROTECTED_PATHS:
        changed = subprocess.run(
            ["git", "diff", "--quiet", f"{START_CHECKPOINT}..{revision}", "--", path],
            cwd=REPO_ROOT,
            check=False,
        )
        if changed.returncode != 0:
            raise R2HFailure(f"R2H changed protected production/fixed input: {path}")
    session_id, session_name, connect_state = windows_session()
    if session_id == 0 or connect_state != 0 or session_name.lower() == "services":
        raise R2HFailure("R2H requires an interactive console/RDP session")
    if target_processes():
        raise R2HFailure("target Camoufox/Firefox/supervisor process exists before R2H")
    return revision, tree, branch, session_name


def classify_historical_failure(test: str, result: dict[str, Any]) -> dict[str, Any]:
    error = str(result.get("error") or "")
    traceback_text = str(result.get("traceback") or "")
    combined = error + "\n" + traceback_text
    if "profile_in_use" in combined and "relaunch" in combined:
        phase = "profile-lock-release"
        boundary = "crash-recovery-relaunch"
    elif "timed out waiting for Host protocol response" in combined:
        if test == "persistence" or "second = host2.launch" in traceback_text:
            phase = "launch"
            boundary = "stdout-response"
        elif "first = host1.launch" in traceback_text:
            phase = "launch"
            boundary = "stdout-response"
        elif "status" in traceback_text:
            phase = "status"
            boundary = "stdout-response"
        else:
            phase = "unknown"
            boundary = "stdout-response"
    elif test == "lock-crash" and "closeOutcome" in combined:
        phase = "status"
        boundary = "close-outcome-ownership"
    elif any(token in combined for token in ("WinError 3", "copytree", "cache")):
        phase = "cache-seed"
        boundary = "test-setup"
    else:
        phase = "unknown"
        boundary = "unknown"
    return {
        "test": test,
        "phase": phase,
        "boundary": boundary,
        "timeout": "timed out waiting for Host protocol response" in combined,
        "errorSha256": hashlib.sha256(error.encode("utf-8")).hexdigest(),
        "tracebackSha256": hashlib.sha256(traceback_text.encode("utf-8")).hexdigest(),
        "reportRelativePath": result.get("reportFile"),
        "reportSha256": result.get("reportSha256"),
    }


def historical_failure_history() -> dict[str, Any]:
    if not OLD_R2_MANIFEST.is_file():
        raise R2HFailure("the rejected R2 manifest is missing")
    manifest_sha = sha256_file(OLD_R2_MANIFEST)
    old = json.loads(OLD_R2_MANIFEST.read_text(encoding="utf-8"))
    attempts = old.get("failureHistory", {}).get("r2HostMatrixAttempts")
    if not isinstance(attempts, list) or len(attempts) != 6:
        raise R2HFailure("R2 failure history does not contain all six Host attempts")
    bound: list[dict[str, Any]] = []
    for attempt in attempts:
        summary_relative = attempt.get("summaryRelativePath")
        summary_sha = attempt.get("summarySha256")
        if not isinstance(summary_relative, str) or not isinstance(summary_sha, str):
            raise R2HFailure("R2 failure history has an unbound summary")
        summary_path = (REPO_ROOT / summary_relative).resolve()
        if not summary_path.is_file() or not summary_path.is_relative_to(REPO_ROOT):
            raise R2HFailure(f"R2 failure summary is missing: {summary_relative}")
        actual_sha = sha256_file(summary_path)
        if actual_sha != summary_sha:
            raise R2HFailure(f"R2 failure summary SHA changed: {summary_relative}")
        sidecar = summary_path.with_suffix(".sha256")
        if not sidecar.is_file() or sidecar.read_text(encoding="utf-8").split()[0] != actual_sha:
            raise R2HFailure(f"R2 failure summary sidecar mismatch: {summary_relative}")
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        failures: list[dict[str, Any]] = []
        results = summary.get("results", {})
        if not isinstance(results, dict):
            raise R2HFailure(f"R2 failure summary results are malformed: {summary_relative}")
        for test, result in sorted(results.items()):
            if not isinstance(result, dict) or result.get("status") != "failed":
                continue
            report_path = (REPO_ROOT / str(result.get("reportFile"))).resolve()
            report_sha = result.get("reportSha256")
            if not report_path.is_file() or not report_path.is_relative_to(REPO_ROOT):
                raise R2HFailure(f"R2 failed report is missing: {test}")
            if not isinstance(report_sha, str) or sha256_file(report_path) != report_sha:
                raise R2HFailure(f"R2 failed report SHA mismatch: {test}")
            failures.append(classify_historical_failure(test, result))
        bound.append(
            {
                "rootRelativePath": attempt.get("rootRelativePath"),
                "summaryRelativePath": summary_relative,
                "summarySha256": actual_sha,
                "status": summary.get("status"),
                "failedTests": sorted(attempt.get("failedTests", [])),
                "testCount": len(results),
                "failedReportPhases": failures,
            }
        )
    return {
        "sourceManifest": {
            "relativePath": OLD_R2_MANIFEST_RELATIVE,
            "sha256": manifest_sha,
        },
        "attemptCount": len(bound),
        "passedAttemptCount": sum(item["status"] == "passed" for item in bound),
        "failedAttemptCount": sum(item["status"] == "failed" for item in bound),
        "attempts": bound,
        "selectedHistoricalPass": False,
        "selectionPolicy": "no-historical-attempt-used",
        # True is the truthful value for the prior six-attempt history.  The
        # R2H acceptance sequence has its own explicit no-retry field below.
        "selectiveRetry": True,
        "acceptanceSequenceSelectiveRetry": False,
    }


def source_binding(revision: str, tree: str) -> dict[str, str]:
    paths = {
        "host": "apps/camoufox-host/host_v1.py",
        "hostTest": "apps/camoufox-host/test_windows_host.py",
        "runner": "apps/camoufox-host/run_m3_wi_r2h_windows.py",
        "schema": "tests/fixtures/camoufox/evidence-manifest-m3-wi-r2h-windows.schema.json",
    }
    result = {"revision": revision, "tree": tree}
    for key, path in paths.items():
        result[f"{key}Blob"] = git("rev-parse", f"{revision}:{path}")
    return result


def parse_single_receipt(
    completed: subprocess.CompletedProcess[str],
    attempt_root: Path,
    case: str,
    index: int,
    source: dict[str, str],
) -> dict[str, Any]:
    output = completed.stdout or ""
    summary_match = re.search(r"^single-summary-file=(.+)$", output, re.MULTILINE)
    summary_sha_match = re.search(r"^single-summary-sha256=([0-9a-f]{64})$", output, re.MULTILINE)
    if not summary_match or not summary_sha_match:
        raise R2HFailure(
            f"R2H {case} attempt {index} did not emit summary bindings",
            command_receipt(f"single Host test {case}", completed),
        )
    summary_path = (REPO_ROOT / summary_match.group(1).strip()).resolve()
    if not summary_path.is_file() or not summary_path.is_relative_to(attempt_root.resolve()):
        raise R2HFailure(f"R2H single summary escaped its attempt root: {summary_path}")
    summary_sha = sha256_file(summary_path)
    sidecar = summary_path.with_suffix(".sha256")
    if summary_sha != summary_sha_match.group(1):
        raise R2HFailure(f"R2H single summary SHA mismatch: {summary_path}")
    if not sidecar.is_file() or sidecar.read_text(encoding="utf-8").split()[0] != summary_sha:
        raise R2HFailure(f"R2H single summary sidecar mismatch: {summary_path}")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    result = summary.get("result")
    if not isinstance(result, dict) or summary.get("test") != case:
        raise R2HFailure(f"R2H single summary result is malformed: {summary_path}")
    report_path = (REPO_ROOT / str(result.get("reportFile"))).resolve()
    report_sha = result.get("reportSha256")
    if not report_path.is_file() or not report_path.is_relative_to(attempt_root.resolve()):
        raise R2HFailure(f"R2H single report escaped its attempt root: {report_path}")
    if not isinstance(report_sha, str) or sha256_file(report_path) != report_sha:
        raise R2HFailure(f"R2H single report SHA mismatch: {report_path}")
    receipt = {
        "index": index,
        "case": case,
        "status": summary.get("status"),
        "runId": summary.get("runId"),
        "source": source,
        "attemptRoot": attempt_root.relative_to(REPO_ROOT).as_posix(),
        "summary": {
            "relativePath": summary_path.relative_to(REPO_ROOT).as_posix(),
            "sha256": summary_sha,
            "sidecarRelativePath": sidecar.relative_to(REPO_ROOT).as_posix(),
            "sidecarSha256": sha256_file(sidecar),
        },
        "report": {
            "relativePath": report_path.relative_to(REPO_ROOT).as_posix(),
            "sha256": report_sha,
            "sidecarRelativePath": report_path.with_suffix(".sha256").relative_to(REPO_ROOT).as_posix(),
            "sidecarSha256": sha256_file(report_path.with_suffix(".sha256")),
        },
        "result": result,
        "command": command_receipt(f"R2H acceptance {index} {case}", completed),
    }
    if completed.returncode != 0 or summary.get("status") != "passed" or result.get("status") != "passed":
        raise R2HFailure(f"R2H acceptance attempt {index} {case} failed", receipt)
    return receipt


def run_acceptance_sequence(
    run_id: str, revision: str, tree: str, python_path: Path, run_dir: Path
) -> list[dict[str, Any]]:
    source = source_binding(revision, tree)
    sequence_root = run_dir / "acceptance-sequence"
    sequence_root.mkdir(parents=True, exist_ok=False)
    cache_root = sequence_root / "shared-cache"
    receipts: list[dict[str, Any]] = []
    for index, case in enumerate(ACCEPTANCE_SEQUENCE, start=1):
        attempt_root = sequence_root / f"attempt-{index:02d}-{case}"
        temp_root = attempt_root / "temp"
        runs_root = attempt_root / "runs"
        temp_root.mkdir(parents=True, exist_ok=False)
        runs_root.mkdir(parents=True, exist_ok=False)
        env = os.environ.copy()
        env.update(
            {
                "VERISILO_WINDOWS_HOST_RUNS_ROOT": str(runs_root),
                "VERISILO_CAMOUFOX_CACHE_DIR": str(cache_root),
                "TEMP": str(temp_root),
                "TMP": str(temp_root),
            }
        )
        if index == 1:
            env["VERISILO_WINDOWS_HOST_EMPTY_CACHE"] = str(cache_root)
        else:
            env.pop("VERISILO_WINDOWS_HOST_EMPTY_CACHE", None)
        command = [str(python_path), str(WINDOWS_HOST_TEST), "--single-test", case]
        try:
            completed = run_command(command, cwd=HOST_DIR, env=env, timeout=600)
        except subprocess.TimeoutExpired as exc:
            receipt = {
                "index": index,
                "case": case,
                "status": "failed",
                "phase": "test-process-timeout",
                "source": source,
                "attemptRoot": attempt_root.relative_to(REPO_ROOT).as_posix(),
                "timeoutSeconds": 600,
            }
            raise R2HFailure(f"R2H acceptance attempt {index} timed out", receipt) from exc
        try:
            receipt = parse_single_receipt(completed, attempt_root, case, index, source)
        except R2HFailure as exc:
            if exc.receipt is None:
                exc.receipt = {
                    "index": index,
                    "case": case,
                    "status": "failed",
                    "source": source,
                    "attemptRoot": attempt_root.relative_to(REPO_ROOT).as_posix(),
                    "command": command_receipt(f"R2H acceptance {index} {case}", completed),
                }
            raise
        receipts.append(receipt)
        if target_processes():
            raise R2HFailure(
                f"target process remained after R2H acceptance {index}", receipt
            )
    if [receipt["case"] for receipt in receipts] != list(ACCEPTANCE_SEQUENCE):
        raise R2HFailure("R2H acceptance sequence order changed")
    if any(receipt["status"] != "passed" for receipt in receipts):
        raise R2HFailure("R2H acceptance sequence did not pass fail-closed")
    return receipts


def run_full_host_matrix(
    run_id: str, python_path: Path, run_dir: Path
) -> dict[str, Any]:
    root = run_dir / "fresh-empty-cache-host-10of10"
    runs_root = root / "runs"
    temp_root = root / "temp"
    cache_root = root / "empty-cache"
    runs_root.mkdir(parents=True, exist_ok=False)
    temp_root.mkdir(parents=True, exist_ok=False)
    if cache_root.exists():
        raise R2HFailure("R2H full Host cache unexpectedly exists before the run")
    env = os.environ.copy()
    env.update(
        {
            "VERISILO_WINDOWS_HOST_RUNS_ROOT": str(runs_root),
            "VERISILO_WINDOWS_HOST_EMPTY_CACHE": str(cache_root),
            "VERISILO_CAMOUFOX_CACHE_DIR": str(cache_root),
            "TEMP": str(temp_root),
            "TMP": str(temp_root),
        }
    )
    command = [str(python_path), str(WINDOWS_HOST_TEST)]
    try:
        completed = run_command(command, cwd=HOST_DIR, env=env, timeout=3600)
    except subprocess.TimeoutExpired as exc:
        raise R2HFailure(
            "R2H fresh empty-cache Host matrix timed out",
            {
                "phase": "test-process-timeout",
                "timeoutSeconds": 3600,
                "root": root.relative_to(REPO_ROOT).as_posix(),
            },
        ) from exc
    command_receipt_value = command_receipt("R2H fresh empty-cache Host 10/10", completed)
    if completed.returncode != 0:
        raise R2HFailure("R2H fresh empty-cache Host matrix failed", command_receipt_value)
    parsed = parse_windows_host_regression(completed, runs_root)
    if parsed.get("testCount") != 10 or parsed.get("passed") != 10:
        raise R2HFailure("R2H fresh empty-cache Host matrix was not 10/10")
    if not cache_root.is_dir():
        raise R2HFailure("R2H full Host matrix did not seed its empty cache")
    if target_processes():
        raise R2HFailure("target process remained after R2H full Host matrix")
    return {
        "status": "passed",
        "root": root.relative_to(REPO_ROOT).as_posix(),
        "emptyCacheInitiallyAbsent": True,
        "command": command_receipt_value,
        "receipt": parsed,
    }


def run_validation_matrix(python_path: Path) -> dict[str, Any]:
    tools: dict[str, str] = {}
    for name in ("pnpm", "cargo", "rustc", "node"):
        path = shutil.which(name)
        if not path:
            raise R2HFailure(f"required validation tool is unavailable: {name}")
        tools[name] = path
    commands: list[dict[str, Any]] = []
    engine = run_command([tools["pnpm"], "engine:verify"], timeout=120)
    engine_receipt = command_receipt("pnpm engine:verify", engine)
    commands.append(engine_receipt)
    if engine.returncode != 0:
        raise R2HFailure("pnpm engine:verify failed", engine_receipt)
    engine_json = parse_engine_verify(engine.stdout or "")
    for label, command, cwd, timeout in [
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
            "Artifact identity 25/25",
            [str(python_path), "test_identity_artifact.py"],
            HOST_DIR,
            300,
        ),
        (
            "schema parse and boundary check",
            [
                str(python_path),
                "-c",
                "import json; from pathlib import Path; paths=[Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-windows.schema.json'),Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-r2-windows.schema.json'),Path('tests/fixtures/camoufox/evidence-manifest-m3-wi-r2h-windows.schema.json')]; values=[json.loads(p.read_text(encoding='utf-8')) for p in paths]; assert all(v.get('$schema') == 'https://json-schema.org/draft/2020-12/schema' for v in values); print('schemaCount='+str(len(values)))",
            ],
            REPO_ROOT,
            120,
        ),
    ]:
        completed = run_command(command, cwd=cwd, timeout=timeout)
        receipt = command_receipt(label, completed)
        commands.append(receipt)
        if completed.returncode != 0:
            raise R2HFailure(f"R2H validation failed: {label}", receipt)
        if target_processes():
            raise R2HFailure(f"target process appeared after {label}", receipt)
    if target_processes():
        raise R2HFailure("target process remained after R2H validation matrix")
    return {
        "status": "passed",
        "tools": {name: {"path": path, "version": version([path, "--version"])} for name, path in tools.items()},
        "engineVerify": engine_json,
        "commands": commands,
    }


def secret_process_validation(run_dir: Path) -> dict[str, Any]:
    matches: list[str] = []
    scanned = 0
    for path in run_dir.rglob("*"):
        if not path.is_file() or path.suffix.lower() not in {".json", ".sha256"}:
            continue
        scanned += 1
        content = path.read_text(encoding="utf-8", errors="replace")
        matches.extend(marker for marker in SECRET_SENTINELS if marker in content)
    matches = sorted(set(matches))
    processes = target_processes()
    if matches or processes:
        raise R2HFailure("R2H secret/process boundary failed")
    return {
        "status": "passed",
        "secretMatches": [],
        "receiptFilesScanned": scanned,
        "targetProcessesAfter": [],
    }


def write_receipts(
    run_dir: Path,
    report: dict[str, Any],
    *,
    status: str,
) -> tuple[Path, str, Path, str]:
    report_path = run_dir / "r2h-report.json"
    report_bytes = (json.dumps(report, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    report_path.write_bytes(report_bytes)
    report_sha = hashlib.sha256(report_bytes).hexdigest()
    report_sidecar = run_dir / "r2h-report.sha256"
    report_sidecar.write_text(f"{report_sha}  {report_path.name}\n", encoding="utf-8", newline="\n")
    summary = {
        "schema": "verisilo-camoufox-m3-wi-r2h-windows-summary/v1",
        "status": status,
        "runId": report.get("runId"),
        "receiptCommit": report.get("receiptCommit"),
        "receiptTree": report.get("receiptTree"),
        "reportPath": report_path.relative_to(REPO_ROOT).as_posix(),
        "reportSha256": report_sha,
        "acceptanceAttemptCount": len(report.get("acceptanceSequence", {}).get("attempts", [])),
        "fullHostTestCount": report.get("fullHostMatrix", {}).get("receipt", {}).get("testCount"),
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }
    summary_path = run_dir / "r2h-summary.json"
    summary_bytes = (json.dumps(summary, indent=2) + "\n").encode("utf-8")
    summary_path.write_bytes(summary_bytes)
    summary_sha = hashlib.sha256(summary_bytes).hexdigest()
    summary_sidecar = run_dir / "r2h-summary.sha256"
    summary_sidecar.write_text(f"{summary_sha}  {summary_path.name}\n", encoding="utf-8", newline="\n")
    return report_path, report_sha, summary_path, summary_sha


def run_gate(args: argparse.Namespace) -> int:
    global _ACTIVE_RUN_DIR, _ACTIVE_CONTEXT
    revision, tree, branch, session_name = require_clean_start()
    fixed_inputs = fixed_input_preflight()
    if fixed_inputs["artifact"]["rawFileSha256"] != EXPECTED_ARTIFACT_SHA256:
        raise R2HFailure("fixed Artifact SHA changed")
    if fixed_inputs["release"] != EXPECTED_RELEASE:
        raise R2HFailure("fixed Camoufox release changed")
    if fixed_inputs["browserTree"]["canonicalManifestSha256"] != EXPECTED_TREE_CANONICAL_SHA256:
        raise R2HFailure("fixed browser tree canonical SHA changed")
    for path in (R2H_SCHEMA, WINDOWS_HOST_TEST, ARTIFACT):
        if not path.is_file():
            raise R2HFailure(f"R2H fixed source is missing: {path}")
    uv = shutil.which("uv")
    if not uv:
        raise R2HFailure("uv is unavailable")
    python_path = locked_python(uv)
    run_id = args.run_id or f"run-{int(time.time())}-{uuid.uuid4().hex[:8]}"
    if not re.fullmatch(r"run-[0-9]+-[0-9a-f]{8}", run_id):
        raise R2HFailure("run-id must match run-<epoch>-<8 hex>")
    run_dir = R2H_RUNS_ROOT / run_id
    if run_dir.exists():
        raise R2HFailure(f"run-id already exists: {run_id}")
    run_dir.mkdir(parents=True, exist_ok=False)
    _ACTIVE_RUN_DIR = run_dir
    history = historical_failure_history()
    _ACTIVE_CONTEXT = {
        "schema": "verisilo-camoufox-m3-wi-r2h-windows-run-report/v1",
        "status": "failed",
        "runId": run_id,
        "startCheckpoint": START_CHECKPOINT,
        "receiptCommit": revision,
        "receiptTree": tree,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "branch": branch,
        "fixedInputs": fixed_inputs,
        "sourceBinding": source_binding(revision, tree),
        "failureHistory": history,
        "acceptanceSequence": {
            "expectedOrder": list(ACCEPTANCE_SEQUENCE),
            "attemptCount": 0,
            "passed": False,
            "selectiveRetry": False,
            "attempts": [],
            "selectionPolicy": "all-ten-required-stop-on-first-failure",
        },
        "fullHostMatrix": {},
        "validation": {},
        "security": {"targetProcessesBefore": [], "targetProcessesAfter": []},
        "boundaries": {
            "testOnly": True,
            "productionHostChanged": False,
            "productionLauncherChanged": False,
            "artifactChanged": False,
            "fixedBrowserTreeChanged": False,
            "runtimeManagerSoakRun": False,
            "fullM3WiRerun": False,
            "productionPackageVerified": False,
        },
        "environment": {
            "os": platform.platform(),
            "windowsRelease": platform.win32_ver()[0],
            "architecture": platform.machine(),
            "sessionName": session_name,
            "sessionId": windows_session()[0],
            "python": sys.version.split()[0],
            "uv": version([uv, "--version"]),
        },
    }
    try:
        sequence = run_acceptance_sequence(run_id, revision, tree, python_path, run_dir)
        _ACTIVE_CONTEXT["acceptanceSequence"].update(
            {"attemptCount": len(sequence), "passed": True, "attempts": sequence}
        )
        full_host = run_full_host_matrix(run_id, python_path, run_dir)
        _ACTIVE_CONTEXT["fullHostMatrix"] = full_host
        validation = run_validation_matrix(python_path)
        _ACTIVE_CONTEXT["validation"] = validation
        security = secret_process_validation(run_dir)
        security.update({"targetProcessesBefore": [], "targetProcessesAfter": []})
        _ACTIVE_CONTEXT["security"] = security
        if git("status", "--porcelain=v1") or target_processes():
            raise R2HFailure("R2H changed tracked files or left a target process")
        _ACTIVE_CONTEXT["status"] = "passed"
        report_path, report_sha, summary_path, summary_sha = write_receipts(
            run_dir, _ACTIVE_CONTEXT, status="passed"
        )
        print(f"m3-wi-r2h-run-id={run_id}")
        print(f"m3-wi-r2h-report={report_path}")
        print(f"m3-wi-r2h-report-sha256={report_sha}")
        print(f"m3-wi-r2h-summary={summary_path}")
        print(f"m3-wi-r2h-summary-sha256={summary_sha}")
        return 0
    except Exception as exc:  # noqa: BLE001 - one fail-closed acceptance run
        failure: dict[str, Any] = {
            "type": type(exc).__name__,
            "messageSha256": hashlib.sha256(str(exc).encode("utf-8")).hexdigest(),
        }
        if isinstance(exc, R2HFailure) and exc.receipt is not None:
            failure["receipt"] = exc.receipt
        _ACTIVE_CONTEXT["failure"] = failure
        _ACTIVE_CONTEXT["status"] = "failed"
        if isinstance(exc, R2HFailure) and exc.receipt is not None:
            _ACTIVE_CONTEXT["acceptanceSequence"]["failure"] = exc.receipt
        report_path, report_sha, summary_path, summary_sha = write_receipts(
            run_dir, _ACTIVE_CONTEXT, status="failed"
        )
        print(f"m3-wi-r2h-run-id={run_id}")
        print(f"m3-wi-r2h-failed-report={report_path}")
        print(f"m3-wi-r2h-failed-report-sha256={report_sha}")
        print(f"m3-wi-r2h-failed-summary={summary_path}")
        print(f"m3-wi-r2h-failed-summary-sha256={summary_sha}")
        print(f"M3-WI-R2H FAILED: {type(exc).__name__}", file=sys.stderr)
        return 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", help="optional unique run-<epoch>-<8 hex> identifier")
    args = parser.parse_args()
    return run_gate(args)


if __name__ == "__main__":
    raise SystemExit(main())
