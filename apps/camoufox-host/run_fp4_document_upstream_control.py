#!/usr/bin/env python3
"""Run the single pinned-upstream control required by FP4 document failure."""

from __future__ import annotations

import argparse
import os
import shutil
import sys
import tempfile
import traceback
import uuid
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

import host_v1
import identity_policy
import run_fp4_windows as fp4
import run_spike
from generate_identity import rebind_identity_artifact, write_artifact_with_sidecar
from host_platform import process_identity_alive


CONTROL_ARTIFACT_ID = "identity-fp4-upstream-document-control"
ATTEMPT6_REPORT = REPO_ROOT / "artifacts/camoufox-fp4-attempt-6/run-report.json"
ATTEMPT6_REPORT_SHA256 = (
    "426c1ff564900bb69dbca8881335e488f353a71bc4524c2fcc6355758f0bfd3b"
)
ATTEMPT6_NATIVE = REPO_ROOT / "artifacts/camoufox-fp4-attempt-6/native-evidence.json"
ATTEMPT6_NATIVE_SHA256 = (
    "e0b3c7d9d12b11de70d71dfe8f1ab6f68b52f33da1656c365be98e98b7bc0440"
)
OFFICIAL_LOCK = (
    HOST_DIR / "lock/camoufox-v152.0.4-beta.28-windows-x86_64.json"
)
OFFICIAL_LOCK_SHA256 = (
    "8c4713586fb43f5f518e6f76209d596323b0e57e9b1570c5e665738b21133fea"
)
OFFICIAL_TREE_MANIFEST = (
    REPO_ROOT / "tests/fixtures/camoufox/browser-tree-manifest-windows.json"
)
OFFICIAL_TREE_MANIFEST_SHA256 = (
    "80f46b4bc7b40cd54072f1ba89462f8715c13eb339f596c2869560db880f6a40"
)
OFFICIAL_EXECUTABLE_SHA256 = (
    "ca931ff2b79aa4f4c66b9fae38693eeff7f700cbcd0704f2f8c45268b447065e"
)
CAMOUFOX_CFG_SHA256 = (
    "2d4e2d1ebca1a7103b03477d7d11dc056ae0f53ab12ef63e55d8bf2ff4b5722a"
)
OFFICIAL_BROWSER_BINDING = {
    "archiveSha256": "386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d",
    "archiveSizeBytes": 492370020,
    "buildId": "20260719045835",
    "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
    "propertiesJsonSha256": (
        "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018"
    ),
}


class FP4UpstreamControlError(RuntimeError):
    pass


def require_file(path: Path, digest: str) -> None:
    if not path.is_file() or path.is_symlink() or fp4.sha256_file(path) != digest:
        raise FP4UpstreamControlError(f"frozen input unavailable: {path}")


def read_frozen_json(path: Path, digest: str) -> dict[str, Any]:
    require_file(path, digest)
    value = fp4.fp3.strict_json(path)
    require_file(path, digest)
    return value


def official_binding(lock: dict[str, Any]) -> dict[str, Any]:
    binding = {
        "archiveSha256": lock["sha256"],
        "archiveSizeBytes": lock["sizeBytes"],
        **identity_policy.read_bundle_metadata(run_spike.EXECUTABLE),
    }
    if binding != OFFICIAL_BROWSER_BINDING:
        raise FP4UpstreamControlError("pinned upstream browser binding changed")
    return binding


def build_control_artifact(
    path: Path, source: dict[str, Any], binding: dict[str, Any]
) -> tuple[str, dict[str, Any]]:
    artifact = rebind_identity_artifact(
        source,
        artifact_id=CONTROL_ARTIFACT_ID,
        binding=binding,
    )
    if (
        artifact["resolvedConfig"] != source["resolvedConfig"]
        or artifact["networkIdentity"] != source["networkIdentity"]
    ):
        raise FP4UpstreamControlError("control rebind changed identity or network values")
    digest = write_artifact_with_sidecar(path, artifact)
    identity_policy.verify_artifact(path, expected_file_sha=digest)
    return digest, artifact["browserBinding"]


