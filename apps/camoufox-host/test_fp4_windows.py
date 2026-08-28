#!/usr/bin/env python3
"""Focused pure adjudication check for the FP4 native discriminator."""

from __future__ import annotations

import copy
import tempfile
from pathlib import Path

import run_fp4_windows as fp4


def task(name: str, **markers: object) -> dict:
    phase = "phaseB" if name == "formStateReplay" else "phaseA"
    return {
        "name": name,
        "phase": phase,
        "url": fp4.SELECTED_URLS[name],
        "status": "passed",
        "verified": False,
        "elapsedMs": 100,
        "budgetMs": round(fp4.TASK_BUDGET_SECONDS[name] * 1000),
        "crashed": False,
        "unexpectedPageClose": False,
        "pageErrors": [],
        "pageClose": {"status": "success"},
        "screenshot": {
            "path": f"artifacts/fp4/{name}.png",
            "sha256": "a" * 64,
            "sizeBytes": 100,
        },
        **markers,
    }


def passing_evidence() -> dict:
    tasks_a = [
        task(
            "documentNavigation",
            initialHttpStatus=200,
            initialHeading="Search results",
            articleUrl="https://en.wikipedia.org/wiki/Military_camouflage",
            articleHeading="Military camouflage",
            historyVisible=True,
            historyLengthBeforeArticle=2,
            historyLengthAfterArticle=3,
            historyLengthAfterBack=3,
            backAction="history.back()",
            backActionInvoked=True,
            backWaitTimedOut=False,
            backTraversalObserved=True,
            backDialogTypes=[],
            backNavigationUrls=[fp4.DOCUMENT_URL],
            backNavigationRequests=[fp4.DOCUMENT_URL],
            backNavigationRequestFailures=[],
            backPopstateCount=0,
            backUrl=fp4.DOCUMENT_URL,
            backHeading="Search results",
            forwardAction="history.forward()",
            forwardActionInvoked=True,
            forwardWaitTimedOut=False,
            forwardTraversalObserved=True,
            forwardUrl="https://en.wikipedia.org/wiki/Military_camouflage",
            forwardHeading="Military camouflage",
            finalUrl="https://en.wikipedia.org/wiki/Military_camouflage",
        ),
        task(
            "complexJavaScript",
            initialHttpStatus=200,
            queryInputValue="is:issue state:open label:browser-chromium",
            decodedQuery="is:issue state:open label:browser-chromium",
            issueLinkCount=3,
            noResultsVisible=False,
            finalUrl=(
                "https://github.com/microsoft/playwright/issues?"
                "q=is%3Aissue%20state%3Aopen%20label%3Abrowser-chromium"
            ),
        ),
        task(
            "interactiveGraphics",
            initialHttpStatus=200,
            initialHash="map=12/51.5074/-0.1278",
            initialCompletedTileCount=4,
            placeQuery="Hong Kong",
            searchResultCount=2,
            prePanHash="map=12/51.5074/-0.1278",
            panHash="map=12/51.5074/-0.1100",
            zoomHash="map=13/51.5074/-0.1100",
            cyclosmChecked=True,
            newLayerTileCount=4,
            finalUrl="https://www.openstreetmap.org/#map=13/51.5074/-0.1100",
        ),
        task(
            "audioVideo",
            initialHttpStatus=200,
            readyState=4,
            durationSeconds=20.0,
            startTimeSeconds=0.2,
            progressedTimeSeconds=1.4,
            paused=True,
            pausedTimeSeconds=1.4,
            seekTimeSeconds=5.0,
            finalUrl=fp4.MEDIA_URL,
        ),
        task(
            "formState",
            initialHttpStatus=200,
            reloadHttpStatus=200,
            largeCheckedBeforeReload=True,
            largeCheckedAfterReload=True,
            largeClassAfterReload=True,
            finalUrl=fp4.STATE_URL,
        ),
    ]
    task_b = task(
        "formStateReplay",
        initialHttpStatus=200,
        stateControlAvailable=True,
        largeCheckedBeforeMutation=True,
        largeClassBeforeMutation=True,
        standardCheckedAfterRestore=True,
        largeCheckedAfterRestore=False,
        largeClassAfterRestore=False,
        finalUrl=fp4.STATE_URL,
    )
    close = {
        "ok": True,
        "result": {
            "state": "exited",
            "exitStatus": 0,
            "exitFileObserved": True,
            "closeOutcome": {"status": "success"},
            "processTreeExit": {
                "exited": True,
                "remaining": [],
                "job": {"available": True, "activeProcessCount": 0},
            },
        },
    }
    launch_a = {
        "ok": True,
        "result": {
            "artifactId": fp4.ARTIFACT_ID,
            "artifactFileSha256": fp4.SOURCE_ARTIFACT_SHA256,
            "profileId": "fp4-attempt-1",
            "browserProxyServer": fp4.PROXY_URI,
            "bootCountBefore": 0,
            "bootCountAfter": 1,
            "verified": False,
        },
    }
    launch_b = copy.deepcopy(launch_a)
    launch_b["result"]["bootCountBefore"] = 1
    launch_b["result"]["bootCountAfter"] = 2
    return {
        "fixedInputs": {
            "matrixVersion": fp4.MATRIX_VERSION,
            "artifactId": fp4.ARTIFACT_ID,
            "artifactFileSha256": fp4.SOURCE_ARTIFACT_SHA256,
            "profileId": "fp4-attempt-1",
            "requiredProxy": fp4.PROXY_URI,
            "runtimeExecutableSha256": fp4.fp3.EXECUTABLE_SHA256,
            "selectedUrls": fp4.SELECTED_URLS,
        },
        "responses": {
            "hello": {
                "ok": True,
                "result": {
                    "platform": "windows-x64",
                    "browserRelease": fp4.fp3.RELEASE,
                    "assetSha256": fp4.fp3.ARCHIVE_SHA256,
                    "treeManifestSha256": fp4.fp3.TREE_MANIFEST_SHA256,
                    "verified": False,
                },
            },
            "phaseA": {
                "launch": launch_a,
                "status": {
                    "ok": True,
                    "result": {
                        "state": "running",
                        "browserProxyServer": fp4.PROXY_URI,
                    },
                },
                "close": close,
            },
            "phaseB": {
                "launch": launch_b,
                "status": {
                    "ok": True,
                    "result": {
                        "state": "running",
                        "browserProxyServer": fp4.PROXY_URI,
                    },
                },
                "close": copy.deepcopy(close),
            },
            "shutdown": {
                "ok": True,
                "result": {
                    "state": "shutdown",
                    "selfCheck": {"argvMatches": [], "stderrLogMatches": []},
                },
            },
        },
        "observations": {
            "phaseA": {
                "fp4CompatibilityObservation": {
                    "schema": (
                        "verisilo-camoufox-fp4-compatibility-observation/v1"
                    ),
                    "matrixVersion": fp4.MATRIX_VERSION,
                    "phase": "phaseA",
                    "status": "completed",
                    "verified": False,
                    "tasks": tasks_a,
                }
            },
            "phaseB": {
                "fp4CompatibilityObservation": {
                    "schema": (
                        "verisilo-camoufox-fp4-compatibility-observation/v1"
                    ),
                    "matrixVersion": fp4.MATRIX_VERSION,
                    "phase": "phaseB",
                    "status": "completed",
                    "verified": False,
                    "tasks": [task_b],
                }
            },
        },
        "childExitCode": 0,
        "residualOwnedPids": [],
        "runtimeCleanup": {"status": "removed"},
        "screenshotFilesVerified": True,
        "readErrors": [],
    }


