#!/usr/bin/env python3
"""VeriSilo M1.1 identity artifact spike.

The chain under test is:

    disk artifact -> byte-identical CAMOU_CONFIG every start -> stable,
    distinguishable website-visible identity

Every cold start:
1. re-reads and strictly validates the artifact from DISK;
2. deep-copies its resolvedConfig;
3. records configuredIdentityDigest BEFORE launch_options();
4. calls launch_options() and compares the sent CAMOU_CONFIG to the disk
   config — any added/changed/removed key fails BEFORE the browser launches;
5. probes the page and computes ObservedWebsiteDigest from website-observed
   values only (no artifactId, no internal seeds, no canvas).

Subcommands:

  stability  --artifact PATH [--runs 5]
  separation --artifacts A,B,C
  tamper     --artifact PATH [--out-dir DIR]
      Runs four tamper modes: digest, missing-field, type-error,
      policy-mismatch. All must be rejected before any browser starts.

Every invocation writes an immutable artifacts/camoufox-m1/runs/<run-id>/
report.json + report.sha256.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
from functools import partial
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any, Optional

from identity_policy import (
    ARTIFACT_SCHEMA,
    ArtifactIntegrityError,
    SESSION_VARIABLE_SIGNAL_KEYS,
    STABLE_WEBSITE_SIGNAL_KEYS,
    UNAVAILABLE_SIGNALS,
    build_projection,
    canonical_json_bytes,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    sha256_hex,
    verify_browser_binding,
    verify_artifact,
    verify_artifact_raw,
)
from browser_tree import (
    load_tree_manifest,
    verify_tree,
)
from host_platform import IS_WINDOWS, JobHandle, ProfileLock, process_identity_alive
from host_fonts import (
    FONT_UNIVERSE,
    host_negative_control_families,
)
from run_spike import (
    CAMOUFOX_INSTALL_DIR,
    DownloadGuard,
    EXECUTABLE,
    REPO_ROOT,
    SUPERVISOR,
    XDG_CACHE_DIR,
    ensure_browser_asset,
    install_download_guard,
    installed_versions,
    load_asset_lock,
    new_run_id,
    normalize_camou_config_env,
    seed_camoufox_cache,
    start_probe_server,
    start_xvfb,
    stop_xvfb,
    utcnow,
)

M1_ARTIFACT_DIR = REPO_ROOT / "artifacts" / (
    "camoufox-m2-windows-gate" if IS_WINDOWS else "camoufox-m1"
)
M1_RUNS_DIR = M1_ARTIFACT_DIR / "runs"
TREE_MANIFEST = REPO_ROOT / "tests" / "fixtures" / "camoufox" / (
    "browser-tree-manifest-windows.json"
    if IS_WINDOWS
    else "browser-tree-manifest.json"
)

REPORT_SCHEMA = "verisilo-camoufox-m1-run-report/v3"


def reassemble_camou_config(env: dict) -> dict:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    if not chunks:
        raise RuntimeError("launch_options returned no CAMOU_CONFIG env chunks")
    return json.loads("".join(value for _, value in chunks))


def extract_observed_website_signals(
    observed: dict, font_mode: str = "inherit"
) -> dict:
    signals = {
        "userAgent": observed["userAgent"],
        "language": observed["language"],
        "languages": observed["languages"],
        "platform": observed["platform"],
        "oscpu": observed["oscpu"],
        "doNotTrack": observed["doNotTrack"],
        "globalPrivacyControl": observed["globalPrivacyControl"],
        "screen": observed["screen"],
        "devicePixelRatio": observed["devicePixelRatio"],
        "hardwareConcurrency": observed["hardwareConcurrency"],
        "historyLength": observed["historyLength"],
        "mediaDevices": observed["mediaDevices"],
        "timezone": observed["session"]["timezone"],
        "utcOffsetMinutes": observed["session"]["utcOffsetMinutes"],
        "fontNegativeControls": observed["fontNegativeControls"],
        "webglVendor": observed["webglVendor"],
        "webglRenderer": observed["webglRenderer"],
        "webglSummary": observed["webglSummary"],
        "voices": observed["voices"],
        "audioHash": observed["audioHash"],
    }
    # In inherit mode font widths are host-bound (the host font set can leak
    # into width measurement), so they never enter ObservedWebsiteDigest.
    # Only managed mode, after all host negative controls prove unavailable,
    # may include them.
    if font_mode == "managed":
        signals["fontUniverseWidths"] = observed["fontUniverseWidths"]
    return signals


async def cold_start(
    playwright: Any,
    artifact_path: Path,
    index: int,
    run_dir: Path,
    display: str,
    executable: Path,
    probe_url: str,
    lock: dict,
) -> dict:
    # 1. Fresh read from disk + strict validation + deepcopy.
    artifact, artifact_file_sha = verify_artifact_raw(artifact_path)
    verify_browser_binding(
        artifact, lock, executable, installed_versions()
    )
    policy = artifact["policy"]
    window = tuple(policy["window"])
    disk_config = copy.deepcopy(artifact["resolvedConfig"])
    disk_digest = configured_identity_digest(disk_config)

    profile = run_dir / f"profile-cold-{index}"
    profile_pre_existed = profile.exists()
    if profile_pre_existed:
        raise RuntimeError(f"cold start profile already exists: {profile}")
    exit_file = run_dir / f"cold-{index}-exit.json"
    supervisor_file = run_dir / f"cold-{index}-supervisor.json"
    if exit_file.exists():
        exit_file.unlink()
    if supervisor_file.exists():
        supervisor_file.unlink()
    profile_lock = None
    if IS_WINDOWS:
        profile_lock = ProfileLock.acquire(profile.parent / f"{profile.name}.lock")
    os.environ["VERISILO_REAL_EXE"] = str(executable)
    os.environ["VERISILO_EXIT_FILE"] = str(exit_file)
    os.environ["VERISILO_SUPERVISOR_FILE"] = str(supervisor_file)
    if IS_WINDOWS:
        os.environ["VERISILO_PROFILE_LOCK_PATH"] = str(
            profile.parent / f"{profile.name}.lock"
        )
        os.environ["VERISILO_JOB_NAME"] = f"Local\\VeriSiloM1-{run_dir.name}-{index}"

    from camoufox import AsyncNewBrowser
    from camoufox import DefaultAddons
    from camoufox.utils import launch_options

    # 2-4. Build launch options; the sent config must equal the disk config.
    launch_start = time.perf_counter()
    opts = await asyncio.get_event_loop().run_in_executor(
        None,
        partial(
            launch_options,
             config=copy.deepcopy(disk_config),
            os=policy["targetOs"],
            window=window,
            locale=policy["locale"],
            ff_version=policy["ffVersion"],
            headless=False,
            executable_path=str(executable),
            user_data_dir=str(profile),
            virtual_display=display or None,
            firefox_user_prefs={
                "app.update.auto": False,
                "app.update.enabled": False,
                "browser.shell.checkDefaultBrowser": False,
            },
            exclude_addons=[DefaultAddons.UBO],
            i_know_what_im_doing=True,
        ),
    )
    sent_config, config_diff, opts["env"] = normalize_camou_config_env(
        opts["env"], disk_config
    )
    sent_digest = configured_identity_digest(sent_config)
    config_unchanged = (
        sent_digest == disk_digest
        and not config_diff["added"]
        and not config_diff["removed"]
        and not config_diff["changed"]
    )
    if not config_unchanged:
        raise RuntimeError(
            "launch_options mutated the disk config before launch: "
            + json.dumps(
                {
                    "diskConfigDigest": disk_digest,
                    "sentConfigDigest": sent_digest,
                    "diff": config_diff,
                }
            )
        )

    opts["executable_path"] = str(SUPERVISOR)
    ctx = await AsyncNewBrowser(
        playwright,
        from_options=opts,
        persistent_context=True,
    )
    if DownloadGuard.tripped:
        await ctx.close()
        raise RuntimeError("unpinned download attempted during launch")
    spawn_seconds = time.perf_counter() - launch_start

    page = await ctx.new_page()
    await page.goto(probe_url, wait_until="domcontentloaded", timeout=60_000)
    fonts = artifact["stableSignalsDeclared"]["fonts"]
    await page.evaluate(f"window.__probeFonts = {json.dumps(fonts)}")
    await page.evaluate(
        f"window.__probeFontUniverse = {json.dumps(FONT_UNIVERSE)}"
    )
    host_controls = host_negative_control_families(fonts)
    await page.evaluate(
        f"window.__probeHostFonts = {json.dumps(host_controls)}"
    )
    await page.evaluate("document.fonts.ready")
    page_start = time.perf_counter()
    observed = await page.evaluate("window.__probe.readIdentity()")
    probe_seconds = time.perf_counter() - page_start

    font_mode = artifact["policy"].get("fontMode", "inherit")
    host_masking = {
        "controlsTested": len(observed.get("hostFontNegativeControls", {})),
        "allUnavailable": all(
            available is False
            for available in observed.get("hostFontNegativeControls", {}).values()
        ),
        "failures": [
            family
            for family, available in observed.get(
                "hostFontNegativeControls", {}
            ).items()
            if available is not False
        ],
    }
    if font_mode == "managed" and not host_masking["allUnavailable"]:
        await ctx.close()
        raise RuntimeError(
            "managed font mode requires all host negative controls "
            "unavailable; masking failed: " + ", ".join(host_masking["failures"])
        )

    close_start = time.perf_counter()
    await ctx.close()
    close_seconds = time.perf_counter() - close_start
    if profile_lock is not None:
        profile_lock.release()
        profile_lock = None

    exit_code = None
    exit_file_observed = exit_file.exists()
    if exit_file_observed:
        try:
            exit_code = int(json.loads(exit_file.read_text())["exitCode"])
        except (OSError, ValueError, KeyError, json.JSONDecodeError):
            exit_code = None
    job_result = job_evidence(supervisor_file) if IS_WINDOWS else None
    try:
        supervisor_meta = json.loads(supervisor_file.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        supervisor_meta = None

    observed_website_signals = extract_observed_website_signals(observed, font_mode)
    projection = build_projection(
        artifact["artifactId"],
        run_id_from_dir(run_dir),
        index,
        disk_digest,
        observed_website_signals,
    )
    return {
        "coldStartIndex": index,
        "artifactId": artifact["artifactId"],
        "artifactDigest": artifact["canonicalDigest"],
        "artifactFileSha256": artifact_file_sha,
        "startedAtUtc": utcnow(),
        "spawnSeconds": round(spawn_seconds, 3),
        "probeSeconds": round(probe_seconds, 3),
        "closeSeconds": round(close_seconds, 3),
        "exitStatus": exit_code,
        "exitFileObserved": exit_file_observed,
        "jobObject": job_result,
        "supervisorMeta": supervisor_meta,
        "profileDir": str(profile),
        "profilePreExisted": profile_pre_existed,
        "diskConfigDigest": disk_digest,
        "sentConfigDigest": sent_digest,
        "configUnchanged": config_unchanged,
        "configDiff": config_diff,
        "webdlTripped": bool(DownloadGuard.tripped),
        "fontMode": font_mode,
        "injectedFontsAllAvailable": all(
            entry.get("available") for entry in observed.get("injectedFonts", [])
        ),
        "hostFontMasking": host_masking,
        "canvasObserved": {
            "rawHash": observed["canvas"]["rawHash"],
            "exportHash": observed["canvas"]["exportHash"],
        },
        "sessionVariable": observed["session"],
        "unavailable": observed["unavailable"],
        "observedWebsiteSignals": observed_website_signals,
        "projection": projection,
    }


def run_id_from_dir(run_dir: Path) -> str:
    return run_dir.name


def prepare_host() -> tuple[dict, Path]:
    lock = load_asset_lock()
    if lock.get("digestAgreement") is not True:
        raise SystemExit(
            "asset lock digestAgreement is not true; refresh with "
            "`uv run python fetch-browser.py --record --force`"
        )
    executable = ensure_browser_asset(lock, allow_download=False)
    cache_root = Path(os.environ.get("VERISILO_CAMOUFOX_CACHE_DIR", str(XDG_CACHE_DIR)))
    seed_camoufox_cache(lock, executable, install_dir=cache_root / "camoufox")
    if not IS_WINDOWS:
        SUPERVISOR.chmod(0o755)
    os.environ["XDG_CACHE_HOME"] = str(cache_root)
    install_download_guard()
    DownloadGuard.reset()
    return lock, executable


def job_evidence(supervisor_path: Path) -> dict:
    try:
        metadata = json.loads(supervisor_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {"available": False, "metadataObserved": False, "activeProcessCount": None}
    name = metadata.get("jobName")
    identities = [
        {
            "pid": metadata.get("supervisorPid"),
            "creationTime100ns": metadata.get("supervisorCreationTime100ns"),
        },
        {
            "pid": metadata.get("childPid"),
            "creationTime100ns": metadata.get("childCreationTime100ns"),
        },
    ]
    if not isinstance(name, str):
        return {"available": False, "metadataObserved": True, "activeProcessCount": None}
    try:
        job = JobHandle.open(name)
    except OSError:
        return {
            "available": False,
            "metadataObserved": True,
            "jobObjectClosed": all(
                not process_identity_alive(identity) for identity in identities
            ),
            "activeProcessCount": 0,
            "name": name,
        }
    try:
        return {
            "available": True,
            "metadataObserved": True,
            "jobObjectClosed": False,
            "activeProcessCount": job.active_process_count(),
            "name": name,
        }
    finally:
        job.close()


async def run_cold_starts(
    artifact_paths: list[Path],
    runs: list[int],
    display: Optional[str],
) -> dict:
    run_id = new_run_id()
    run_dir = M1_RUNS_DIR / run_id
    run_dir.mkdir(parents=True, exist_ok=False)

    lock, executable = prepare_host()
    tree_result = verify_tree(EXECUTABLE.parent, load_tree_manifest(TREE_MANIFEST))
    xvfb_proc: Optional[subprocess.Popen] = None
    server = None
    display_value = None if IS_WINDOWS else (display or os.environ.get("DISPLAY"))
    if not IS_WINDOWS and not display_value:
        display_value, xvfb_proc = start_xvfb()
    server, probe_url = start_probe_server()

    starts: list[dict] = []
    failure: Optional[str] = None
    try:
        from playwright.async_api import async_playwright

        async with async_playwright() as playwright:
            for position, (artifact_path, index) in enumerate(
                zip(artifact_paths, runs), start=1
            ):
                print(
                    f"cold start {position}/{len(runs)}: artifact={artifact_path.name} index={index}"
                )
                start = await cold_start(
                    playwright,
                    artifact_path,
                    index,
                    run_dir,
                    display_value,
                    executable,
                    probe_url,
                    lock,
                )
                starts.append(start)
                print(
                    f"  observedWebsiteDigest={start['projection']['observedWebsiteDigest']} "
                    f"configUnchanged={start['configUnchanged']}"
                )
    except Exception as exc:  # keep report even on failure
        failure = f"{type(exc).__name__}: {exc}"
        print(f"run failed: {failure}", file=sys.stderr)
    finally:
        if server is not None:
            server.shutdown()
        if xvfb_proc is not None:
            stop_xvfb(xvfb_proc)

    return {
        "runId": run_id,
        "runDir": str(run_dir),
        "display": display_value,
        "xvfbOwnedBySpike": xvfb_proc is not None,
        "lock": {
            "browserRelease": lock["release"],
            "assetSha256": lock["sha256"],
            "digestAgreement": lock["digestAgreement"],
        },
        "treeVerification": tree_result,
        "starts": starts,
        "failure": failure,
    }


async def cmd_stability(args: argparse.Namespace) -> int:
    # Strict validation happens once up front; per-cold-start disk re-reads
    # happen inside cold_start().
    verify_artifact(args.artifact)
    artifact_path = Path(args.artifact)
    runs = list(range(1, args.runs + 1))
    result = await run_cold_starts([artifact_path] * len(runs), runs, args.display)
    digests = [s["projection"]["observedWebsiteDigest"] for s in result["starts"]]
    all_identical = len(set(digests)) == 1 and len(digests) == args.runs
    configs_unchanged = all(
        s["configUnchanged"]
        and not s["webdlTripped"]
        and s["exitStatus"] == 0
        and s["exitFileObserved"]
        and not s["profilePreExisted"]
        and s["injectedFontsAllAvailable"]
        and (s.get("jobObject") or {}).get("activeProcessCount", 0) == 0
        for s in result["starts"]
    )
    runs_complete = len(result["starts"]) == args.runs
    artifact_shas = [s["artifactFileSha256"] for s in result["starts"]]
    same_artifact_every_start = len(set(artifact_shas)) == 1 and runs_complete
    stable = (
        all_identical
        and configs_unchanged
        and runs_complete
        and same_artifact_every_start
        and result["failure"] is None
    )

    report = base_report("stability", result)
    report["stability"] = {
        "requestedRuns": args.runs,
        "completedStarts": len(result["starts"]),
        "fontModeEveryStart": [s["fontMode"] for s in result["starts"]],
        "observedWebsiteDigests": digests,
        "allObservedWebsiteDigestsIdentical": all_identical,
        "stableObservedWebsiteDigest": digests[0] if digests else None,
        "configUnchangedEveryStart": configs_unchanged,
        "diskReloadedEveryStart": same_artifact_every_start,
        "artifactFileSha256EveryStart": artifact_shas,
        "exitStatusEveryStart": [s["exitStatus"] for s in result["starts"]],
        "exitFileObservedEveryStart": [s["exitFileObserved"] for s in result["starts"]],
        "profileFreshEveryStart": [
            not s["profilePreExisted"] for s in result["starts"]
        ],
        "accepted": stable,
    }
    report["conclusion"] = conclusion(
        stable,
        "5 cold starts each re-read the artifact from disk, sent a config "
        "byte-identical to the disk artifact, and produced one identical "
        "ObservedWebsiteDigest."
        if stable
        else "Stability acceptance failed.",
    )
    write_report(Path(result["runDir"]), report)
    print(f"run-id={result['runId']}")
    return 0 if stable else 1


async def cmd_separation(args: argparse.Namespace) -> int:
    paths = [Path(p) for p in args.artifacts.split(",")]
    if len(paths) < 2:
        raise SystemExit("separation requires at least two artifacts")
    for path in paths:
        verify_artifact(path)
    ids = [verify_artifact(p)["artifactId"] for p in paths]
    if len(set(ids)) != len(ids):
        raise SystemExit("artifact ids must be distinct")
    result = await run_cold_starts(paths, list(range(1, len(paths) + 1)), args.display)
    digests = [s["projection"]["observedWebsiteDigest"] for s in result["starts"]]
    distinct = len(set(digests)) == len(digests) and len(digests) == len(paths)
    configs_unchanged = all(
        s["configUnchanged"]
        and not s["webdlTripped"]
        and s["exitStatus"] == 0
        and s["exitFileObserved"]
        and not s["profilePreExisted"]
        and s["injectedFontsAllAvailable"]
        and (s.get("jobObject") or {}).get("activeProcessCount", 0) == 0
        for s in result["starts"]
    )
    runs_complete = len(result["starts"]) == len(paths)
    accepted = (
        distinct
        and configs_unchanged
        and runs_complete
        and result["failure"] is None
    )

    report = base_report("separation", result)
    report["separation"] = {
        "fontModeEveryStart": [s["fontMode"] for s in result["starts"]],
        "artifacts": [
            {
                "path": str(paths[i]),
                "artifactId": ids[i],
                "canonicalDigest": result["starts"][i]["artifactDigest"],
                "observedWebsiteDigest": digests[i],
            }
            for i in range(len(paths))
        ],
        "observedWebsiteDigests": digests,
        "allObservedWebsiteDigestsDistinct": distinct,
        "configUnchangedEveryStart": configs_unchanged,
        "exitStatusEveryStart": [s["exitStatus"] for s in result["starts"]],
        "exitFileObservedEveryStart": [s["exitFileObserved"] for s in result["starts"]],
        "profileFreshEveryStart": [
            not s["profilePreExisted"] for s in result["starts"]
        ],
        "accepted": accepted,
    }
    report["conclusion"] = conclusion(
        accepted,
        "Different artifacts produce pairwise-distinct ObservedWebsiteDigests "
        "and every sent config matched its disk artifact."
        if accepted
        else "Separation acceptance failed.",
    )
    write_report(Path(result["runDir"]), report)
    print(f"run-id={result['runId']}")
    return 0 if accepted else 1


def _write_with_sidecar(path: Path, artifact: dict) -> None:
    path.write_text(
        json.dumps(artifact, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    sidecar = path.with_suffix(path.suffix + ".sha256")
    sidecar.write_text(
        f"{sha256_hex(path.read_bytes())}  {path.name}\n",
        encoding="utf-8",
    )


async def cmd_tamper(args: argparse.Namespace) -> int:
    source = verify_artifact(args.artifact)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    modes: list[dict] = []

    def run_mode(name: str, mutate) -> dict:
        artifact = json.loads(Path(args.artifact).read_text(encoding="utf-8"))
        mutate(artifact)
        out = out_dir / f"tampered-{name}.json"
        _write_with_sidecar(out, artifact)
        rejected = False
        error = None
        try:
            verify_artifact(out)
        except ArtifactIntegrityError as exc:
            rejected = True
            error = str(exc)
        return {
            "mode": name,
            "tamperedArtifact": str(out),
            "tamperedArtifactDigest": compute_artifact_digest(artifact),
            "expectedDigest": source["canonicalDigest"],
            "rejectedBeforeLaunch": rejected,
            "browserLaunched": False,
            "validationError": error,
        }

    modes.append(
        run_mode(
            "digest",
            lambda a: a["resolvedConfig"].__setitem__(
                "canvas:seed", (int(a["resolvedConfig"]["canvas:seed"]) + 1) % 2**32
            ),
        )
    )
    modes.append(
        run_mode(
            "missing-field",
            lambda a: a["resolvedConfig"].pop("screen.availTop", None),
        )
    )
    modes.append(
        run_mode(
            "type-error",
            lambda a: a["resolvedConfig"].__setitem__(
                "navigator.hardwareConcurrency", "8"
            ),
        )
    )
    modes.append(
        run_mode(
            "policy-mismatch",
            lambda a: a["policy"].__setitem__("window", [999, 999]),
        )
    )

    all_rejected = all(mode["rejectedBeforeLaunch"] for mode in modes)
    run_id = new_run_id()
    run_dir = M1_RUNS_DIR / run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    report = {
        "schema": REPORT_SCHEMA,
        "command": "tamper",
        "runId": run_id,
        "generatedAtUtc": utcnow(),
        "artifactId": source["artifactId"],
        "sourceArtifact": str(args.artifact),
        "sourceCanonicalDigest": source["canonicalDigest"],
        "tamperModes": modes,
        "allRejectedBeforeLaunch": all_rejected,
        "accepted": all_rejected,
        "conclusion": conclusion(
            all_rejected,
            "Digest, missing-field, type-error, and policy/config mismatch "
            "tampering are all rejected before any browser starts."
            if all_rejected
            else "At least one tamper mode was NOT rejected.",
        ),
    }
    write_report(run_dir, report)
    print(f"run-id={run_id}")
    for mode in modes:
        print(f"  {mode['mode']}: rejected={mode['rejectedBeforeLaunch']}")
    return 0 if all_rejected else 1


def base_report(command: str, result: dict) -> dict:
    first = result["starts"][0] if result["starts"] else {}
    artifact_id = first.get("artifactId")
    return {
        "schema": REPORT_SCHEMA,
        "command": command,
        "runId": result["runId"],
        "generatedAtUtc": utcnow(),
        "artifactId": artifact_id,
        "artifactDigest": first.get("artifactDigest"),
        "host": {
            "machine": _machine(),
            "cores": os.cpu_count(),
            "platform": _platform(),
        },
        "run": {
            "runDir": result["runDir"],
            "display": result["display"],
            "xvfbOwnedBySpike": result["xvfbOwnedBySpike"],
            "assetLock": result["lock"],
            "failure": result["failure"],
            "treeVerification": result["treeVerification"],
        },
        "classifications": {
            "stableWebsiteFields": STABLE_WEBSITE_SIGNAL_KEYS,
            "sessionVariableFields": SESSION_VARIABLE_SIGNAL_KEYS,
            "unavailableFields": UNAVAILABLE_SIGNALS,
            "canvasClassification": {
                "rawPixels": "stable per bundle, seed noise not observable",
                "exportEncoding": "session-variable across restarts",
                "identity": "not stable; excluded from ObservedWebsiteDigest",
            },
        },
        "coldStarts": [
            {
                "coldStartIndex": s["coldStartIndex"],
                "artifactId": s["artifactId"],
                "startedAtUtc": s["startedAtUtc"],
                "spawnSeconds": s["spawnSeconds"],
                "probeSeconds": s["probeSeconds"],
                "closeSeconds": s["closeSeconds"],
                "exitStatus": s["exitStatus"],
                "exitFileObserved": s["exitFileObserved"],
                "jobObject": s.get("jobObject"),
                "supervisorMeta": s.get("supervisorMeta"),
                "profileDir": s["profileDir"],
                "profilePreExisted": s["profilePreExisted"],
                "diskConfigDigest": s["diskConfigDigest"],
                "sentConfigDigest": s["sentConfigDigest"],
                "configUnchanged": s["configUnchanged"],
                "configDiff": s["configDiff"],
                "webdlTripped": s["webdlTripped"],
                "fontMode": s["fontMode"],
                "artifactFileSha256": s["artifactFileSha256"],
                "injectedFontsAllAvailable": s["injectedFontsAllAvailable"],
                "hostFontMasking": s["hostFontMasking"],
                "canvasObserved": s["canvasObserved"],
                "sessionVariable": s["sessionVariable"],
                "unavailable": s["unavailable"],
                "observedWebsiteSignals": s["observedWebsiteSignals"],
                "projection": s["projection"],
            }
            for s in result["starts"]
        ],
    }


def conclusion(allowed: bool, summary: str) -> dict:
    return {
        "verified": False,
        "m2GateAllowed": allowed,
        "summary": summary,
        "evidenceClass": "observed-on-this-host",
    }


def write_report(run_dir: Path, report: dict) -> None:
    run_dir = Path(run_dir)
    report_bytes = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    report_path = run_dir / "report.json"
    report_path.write_text(report_bytes, encoding="utf-8")
    (run_dir / "report.sha256").write_text(
        sha256_hex(report_bytes.encode("utf-8")) + "  report.json\n",
        encoding="utf-8",
    )
    print(f"report written to {report_path}")


def _machine() -> str:
    import platform

    return platform.machine()


def _platform() -> str:
    import platform

    return platform.platform()


def main() -> int:
    try:
        sys.stdout.reconfigure(line_buffering=True)
        sys.stderr.reconfigure(line_buffering=True)
    except Exception:
        pass
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p_stability = sub.add_parser("stability")
    p_stability.add_argument("--artifact", required=True, type=Path)
    p_stability.add_argument("--runs", type=int, default=5)
    p_stability.add_argument("--display", default=None)
    p_stability.set_defaults(func=cmd_stability)

    p_separation = sub.add_parser("separation")
    p_separation.add_argument("--artifacts", required=True, help="Comma-separated artifact paths")
    p_separation.add_argument("--display", default=None)
    p_separation.set_defaults(func=cmd_separation)

    p_tamper = sub.add_parser("tamper")
    p_tamper.add_argument("--artifact", required=True, type=Path)
    p_tamper.add_argument(
        "--out-dir",
        default=str(M1_ARTIFACT_DIR / "tampered"),
        type=Path,
    )
    p_tamper.set_defaults(func=cmd_tamper)

    args = parser.parse_args()
    return asyncio.run(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
