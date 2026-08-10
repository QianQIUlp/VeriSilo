#!/usr/bin/env python3
"""Freeze one completed R2H run into an independent tracked manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from run_m3_wi_r2h_windows import (
    ACCEPTANCE_SEQUENCE,
    EXPECTED_BRANCH,
    OLD_R2_MANIFEST,
    OLD_R2_MANIFEST_RELATIVE,
    R2H_MANIFEST,
    R2H_ROOT,
    R2H_SCHEMA,
    START_CHECKPOINT,
    REPO_ROOT,
    git,
    sha256_file,
)


def file_receipt(path: Path, root: Path | None = None) -> dict[str, Any]:
    if not path.is_file():
        raise RuntimeError(f"receipt is missing: {path}")
    resolved = path.resolve()
    if root is not None and not resolved.is_relative_to(root.resolve()):
        raise RuntimeError(f"receipt escaped its run root: {path}")
    sidecar = path.with_suffix(".sha256")
    if not sidecar.is_file():
        raise RuntimeError(f"receipt sidecar is missing: {sidecar}")
    digest = sha256_file(path)
    sidecar_digest = sidecar.read_text(encoding="utf-8").split()[0]
    if sidecar_digest != digest:
        raise RuntimeError(f"receipt sidecar mismatch: {path}")
    return {
        "relativePath": path.relative_to(REPO_ROOT).as_posix(),
        "sha256": digest,
        "sidecarRelativePath": sidecar.relative_to(REPO_ROOT).as_posix(),
        "sidecarSha256": sha256_file(sidecar),
    }


def report_receipt(run_dir: Path, report: dict[str, Any]) -> dict[str, Any]:
    report_path = (REPO_ROOT / str(report["reportPath"])).resolve()
    summary_path = run_dir / "r2h-summary.json"
    result = {
        "report": file_receipt(report_path, run_dir),
        "summary": file_receipt(summary_path, run_dir),
    }
    return result


def validate_attempts(report: dict[str, Any]) -> list[dict[str, Any]]:
    sequence = report.get("acceptanceSequence", {})
    attempts = sequence.get("attempts")
    if (
        sequence.get("expectedOrder") != list(ACCEPTANCE_SEQUENCE)
        or sequence.get("attemptCount") != 10
        or sequence.get("passed") is not True
        or sequence.get("selectiveRetry") is not False
        or not isinstance(attempts, list)
        or len(attempts) != 10
    ):
        raise RuntimeError("R2H acceptance sequence is not exactly the declared 5+5 order")
    if [item.get("case") for item in attempts] != list(ACCEPTANCE_SEQUENCE):
        raise RuntimeError("R2H acceptance sequence order or count changed")
    receipts: list[dict[str, Any]] = []
    for index, item in enumerate(attempts, start=1):
        if item.get("index") != index or item.get("status") != "passed":
            raise RuntimeError(f"R2H acceptance attempt {index} was not passed")
        result = item.get("result")
        if not isinstance(result, dict) or result.get("status") != "passed":
            raise RuntimeError(f"R2H acceptance attempt {index} has no passed test receipt")
        leases = result.get("leaseReceipts")
        if not isinstance(leases, list) or not leases:
            raise RuntimeError(f"R2H acceptance attempt {index} lacks lock receipts")
        if any(
            lease.get("status") != "released"
            or lease.get("lock", {}).get("profileByteAvailable") is not True
            or lease.get("lock", {}).get("supervisorByteAvailable") is not True
            for lease in leases
        ):
            raise RuntimeError(f"R2H acceptance attempt {index} lacks an observed released lock")
        diagnostics = result.get("transportDiagnostics")
        if not isinstance(diagnostics, list) or not diagnostics:
            raise RuntimeError(f"R2H acceptance attempt {index} lacks transport diagnostics")
        if any(entry.get("stderrSecretFree") is not True for entry in diagnostics):
            raise RuntimeError(f"R2H acceptance attempt {index} is not stderr-secret-free")
        if item.get("command", {}).get("exitCode") != 0:
            raise RuntimeError(f"R2H acceptance command {index} was not successful")
        receipts.append(
            {
                "index": index,
                "case": item["case"],
                "runId": item.get("runId"),
                "summary": item.get("summary"),
                "report": item.get("report"),
                "command": item.get("command"),
                "leaseReceipts": leases,
                "transportDiagnostics": diagnostics,
                "observedWebsiteDigest": result.get("observedWebsiteDigest"),
                "cookieSqlite": result.get("cookieSqlite"),
                "crashState": result.get("crashState"),
                "relaunch": result.get("relaunch"),
            }
        )
    return receipts


def freeze(run_id: str) -> None:
    if R2H_MANIFEST.exists():
        raise RuntimeError(f"R2H manifest already exists: {R2H_MANIFEST}")
    if not R2H_SCHEMA.is_file() or not OLD_R2_MANIFEST.is_file():
        raise RuntimeError("R2H schema or preserved R2 manifest is missing")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before freezing R2H evidence")
    run_dir = R2H_ROOT / "runs" / run_id
    report_path = run_dir / "r2h-report.json"
    summary_path = run_dir / "r2h-summary.json"
    report_sidecar = run_dir / "r2h-report.json.sha256"
    summary_sidecar = run_dir / "r2h-summary.json.sha256"
    for path in (report_path, summary_path, report_sidecar, summary_sidecar):
        if not path.is_file():
            raise RuntimeError(f"R2H receipt is missing: {path}")
    report = json.loads(report_path.read_text(encoding="utf-8"))
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
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
        or branch != EXPECTED_BRANCH
        or summary.get("reportSha256") != sha256_file(report_path)
        or summary.get("status") != "passed"
    ):
        raise RuntimeError("R2H report/summary does not bind the clean receipt commit")
    if report.get("startCheckpoint") != START_CHECKPOINT:
        raise RuntimeError("R2H start checkpoint changed")
    for path in (
        "apps/camoufox-host/host_v1.py",
        "apps/desktop/src-tauri/src/launcher.rs",
        "apps/desktop/src-tauri/src/engine.rs",
        OLD_R2_MANIFEST_RELATIVE,
    ):
        changed = subprocess.run(
            ["git", "diff", "--quiet", f"{START_CHECKPOINT}..{revision}", "--", path],
            cwd=REPO_ROOT,
            check=False,
        )
        if changed.returncode != 0:
            raise RuntimeError(f"R2H changed a forbidden path: {path}")
    history = report.get("failureHistory", {})
    if (
        history.get("attemptCount") != 6
        or history.get("passedAttemptCount") != 1
        or history.get("failedAttemptCount") != 5
        or history.get("selectedHistoricalPass") is not False
        or history.get("selectiveRetry") is not True
        or history.get("acceptanceSequenceSelectiveRetry") is not False
    ):
        raise RuntimeError("R2H failure history or selectiveRetry semantics are inconsistent")
    acceptance = validate_attempts(report)
    full = report.get("fullHostMatrix", {})
    full_receipt = full.get("receipt", {})
    if (
        full.get("status") != "passed"
        or full.get("emptyCacheInitiallyAbsent") is not True
        or full_receipt.get("testCount") != 10
        or full_receipt.get("passed") != 10
        or full.get("command", {}).get("exitCode") != 0
    ):
        raise RuntimeError("R2H final fresh empty-cache Host matrix is not 10/10")
    validation = report.get("validation", {})
    commands = validation.get("commands")
    if validation.get("status") != "passed" or not isinstance(commands, list) or any(
        item.get("exitCode") != 0 for item in commands
    ):
        raise RuntimeError("R2H validation matrix is incomplete")
    security = report.get("security", {})
    if security.get("secretMatches") != [] or security.get("targetProcessesAfter") != []:
        raise RuntimeError("R2H secret/process validation failed closed")
    source = report.get("sourceBinding", {})
    if source.get("revision") != revision or source.get("tree") != tree:
        raise RuntimeError("R2H source binding changed")
    for key, path in {
        "hostBlob": "apps/camoufox-host/host_v1.py",
        "hostTestBlob": "apps/camoufox-host/test_windows_host.py",
        "runnerBlob": "apps/camoufox-host/run_m3_wi_r2h_windows.py",
        "schemaBlob": "tests/fixtures/camoufox/evidence-manifest-m3-wi-r2h-windows.schema.json",
    }.items():
        if source.get(key) != git("rev-parse", f"{revision}:{path}"):
            raise RuntimeError(f"R2H source blob binding changed: {key}")
    report_files = report_receipt(run_dir, report)
    manifest = {
        "schema": "verisilo-camoufox-m3-wi-r2h-windows-evidence-manifest/v1",
        "generatedAtUtc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "status": "execution-passed-awaiting-main-brain-gate",
        "runId": run_id,
        "startCheckpoint": START_CHECKPOINT,
        "receiptCommit": revision,
        "receiptTree": tree,
        "codeGitRevision": revision,
        "codeTreeHash": tree,
        "branch": branch,
        "integrationPath": "test-only-real-host-matrix",
        "productionPackageVerified": False,
        "productionVerifierFailClosed": True,
        "shipped": False,
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
        "fixedInputs": report["fixedInputs"],
        "sourceBinding": source,
        "receipts": {
            "report": report_files["report"],
            "summary": report_files["summary"],
            "acceptanceAttempts": [
                {"index": item["index"], "case": item["case"], "summary": item["summary"], "report": item["report"]}
                for item in acceptance
            ],
            "fullHost": full,
            "preservedR2Manifest": history["sourceManifest"],
        },
        "acceptanceSequence": {
            "expectedOrder": list(ACCEPTANCE_SEQUENCE),
            "attemptCount": 10,
            "passed": True,
            "selectiveRetry": False,
            "selectionPolicy": "all-ten-required-stop-on-first-failure",
            "attempts": acceptance,
        },
        "fullHostMatrix": full,
        "validation": validation,
        "security": security,
        "failureHistory": history,
        "boundaries": report["boundaries"],
    }
    R2H_MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n")
    print(f"m3-wi-r2h-manifest={R2H_MANIFEST}")
    print(f"m3-wi-r2h-manifest-sha256={sha256_file(R2H_MANIFEST)}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    freeze(args.run_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