class FP4UpstreamControlHost(host_v1.CamoufoxHost):
    async def _launch_browser(self, session: dict, artifact: dict) -> None:
        await super()._launch_browser(session, artifact)
        observed_path = Path(session["sessionDir"]) / "observed.json"
        observation: dict[str, Any] = {
            "schema": "verisilo-camoufox-fp4-upstream-control-observation/v1",
            "matrixVersion": fp4.MATRIX_VERSION,
            "phase": "upstreamControl",
            "status": "running",
            "startedAtUtc": fp4.utc_now(),
            "tasks": [],
            "verified": False,
        }
        fp4.persist_observation(observed_path, observation)
        try:
            context = session.get("ctx")
            if context is None:
                raise FP4UpstreamControlError("live context is unavailable")
            observation["tasks"].append(
                await fp4.observe_task(
                    context, "upstreamControl", "documentNavigation", 1
                )
            )
            observation["status"] = "completed"
        except Exception as exc:  # noqa: BLE001 - bounded immutable evidence
            observation["status"] = "failed"
            observation["error"] = fp4.bounded_error(exc)
        observation["completedAtUtc"] = fp4.utc_now()
        fp4.persist_observation(observed_path, observation)


def run_child_host() -> int:
    if os.name != "nt":
        raise FP4UpstreamControlError("control requires native Windows")
    if fp4.SCREENSHOT_ROOT_ENV not in os.environ:
        raise FP4UpstreamControlError("control screenshot root is required")
    host_v1.CamoufoxHost = FP4UpstreamControlHost
    return host_v1.main()


def attempt6_document_task(evidence: dict[str, Any]) -> dict[str, Any]:
    return fp4.observation_tasks(evidence, "phaseA").get("documentNavigation", {})


def control_task(evidence: dict[str, Any]) -> dict[str, Any]:
    return fp4.observation_tasks(evidence, "phaseA").get("documentNavigation", {})


def screenshot_exact(task: dict[str, Any], control_root: Path) -> bool:
    receipt = task.get("screenshot")
    if not fp4.screenshot_valid(receipt):
        return False
    try:
        path = (REPO_ROOT / receipt["path"]).resolve(strict=True)
    except OSError:
        return False
    return (
        path.is_relative_to(control_root)
        and path.stat().st_size == receipt["sizeBytes"]
        and fp4.sha256_file(path) == receipt["sha256"]
    )


def classify_control(
    *,
    formal_direct_failure: bool,
    upstream_passed: bool,
    upstream_same_failure: bool,
    lifecycle_clean: bool,
) -> tuple[str, str, str]:
    if not formal_direct_failure or not lifecycle_clean:
        return "inconclusive", "control-unadjudicable", "unavailable"
    if upstream_passed:
        return "failed", "upstream-passed", "verisilo-patch-or-host-application"
    if upstream_same_failure:
        return (
            "failed",
            "same-direct-failure",
            "inherited-camoufox-firefox-product-limitation",
        )
    return "inconclusive", "control-unadjudicable", "unavailable"


