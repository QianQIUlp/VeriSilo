#!/usr/bin/env python3
"""Focused pure check for the one FP4 upstream-control adjudicator."""

from __future__ import annotations

import copy
import tempfile
from pathlib import Path

import run_fp4_document_upstream_control as control


def response(result: dict) -> dict:
    return {"ok": True, "result": result}


def binding_evidence() -> dict:
    artifact_sha256 = "a" * 64
    launch = {
        "state": "running",
        "sessionId": "session-a",
        "artifactId": control.CONTROL_ARTIFACT_ID,
        "profileId": "fp4-upstream-document-control",
        "artifactFileSha256": artifact_sha256,
        "bootCountBefore": 0,
        "bootCountAfter": 1,
        "browserProxyServer": control.fp4.PROXY_URI,
    }
    return {
        "fixedInputs": {
            "matrixVersion": control.fp4.MATRIX_VERSION,
            "formalAttempt": 6,
            "formalAttemptReportSha256": control.ATTEMPT6_REPORT_SHA256,
            "formalAttemptNativeSha256": control.ATTEMPT6_NATIVE_SHA256,
            "sourceArtifactSha256": control.fp4.SOURCE_ARTIFACT_SHA256,
            "controlArtifactId": control.CONTROL_ARTIFACT_ID,
            "controlArtifactSha256": artifact_sha256,
            "controlBrowserBinding": control.OFFICIAL_BROWSER_BINDING,
            "requiredProxy": control.fp4.PROXY_URI,
            "selectedUrl": control.fp4.DOCUMENT_URL,
            "runtimeExecutableSha256": control.OFFICIAL_EXECUTABLE_SHA256,
            "camoufoxCfgSha256": control.CAMOUFOX_CFG_SHA256,
            "artifactRoot": r"C:\control\artifacts",
            "profileRoot": r"C:\control\profiles",
            "stateRoot": r"C:\control\state",
        },
        "responses": {
            "hello": response(
                {
                    "protocol": "verisilo-camoufox-host/v1",
                    "browserRelease": "v152.0.4-beta.28",
                    "assetSha256": control.OFFICIAL_BROWSER_BINDING["archiveSha256"],
                    "treeManifest": str(control.OFFICIAL_TREE_MANIFEST),
                    "treeManifestSha256": control.OFFICIAL_TREE_MANIFEST_SHA256,
                    "platform": "windows-x64",
                    "state": "idle",
                    "artifactRoot": r"C:\control\artifacts",
                    "profileRoot": r"C:\control\profiles",
                    "stateRoot": r"C:\control\state",
                }
            ),
            "phaseA": {
                "launch": response(launch),
                "status": response(
                    {
                        **launch,
                        "failure": None,
                    }
                ),
            },
            "shutdown": response(
                {
                    "state": "shutdown",
                    "sessionsClosed": 1,
                    "selfCheck": {"argvMatches": [], "stderrLogMatches": []},
                }
            ),
        },
    }


def main() -> None:
    classify = control.classify_control
    assert classify(
        formal_direct_failure=True,
        upstream_passed=False,
        upstream_same_failure=True,
        lifecycle_clean=True,
    ) == (
        "failed",
        "same-direct-failure",
        "inherited-camoufox-firefox-product-limitation",
    )
    assert classify(
        formal_direct_failure=True,
        upstream_passed=True,
        upstream_same_failure=False,
        lifecycle_clean=True,
    ) == (
        "failed",
        "upstream-passed",
        "verisilo-patch-or-host-application",
    )
    assert classify(
        formal_direct_failure=True,
        upstream_passed=False,
        upstream_same_failure=False,
        lifecycle_clean=True,
    )[0] == "inconclusive"

    evidence = binding_evidence()
    assert all(control.control_binding_checks(evidence).values())
    wrong_route = copy.deepcopy(evidence)
    wrong_route["responses"]["phaseA"]["status"]["result"][
        "browserProxyServer"
    ] = "socks5://127.0.0.1:7898"
    assert not control.control_binding_checks(wrong_route)["statusBindingExact"]
    runner_failed = copy.deepcopy(evidence)
    runner_failed["failure"] = {"type": "RuntimeError", "message": "failed"}
    assert not control.control_binding_checks(runner_failed)[
        "runnerCompletedWithoutError"
    ]

    artifacts = control.REPO_ROOT / "artifacts"
    with tempfile.TemporaryDirectory(
        prefix="fp4-control-unavailable-test-", dir=artifacts
    ) as temporary:
        root = Path(temporary)
        control.write_unavailable_report(
            root,
            started_at="2026-08-28T00:00:00.000000Z",
            revision="a" * 40,
            tree="b" * 40,
            error=control.FP4UpstreamControlError("missing frozen input"),
            runtime_cleanup={"status": "not-created"},
        )
        report_path = root / "run-report.json"
        report = control.fp4.fp3.strict_json(report_path)
        assert report["status"] == "inconclusive"
        assert report["execution"]["browserLaunched"] is False
        assert report["adjudication"]["controlOutcome"] == "control-unavailable"
        assert report_path.with_suffix(".json.sha256").is_file()

    partial_runtime = Path(
        tempfile.mkdtemp(prefix="verisilo-fp4-upstream-control-")
    ).resolve()
    (partial_runtime / "partial-profile").mkdir()
    assert control.remove_runtime_root(partial_runtime) == {"status": "removed"}
    assert not partial_runtime.exists()


if __name__ == "__main__":
    main()
