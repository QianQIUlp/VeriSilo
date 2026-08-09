#!/usr/bin/env python3
"""Freeze one completed M3-WI-R1 Windows run into a sanitized manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from run_m3_wi_r1_windows import (
    EXPECTED_RELEASE,
    REPO_ROOT,
    R1_SCHEMA,
    RUNS_ROOT,
    START_CHECKPOINT,
    validate_r1_runtime,
)


OUTPUT = REPO_ROOT / "tests/fixtures/camoufox/evidence-manifest-m3-wi-r1-windows.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def git(*args: str) -> str:
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
    if completed.returncode != 0:
        raise RuntimeError(completed.stderr.strip() or f"git {' '.join(args)} failed")
    return completed.stdout.strip()


def sidecar_receipt(path: Path, sidecar: Path, run_dir: Path) -> dict[str, Any]:
    expected = sidecar.read_text(encoding="utf-8").split()[0]
    actual = sha256_file(path)
    if expected != actual:
        raise RuntimeError(f"sidecar mismatch for {path}")
    if not path.resolve().is_relative_to(run_dir.resolve()):
        raise RuntimeError(f"receipt escaped run root: {path}")
    return {
        "relativePath": path.relative_to(REPO_ROOT).as_posix(),
        "sha256": actual,
        "sidecarRelativePath": sidecar.relative_to(REPO_ROOT).as_posix(),
        "sidecarSha256": sha256_file(sidecar),
    }


def file_receipt(path: Path) -> dict[str, str]:
    if not path.is_file():
        raise RuntimeError(f"receipt is missing: {path}")
    return {
        "relativePath": path.relative_to(REPO_ROOT).as_posix(),
        "sha256": sha256_file(path),
    }


def compact_cycle(cycle: dict[str, Any], receipt: dict[str, Any]) -> dict[str, Any]:
    running = cycle["running"]
    return {
        "cycle": cycle["cycle"],
        "receipt": receipt,
        "profileId": cycle["profileId"],
        "sessionId": cycle["sessionId"],
        "hostPid": cycle["hostPid"],
        "managedPids": cycle["managedPids"],
        "bootCountBefore": cycle["bootCountBefore"],
        "bootCountAfter": cycle["bootCountAfter"],
        "observedWebsiteDigest": cycle["observedWebsiteDigest"],
        "launchSurface": running["launchSurface"],
        "cookieEvidence": cycle["cookieApi"],
        "pageCookie": cycle["pageCookie"],
        "cookieSqlite": cycle["sqlite"],
        "close": cycle["close"],
        "stageDiagnostics": cycle["stageDiagnostics"],
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }


def freeze(run_id: str) -> None:
    if OUTPUT.exists():
        raise RuntimeError(f"R1 manifest already exists: {OUTPUT}")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before freezing R1 evidence")
    run_dir = RUNS_ROOT / run_id
    report_path = run_dir / "r1-report.json"
    report_sidecar = run_dir / "r1-report.sha256"
    summary_path = run_dir / "r1-summary.json"
    summary_sidecar = run_dir / "r1-summary.sha256"
    runtime_path = run_dir / "r1-runtime-evidence.json"
    for path in [report_path, report_sidecar, summary_path, summary_sidecar, runtime_path, R1_SCHEMA]:
        if not path.is_file():
            raise RuntimeError(f"required R1 receipt is missing: {path}")
    report_receipt = sidecar_receipt(report_path, report_sidecar, run_dir)
    summary_receipt = sidecar_receipt(summary_path, summary_sidecar, run_dir)
    report = json.loads(report_path.read_text(encoding="utf-8"))
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    runtime = json.loads(runtime_path.read_text(encoding="utf-8"))

    revision = git("rev-parse", "HEAD")
    tree = git("show", "-s", "--format=%T", revision)
    branch = git("branch", "--show-current")
    if (
        report.get("status") != "passed"
        or report.get("runId") != run_id
        or report.get("receiptCommit") != revision
        or report.get("receiptTree") != tree
        or report.get("codeGitRevision") != revision
        or report.get("codeTreeHash") != tree
        or report.get("branch") != branch
        or summary.get("reportSha256") != report_receipt["sha256"]
        or summary.get("codeGitRevision") != revision
        or summary.get("codeTreeHash") != tree
        or runtime.get("codeGitRevision") != revision
        or runtime.get("codeTreeHash") != tree
    ):
        raise RuntimeError("R1 report/summary/runtime does not bind the current clean receipt commit")
    if (
        report.get("integrationPath") != "test-only-real-host"
        or report.get("productionPackageVerified") is not False
        or report.get("shipped") is not False
        or report.get("verified") is not False
        or report.get("evidenceClass") != "observed-on-this-windows-host"
        or report.get("fixedInputs", {}).get("release") != EXPECTED_RELEASE
        or report.get("failureHistory", {}).get("m3WiFullGateRerun") is not False
        or report.get("failureHistory", {}).get("selectiveRetry") is not False
    ):
        raise RuntimeError("R1 report crossed the frozen execution boundary")
    validate_r1_runtime(runtime, revision, tree)
    commands = report.get("commands")
    if not isinstance(commands, list) or any(item.get("exitCode") != 0 for item in commands):
        raise RuntimeError("one or more R1 validation commands failed")
    sqlite = report.get("sqliteRegression", {})
    if (
        sqlite.get("status") != "passed"
        or sqlite.get("pathCompatibility", {}).get("normalAndVerbatimReadSameCookie") is not True
        or sqlite.get("pathCompatibility", {}).get("readOnly") is not True
        or sqlite.get("pathCompatibility", {}).get("fileBytesUnchanged") is not True
    ):
        raise RuntimeError("R1 SQLite URI receipt is incomplete")
    host = report.get("windowsHostRegression", {})
    if host.get("testCount") != 10 or host.get("passed") != 10 or host.get("emptyCacheInitiallyAbsent") is not True:
        raise RuntimeError("R1 Windows Host receipt is not fresh 10/10")
    security = report.get("secretProcessValidation", {})
    if security.get("secretMatches") != [] or security.get("targetProcessesAfterRuntime") != []:
        raise RuntimeError("R1 secret/process receipt failed closed")

    cycle_receipts = []
    for item in report.get("cycleReceipts", []):
        path = (REPO_ROOT / item["relativePath"]).resolve()
        sidecar = (REPO_ROOT / item["sidecarRelativePath"]).resolve()
        actual = sidecar_receipt(path, sidecar, run_dir)
        if actual["sha256"] != item["sha256"]:
            raise RuntimeError(f"R1 cycle receipt SHA changed: {path}")
        actual["cycle"] = int(item["cycle"])
        cycle_receipts.append(actual)
    if len(cycle_receipts) != 5:
        raise RuntimeError("R1 evidence must bind five cycle receipt/SHA pairs")

    compact_cycles = [
        compact_cycle(cycle, receipt)
        for cycle, receipt in zip(runtime["cycles"], sorted(cycle_receipts, key=lambda item: item["cycle"]))
    ]
    manifest = {
        "schema": "verisilo-camoufox-m3-wi-r1-windows-evidence-manifest/v1",
        "generatedAtUtc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "status": "execution-passed-awaiting-main-brain-gate",
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
        "fixedInputs": report["fixedInputs"],
        "receipts": {
            "report": report_receipt,
            "summary": summary_receipt,
            "runtime": file_receipt(runtime_path),
            "cycles": cycle_receipts,
        },
        "r1Runtime": {
            "cycleCount": 5,
            "sameProfile": True,
            "observedWebsiteDigestStable": True,
            "cycles": compact_cycles,
        },
        "validation": {
            "commands": [
                {
                    "label": item.get("label"),
                    "exitCode": item.get("exitCode"),
                    "counts": item.get("counts", {}),
                    "outputSha256": item.get("outputSha256"),
                }
                for item in commands
            ],
            "sqlite": sqlite,
            "windowsHost": host,
            "schema": report.get("schemaValidation", {}),
        },
        "security": {
            "secretMatches": [],
            "targetProcessesBefore": [],
            "targetProcessesAfter": [],
            "aliveOwnedPids": [],
            "directOnly": True,
            "productionPackageVerified": False,
        },
        "failureHistory": report["failureHistory"],
        "boundaries": report["boundaries"],
    }
    OUTPUT.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print(f"m3-wi-r1-manifest={OUTPUT}")
    print(f"m3-wi-r1-manifest-sha256={sha256_file(OUTPUT)}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    freeze(args.run_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