def control_binding_checks(evidence: dict[str, Any]) -> dict[str, bool]:
    hello = fp4.protocol_result(evidence.get("responses", {}).get("hello"))
    phase = evidence.get("responses", {}).get("phaseA", {})
    launch = fp4.protocol_result(phase.get("launch"))
    status = fp4.protocol_result(phase.get("status"))
    shutdown = fp4.protocol_result(evidence.get("responses", {}).get("shutdown"))
    fixed = evidence.get("fixedInputs", {})
    artifact_sha256 = fixed.get("controlArtifactSha256")
    artifact_sha256_valid = (
        type(artifact_sha256) is str
        and len(artifact_sha256) == 64
        and set(artifact_sha256) <= set("0123456789abcdef")
    )
    profile_id = "fp4-upstream-document-control"
    return {
        "officialEngineExact": (
            hello.get("protocol") == "verisilo-camoufox-host/v1"
            and hello.get("browserRelease") == "v152.0.4-beta.28"
            and hello.get("assetSha256")
            == OFFICIAL_BROWSER_BINDING["archiveSha256"]
            and hello.get("treeManifest") == str(OFFICIAL_TREE_MANIFEST)
            and hello.get("treeManifestSha256") == OFFICIAL_TREE_MANIFEST_SHA256
            and hello.get("platform") == "windows-x64"
            and hello.get("state") == "idle"
            and hello.get("artifactRoot") == fixed.get("artifactRoot")
            and hello.get("profileRoot") == fixed.get("profileRoot")
            and hello.get("stateRoot") == fixed.get("stateRoot")
        ),
        "fixedInputsExact": (
            fixed.get("matrixVersion") == fp4.MATRIX_VERSION
            and fixed.get("formalAttempt") == 6
            and fixed.get("formalAttemptReportSha256") == ATTEMPT6_REPORT_SHA256
            and fixed.get("formalAttemptNativeSha256") == ATTEMPT6_NATIVE_SHA256
            and fixed.get("sourceArtifactSha256") == fp4.SOURCE_ARTIFACT_SHA256
            and fixed.get("controlArtifactId") == CONTROL_ARTIFACT_ID
            and artifact_sha256_valid
            and fixed.get("controlBrowserBinding") == OFFICIAL_BROWSER_BINDING
            and fixed.get("requiredProxy") == fp4.PROXY_URI
            and fixed.get("selectedUrl") == fp4.DOCUMENT_URL
            and fixed.get("runtimeExecutableSha256")
            == OFFICIAL_EXECUTABLE_SHA256
            and fixed.get("camoufoxCfgSha256") == CAMOUFOX_CFG_SHA256
        ),
        "launchBindingExact": (
            launch.get("state") == "running"
            and launch.get("artifactId") == CONTROL_ARTIFACT_ID
            and launch.get("profileId") == profile_id
            and launch.get("artifactFileSha256") == artifact_sha256
            and launch.get("bootCountBefore") == 0
            and launch.get("bootCountAfter") == 1
            and launch.get("browserProxyServer") == fp4.PROXY_URI
        ),
        "statusBindingExact": (
            status.get("state") == "running"
            and status.get("sessionId") == launch.get("sessionId")
            and status.get("artifactId") == CONTROL_ARTIFACT_ID
            and status.get("profileId") == profile_id
            and status.get("artifactFileSha256") == artifact_sha256
            and status.get("browserProxyServer") == fp4.PROXY_URI
            and status.get("failure") is None
        ),
        "shutdownSelfCheckClean": (
            shutdown.get("state") == "shutdown"
            and shutdown.get("sessionsClosed") == 1
            and shutdown.get("selfCheck", {}).get("argvMatches") == []
            and shutdown.get("selfCheck", {}).get("stderrLogMatches") == []
        ),
        "runnerCompletedWithoutError": (
            evidence.get("failure") is None
            and evidence.get("cleanupFailure") is None
        ),
    }


def adjudicate_control(
    evidence: dict[str, Any], control_root: Path, formal_task: dict[str, Any]
) -> dict[str, Any]:
    task = control_task(evidence)
    close = fp4.phase_result(evidence, "phaseA", "close")
    observation = fp4.phase_observation(evidence, "phaseA")
    checks = {
        **control_binding_checks(evidence),
        "observationComplete": (
            observation.get("schema")
            == "verisilo-camoufox-fp4-upstream-control-observation/v1"
            and observation.get("matrixVersion") == fp4.MATRIX_VERSION
            and observation.get("phase") == "upstreamControl"
            and observation.get("status") == "completed"
            and list(fp4.observation_tasks(evidence, "phaseA"))
            == ["documentNavigation"]
        ),
        "cleanClose": fp4.clean_close(close),
        "exactHostChildExit": evidence.get("childExitCode") == 0,
        "residualProcessTreeEmpty": evidence.get("residualOwnedPids") == [],
        "runtimeRemovedAfterCleanExit": (
            evidence.get("runtimeCleanup", {}).get("status") == "removed"
        ),
        "evidenceReadsComplete": evidence.get("readErrors") == [],
        "screenshotExact": screenshot_exact(task, control_root),
    }
    lifecycle_clean = all(checks.values())
    formal_direct_failure = fp4.document_direct_failure(formal_task)
    upstream_passed = task.get("status") == "passed" and fp4.document_markers_passed(
        task
    )
    upstream_same_failure = (
        task.get("status") == "failed" and fp4.document_direct_failure(task)
    )
    status, outcome, attribution = classify_control(
        formal_direct_failure=formal_direct_failure,
        upstream_passed=upstream_passed,
        upstream_same_failure=upstream_same_failure,
        lifecycle_clean=lifecycle_clean,
    )
    return {
        "status": status,
        "fp4Status": status,
        "controlOutcome": outcome,
        "attribution": attribution,
        "formalAttempt6DirectFailure": formal_direct_failure,
        "upstreamPassed": upstream_passed,
        "upstreamSameDirectFailure": upstream_same_failure,
        "lifecycleClean": lifecycle_clean,
        "checks": checks,
        "terminal": status == "failed",
        "verified": False,
    }