def main() -> None:
    with tempfile.TemporaryDirectory(prefix="verisilo-fp4-test-") as root:
        staged = Path(root)
        fp4.stage_artifact(staged)
        assert fp4.sha256_file(staged / fp4.SOURCE_ARTIFACT.name) == (
            fp4.SOURCE_ARTIFACT_SHA256
        )
        assert fp4.sha256_file(staged / fp4.SOURCE_ARTIFACT_SIDECAR.name) == (
            fp4.SOURCE_ARTIFACT_SIDECAR_SHA256
        )

    evidence = passing_evidence()
    assert fp4.adjudicate_native(evidence)["status"] == "passed"

    history_failure = copy.deepcopy(
        evidence["observations"]["phaseA"]["fp4CompatibilityObservation"]["tasks"][0]
    )
    history_failure.update(
        backWaitTimedOut=True,
        backTraversalObserved=False,
        backNavigationUrls=[],
        backNavigationRequests=[],
        backUrl="https://en.wikipedia.org/wiki/Military_camouflage",
        backHeading="Military camouflage",
        forwardActionInvoked=False,
        forwardTraversalObserved=False,
    )
    assert fp4.document_direct_failure(history_failure) is True

    no_new_entry = copy.deepcopy(history_failure)
    no_new_entry["historyLengthAfterArticle"] = no_new_entry[
        "historyLengthBeforeArticle"
    ]
    assert fp4.document_direct_failure(no_new_entry) is False

    dialog_blocked = copy.deepcopy(history_failure)
    dialog_blocked["backDialogTypes"] = ["beforeunload"]
    assert fp4.document_direct_failure(dialog_blocked) is False

    request_failed = copy.deepcopy(history_failure)
    request_failed["backNavigationRequestFailures"] = [
        {"url": fp4.DOCUMENT_URL, "error": "NS_ERROR_NET_TIMEOUT"}
    ]
    assert fp4.document_direct_failure(request_failed) is False

    request_started = copy.deepcopy(history_failure)
    request_started["backNavigationRequests"] = [fp4.DOCUMENT_URL]
    assert fp4.document_direct_failure(request_started) is False

    same_document_entry = copy.deepcopy(history_failure)
    same_document_entry["backPopstateCount"] = 1
    assert fp4.document_direct_failure(same_document_entry) is False

    state_lost = copy.deepcopy(evidence)
    replay = state_lost["observations"]["phaseB"][
        "fp4CompatibilityObservation"
    ]["tasks"][0]
    replay["largeCheckedBeforeMutation"] = False
    replay["status"] = "inconclusive"
    result = fp4.adjudicate_native(state_lost)
    assert result["status"] == "failed"
    assert result["profileStateLost"] is True

    site_drift = copy.deepcopy(evidence)
    complex_task = site_drift["observations"]["phaseA"][
        "fp4CompatibilityObservation"
    ]["tasks"][1]
    complex_task["status"] = "inconclusive"
    complex_task["issueLinkCount"] = 0
    result = fp4.adjudicate_native(site_drift)
    assert result["status"] == "inconclusive"
    assert result["checks"]["tasks"]["complexJavaScript"] is False

    incomplete_reads = copy.deepcopy(evidence)
    incomplete_reads["readErrors"] = [{"label": "phaseA.session"}]
    result = fp4.adjudicate_native(incomplete_reads)
    assert result["status"] == "inconclusive"
    assert result["checks"]["evidenceReadsComplete"] is False

    direct_site_failure = copy.deepcopy(evidence)
    direct_site_failure["observations"]["phaseA"][
        "fp4CompatibilityObservation"
    ]["tasks"][1]["status"] = "failed"
    result = fp4.adjudicate_native(direct_site_failure)
    assert result["status"] == "failed"
    assert result["upstreamControlRequired"] is True
    assert result["terminal"] is False

    dirty_close = copy.deepcopy(evidence)
    dirty_close["responses"]["phaseB"]["close"]["result"]["processTreeExit"][
        "job"
    ]["activeProcessCount"] = 1
    result = fp4.adjudicate_native(dirty_close)
    assert result["status"] == "failed"
    assert result["checks"]["phaseBCleanClose"] is False
    print("FP4 focused adjudication check: passed")


if __name__ == "__main__":
    main()
