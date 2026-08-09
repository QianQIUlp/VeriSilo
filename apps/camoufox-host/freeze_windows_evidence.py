#!/usr/bin/env python3
"""Freeze one post-sync M2-W run set into the tracked receipt manifest.

All dynamic run references are derived from immutable report files and their
sidecars. The script refuses failed, mismatched, warm-only, or uncontrolled
cache receipts instead of allowing the manifest to be hand-reconciled.
"""

from __future__ import annotations

import argparse
from datetime import datetime
import hashlib
import json
import subprocess
from pathlib import Path

from identity_policy import STABLE_WEBSITE_SIGNAL_KEYS

REPO_ROOT = Path(__file__).resolve().parents[2]
MANIFEST = REPO_ROOT / "tests" / "fixtures" / "camoufox" / "evidence-manifest-windows.json"
REQUIRED_HOST_RESULTS = {
    "protocol",
    "persistence",
    "lock-crash",
    "quarantine",
    "eof-force-exit",
    "tamper",
    "reparse",
    "mount-point",
    "tree-integrity",
    "pid-reuse",
}


def load_json(path: Path) -> dict:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError(f"JSON root must be an object: {path}")
    return value


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def relative(path: Path) -> str:
    return path.resolve().relative_to(REPO_ROOT.resolve()).as_posix()


def verify_sidecar(path: Path, sidecar: Path, expected_name: str) -> str:
    if not path.is_file() or not sidecar.is_file():
        raise RuntimeError(f"missing receipt or sidecar: {path}, {sidecar}")
    parts = sidecar.read_text(encoding="utf-8").split()
    actual = sha256_file(path)
    if len(parts) != 2 or parts[0] != actual or parts[1] != expected_name:
        raise RuntimeError(f"sidecar mismatch: {sidecar}")
    return actual


def identity_receipt(path: Path, command: str) -> tuple[dict, list[str]]:
    report = load_json(path)
    digest = verify_sidecar(path, path.parent / "report.sha256", "report.json")
    if report.get("command") != command or report.get("accepted") is not True:
        raise RuntimeError(f"{command} report is not accepted: {path}")
    if report.get("runId") != path.parent.name:
        raise RuntimeError(f"runId/path mismatch: {path}")
    return report, [report["runId"], relative(path), digest]


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args],
        cwd=REPO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def observed_signal_changes(report: dict) -> list[str]:
    starts = report.get("coldStarts", [])
    if len(starts) < 2:
        raise RuntimeError("pre-sync transition report has fewer than two starts")
    fields = set().union(
        *(set(start.get("observedWebsiteSignals", {})) for start in starts)
    )
    return sorted(
        field
        for field in fields
        if len(
            {
                json.dumps(
                    start.get("observedWebsiteSignals", {}).get(field),
                    sort_keys=True,
                    separators=(",", ":"),
                )
                for start in starts
            }
        )
        > 1
    )