def managed_identities(close: dict[str, Any]) -> list[dict[str, Any]]:
    values = close.get("processTreeExit", {}).get("managedIdentities", [])
    return [value for value in values if type(value) is dict]


def remove_runtime_root(runtime_root: Path) -> dict[str, Any]:
    expected_parent = Path(tempfile.gettempdir()).resolve()
    if (
        runtime_root.parent != expected_parent
        or not runtime_root.name.startswith("verisilo-fp4-upstream-control-")
    ):
        return {"status": "unsafe-target-rejected", "path": str(runtime_root)}
    try:
        shutil.rmtree(runtime_root)
        return {"status": "removed"}
    except BaseException as exc:  # noqa: BLE001 - preserve immutable evidence
        return {
            "status": "failed",
            "path": str(runtime_root),
            "error": fp4.bounded_error(exc),
        }


def write_unavailable_report(
    control_root: Path,
    *,
    started_at: str,
    revision: str,
    tree: str,
    error: BaseException,
    runtime_cleanup: dict[str, Any],
) -> None:
    artifact_path = control_root / f"{CONTROL_ARTIFACT_ID}.json"
    adjudication = {
        "status": "inconclusive",
        "fp4Status": "inconclusive",
        "controlOutcome": "control-unavailable",
        "attribution": "unavailable",
        "formalAttempt6DirectFailure": None,
        "upstreamPassed": None,
        "upstreamSameDirectFailure": None,
        "lifecycleClean": False,
        "checks": {"controlSetupCompleted": False, "browserLaunched": False},
        "terminal": False,
        "verified": False,
    }
    report = {
        "schema": "verisilo-camoufox-fp4-upstream-control-report/v1",
        "status": "inconclusive",
        "verified": False,
        "startedAtUtc": started_at,
        "completedAtUtc": fp4.utc_now(),
        "repository": {
            "branch": fp4.BRANCH,
            "commit": revision,
            "tree": tree,
        },
        "contract": {
            "path": fp4.CONTRACT.relative_to(REPO_ROOT).as_posix(),
            "sha256": fp4.CONTRACT_SHA256,
        },
        "formalTrigger": {
            "attempt": 6,
            "expectedReportSha256": ATTEMPT6_REPORT_SHA256,
            "expectedNativeEvidenceSha256": ATTEMPT6_NATIVE_SHA256,
        },
        "upstream": {
            "assetLock": OFFICIAL_LOCK.relative_to(REPO_ROOT).as_posix(),
            "assetLockSha256": OFFICIAL_LOCK_SHA256,
            "browserBinding": OFFICIAL_BROWSER_BINDING,
            "executableSha256": OFFICIAL_EXECUTABLE_SHA256,
            "treeManifestSha256": OFFICIAL_TREE_MANIFEST_SHA256,
            "camoufoxCfgSha256": CAMOUFOX_CFG_SHA256,
        },
        "execution": {
            "browserLaunched": False,
            "error": fp4.bounded_error(error),
            "runtimeCleanup": runtime_cleanup,
        },
        "adjudication": adjudication,
        "boundaries": {
            "controlScope": "document-navigation-only",
            "universalCompatibility": "not_claimed",
            "verified": False,
            "nextGate": "restore-exact-control-input-and-run-once",
        },
        "outputs": fp4.output_receipts(
            [artifact_path, artifact_path.with_suffix(".json.sha256")]
        ),
    }
    report_path = control_root / "run-report.json"
    fp4.write_json(report_path, report)
    print(f"inconclusive: {report_path}")


