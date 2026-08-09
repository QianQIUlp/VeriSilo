#!/usr/bin/env python3
"""Freeze one completed M3-WI native-Windows report into a sanitized manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parents[1]
RUNS_ROOT = REPO_ROOT / "artifacts" / "camoufox-m3-wi-windows-gate" / "runs"
OUTPUT = REPO_ROOT / "tests" / "fixtures" / "camoufox" / "evidence-manifest-m3-wi-windows.json"
SCHEMA_PATH = (
    REPO_ROOT
    / "tests"
    / "fixtures"
    / "camoufox"
    / "evidence-manifest-m3-wi-windows.schema.json"
)


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


def verify_sidecar(path: Path, sidecar: Path) -> str:
    expected = sidecar.read_text(encoding="utf-8").split()[0]
    actual = sha256_file(path)
    if expected != actual:
        raise RuntimeError(f"sidecar mismatch for {path.name}")
    return actual


def value_at(value: dict[str, Any], pointer: str) -> Any:
    current: Any = value
    for part in pointer.strip("/").split("/"):
        if not isinstance(current, dict) or part not in current:
            raise RuntimeError(f"report is missing {pointer}")
        current = current[part]
    return current


def cycle_receipt(cycle: dict[str, Any], close: dict[str, Any]) -> dict[str, Any]:
    session = cycle["session"]
    return {
        "sessionId": cycle["sessionId"],
        "hostPid": cycle["hostPid"],
        "managedPids": session["managedPids"],
        "jobName": session["supervisorMeta"]["jobName"],
        "artifactId": cycle["artifactId"],
        "artifactFileSha256": cycle["artifactFileSha256"],
        "profileId": cycle["profileId"],
        "bootCountBefore": session["bootCountBefore"],
        "bootCountAfter": session["bootCountAfter"],
        "observedWebsiteDigest": cycle["observedWebsiteDigest"],
        "mediaDevicesMatched": cycle["observed"]["mediaDeviceReadiness"]["matched"],
        "cookieEvidence": session["cookieEvidence"],
        "processTreeExited": close["processTreeExit"]["exited"],
        "jobActiveProcessCount": close["processTreeExit"]["job"]["activeProcessCount"],
        "cookieSqlite": close["cookieSqlite"],
        "verified": False,
        "evidenceClass": "observed-on-this-windows-host",
    }


def freeze(run_id: str) -> None:
    if OUTPUT.exists():
        raise RuntimeError(f"tracked M3-WI manifest already exists: {OUTPUT}")
    if git("status", "--porcelain=v1"):
        raise RuntimeError("worktree must be clean before freezing evidence")
    run_dir = RUNS_ROOT / run_id
    report_path = run_dir / "report.json"
    report_sidecar = run_dir / "report.sha256"
    summary_path = run_dir / "summary.json"
    summary_sidecar = run_dir / "summary.sha256"
    runtime_path = run_dir / "runtime-evidence.json"
    for path in [
        report_path,
        report_sidecar,
        summary_path,
        summary_sidecar,
        runtime_path,
        SCHEMA_PATH,
    ]:
        if not path.is_file():
            raise RuntimeError(f"required receipt is missing: {path}")
    report_sha = verify_sidecar(report_path, report_sidecar)
    summary_sha = verify_sidecar(summary_path, summary_sidecar)
    report = json.loads(report_path.read_text(encoding="utf-8"))
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    runtime = json.loads(runtime_path.read_text(encoding="utf-8"))
    json.loads(SCHEMA_PATH.read_text(encoding="utf-8"))

    revision = git("rev-parse", "HEAD")
    tree = git("show", "-s", "--format=%T", revision)
    branch = git("branch", "--show-current")
    if any(
        value != expected
        for value, expected in [
            (report.get("status"), "passed"),
            (report.get("runId"), run_id),
            (report.get("codeGitRevision"), revision),
            (report.get("codeTreeHash"), tree),
            (report.get("branch"), branch),
            (summary.get("reportSha256"), report_sha),
            (runtime.get("codeGitRevision"), revision),
            (runtime.get("codeTreeHash"), tree),
        ]
    ):
        raise RuntimeError("report/summary/runtime receipt does not bind the current code revision")
    for source in [report, summary, runtime]:
        if (
            source.get("integrationPath") != "test-only-real-host"
            or source.get("productionPackageVerified") is not False
            or source.get("shipped") is not False
            or source.get("verified") is not False
            or source.get("evidenceClass") != "observed-on-this-windows-host"
        ):
            raise RuntimeError("receipt crossed the frozen test-only/evidence boundary")
    gates = report.get("gateMatrix")
    if (
        not isinstance(gates, list)
        or [gate.get("id") for gate in gates] != list(range(1, 17))
        or any(gate.get("status") != "passed" for gate in gates)
    ):
        raise RuntimeError("report does not contain a passed 1-16 Gate matrix")
    commands = report.get("commands")
    if not isinstance(commands, list) or any(item.get("exitCode") != 0 for item in commands):
        raise RuntimeError("one or more required validation commands did not pass")
    if (
        report.get("productionVerifierFailClosed") is not True
        or report.get("productionVerifier", {}).get("trustedSignerCount") != 0
        or runtime.get("secretScan", {}).get("matches") != []
        or runtime.get("residualProcessCheck", {}).get("aliveOwnedPids") != []
        or report.get("residualProcessCheck", {}).get("targetProcessesAfter") != []
        or report.get("downloadGuard", {}).get("webdlAttemptObserved") is not False
        or report.get("protectedAcceptedFiles", {}).get("unchangedFromStartCheckpoint") is not True
    ):
        raise RuntimeError("security/residual/download/protected-file receipt failed closed")
    semantic_boundary = runtime.get("semanticBoundary", {})
    if (
        semantic_boundary.get("launchExecutable")
        != "uv-resolved-locked-python-interpreter"
        or semantic_boundary.get("hostEntrypoint") != "apps/camoufox-host/host_v1.py"
        or semantic_boundary.get("typedHostArgvRecorded") is not True
        or semantic_boundary.get("argvContainsProxyArguments") is not False
        or semantic_boundary.get("argvContainsSecrets") is not False
        or semantic_boundary.get("hostLaunch") != "observed"
        or semantic_boundary.get("bootstrapDelivery") != "not_applicable"
        or semantic_boundary.get("runtimeReceipts") != "not_applicable"
        or semantic_boundary.get("verifiedAdapter") is not None
        or semantic_boundary.get("productionPackageVerified") is not False
    ):
        raise RuntimeError("runtime plan/argv/evidence boundary is incomplete or overstated")

    persistence = runtime["persistence"]
    cycle1 = cycle_receipt(persistence["cycle1"], persistence["cycle1Close"])
    cycle2 = cycle_receipt(persistence["cycle2"], persistence["cycle2Close"])
    if (
        [cycle1["bootCountAfter"], cycle2["bootCountAfter"]] != [1, 2]
        or cycle1["hostPid"] == cycle2["hostPid"]
        or cycle1["observedWebsiteDigest"] != cycle2["observedWebsiteDigest"]
        or cycle2["cookieEvidence"].get("cookieInApi") is not True
        or cycle2["cookieEvidence"].get("cookieOnPage") is not True
        or cycle2["cookieSqlite"].get("cookieNamePresent") is not True
    ):
        raise RuntimeError("persistence receipt does not prove the frozen 1->2 continuity")

    manifest = {
        "schema": "verisilo-camoufox-m3-wi-windows-evidence-manifest/v1",
        "generatedAtUtc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "status": "execution-passed-awaiting-main-brain-gate",
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
        "executionPlatform": report["environment"],
        "fixedInputs": report["fixedInputs"],
        "receipts": {
            "report": {
                "relativePath": report_path.relative_to(REPO_ROOT).as_posix(),
                "sha256": report_sha,
                "sidecarRelativePath": report_sidecar.relative_to(REPO_ROOT).as_posix(),
                "sidecarSha256": sha256_file(report_sidecar),
            },
            "summary": {
                "relativePath": summary_path.relative_to(REPO_ROOT).as_posix(),
                "sha256": summary_sha,
                "sidecarRelativePath": summary_sidecar.relative_to(REPO_ROOT).as_posix(),
                "sidecarSha256": sha256_file(summary_sidecar),
            },
            "runtime": {
                "relativePath": runtime_path.relative_to(REPO_ROOT).as_posix(),
                "sha256": sha256_file(runtime_path),
            },
        },
        "realRuns": {
            "persistenceCycle1": cycle1,
            "persistenceCycle2": cycle2,
            "activeEof": {
                "sessionId": value_at(runtime, "/activeEof/running/sessionId"),
                "hostPid": value_at(runtime, "/activeEof/running/hostPid"),
                "desktopState": value_at(runtime, "/activeEof/desktopState/state"),
                "profileLeaseRetained": value_at(runtime, "/activeEof/profileLeaseRetained"),
                "processTreeExited": value_at(
                    runtime, "/activeEof/closedSession/processTreeExit/exited"
                ),
                "jobActiveProcessCount": value_at(
                    runtime,
                    "/activeEof/closedSession/processTreeExit/job/activeProcessCount",
                ),
            },
            "hostCrash": {
                "sessionId": value_at(runtime, "/hostCrash/running/sessionId"),
                "hostPid": value_at(runtime, "/hostCrash/running/hostPid"),
                "desktopState": value_at(runtime, "/hostCrash/desktopState/state"),
                "profileLeaseRetained": value_at(runtime, "/hostCrash/profileLeaseRetained"),
                "ownedPidsDead": value_at(runtime, "/hostCrash/ownedPidsDead"),
            },
            "desktopDrop": {
                "sessionId": value_at(runtime, "/desktopDrop/running/sessionId"),
                "hostPid": value_at(runtime, "/desktopDrop/running/hostPid"),
                "recoveredState": value_at(runtime, "/desktopDrop/recoveredState/state"),
                "ownedPidsDead": value_at(runtime, "/desktopDrop/ownedPidsDead"),
            },
            "negativeMatrix": runtime["negativeMatrix"],
        },
        "gateMatrix": gates,
        "validation": commands,
        "security": {
            "secretPatternsChecked": runtime["secretScan"]["patternsChecked"],
            "secretMatches": [],
            "unrelatedSentinelSurvived": runtime["unrelatedSentinel"][
                "survivedAllLifecycleOperations"
            ],
            "ownedPidsAliveAfter": [],
            "targetProcessesAfter": [],
            "downloadGuardAttemptObserved": False,
        },
        "protectedAcceptedFiles": report["protectedAcceptedFiles"],
        "boundaries": [
            "No trusted signer pin or signed Host package exists.",
            "The integration-only adapter is absent from production builds.",
            "The test-only plan launches the uv-resolved locked Python interpreter as the exact Host child and records its typed host_v1.py argv.",
            "No UI, installer, proxy injection, site fallback, Controlled Chromium, or virtualization backend was opened.",
            "Evidence is observed on this Windows host; verifiedAdapter remains null and verified remains false.",
        ],
    }
    encoded = (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    lowered = encoded.decode("utf-8").lower()
    if "c:\\users\\" in lowered or "c:/users/" in lowered:
        raise RuntimeError("sanitized manifest contains an absolute user path")
    OUTPUT.write_bytes(encoded)
    print(f"m3-wi-manifest={OUTPUT}")
    print(f"m3-wi-manifest-sha256={hashlib.sha256(encoded).hexdigest()}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-id", required=True)
    args = parser.parse_args()
    try:
        freeze(args.run_id)
        return 0
    except Exception as exc:  # noqa: BLE001 - one bounded freezer failure
        print(f"M3-WI FREEZE FAILED: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