def parse_time(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--summary", type=Path, required=True)
    parser.add_argument("--stability", type=Path, required=True)
    parser.add_argument("--separation", type=Path, required=True)
    parser.add_argument("--artifact-tamper", type=Path, required=True)
    args = parser.parse_args()

    if git("status", "--short"):
        raise RuntimeError("freeze requires a clean tracked worktree at the code revision")

    manifest = load_json(MANIFEST)
    if "preSyncEvidence" not in manifest:
        manifest["preSyncEvidence"] = {
            "classification": "retained-not-accepted-after-baseline-sync",
            "codeGitRevision": manifest.get("codeGitRevision"),
            "codeTreeHash": manifest.get("codeTreeHash"),
            "gateRunId": manifest.get("gateRunId"),
            "evidenceBundle": manifest.get("evidenceBundle"),
            "runs": manifest.get("runs"),
            "runReports": manifest.get("runReports"),
        }

    summary_path = args.summary.resolve()
    summary = load_json(summary_path)
    summary_sha = verify_sidecar(
        summary_path, summary_path.with_suffix(".sha256"), summary_path.name
    )
    if summary.get("status") != "passed":
        raise RuntimeError("Windows Host driver summary did not pass")
    results = summary.get("results", {})
    if set(results) != REQUIRED_HOST_RESULTS:
        raise RuntimeError(
            "Host driver result set mismatch: "
            f"expected={sorted(REQUIRED_HOST_RESULTS)} actual={sorted(results)}"
        )

    host_receipts: dict[str, list[str]] = {}
    for name, result in results.items():
        if result.get("status") != "passed":
            raise RuntimeError(f"Host driver result did not pass: {name}")
        report_path = REPO_ROOT / result["reportFile"]
        report_sha = verify_sidecar(
            report_path, report_path.parent / "report.sha256", "report.json"
        )
        report = load_json(report_path)
        if (
            report.get("runId") != result.get("runId")
            or report_sha != result.get("reportSha256")
        ):
            raise RuntimeError(f"summary/report mismatch: {name}")
        host_receipts[name] = [result["runId"], relative(report_path), report_sha]

    stability, stability_receipt = identity_receipt(args.stability.resolve(), "stability")
    separation, separation_receipt = identity_receipt(
        args.separation.resolve(), "separation"
    )
    artifact_tamper, artifact_tamper_receipt = identity_receipt(
        args.artifact_tamper.resolve(), "tamper"
    )

    stability_result = stability["stability"]
    cache = stability["run"].get("controlledCache", {})
    starts = stability.get("coldStarts", [])
    if not (
        cache.get("controlled") is True
        and cache.get("wasEmptyBeforeSeed") is True
        and cache.get("seededFromVerifiedArchive") is True
        and stability_result.get("requestedRuns") == 5
        and stability_result.get("completedStarts") == 5
        and stability_result.get("allObservedWebsiteDigestsIdentical") is True
        and all(stability_result.get("mediaDevicesMatchConfiguredEveryStart", []))
        and len(starts) == 5
        and all(start.get("profilePreExisted") is False for start in starts)
        and all(start.get("webdlTripped") is False for start in starts)
    ):
        raise RuntimeError("stability receipt is not a controlled fresh-cache 5-start run")

    pre_sync_receipts = manifest["preSyncEvidence"]["runReports"]
    transition_entry = pre_sync_receipts.get("freshCacheGeneration")
    if not transition_entry:
        raise RuntimeError("pre-sync transition receipt is unavailable")
    transition_path = REPO_ROOT / transition_entry[1]
    transition = load_json(transition_path)
    if sha256_file(transition_path) != transition_entry[2]:
        raise RuntimeError("pre-sync transition receipt hash mismatch")
    changed_fields = observed_signal_changes(transition)
    if changed_fields != ["mediaDevices"]:
        raise RuntimeError(f"unexpected pre-sync transition fields: {changed_fields}")
    transition_starts = transition["coldStarts"]
    if not (
        len({start["artifactFileSha256"] for start in transition_starts}) == 1
        and all(start["configUnchanged"] for start in transition_starts)
        and all(start["profilePreExisted"] is False for start in transition_starts)
    ):
        raise RuntimeError("pre-sync transition was not isolated from artifact/config/profile")

    revision = git("rev-parse", "HEAD")
    tree = git("rev-parse", "HEAD^{tree}")
    summary_id = summary_path.stem
    updated = max(
        [summary["generatedAtUtc"]]
        + [
            stability["generatedAtUtc"],
            separation["generatedAtUtc"],
            artifact_tamper["generatedAtUtc"],
        ],
        key=parse_time,
    )

    report_name_map = {
        "protocol": "protocol",
        "persistence": "persistence",
        "lockCrash": "lock-crash",
        "quarantine": "quarantine",
        "lifecycle": "eof-force-exit",
        "tamper": "tamper",
        "reparse": "reparse",
        "mountPoint": "mount-point",
        "treeIntegrity": "tree-integrity",
        "pidReuse": "pid-reuse",
    }
    run_reports = {
        manifest_name: host_receipts[summary_name]
        for manifest_name, summary_name in report_name_map.items()
    }
    run_reports.update(
        {
            "stability": stability_receipt,
            "freshCacheStability": stability_receipt,
            "separation": separation_receipt,
            "artifactTamper": artifact_tamper_receipt,
        }
    )
    runs = {
        "hostDriverRunId": summary_id,
        "protocolRunId": results["protocol"]["runId"],
        "persistenceRunId": results["persistence"]["runId"],
        "profileLockAndCrashRunId": results["lock-crash"]["runId"],
        "quarantineRunId": results["quarantine"]["runId"],
        "lifecycleRunId": results["eof-force-exit"]["runId"],
        "tamperRunId": results["tamper"]["runId"],
        "reparseRunId": results["reparse"]["runId"],
        "mountPointRunId": results["mount-point"]["runId"],
        "treeIntegrityRunId": results["tree-integrity"]["runId"],
        "pidReuseRunId": results["pid-reuse"]["runId"],
        "stabilityRunId": stability["runId"],
        "freshCacheStabilityRunId": stability["runId"],
        "separationRunId": separation["runId"],
        "artifactTamperRunId": artifact_tamper["runId"],
    }

    manifest.update(
        {
            "updatedAtUtc": updated,
            "codeGitRevision": revision,
            "codeTreeHash": tree,
            "gateRunId": f"gate-{summary_id.removeprefix('summary-')}-{stability['runId']}",
            "evidenceBundle": {
                "summaryFile": relative(summary_path),
                "summarySidecar": relative(summary_path.with_suffix(".sha256")),
                "summarySha256": summary_sha,
                "reportsAreHostLocal": True,
                "reportsAreGitIgnored": True,
            },
            "runs": runs,
            "runReports": run_reports,
        }
    )

    persistence = results["persistence"]
    manifest["persistence"] = {
        "status": "passed",
        "runId": persistence["runId"],
        "bootCount": persistence["bootCounts"],
        "fixedProbeOrigin": True,
        "cookieApi": True,
        "documentCookie": True,
        "cookiesSqlite": persistence["cookieSqlite"]["fileExists"],
        "localStorageAcrossHostProcesses": True,
        "localStorageOrigin": "http://127.0.0.1:<fixed-probe-port>/probe.html",
    }
    digests = stability_result["observedWebsiteDigests"]
    manifest["stability"] = {
        "status": "passed",
        "runId": stability["runId"],
        "freshControlledCache": cache,
        "requestedRuns": stability_result["requestedRuns"],
        "completedRuns": stability_result["completedStarts"],
        "observedWebsiteDigest": stability_result["stableObservedWebsiteDigest"],
        "observedWebsiteDigests": digests,
        "allObservedWebsiteDigestsIdentical": stability_result[
            "allObservedWebsiteDigestsIdentical"
        ],
        "configUnchangedEveryStart": stability_result["configUnchangedEveryStart"],
        "mediaDevicesMatchConfiguredEveryStart": stability_result[
            "mediaDevicesMatchConfiguredEveryStart"
        ],
        "diskReloadedEveryStart": stability_result["diskReloadedEveryStart"],
        "profileFreshEveryStart": stability_result["profileFreshEveryStart"],
        "artifactFileSha256EveryStart": stability_result[
            "artifactFileSha256EveryStart"
        ],
        "exitFileObservedEveryStart": stability_result[
            "exitFileObservedEveryStart"
        ],
        "exitStatusEveryStart": stability_result["exitStatusEveryStart"],
        "jobActiveProcessCountEveryStart": [
            start["jobObject"]["activeProcessCount"] for start in starts
        ],
    }

    separation_items = separation["separation"]["artifacts"]
    manifest["separation"] = {
        "status": "passed",
        "runId": separation["runId"],
        "allObservedWebsiteDigestsDistinct": separation["separation"][
            "allObservedWebsiteDigestsDistinct"
        ],
        "observedWebsiteDigestByArtifact": {
            item["artifactId"]: item["observedWebsiteDigest"]
            for item in separation_items
        },
        "artifactFileSha256ByArtifact": {
            start["artifactId"]: start["artifactFileSha256"]
            for start in separation["coldStarts"]
        },
        "configUnchangedEveryStart": separation["separation"][
            "configUnchangedEveryStart"
        ],
        "mediaDevicesMatchConfiguredEveryStart": separation["separation"][
            "mediaDevicesMatchConfiguredEveryStart"
        ],
        "profileFreshEveryStart": separation["separation"][
            "profileFreshEveryStart"
        ],
        "jobActiveProcessCountEveryStart": [
            start["jobObject"]["activeProcessCount"]
            for start in separation["coldStarts"]
        ],
    }

    host_tamper = results["tamper"]
    tamper_cases = host_tamper["cases"]
    manifest["integrity"] = {
        "status": "passed",
        "runId": artifact_tamper["runId"],
        "hostTamperRunId": host_tamper["runId"],
        "expectedRawShaWrong": tamper_cases["expectedRawSha"]
        == "integrity_rejected",
        "missingArtifactField": tamper_cases["missingField"]
        == "integrity_rejected",
        "wrongArtifactType": all(
            mode["rejectedBeforeLaunch"]
            for mode in artifact_tamper["tamperModes"]
            if mode["mode"] == "type-error"
        ),
        "duplicateJsonKey": tamper_cases["identity-dup"] == "integrity_rejected",
        "nanInfinity": tamper_cases["identity-nan"] == "integrity_rejected",
        "wrongBrowserBinding": tamper_cases["browserBinding"]
        == "integrity_rejected",
        "sidecarMissing": tamper_cases["sidecar"] == "integrity_rejected",
        "allRejectedBeforeLaunch": artifact_tamper["allRejectedBeforeLaunch"],
        "treeMissingExtraModified": results["tree-integrity"]["status"]
        == "passed",
        "reparseJunction": results["reparse"]["junctionRejected"],
        "reparseMountPoint": results["mount-point"]["mountPointRejected"],
    }

    lifecycle = results["eof-force-exit"]
    manifest["lifecycle"].update(
        {
            "status": "passed",
            "runId": lifecycle["runId"],
            "profileInUse": results["lock-crash"]["profileInUse"]
            == "profile_in_use",
            "profileQuarantined": results["quarantine"]["errorCode"]
            == "profile_quarantined",
            "browserCrashFailedAndRelaunched": results["lock-crash"]["relaunch"],
            "stdinEof": True,
            "forcedHostExit": True,
            "pidReuseCreationTimeCounterexample": results["pid-reuse"]["status"]
            == "passed",
            "eofJobActiveProcessCount": lifecycle["eofJob"]["activeProcessCount"],
            "forcedExitJobActiveProcessCount": lifecycle["forcedJob"][
                "activeProcessCount"
            ],
        }
    )
    manifest["jobObject"]["exampleJobNames"] = [
        results["lock-crash"]["jobName"],
        lifecycle["eofJob"]["name"],
        lifecycle["forcedJob"]["name"],
    ]
    manifest["downloadGuard"] = {
        "status": "passed",
        "freshCacheArtifact": stability["artifactId"],
        "freshCacheRoot": cache["root"],
        "camoufoxInstallDir": cache["camoufoxInstallDir"],
        "cacheWasEmptyBeforeSeed": cache["wasEmptyBeforeSeed"],
        "cacheSeededFromVerifiedArchive": cache["seededFromVerifiedArchive"],
        "stabilityRunId": stability["runId"],
        "stabilityReportFile": stability_receipt[1],
        "stabilityReportSha256": stability_receipt[2],
        "firstThroughFifthColdStartStable": len(set(digests)) == 1,
        "noCamoufoxWebdlAttemptObserved": all(
            start["webdlTripped"] is False for start in starts
        ),
        "runtimeAutomaticDownload": False,
    }
    manifest["digestTransitionAttribution"] = {
        "preSyncRunId": transition["runId"],
        "changedObservedWebsiteSignalFields": changed_fields,
        "mediaDevicesIncludedInObservedWebsiteDigestV2": "mediaDevices"
        in STABLE_WEBSITE_SIGNAL_KEYS,
        "artifactFileSha256Stable": True,
        "diskAndSentConfigStable": True,
        "profilesFreshEveryStart": True,
        "preSyncCacheActuallyControlledOnWindows": False,
        "transitionLayer": "website-observation/runtime-host-device-enumeration",
        "reportAggregationError": False,
        "rootFix": [
            "bind platformdirs Windows LocalAppData override before Camoufox import",
            "pair Firefox synthetic media backend with deterministic media permission behavior when Artifact media devices are enabled",
            "bounded-wait for configured media-device enumeration before the authoritative full website observation",
            "require observed media-device counts to match Artifact configuration",
        ],
        "postSyncCounterexampleRunId": stability["runId"],
        "postSyncFirstThroughFifthStable": len(set(digests)) == 1,
    }
    manifest["uncoveredBoundaries"] = [
        "This is one Windows Server 2025 host and does not prove cross-host replay.",
        "fontMode is inherit; font isolation is not claimed.",
        "Canvas, TLS, QUIC, and network identity are outside this gate.",
        "The native supervisor is standalone Host support, not a Tauri or EngineAdapter integration.",
    ]

    MANIFEST.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"manifest={relative(MANIFEST)}")
    print(f"codeGitRevision={revision}")
    print(f"codeTreeHash={tree}")
    print(f"gateRunId={manifest['gateRunId']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