def execute_control(control_root: Path) -> str:
    artifacts_root = (REPO_ROOT / "artifacts").resolve()
    control_root = control_root.resolve()
    if not control_root.is_relative_to(artifacts_root):
        raise FP4UpstreamControlError("control root must be inside artifacts")
    if control_root.exists():
        raise FP4UpstreamControlError(f"immutable control already exists: {control_root}")
    if os.name != "nt":
        raise FP4UpstreamControlError("control requires native Windows")
    if Path(fp4.git("rev-parse", "--show-toplevel")).resolve() != REPO_ROOT:
        raise FP4UpstreamControlError("Git root differs from repository root")
    if fp4.git("branch", "--show-current") != fp4.BRANCH:
        raise FP4UpstreamControlError("wrong Git branch")
    if fp4.git("status", "--porcelain=v1", "--untracked-files=all"):
        raise FP4UpstreamControlError("worktree is not clean")
    revision = fp4.git("rev-parse", "HEAD")
    if revision != fp4.git("rev-parse", f"origin/{fp4.BRANCH}"):
        raise FP4UpstreamControlError("runner commit is not synchronized with origin")
    started_at = fp4.utc_now()
    tree = fp4.git("rev-parse", "HEAD^{tree}")
    control_root.mkdir(parents=True)
    artifact_path = control_root / f"{CONTROL_ARTIFACT_ID}.json"
    runtime_root: Path | None = None
    try:
        for path, digest in (
            (fp4.CONTRACT, fp4.CONTRACT_SHA256),
            (ATTEMPT6_REPORT, ATTEMPT6_REPORT_SHA256),
            (fp4.SOURCE_ARTIFACT_SIDECAR, fp4.SOURCE_ARTIFACT_SIDECAR_SHA256),
            (OFFICIAL_TREE_MANIFEST, OFFICIAL_TREE_MANIFEST_SHA256),
            (run_spike.EXECUTABLE, OFFICIAL_EXECUTABLE_SHA256),
            (run_spike.EXECUTABLE.parent / "camoufox.cfg", CAMOUFOX_CFG_SHA256),
        ):
            require_file(path, digest)
        formal_evidence = read_frozen_json(ATTEMPT6_NATIVE, ATTEMPT6_NATIVE_SHA256)
        formal_task = attempt6_document_task(formal_evidence)
        if not fp4.document_direct_failure(formal_task):
            raise FP4UpstreamControlError("Attempt 6 direct failure trigger is absent")
        source = read_frozen_json(fp4.SOURCE_ARTIFACT, fp4.SOURCE_ARTIFACT_SHA256)
        lock = read_frozen_json(OFFICIAL_LOCK, OFFICIAL_LOCK_SHA256)
        archive = run_spike.ARTIFACT_DIR / run_spike.ASSET_NAME
        if not archive.is_file() or archive.stat().st_size != lock["sizeBytes"]:
            raise FP4UpstreamControlError("pinned upstream archive is unavailable")
        binding = official_binding(lock)
        artifact_sha256, binding = build_control_artifact(
            artifact_path, source, binding
        )
        screenshots = control_root / "screenshots"
        screenshots.mkdir()
        runtime_root = Path(tempfile.mkdtemp(prefix="verisilo-fp4-upstream-control-"))
        runtime_root = runtime_root.resolve()
        artifact_root = runtime_root / "artifacts"
        profile_root = runtime_root / "profiles"
        state_root = runtime_root / "state"
        cache_root = runtime_root / "cache"
        for path in (artifact_root, profile_root, state_root, cache_root):
            path.mkdir()
        shutil.copyfile(artifact_path, artifact_root / artifact_path.name)
        shutil.copyfile(
            artifact_path.with_suffix(".json.sha256"),
            artifact_root / artifact_path.with_suffix(".json.sha256").name,
        )
    except Exception as exc:  # unavailable control is immutable evidence
        write_unavailable_report(
            control_root,
            started_at=started_at,
            revision=revision,
            tree=tree,
            error=exc,
            runtime_cleanup=(
                {"status": "not-created"}
                if runtime_root is None
                else remove_runtime_root(runtime_root)
            ),
        )
        return "inconclusive"
    assert runtime_root is not None

    stderr_path = control_root / "host-stderr.txt"
    native_path = control_root / "native-evidence.json"
    report_path = control_root / "run-report.json"
    command = [
        sys.executable,
        "-u",
        str(Path(__file__).resolve()),
        "--child-host",
        "--artifact-root",
        str(artifact_root),
        "--profile-root",
        str(profile_root),
        "--state-root",
        str(state_root),
        "--tree-manifest",
        str(OFFICIAL_TREE_MANIFEST),
    ]
    environment = os.environ.copy()
    environment["VERISILO_CAMOUFOX_CACHE_DIR"] = str(cache_root)
    environment[fp4.SCREENSHOT_ROOT_ENV] = str(screenshots)
    responses: dict[str, Any] = {"phaseA": {}}
    evidence: dict[str, Any] = {
        "schema": "verisilo-camoufox-fp4-upstream-control-evidence/v1",
        "status": "inconclusive",
        "verified": False,
        "controlId": f"fp4-upstream-document-{uuid.uuid4().hex}",
        "fixedInputs": {
            "matrixVersion": fp4.MATRIX_VERSION,
            "formalAttempt": 6,
            "formalAttemptReportSha256": ATTEMPT6_REPORT_SHA256,
            "formalAttemptNativeSha256": ATTEMPT6_NATIVE_SHA256,
            "sourceArtifactSha256": fp4.SOURCE_ARTIFACT_SHA256,
            "controlArtifactId": CONTROL_ARTIFACT_ID,
            "controlArtifactSha256": artifact_sha256,
            "controlBrowserBinding": binding,
            "requiredProxy": fp4.PROXY_URI,
            "selectedUrl": fp4.DOCUMENT_URL,
            "runtimeExecutableSha256": OFFICIAL_EXECUTABLE_SHA256,
            "camoufoxCfgSha256": CAMOUFOX_CFG_SHA256,
            "artifactRoot": str(artifact_root),
            "profileRoot": str(profile_root),
            "stateRoot": str(state_root),
        },
        "responses": responses,
        "observations": {},
        "sessions": {},
        "readErrors": [],
        "childExitCode": None,
        "residualOwnedPids": None,
    }
    host: fp4.HostProcess | None = None
    session_id: str | None = None
    failure: BaseException | None = None

    def capture(suffix: str) -> None:
        if session_id is None:
            return
        session_dir = state_root / session_id
        observed = fp4.safe_json(
            session_dir / "observed.json", f"control.observed.{suffix}", evidence["readErrors"]
        )
        session = fp4.safe_json(
            session_dir / "session.json", f"control.session.{suffix}", evidence["readErrors"]
        )
        if observed:
            evidence["observations"]["phaseA"] = observed
        if session:
            evidence["sessions"][suffix] = session

    try:
        host = fp4.HostProcess(command, environment, stderr_path)
        responses["hello"] = host.send("hello", {}, 300.0)
        fp4.require_ok(responses["hello"], "hello")
        responses["phaseA"]["launch"] = host.send(
            "launch",
            {
                "artifactId": CONTROL_ARTIFACT_ID,
                "profileId": "fp4-upstream-document-control",
                "expectedArtifactFileSha256": artifact_sha256,
                "browserProxyServer": fp4.PROXY_URI,
            },
            900.0,
        )
        session_id = fp4.require_ok(
            responses["phaseA"]["launch"], "control launch"
        )["sessionId"]
        capture("running")
        responses["phaseA"]["status"] = host.send(
            "status", {"sessionId": session_id}, 30.0
        )
        fp4.require_ok(responses["phaseA"]["status"], "control status")
        responses["phaseA"]["close"] = host.send(
            "close", {"sessionId": session_id}, 150.0
        )
        fp4.require_ok(responses["phaseA"]["close"], "control close")
        capture("stopped")
        session_id = None
    except BaseException as exc:  # noqa: BLE001 - immutable control lineage
        failure = exc
    finally:
        if host is not None and host.process.poll() is None and session_id is not None:
            try:
                responses["phaseA"]["close"] = host.send(
                    "close", {"sessionId": session_id}, 150.0
                )
                capture("stopped")
            except BaseException as exc:  # noqa: BLE001
                failure = failure or exc
        if host is not None and host.process.poll() is None:
            try:
                responses["shutdown"] = host.send("shutdown", {}, 60.0)
                fp4.require_ok(responses["shutdown"], "shutdown")
                evidence["childExitCode"] = host.wait(30.0)
            except BaseException as exc:  # noqa: BLE001
                failure = failure or exc
                try:
                    evidence["childExitCode"] = host.kill()
                except BaseException as cleanup_exc:  # noqa: BLE001
                    evidence["cleanupFailure"] = fp4.bounded_error(cleanup_exc)

    close = fp4.phase_result(evidence, "phaseA", "close")
    identities = managed_identities(close)
    evidence["residualOwnedPids"] = sorted(
        identity["pid"]
        for identity in identities
        if type(identity.get("pid")) is int and process_identity_alive(identity)
    )
    if host is not None:
        evidence["protocolFrames"] = host.frames
    if failure is not None:
        evidence["failure"] = fp4.bounded_error(failure)
        (control_root / "runner-error.txt").write_text(
            "".join(traceback.format_exception(failure)),
            encoding="utf-8",
            newline="\n",
        )

    removable = (
        fp4.clean_close(close)
        and evidence["childExitCode"] == 0
        and evidence["residualOwnedPids"] == []
    )
    if removable:
        evidence["runtimeCleanup"] = remove_runtime_root(runtime_root)
    else:
        evidence["runtimeCleanup"] = {
            "status": "preserved-dirty-boundary",
            "path": str(runtime_root),
        }

    adjudication = adjudicate_control(evidence, control_root, formal_task)
    evidence["adjudication"] = adjudication
    evidence["status"] = adjudication["status"]
    native_sha256 = fp4.write_json(native_path, evidence)
    outputs = [
        artifact_path,
        artifact_path.with_suffix(".json.sha256"),
        native_path,
        stderr_path,
        *sorted(screenshots.glob("*.png")),
    ]
    runner_error = control_root / "runner-error.txt"
    if runner_error.exists():
        outputs.append(runner_error)
    report = {
        "schema": "verisilo-camoufox-fp4-upstream-control-report/v1",
        "status": adjudication["status"],
        "verified": False,
        "startedAtUtc": started_at,
        "completedAtUtc": fp4.utc_now(),
        "repository": {
            "branch": fp4.BRANCH,
            "commit": revision,
            "tree": tree,
        },
        "contract": {
            "path": fp4.CONTRACT.relative_to(REPO_ROOT).as_posix(),
            "sha256": fp4.CONTRACT_SHA256,
        },
        "formalTrigger": {
            "attempt": 6,
            "reportSha256": ATTEMPT6_REPORT_SHA256,
            "nativeEvidenceSha256": ATTEMPT6_NATIVE_SHA256,
            "originalStatus": "inconclusive",
            "correctedDocumentStatus": (
                "failed"
                if adjudication["formalAttempt6DirectFailure"]
                else "inconclusive"
            ),
        },
        "upstream": {
            "assetLock": OFFICIAL_LOCK.relative_to(REPO_ROOT).as_posix(),
            "assetLockSha256": OFFICIAL_LOCK_SHA256,
            "archiveSha256": lock["sha256"],
            "archiveSizeBytes": lock["sizeBytes"],
            "executableSha256": OFFICIAL_EXECUTABLE_SHA256,
            "treeManifestSha256": OFFICIAL_TREE_MANIFEST_SHA256,
            "camoufoxCfgSha256": CAMOUFOX_CFG_SHA256,
        },
        "execution": {"command": command, "childExitCode": evidence["childExitCode"]},
        "nativeEvidence": {
            "path": native_path.relative_to(REPO_ROOT).as_posix(),
            "sha256": native_sha256,
        },
        "adjudication": adjudication,
        "boundaries": {
            "controlScope": "document-navigation-only",
            "universalCompatibility": "not_claimed",
            "verified": False,
            "nextGate": "fix-inherited-session-history-limitation-if-control-confirms",
        },
        "outputs": fp4.output_receipts(outputs),
    }
    fp4.write_json(report_path, report)
    print(f"{report['status']}: {report_path}")
    return report["status"]


def main() -> int:
    if "--child-host" in sys.argv:
        sys.argv.remove("--child-host")
        return run_child_host()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--control-root", type=Path, required=True)
    status = execute_control(parser.parse_args().control_root)
    return {"failed": 1, "inconclusive": 2}[status]


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FP4UpstreamControlError as exc:
        raise SystemExit(f"FP4 upstream control blocked: {exc}") from exc
