#!/usr/bin/env python3
"""VeriSilo M2 standalone Camoufox Host v1 (stdio protocol).

Protocol: JSON Lines on stdin/stdout, one object per line, LF-terminated.
Maximum frame size: 32 KiB (requests and responses). stdout carries ONLY
protocol frames; all logs go to stderr.

Commands: hello, launch, status, close, shutdown.

Launch requests carry artifactId/profileId/expectedArtifactFileSha256 only;
the caller can never pass arbitrary paths. Roots are fixed at process start
(--artifact-root, --profile-root, --state-root).

State machine per session: idle -> starting -> running -> closing ->
exited/failed. Profile directories hold an exclusive OS file lease; a
concurrent launch of the same profile is rejected with profile_in_use.

Every response keeps verified:false / observed-on-this-host.
"""

from __future__ import annotations

import argparse
import asyncio
import contextlib
import copy
import hashlib
import json
import os
import re
import signal
import sys
import time
import uuid
from functools import partial
from pathlib import Path
from threading import Thread
from typing import Any, Optional

from identity_policy import (
    ARTIFACT_ID_RE,
    ArtifactIntegrityError,
    UnsupportedSchemaVersionError,
    _strict_json_loads,
    build_projection,
    configured_identity_digest,
    observed_website_digest,
    verify_artifact_raw,
    verify_browser_binding,
)
from browser_tree import (
    TreeIntegrityError,
    load_tree_manifest,
    verify_tree,
)
from host_platform import (
    IS_WINDOWS,
    JobHandle,
    ProfileLock,
    ensure_no_reparse_points,
    flush_path,
    process_creation_time,
    process_identity_alive as windows_process_identity_alive,
    probe_supervisor_lock,
    replace_file_durable,
    set_binary_stdio,
    terminate_windows_job,
)
from host_fonts import (
    FONT_UNIVERSE,
    host_negative_control_families,
)
from run_identity_spike import (
    extract_observed_website_signals,
    wait_for_configured_media_devices,
)
from run_spike import (
    COOKIE_NAME,
    configure_camoufox_cache,
    DownloadGuard,
    EXECUTABLE,
    REPO_ROOT,
    SUPERVISOR,
    XDG_CACHE_DIR,
    ensure_browser_asset,
    firefox_user_prefs_for_config,
    install_download_guard,
    installed_versions,
    load_asset_lock,
    normalize_camou_config_env,
    seed_camoufox_cache,
    start_probe_server,
    start_xvfb,
    stop_xvfb,
    utcnow,
)

PROTOCOL = "verisilo-camoufox-host/v1"
HOST_VERSION = "0.1.0"
MAX_FRAME_BYTES = 32768
PROFILE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
_FRAME_TOO_LARGE = object()

SECRET_PATTERNS = [
    "password=",
    "passwd=",
    "token=",
    "secret=",
    "api_key=",
    "apikey=",
    "client_secret=",
    "authorization:",
    "bearer ",
    "-----begin",
    "VERISILO_",
    "canvas:seed",
    "audio:seed",
    "fonts:spacing_seed",
]


set_binary_stdio()


class ProtocolError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


# fd 1/2/0 may be redirected during a session (browser log, /dev/null stdin);
# protocol/log/input always go through duplicated descriptors so stdout stays
# pure protocol and stdin stays readable by the host loop.
_PROTOCOL_FD = os.dup(1)
_STDERR_FD = os.dup(2)
_STDIN_FD = os.dup(0)
_LOG_FILE: Optional[Any] = None


def _send(obj: dict) -> None:
    frame = json.dumps(
        obj, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8") + b"\n"
    if len(frame) > MAX_FRAME_BYTES:
        frame = json.dumps(
            {
                "id": obj.get("id"),
                "ok": False,
                "error": {"code": "response_too_large", "message": "response exceeds 32 KiB"},
            },
            separators=(",", ":"),
        ).encode() + b"\n"
    view = memoryview(frame)
    while view:
        written = os.write(_PROTOCOL_FD, view)
        if written <= 0:
            raise OSError("protocol stdout write made no progress")
        view = view[written:]


def _log(message: str) -> None:
    line = (message + "\n").encode("utf-8", errors="replace")
    try:
        os.write(_STDERR_FD, line)
    except OSError:
        pass
    if _LOG_FILE is not None:
        try:
            _LOG_FILE.write(line)
            _LOG_FILE.flush()
        except Exception:
            pass


# --------------------------------------------------------------------------
# Frame parsing / request validation
# --------------------------------------------------------------------------


def parse_frame(raw: bytes) -> dict:
    if len(raw) > MAX_FRAME_BYTES:
        raise ProtocolError("frame_too_large", f"frame exceeds {MAX_FRAME_BYTES} bytes")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise ProtocolError("invalid_utf8", "frame is not valid UTF-8") from exc

    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ProtocolError("duplicate_key", f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def reject_constants(token: str) -> None:
        raise ProtocolError("invalid_number", f"invalid JSON constant: {token}")

    try:
        obj = json.loads(
            text,
            object_pairs_hook=reject_duplicates,
            parse_constant=reject_constants,
        )
    except ProtocolError:
        raise
    except json.JSONDecodeError as exc:
        raise ProtocolError("invalid_json", "frame is not valid JSON") from exc
    if not isinstance(obj, dict):
        raise ProtocolError("frame_not_object", "frame must be a JSON object")
    return obj


REQUEST_FIELDS = {"id", "command", "params"}
PARAMS_FIELDS = {
    "hello": set(),
    "launch": {"artifactId", "profileId", "expectedArtifactFileSha256"},
    "status": {"sessionId"},
    "close": {"sessionId"},
    "shutdown": set(),
}


def validate_request(obj: dict) -> tuple[str, str, dict]:
    unknown = set(obj) - REQUEST_FIELDS
    if unknown:
        raise ProtocolError(
            "unknown_field", "unknown request fields: " + ", ".join(sorted(unknown))
        )
    request_id = obj.get("id")
    command = obj.get("command")
    if not isinstance(request_id, str) or not 1 <= len(request_id) <= 128:
        raise ProtocolError("bad_type", "id must be a string of 1..128 chars")
    if not isinstance(command, str) or command not in PARAMS_FIELDS:
        raise ProtocolError("unknown_command", f"unknown command: {command!r}")
    params = obj.get("params", {})
    if not isinstance(params, dict):
        raise ProtocolError("bad_type", "params must be an object")
    unknown_params = set(params) - PARAMS_FIELDS[command]
    if unknown_params:
        raise ProtocolError(
            "unknown_field",
            f"unknown params for {command}: " + ", ".join(sorted(unknown_params)),
        )
    if command == "launch":
        artifact_id = params.get("artifactId")
        profile_id = params.get("profileId")
        expected = params.get("expectedArtifactFileSha256")
        if not isinstance(artifact_id, str) or not ARTIFACT_ID_RE.match(artifact_id):
            raise ProtocolError("bad_type", "artifactId must match identity-* pattern")
        if not isinstance(profile_id, str) or not PROFILE_ID_RE.match(profile_id):
            raise ProtocolError("bad_type", "profileId has invalid format")
        if not isinstance(expected, str) or not HEX64_RE.fullmatch(expected):
            raise ProtocolError(
                "bad_type", "expectedArtifactFileSha256 must be 64 hex chars"
            )
    if command == "close" and not isinstance(params.get("sessionId"), str):
        raise ProtocolError("bad_type", "sessionId must be a string")
    if command == "status" and "sessionId" in params and not isinstance(
        params.get("sessionId"), str
    ):
        raise ProtocolError("bad_type", "sessionId must be a string")
    return request_id, command, params


# --------------------------------------------------------------------------
# Host
# --------------------------------------------------------------------------


async def close_context_bounded(ctx: Any, timeout: float) -> bool:
    """Do not let a stuck Playwright transport block Job-level cleanup."""
    task = asyncio.create_task(ctx.close())
    try:
        await asyncio.wait_for(asyncio.shield(task), timeout=timeout)
        return True
    except asyncio.TimeoutError:
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError, Exception):
            await task
        return False
    except Exception:
        return False


class CamoufoxHost:
    def __init__(
        self,
        artifact_root: Path,
        profile_root: Path,
        state_root: Path,
        tree_manifest: Path,
        display: Optional[str],
        probe_port: int = 0,
    ) -> None:
        self.artifact_root = artifact_root.absolute()
        self.profile_root = profile_root.absolute()
        self.state_root = state_root.absolute()
        self.tree_manifest = tree_manifest.absolute()
        self.display_arg = display
        self.probe_port = probe_port
        self.playwright: Any = None
        self.lock: dict = {}
        self.executable: Optional[Path] = None
        self.session: Optional[dict] = None
        self._prepare()

    def _prepare(self) -> None:
        self.artifact_root.mkdir(parents=True, exist_ok=True)
        self.profile_root.mkdir(parents=True, exist_ok=True)
        self.state_root.mkdir(parents=True, exist_ok=True)
        ensure_no_reparse_points(self.artifact_root)
        ensure_no_reparse_points(self.profile_root)
        ensure_no_reparse_points(self.state_root)
        lock = load_asset_lock()
        if lock.get("digestAgreement") is not True:
            raise SystemExit("asset lock digestAgreement is not true")
        executable = ensure_browser_asset(lock, allow_download=False)
        cache_root = Path(
            os.environ.get("VERISILO_CAMOUFOX_CACHE_DIR", str(XDG_CACHE_DIR))
        )
        install_dir = configure_camoufox_cache(cache_root)
        seed_camoufox_cache(lock, executable, install_dir=install_dir)
        if not SUPERVISOR.exists():
            raise SystemExit(f"missing native supervisor: {SUPERVISOR}")
        if not IS_WINDOWS:
            SUPERVISOR.chmod(0o755)
        install_download_guard()
        DownloadGuard.reset()
        self.lock = lock
        self.executable = executable

    def set_playwright(self, playwright: Any) -> None:
        self.playwright = playwright

    def _state(self) -> str:
        if self.session is None:
            return "idle"
        return self.session["state"]

    def hello(self) -> dict:
        return {
            "protocol": PROTOCOL,
            "hostVersion": HOST_VERSION,
            "pythonVersion": sys.version.split()[0],
            "artifactRoot": str(self.artifact_root),
            "profileRoot": str(self.profile_root),
            "stateRoot": str(self.state_root),
            "maxFrameBytes": MAX_FRAME_BYTES,
            "probePortPolicy": "fixed" if self.probe_port else "ephemeral",
            "browserRelease": self.lock["release"],
            "assetSha256": self.lock["sha256"],
            "treeManifest": str(self.tree_manifest),
            "treeManifestSha256": hashlib.sha256(
                self.tree_manifest.read_bytes()
            ).hexdigest(),
            "platform": "windows-x64" if IS_WINDOWS else "linux-x64",
            "state": self._state(),
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }

    # -- launch ------------------------------------------------------------

    async def launch(
        self, artifact_id: str, profile_id: str, expected_sha: str
    ) -> dict:
        if self.session is not None and self.session["state"] in (
            "starting",
            "running",
            "closing",
        ):
            raise ProtocolError(
                "session_busy", f"session {self.session['sessionId']} is active"
            )
        artifact_path = self.artifact_root / f"{artifact_id}.json"
        if not artifact_path.is_file():
            raise ProtocolError("artifact_not_found", f"artifact {artifact_id} not found")
        artifact, file_sha = verify_artifact_raw(
            artifact_path, expected_file_sha=expected_sha
        )
        verify_browser_binding(
            artifact, self.lock, self.executable, installed_versions()
        )
        verify_tree(self.executable.parent, load_tree_manifest(self.tree_manifest))

        profile_dir = self.profile_root / profile_id
        profile_dir.mkdir(parents=True, exist_ok=True)
        ensure_no_reparse_points(profile_dir)
        lock_path = self.profile_root / f"{profile_id}.lock"
        try:
            profile_lock = ProfileLock.acquire(lock_path)
            if IS_WINDOWS and not probe_supervisor_lock(lock_path):
                profile_lock.release()
                raise OSError("supervisor lock byte is already held")
        except OSError as exc:
            raise ProtocolError(
                "profile_in_use",
                f"profile {profile_id} is locked by another session: {exc}",
            ) from exc
        # A prior Host may have quarantined this profile. The new Host may
        # only clean the record and take over after every recorded process
        # identity (PID + starttime) is gone.
        quarantine_check = clear_quarantine_if_stale(self.state_root, profile_id)
        if quarantine_check["alive"] or quarantine_check.get("invalid"):
            profile_lock.release()
            reason = quarantine_check.get("invalid") or (
                "original process is still alive: "
                + json.dumps(quarantine_check["alive"])
            )
            raise ProtocolError(
                "profile_quarantined",
                f"profile {profile_id} is quarantined; {reason}",
            )
        if quarantine_check["cleared"]:
            _log(
                f"profile {profile_id}: cleared stale quarantine "
                "(recorded process identities no longer exist)"
            )

        session_id = uuid.uuid4().hex
        session_dir = self.state_root / session_id
        session_dir.mkdir(parents=True, exist_ok=False)
        browser_log = session_dir / "browser.log"
        log_fh = browser_log.open("ab")
        if not IS_WINDOWS:
            os.dup2(log_fh.fileno(), 1)
            os.dup2(log_fh.fileno(), 2)
            devnull = os.open(os.devnull, os.O_RDONLY)
            os.dup2(devnull, 0)
            os.close(devnull)

        session = {
            "sessionId": session_id,
            "sessionDir": session_dir,
            "artifactId": artifact_id,
            "profileId": profile_id,
            "profileDir": profile_dir,
            "artifactFileSha256": file_sha,
            "artifactDigest": artifact["canonicalDigest"],
            "state": "starting",
            "profileLock": profile_lock,
            "lockFd": profile_lock.handle_value,
            "logFh": log_fh,
            "ctx": None,
            "pid": None,
            "childPid": None,
            "supervisorMeta": None,
            "managedIdentities": [],
            "jobHandle": None,
            "exitFile": session_dir / "exit.json",
            "exitFileObserved": False,
            "processTreeExit": None,
            "configuredIdentityDigest": None,
            "observedWebsiteDigest": None,
            "observedSignals": None,
            "exitStatus": None,
            "failure": None,
            "xvfb": None,
            "server": None,
            "stopMonitor": asyncio.Event(),
            "monitorTask": None,
            "bootCountBefore": None,
            "bootCountAfter": None,
            "spawnSeconds": None,
            "probeSeconds": None,
            "fontMode": None,
            "cookieEvidence": None,
            "cookieSqlite": None,
            "probePort": None,
            "quarantine": None,
        }
        self.session = session
        _log(
            f"session {session_id}: launching artifact={artifact_id} "
            f"profile={profile_id}"
        )
        try:
            await self._launch_browser(session, artifact)
        except ProtocolError as exc:
            await self._fail_session(session, f"launch rejected: {exc}")
            raise
        except Exception as exc:  # noqa: BLE001
            await self._fail_session(session, f"{type(exc).__name__}: {exc}")
            raise ProtocolError("launch_failed", f"{type(exc).__name__}: {exc}") from exc
        return {
            "sessionId": session_id,
            "state": session["state"],
            "artifactId": artifact_id,
            "profileId": profile_id,
            "artifactFileSha256": file_sha,
            "configuredIdentityDigest": session["configuredIdentityDigest"],
            "observedWebsiteDigest": session["observedWebsiteDigest"],
            "bootCountBefore": session["bootCountBefore"],
            "bootCountAfter": session["bootCountAfter"],
            "spawnSeconds": session["spawnSeconds"],
            "probeSeconds": session["probeSeconds"],
            "fontMode": session.get("fontMode"),
            "managedPids": managed_pids(session),
            "cookieEvidence": session["cookieEvidence"],
            "probePort": session.get("probePort"),
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }

    async def _launch_browser(self, session: dict, artifact: dict) -> None:
        from camoufox import AsyncNewBrowser
        from camoufox import DefaultAddons
        from camoufox.utils import launch_options

        policy = artifact["policy"]
        window = tuple(policy["window"])
        disk_config = copy.deepcopy(artifact["resolvedConfig"])
        disk_digest = configured_identity_digest(disk_config)

        display = None if IS_WINDOWS else (self.display_arg or os.environ.get("DISPLAY"))
        xvfb = None
        if not IS_WINDOWS and not display:
            display, xvfb = start_xvfb()
        session["xvfb"] = xvfb
        server, probe_url = start_probe_server(self.probe_port)
        session["server"] = server
        # Remember the actual probe port: later launches (same Host process,
        # or a restarted Host given this port) keep the cookie / localStorage
        # origin stable.
        self.probe_port = server.server_address[1]
        session["probePort"] = self.probe_port
        os.environ["VERISILO_REAL_EXE"] = str(self.executable)
        os.environ["VERISILO_EXIT_FILE"] = str(session["exitFile"])
        os.environ["VERISILO_SUPERVISOR_FILE"] = str(
            session["sessionDir"] / "supervisor.json"
        )
        if IS_WINDOWS:
            os.environ["VERISILO_PROFILE_LOCK_PATH"] = str(
                self.profile_root / f"{session['profileId']}.lock"
            )
            os.environ["VERISILO_JOB_NAME"] = (
                f"Local\\VeriSiloCamoufox-{session['sessionId']}"
            )

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
                executable_path=str(self.executable),
                user_data_dir=str(session["profileDir"]),
                virtual_display=display,
                firefox_user_prefs=firefox_user_prefs_for_config(disk_config),
                exclude_addons=[DefaultAddons.UBO],
                i_know_what_im_doing=True,
            ),
        )
        sent_config, diff, opts["env"] = normalize_camou_config_env(
            opts["env"], disk_config
        )
        sent_digest = configured_identity_digest(sent_config)
        if (
            sent_digest != disk_digest
            or diff["added"]
            or diff["removed"]
            or diff["changed"]
        ):
            raise ProtocolError(
                "config_mutation",
                "launch_options mutated the disk config: "
                + json.dumps(
                    {
                        "diskDigest": disk_digest,
                        "sentDigest": sent_digest,
                        "diff": diff,
                    }
                ),
            )
        opts["executable_path"] = str(SUPERVISOR)

        ctx = await AsyncNewBrowser(
            self.playwright,
            from_options=opts,
            persistent_context=True,
        )
        session["ctx"] = ctx
        if DownloadGuard.tripped:
            raise ProtocolError(
                "webdl_attempted", "unpinned download attempted during launch"
            )
        spawn_seconds = time.perf_counter() - launch_start

        # Managed-process identity comes from the supervisor's own status
        # file. Windows additionally requires a named Job Object; the Host
        # never treats PID enumeration as process-tree ownership.
        supervisor_path = session["sessionDir"] / "supervisor.json"
        supervisor_meta: Optional[dict] = None
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if supervisor_path.exists():
                try:
                    candidate = json.loads(supervisor_path.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError):
                    candidate = None
                if (
                    isinstance(candidate, dict)
                    and isinstance(candidate.get("supervisorPid"), int)
                    and isinstance(candidate.get("childPid"), int)
                    and (
                        not IS_WINDOWS
                        or (
                            isinstance(candidate.get("jobName"), str)
                            and isinstance(candidate.get("supervisorCreationTime100ns"), int)
                            and isinstance(candidate.get("childCreationTime100ns"), int)
                            and candidate.get("jobKillOnClose") is True
                            and candidate.get("jobAssignmentVerified") is True
                            and candidate.get("processHandleEvidence") is True
                        )
                    )
                ):
                    supervisor_meta = candidate
                    break
            await asyncio.sleep(0.1)
        if supervisor_meta is None:
            raise ProtocolError(
                "supervisor_metadata_missing",
                "supervisor status file missing or invalid",
            )
        session["supervisorMeta"] = supervisor_meta
        session["pid"] = supervisor_meta["supervisorPid"]
        session["childPid"] = supervisor_meta.get("childPid")
        session["managedIdentities"] = managed_identities(session)
        if IS_WINDOWS:
            try:
                session["jobHandle"] = JobHandle.open(supervisor_meta["jobName"])
            except OSError as exc:
                raise ProtocolError(
                    "job_unavailable", f"cannot open supervisor Job Object: {exc}"
                ) from exc

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
        media_readiness = await wait_for_configured_media_devices(page, disk_config)
        probe_start = time.perf_counter()
        observed = await page.evaluate("window.__probe.readIdentity()")
        session["probeSeconds"] = round(time.perf_counter() - probe_start, 3)
        session["spawnSeconds"] = round(spawn_seconds, 3)

        font_mode = policy.get("fontMode", "inherit")
        session["fontMode"] = font_mode
        host_controls_result = {
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
        if font_mode == "managed" and not host_controls_result["allUnavailable"]:
            raise ProtocolError(
                "host_font_masking_failed",
                "managed font mode requires all host negative controls "
                "unavailable; masking failed: "
                + ", ".join(host_controls_result["failures"]),
            )

        boot_before = int(observed.get("bootCount", 0))
        await page.evaluate(f"window.__probe.writeBootCount({boot_before + 1})")
        session["bootCountBefore"] = boot_before
        session["bootCountAfter"] = boot_before + 1

        cookie_evidence = await _collect_cookie_evidence(
            ctx, page, boot_before, session["sessionId"]
        )
        session["cookieEvidence"] = cookie_evidence

        signals = extract_observed_website_signals(observed, font_mode)
        session["configuredIdentityDigest"] = disk_digest
        session["observedWebsiteDigest"] = observed_website_digest(signals)
        session["observedSignals"] = signals
        projection = build_projection(
            artifact["artifactId"],
            session["sessionId"],
            1,
            disk_digest,
            signals,
        )
        observed_payload = {
            "generatedAtUtc": utcnow(),
            "projection": projection,
            "observedFull": observed,
            "hostFontControls": host_controls,
            "hostFontMasking": host_controls_result,
            "mediaDeviceReadiness": media_readiness,
            "fontMode": font_mode,
            "cookieEvidence": cookie_evidence,
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }
        (session["sessionDir"] / "observed.json").write_text(
            json.dumps(observed_payload, indent=2) + "\n"
        )

        session["state"] = "running"
        session["stopMonitor"] = asyncio.Event()
        session["monitorTask"] = asyncio.create_task(
            self._monitor_session(session)
        )
        write_session_state(session)

    async def _monitor_session(self, session: dict) -> None:
        while not session["stopMonitor"].is_set():
            identities = managed_identities(session)
            process_failure = identities and not all(
                proc_identity_alive(identity) for identity in identities
            )
            job_failure = False
            if IS_WINDOWS and session.get("jobHandle") is not None:
                try:
                    job_failure = session["jobHandle"].active_process_count() == 0
                except OSError:
                    job_failure = True
            if process_failure or job_failure:
                await self._fail_session(
                    session,
                    "browser process exited unexpectedly",
                )
                return
            try:
                await asyncio.wait_for(session["stopMonitor"].wait(), timeout=0.5)
            except asyncio.TimeoutError:
                pass

    async def _fail_session(self, session: dict, error: str) -> None:
        session["state"] = "failed"
        session["failure"] = error
        session["exitStatus"] = read_exit_status(session["exitFile"])
        session["exitFileObserved"] = session["exitFile"].exists()
        session["stopMonitor"].set()
        if session["monitorTask"] is not None and session["monitorTask"] is not asyncio.current_task():
            try:
                await asyncio.wait_for(session["monitorTask"], timeout=3)
            except asyncio.TimeoutError:
                session["monitorTask"].cancel()
        ctx = session.get("ctx")
        if ctx is not None:
            await close_context_bounded(ctx, timeout=5)
            session["ctx"] = None
        # Confirm the whole managed process tree is gone BEFORE releasing the
        # profile lock, so no second Host can ever touch the same profile
        # while the old browser is still alive.
        session["processTreeExit"] = terminate_managed_tree(session, timeout=6)
        if not session["processTreeExit"].get("exited", False):
            await self._quarantine_session(
                session,
                f"{error}; managed process tree did not exit",
                session["processTreeExit"].get("remaining", []),
            )
            return
        session["pid"] = None
        session["childPid"] = None
        await release_session(self, session)
        write_session_state(session)
        _log(f"session {session['sessionId']} failed: {error}")

    async def _quarantine_session(
        self, session: dict, reason: str, remaining: list[dict]
    ) -> None:
        """Fail-closed state: a managed process is still alive, so the profile
        lock is KEPT by this Host and a persistent quarantine record is
        written. The session is never marked exited."""
        session["state"] = "quarantined"
        session["failure"] = reason
        record_path: Optional[Path] = None
        try:
            record_path = write_quarantine_record(
                self.state_root, session, reason, remaining
            )
            session["quarantine"] = {
                "reason": reason,
                "processes": remaining,
                "recordPath": str(record_path),
            }
        except OSError as exc:
            # Fail-closed: even without a persisted record the lock is kept
            # and the state is quarantined; the write error is recorded.
            session["quarantine"] = {
                "reason": reason,
                "processes": remaining,
                "recordPath": None,
                "writeError": f"{type(exc).__name__}: {exc}",
            }
        # Clean up server/Xvfb/log fds, but DO NOT release the profile lock.
        await release_session(self, session, release_lock=False)
        write_session_state(session)
        _log(
            f"session {session['sessionId']} QUARANTINED: {reason} "
            f"(lock retained, record {record_path})"
        )

    def status(self, session_id: Optional[str]) -> dict:
        session = self.session
        if session_id is not None:
            if session is None or session["sessionId"] != session_id:
                raise ProtocolError("session_not_found", f"no session {session_id}")
        if session is None:
            return {"state": "idle"}
        return {
            "state": session["state"],
            "sessionId": session["sessionId"],
            "artifactId": session["artifactId"],
            "profileId": session["profileId"],
            "artifactFileSha256": session["artifactFileSha256"],
            "configuredIdentityDigest": session["configuredIdentityDigest"],
            "observedWebsiteDigest": session["observedWebsiteDigest"],
            "exitStatus": session["exitStatus"],
            "exitFileObserved": session.get("exitFileObserved"),
            "quarantine": session.get("quarantine"),
            "failure": session["failure"],
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }

    async def close(self, session_id: str) -> dict:
        session = self.session
        if session is None or session["sessionId"] != session_id:
            raise ProtocolError("session_not_found", f"no session {session_id}")
        if session["state"] in ("exited", "failed", "quarantined"):
            return {
                "sessionId": session["sessionId"],
                "state": session["state"],
                "exitStatus": session["exitStatus"],
                "exitFileObserved": session.get("exitFileObserved"),
                "processTreeExit": session.get("processTreeExit"),
                "quarantine": session.get("quarantine"),
            }
        session["state"] = "closing"
        session["stopMonitor"].set()
        if session["monitorTask"] is not None:
            try:
                await asyncio.wait_for(session["monitorTask"], timeout=5)
            except asyncio.TimeoutError:
                session["monitorTask"].cancel()
        close_start = time.perf_counter()
        ctx = session.get("ctx")
        if ctx is not None:
            if not await close_context_bounded(ctx, timeout=10):
                _log(f"session {session['sessionId']}: ctx.close() timed out, terminating Job")
        session["ctx"] = None
        # close() must CONFIRM the managed process tree is fully gone (and
        # terminate it if not) before the profile lock is released.
        session["processTreeExit"] = terminate_managed_tree(session, timeout=8)
        if not session["processTreeExit"].get("exited", False):
            await self._quarantine_session(
                session,
                "close: managed process tree did not exit",
                session["processTreeExit"].get("remaining", []),
            )
            return {
                "sessionId": session["sessionId"],
                "state": session["state"],
                "exitStatus": session["exitStatus"],
                "exitFileObserved": session.get("exitFileObserved"),
                "processTreeExit": session["processTreeExit"],
                "quarantine": session.get("quarantine"),
                "closeSeconds": round(time.perf_counter() - close_start, 3),
            }
        session["pid"] = None
        session["childPid"] = None
        session["exitFileObserved"] = session["exitFile"].exists()
        session["exitStatus"] = read_exit_status(session["exitFile"])
        session["cookieSqlite"] = read_cookie_sqlite_evidence(
            session["profileDir"]
        )
        session["state"] = "exited"
        session["closeSeconds"] = round(time.perf_counter() - close_start, 3)
        await release_session(self, session)
        write_session_state(session)
        return {
            "sessionId": session["sessionId"],
            "state": session["state"],
            "exitStatus": session["exitStatus"],
            "exitFileObserved": session["exitFileObserved"],
            "processTreeExit": session["processTreeExit"],
            "cookieSqlite": session["cookieSqlite"],
            "closeSeconds": session["closeSeconds"],
        }


def _reassemble_config(env: dict) -> dict:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    if not chunks:
        raise RuntimeError("launch_options returned no CAMOU_CONFIG env chunks")
    return json.loads("".join(value for _, value in chunks))


def proc_starttime_ticks(pid: int) -> Optional[int]:
    """Field 22 of /proc/<pid>/stat (starttime in clock ticks)."""
    if IS_WINDOWS:
        return None
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return None
    fields = text.rsplit(")", 1)
    if len(fields) != 2:
        return None
    parts = fields[1].split()
    try:
        return int(parts[19])
    except (IndexError, ValueError):
        return None


def proc_identity_alive(identity: dict) -> bool:
    """A process is 'ours' only when BOTH the PID exists AND its starttime
    matches the supervisor-recorded identity. A reused PID with a different
    starttime is never treated as the original process. Zombies (state Z)
    are not running and are not alive."""
    if IS_WINDOWS:
        return windows_process_identity_alive(identity)
    pid = identity.get("pid")
    expected = identity.get("startTimeTicks")
    if not isinstance(pid, int) or pid <= 0:
        return False
    if not isinstance(expected, int) or expected <= 0:
        return False
    actual = proc_starttime_ticks(pid)
    if actual is None or actual != expected:
        return False
    try:
        stat = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return False
    fields = stat.rsplit(")", 1)
    if len(fields) != 2:
        return False
    return fields[1].split()[0].strip() != "Z"


def process_descendants(pid: int) -> list[int]:
    """All live process IDs in the tree rooted at pid (pid itself first)."""
    if IS_WINDOWS:
        raise RuntimeError("Windows process containment is provided by Job Objects")
    seen: set[int] = set()
    stack = [pid]
    result: list[int] = []
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        result.append(current)
        try:
            children = Path(f"/proc/{current}/task/{current}/children").read_text()
        except OSError:
            continue
        for child in children.split():
            stack.append(int(child))
    return result


def managed_pids(session: dict) -> list[int]:
    return sorted(
        {
            pid
            for pid in (session.get("pid"), session.get("childPid"))
            if isinstance(pid, int) and pid > 0
        }
    )


def managed_identities(session: dict) -> list[dict]:
    """Supervisor/browser identities from the supervisor's own metadata.
    Identities without a start-time are NOT considered verified and are not
    killable (we never guess ownership)."""
    meta = session.get("supervisorMeta")
    identities: list[dict] = []
    if isinstance(meta, dict):
        if IS_WINDOWS:
            if isinstance(meta.get("supervisorPid"), int):
                identities.append(
                    {
                        "pid": meta["supervisorPid"],
                        "creationTime100ns": meta.get("supervisorCreationTime100ns"),
                        "role": "supervisor",
                    }
                )
            if isinstance(meta.get("childPid"), int):
                identities.append(
                    {
                        "pid": meta["childPid"],
                        "creationTime100ns": meta.get("childCreationTime100ns"),
                        "role": "browser",
                    }
                )
            return identities
        if isinstance(meta.get("supervisorPid"), int):
            identities.append(
                {
                    "pid": meta["supervisorPid"],
                    "startTimeTicks": meta.get("supervisorStartTimeTicks"),
                    "processGroup": meta.get("supervisorProcessGroup"),
                    "role": "supervisor",
                }
            )
        if isinstance(meta.get("childPid"), int):
            identities.append(
                {
                    "pid": meta["childPid"],
                    "startTimeTicks": meta.get("childStartTimeTicks"),
                    "processGroup": meta.get("childProcessGroup"),
                    "role": "browser",
                }
            )
    return identities


def _identity_matches_live(target: dict) -> bool:
    if IS_WINDOWS:
        return windows_process_identity_alive(target)
    expected = target.get("startTimeTicks")
    if not isinstance(expected, int) or expected <= 0:
        return False
    return proc_starttime_ticks(target.get("pid")) == expected


def terminate_managed_tree(session: dict, timeout: float = 8.0) -> dict:
    """Terminate every managed supervisor/browser descendant and CONFIRM the
    tree is gone. Every signal is sent only after re-verifying the target's
    PID+starttime identity (no PID-reuse mis-kill). exited=True is returned
    ONLY when every captured PID+starttime identity (roots AND descendants)
    is gone; surviving descendants are reported in `remaining` so the caller
    can quarantine instead of releasing the profile lock."""
    if IS_WINDOWS:
        return terminate_windows_job(session, timeout=timeout)

    roots = [identity for identity in managed_identities(session) if proc_identity_alive(identity)]
    if not roots:
        return {
            "exited": True,
            "managedIdentities": [],
            "remaining": [],
            "sigterm": False,
            "sigkill": False,
        }
    targets: list[dict] = []
    seen: set[int] = set()
    for root in roots:
        for descendant_pid in process_descendants(root["pid"]):
            if descendant_pid not in seen:
                seen.add(descendant_pid)
                targets.append(
                    {
                        "pid": descendant_pid,
                        "startTimeTicks": proc_starttime_ticks(descendant_pid),
                    }
                )
    for target in reversed(targets):  # children first, supervisors last
        if not _identity_matches_live(target):
            continue
        try:
            os.kill(target["pid"], signal.SIGTERM)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        alive_targets = [target for target in targets if proc_identity_alive(target)]
        if not alive_targets:
            return {
                "exited": True,
                "managedIdentities": roots,
                "remaining": [],
                "sigterm": True,
                "sigkill": False,
            }
        # Continuously capture descendants spawned after the first
        # enumeration while their parent is still alive, so they are also
        # confirmed gone before exited=true.
        for base in alive_targets:
            for descendant_pid in process_descendants(base["pid"]):
                if descendant_pid in seen:
                    continue
                seen.add(descendant_pid)
                targets.append(
                    {
                        "pid": descendant_pid,
                        "startTimeTicks": proc_starttime_ticks(descendant_pid),
                    }
                )
        time.sleep(0.05)
    # Re-enumerate from every still-alive captured target (an orphaned
    # descendant can spawn its own children after the root dies).
    kill_targets: list[dict] = []
    seen_kill: set[int] = set()
    for base in alive_targets:
        for descendant_pid in process_descendants(base["pid"]):
            if descendant_pid in seen_kill:
                continue
            seen_kill.add(descendant_pid)
            kill_targets.append(
                {
                    "pid": descendant_pid,
                    "startTimeTicks": proc_starttime_ticks(descendant_pid),
                }
            )
    for target in kill_targets:
        if not _identity_matches_live(target):
            continue
        try:
            os.kill(target["pid"], signal.SIGKILL)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        alive_targets = [target for target in kill_targets if proc_identity_alive(target)]
        if not alive_targets:
            return {
                "exited": True,
                "managedIdentities": roots,
                "remaining": [],
                "sigterm": True,
                "sigkill": True,
            }
        time.sleep(0.05)
    return {
        "exited": False,
        "managedIdentities": roots,
        "sigterm": True,
        "sigkill": True,
        "remaining": [
            target for target in kill_targets if proc_identity_alive(target)
        ],
    }


def quarantine_record_path(state_root: Path, profile_id: str) -> Path:
    return Path(state_root) / "quarantine" / f"{profile_id}.json"


QUARANTINE_SCHEMA = "verisilo-camoufox-profile-quarantine/v1"
QUARANTINE_ALLOWED_KEYS = {
    "schema",
    "profileId",
    "sessionId",
    "artifactId",
    "artifactFileSha256",
    "createdAtUtc",
    "reason",
    "processes",
    "evidenceClass",
}


def _atomic_write_text(path: Path, text: str) -> None:
    """Same-directory temp file + fsync + os.replace + directory fsync, so a
    crash mid-write can never leave a partially-written quarantine record."""
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.parent / f"{path.name}.tmp-{uuid.uuid4().hex}"
    fd: Optional[int] = None
    try:
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, "wb") as fh:
            fd = None
            fh.write(text.encode("utf-8"))
            fh.flush()
            os.fsync(fh.fileno())
        if IS_WINDOWS:
            flush_path(tmp)
            replace_file_durable(tmp, path)
            flush_path(path)
        else:
            os.replace(tmp, path)
            dir_fd = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(dir_fd)
            finally:
                os.close(dir_fd)
    except OSError:
        if fd is not None:
            try:
                os.close(fd)
            except OSError:
                pass
        try:
            tmp.unlink()
        except OSError:
            pass
        raise


def _validate_quarantine_record(record: dict) -> list[str]:
    """Strict closed schema for quarantine records: exact key set, exact
    types. A record that fails this can never be trusted for takeover."""
    errors: list[str] = []
    unknown = set(record) - QUARANTINE_ALLOWED_KEYS
    if unknown:
        errors.append("unknown keys: " + ", ".join(sorted(unknown)))
    if record.get("schema") != QUARANTINE_SCHEMA:
        errors.append(f"schema must be {QUARANTINE_SCHEMA!r}")
    for key in ("profileId", "sessionId", "artifactId", "evidenceClass"):
        if type(record.get(key)) is not str or not record[key]:
            errors.append(f"{key} must be a non-empty string")
    if type(record.get("artifactFileSha256")) is not str or not HEX64_RE.fullmatch(
        record.get("artifactFileSha256", "")
    ):
        errors.append("artifactFileSha256 must be 64 hex chars")
    if type(record.get("createdAtUtc")) is not str or not record["createdAtUtc"]:
        errors.append("createdAtUtc must be a non-empty string")
    if type(record.get("reason")) is not str or not record["reason"]:
        errors.append("reason must be a non-empty string")
    processes = record.get("processes")
    if type(processes) is not list:
        errors.append("processes must be a list")
    else:
        for index, proc in enumerate(processes):
            prefix = f"processes[{index}]"
            if type(proc) is not dict:
                errors.append(f"{prefix} must be an object")
                continue
            process_keys = (
                {"pid", "creationTime100ns", "role"}
                if IS_WINDOWS
                else {"pid", "startTimeTicks", "processGroup", "role"}
            )
            unknown_proc = set(proc) - process_keys
            if unknown_proc:
                errors.append(
                    f"{prefix} unknown keys: " + ", ".join(sorted(unknown_proc))
                )
            if type(proc.get("pid")) is not int or proc.get("pid", 0) <= 0:
                errors.append(f"{prefix}.pid must be a positive int")
            if IS_WINDOWS:
                if (
                    type(proc.get("creationTime100ns")) is not int
                    or proc.get("creationTime100ns", 0) <= 0
                ):
                    errors.append(f"{prefix}.creationTime100ns must be a positive int")
            else:
                if type(proc.get("startTimeTicks")) is not int or proc.get("startTimeTicks", 0) <= 0:
                    errors.append(f"{prefix}.startTimeTicks must be a positive int")
                pgrp = proc.get("processGroup")
                if pgrp is not None and type(pgrp) is not int:
                    errors.append(f"{prefix}.processGroup must be int or null")
            if type(proc.get("role")) is not str or not proc["role"]:
                errors.append(f"{prefix}.role must be a non-empty string")
    return errors


def write_quarantine_record(
    state_root: Path,
    session: dict,
    reason: str,
    remaining: list[dict],
) -> Path:
    """Persistent, machine-readable quarantine record. A new Host must verify
    every recorded PID+starttime identity is gone before it may clean the
    record and take over the profile. The write is atomic (temp + fsync +
    replace); failures raise and the caller stays fail-closed."""
    path = quarantine_record_path(state_root, session["profileId"])
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "schema": "verisilo-camoufox-profile-quarantine/v1",
        "profileId": session["profileId"],
        "sessionId": session["sessionId"],
        "artifactId": session["artifactId"],
        "artifactFileSha256": session["artifactFileSha256"],
        "createdAtUtc": utcnow(),
        "reason": reason,
        "processes": [
            (
                {
                    "pid": identity.get("pid"),
                    "creationTime100ns": identity.get("creationTime100ns"),
                    "role": identity.get("role"),
                }
                if IS_WINDOWS
                else {
                    "pid": identity.get("pid"),
                    "startTimeTicks": identity.get("startTimeTicks"),
                    "processGroup": identity.get("processGroup"),
                    "role": identity.get("role"),
                }
            )
            for identity in remaining
        ],
        "evidenceClass": "observed-on-this-host",
    }
    _atomic_write_text(path, json.dumps(payload, indent=2) + "\n")
    return path


def read_quarantine_record(
    state_root: Path, profile_id: str
) -> tuple[str, Optional[dict], Optional[str]]:
    """Tri-state read: ('absent'|'valid'|'invalid', record, error). Any
    existing-but-unreadable or schema-invalid record is 'invalid' and must
    block takeover — it is never treated as absent."""
    path = quarantine_record_path(state_root, profile_id)
    try:
        ensure_no_reparse_points(path, allow_missing=True)
    except OSError as exc:
        return "invalid", None, f"reparse quarantine path rejected: {exc}"
    if not path.exists():
        return "absent", None, None
    try:
        record = _strict_json_loads(path.read_bytes())
    except (OSError, ArtifactIntegrityError) as exc:
        return "invalid", None, f"unreadable quarantine record: {exc}"
    if type(record) is not dict:
        return "invalid", None, "quarantine record is not an object"
    errors = _validate_quarantine_record(record)
    if errors:
        return "invalid", None, "invalid quarantine record: " + "; ".join(errors)
    return "valid", record, None


def quarantine_processes_alive(record: dict) -> list[dict]:
    return [
        proc
        for proc in record.get("processes", [])
        if isinstance(proc, dict) and proc_identity_alive(proc)
    ]


def clear_quarantine_if_stale(state_root: Path, profile_id: str) -> dict:
    """A new Host may only clear the quarantine after every recorded process
    identity (PID + starttime) is gone. Otherwise it reports the still-alive
    identities (or the validation error) and the caller must NOT take over
    the profile. A file that exists but cannot be validated is fail-closed."""
    status, record, error = read_quarantine_record(state_root, profile_id)
    if status == "absent":
        return {
            "recordPresent": False,
            "cleared": False,
            "alive": [],
            "invalid": None,
        }
    if status == "invalid":
        return {
            "recordPresent": True,
            "cleared": False,
            "alive": [],
            "invalid": error,
        }
    alive = quarantine_processes_alive(record)
    if alive:
        return {
            "recordPresent": True,
            "cleared": False,
            "alive": alive,
            "invalid": None,
        }
    try:
        quarantine_record_path(state_root, profile_id).unlink()
    except OSError as exc:
        return {
            "recordPresent": True,
            "cleared": False,
            "alive": [],
            "invalid": f"cannot remove stale quarantine record: {exc}",
        }
    return {
        "recordPresent": True,
        "cleared": True,
        "alive": [],
        "invalid": None,
    }


async def _collect_cookie_evidence(
    ctx: Any,
    page: Any,
    boot_before: int,
    session_id: str,
) -> dict:
    """First-boot cookie write + readback; later boots prove the cookie
    persisted from the previous Host process. Origin is the probe server's,
    which is fixed across Host restarts in the persistence test."""
    evidence = {
        "cookieAbsentBeforeWrite": None,
        "cookieInApi": None,
        "cookieOnPage": None,
        "cookieValueLooksManaged": None,
    }
    api_before = await ctx.cookies()
    cookie_before = any(c["name"] == COOKIE_NAME for c in api_before)
    if boot_before == 0:
        # Evidence only, not a hard rejection: after a browser crash a
        # persistent profile can legitimately hold the cookie while
        # localStorage (the bootCount origin state) was not flushed.
        evidence["cookieAbsentBeforeWrite"] = not cookie_before
        cookie_value = f"m2-{session_id}-cookie"
        await ctx.add_cookies(
            [
                {
                    "name": COOKIE_NAME,
                    "value": cookie_value,
                    "url": page.url.rsplit("/", 1)[0] + "/",
                    "expires": int(time.time()) + 30 * 86400,
                }
            ]
        )
        await page.reload(wait_until="domcontentloaded")
        api_after = await ctx.cookies()
        page_cookie = await page.evaluate("document.cookie")
        evidence["cookieInApi"] = any(
            c["name"] == COOKIE_NAME for c in api_after
        )
        evidence["cookieOnPage"] = cookie_value in page_cookie
        evidence["cookieValueLooksManaged"] = any(
            c["name"] == COOKIE_NAME
            and str(c.get("value", "")).startswith("m2-")
            for c in api_after
        )
    else:
        evidence["cookieAbsentBeforeWrite"] = False
        evidence["cookieInApi"] = cookie_before
        page_cookie = await page.evaluate("document.cookie")
        evidence["cookieOnPage"] = COOKIE_NAME in page_cookie
        evidence["cookieValueLooksManaged"] = any(
            c["name"] == COOKIE_NAME
            and str(c.get("value", "")).startswith("m2-")
            for c in api_before
        )
    return evidence


def read_cookie_sqlite_evidence(profile_dir: Path) -> dict:
    """cookies.sqlite evidence: file presence/size plus a best-effort read of
    the actual moz_cookies row for the probe cookie (Firefox schema)."""
    db = profile_dir / "cookies.sqlite"
    result = {
        "fileExists": db.exists(),
        "fileBytes": db.stat().st_size if db.exists() else 0,
        "cookieNamePresent": None,
    }
    if not db.exists():
        result["cookieNamePresent"] = False
        return result
    try:
        import sqlite3

        conn = sqlite3.connect(f"file:{db}?mode=ro", uri=True)
        try:
            rows = conn.execute(
                "SELECT name, host, value FROM moz_cookies WHERE name = ?",
                (COOKIE_NAME,),
            ).fetchall()
        finally:
            conn.close()
    except Exception as exc:  # noqa: BLE001 - evidence only, never blocks close
        result["sqliteReadError"] = f"{type(exc).__name__}: {exc}"
        result["cookieNamePresent"] = None
        return result
    result["cookieNamePresent"] = len(rows) > 0
    result["cookieRows"] = len(rows)
    result["hosts"] = sorted({row[1] for row in rows})
    result["valuesManaged"] = all(str(row[2]).startswith("m2-") for row in rows)
    return result


def read_exit_status(exit_file: Path) -> Optional[int]:
    try:
        return int(json.loads(exit_file.read_text())["exitCode"])
    except Exception:
        return None


def write_session_state(session: dict) -> None:
    session_dir = Path(session["sessionDir"])
    payload = {
        "sessionId": session["sessionId"],
        "state": session["state"],
        "artifactId": session["artifactId"],
        "profileId": session["profileId"],
        "artifactFileSha256": session["artifactFileSha256"],
        "artifactDigest": session["artifactDigest"],
        "configuredIdentityDigest": session["configuredIdentityDigest"],
        "observedWebsiteDigest": session["observedWebsiteDigest"],
        "exitStatus": session["exitStatus"],
        "failure": session["failure"],
        "bootCountBefore": session["bootCountBefore"],
        "bootCountAfter": session["bootCountAfter"],
        "fontMode": session.get("fontMode"),
        "managedPids": managed_pids(session),
        "managedIdentities": session.get("managedIdentities"),
        "supervisorMeta": session.get("supervisorMeta"),
        "jobObject": (
            {
                "name": (session.get("supervisorMeta") or {}).get("jobName"),
                "activeProcessCount": (
                    session["jobHandle"].active_process_count()
                    if IS_WINDOWS and session.get("jobHandle") is not None
                    else None
                ),
            }
            if IS_WINDOWS
            else None
        ),
        "exitFileObserved": session.get("exitFileObserved"),
        "processTreeExit": session.get("processTreeExit"),
        "cookieEvidence": session.get("cookieEvidence"),
        "cookieSqlite": session.get("cookieSqlite"),
        "probePort": session.get("probePort"),
        "quarantine": session.get("quarantine"),
    }
    (session_dir / "session.json").write_text(json.dumps(payload, indent=2) + "\n")


def release_profile_lock(session: dict) -> None:
    profile_lock = session.get("profileLock")
    if profile_lock is not None:
        try:
            profile_lock.release()
        except OSError:
            pass
        session["lockFd"] = None
        session["profileLock"] = None
        return
    legacy_fd = session.get("lockFd")
    if legacy_fd is not None and not IS_WINDOWS:
        import fcntl

        try:
            fcntl.flock(legacy_fd, fcntl.LOCK_UN)
            os.close(legacy_fd)
        except OSError:
            pass
        session["lockFd"] = None


async def release_session(
    host: CamoufoxHost, session: dict, release_lock: bool = True
) -> None:
    if not IS_WINDOWS:
        try:
            os.dup2(_PROTOCOL_FD, 1)
            os.dup2(_STDERR_FD, 2)
            os.dup2(_STDIN_FD, 0)
        except OSError:
            pass
    if session.get("logFh") is not None:
        try:
            session["logFh"].close()
        except Exception:
            pass
        session["logFh"] = None
    if session.get("server") is not None:
        try:
            session["server"].shutdown()
        except Exception:
            pass
        try:
            session["server"].server_close()
        except Exception:
            pass
        session["server"] = None
    if session.get("xvfb") is not None:
        stop_xvfb(session["xvfb"])
        session["xvfb"] = None
        # The host owns the display; clear it so the next launch starts a
        # fresh Xvfb instead of reusing a dead display number.
        os.environ.pop("DISPLAY", None)
    if IS_WINDOWS:
        job = session.get("jobHandle")
        if job is not None:
            try:
                job.close()
            except OSError:
                pass
            session["jobHandle"] = None
        for key in (
            "VERISILO_REAL_EXE",
            "VERISILO_EXIT_FILE",
            "VERISILO_SUPERVISOR_FILE",
            "VERISILO_PROFILE_LOCK_PATH",
            "VERISILO_JOB_NAME",
        ):
            os.environ.pop(key, None)
    # The profile lock is released LAST, only after the process tree is
    # confirmed gone, the context is closed, and server/Xvfb are cleaned up.
    # Quarantined sessions pass release_lock=False and KEEP the lock.
    if release_lock:
        release_profile_lock(session)


# --------------------------------------------------------------------------
# Main loop
# --------------------------------------------------------------------------


async def handle_frame(host: CamoufoxHost, raw: bytes) -> bool:
    """Process one frame. Returns True when the host should shut down."""
    try:
        obj = parse_frame(raw)
        request_id, command, params = validate_request(obj)
    except ProtocolError as exc:
        _send(
            {
                "id": None,
                "ok": False,
                "error": {"code": exc.code, "message": str(exc)},
            }
        )
        return False
    try:
        if command == "hello":
            result = host.hello()
        elif command == "launch":
            result = await host.launch(
                params["artifactId"],
                params["profileId"],
                params["expectedArtifactFileSha256"],
            )
        elif command == "status":
            result = host.status(params.get("sessionId"))
        elif command == "close":
            result = await host.close(params["sessionId"])
        elif command == "shutdown":
            if host.session is not None and host.session["state"] in (
                "starting",
                "running",
                "closing",
            ):
                await host.close(host.session["sessionId"])
            result = {
                "state": "shutdown",
                "sessionsClosed": 1 if host.session is not None else 0,
                "selfCheck": scan_self(host),
            }
        else:  # pragma: no cover - validate_request restricts
            raise ProtocolError("unknown_command", command)
    except ProtocolError as exc:
        _send(
            {
                "id": request_id,
                "ok": False,
                "error": {"code": exc.code, "message": str(exc)},
            }
        )
        return command == "shutdown"
    except UnsupportedSchemaVersionError as exc:
        _send(
            {
                "id": request_id,
                "ok": False,
                "error": {
                    "code": "unsupported_schema_version",
                    "message": str(exc),
                },
            }
        )
        return False
    except (ArtifactIntegrityError, TreeIntegrityError) as exc:
        _send(
            {
                "id": request_id,
                "ok": False,
                "error": {"code": "integrity_rejected", "message": str(exc)},
            }
        )
        return False
    except Exception as exc:  # noqa: BLE001 - sanitized protocol error
        _log(f"internal error: {type(exc).__name__}: {exc}")
        _send(
            {
                "id": request_id,
                "ok": False,
                "error": {
                    "code": "internal_error",
                    "message": f"{type(exc).__name__}: {exc}",
                },
            }
        )
        return False
    _send({"id": request_id, "ok": True, "result": result})
    return command == "shutdown"


def scan_self(host: CamoufoxHost) -> dict:
    patterns = list(SECRET_PATTERNS)
    if host.session is not None:
        session_dir = Path(host.session["sessionDir"])
        state_file = session_dir / "session.json"
        if state_file.exists():
            try:
                state = json.loads(state_file.read_text())
            except Exception:
                state = {}
        artifact = host.artifact_root / f"{host.session['artifactId']}.json"
        if artifact.exists():
            try:
                artifact_data = json.loads(artifact.read_text())
                config = artifact_data.get("resolvedConfig", {})
                for key in ("canvas:seed", "audio:seed", "fonts:spacing_seed"):
                    if key in config:
                        patterns.append(str(config[key]))
            except Exception:
                pass
    argv_text = " ".join(sys.argv).lower()
    argv_matches = sorted({p for p in patterns if p.lower() in argv_text})
    stderr_matches: list[str] = []
    log_path = host.state_root / "host-stderr.log"
    if log_path.exists():
        text = log_path.read_text(errors="replace").lower()
        stderr_matches = sorted({p for p in patterns if p.lower() in text})
    return {
        "argvMatches": argv_matches,
        "stderrLogMatches": stderr_matches,
        "patternsChecked": len(patterns),
    }


async def run_host(host: CamoufoxHost) -> int:
    from playwright.async_api import async_playwright

    loop = asyncio.get_event_loop()
    queue: asyncio.Queue = asyncio.Queue()

    def reader() -> None:
        """Memory-bounded frame reader: never buffers more than
        MAX_FRAME_BYTES per frame. Oversized frames are drained to the next
        LF and reported as one frame_too_large marker."""
        _log("host: reader thread started")
        buf = bytearray()
        too_large = False
        try:
            while True:
                chunk = os.read(_STDIN_FD, 65536)
                if not chunk:
                    break
                for byte in chunk:
                    if byte == 0x0A:
                        if too_large:
                            loop.call_soon_threadsafe(
                                queue.put_nowait, _FRAME_TOO_LARGE
                            )
                        else:
                            loop.call_soon_threadsafe(queue.put_nowait, bytes(buf))
                        buf = bytearray()
                        too_large = False
                    elif too_large:
                        continue
                    elif len(buf) < MAX_FRAME_BYTES:
                        buf.append(byte)
                    else:
                        too_large = True
        except Exception:
            _log("host: reader thread exception")
        finally:
            _log("host: reader thread EOF")
            loop.call_soon_threadsafe(queue.put_nowait, None)

    Thread(target=reader, daemon=True, name="stdin-reader").start()
    shutdown_event = asyncio.Event()

    def request_shutdown() -> None:
        shutdown_event.set()

    for sig in (signal.SIGTERM, signal.SIGINT):
        try:
            loop.add_signal_handler(sig, request_shutdown)
        except NotImplementedError:
            pass

    async with async_playwright() as playwright:
        host.set_playwright(playwright)
        while True:
            if shutdown_event.is_set():
                break
            get_task = asyncio.ensure_future(queue.get())
            wait_task = asyncio.ensure_future(shutdown_event.wait())
            done, pending = await asyncio.wait(
                {get_task, wait_task}, return_when=asyncio.FIRST_COMPLETED
            )
            for task in pending:
                task.cancel()
            if shutdown_event.is_set():
                break
            raw = get_task.result()
            if raw is None:
                _log("host: stdin EOF")
                break
            if raw is _FRAME_TOO_LARGE:
                _send(
                    {
                        "id": None,
                        "ok": False,
                        "error": {
                            "code": "frame_too_large",
                            "message": f"frame exceeds {MAX_FRAME_BYTES} bytes",
                        },
                    }
                )
                continue
            should_shutdown = await handle_frame(host, raw)
            if should_shutdown:
                break

        _log("host: loop finished, closing active sessions")
        if host.session is not None and host.session["state"] in (
            "starting",
            "running",
            "closing",
        ):
            await host.close(host.session["sessionId"])
        elif host.session is not None and host.session["state"] == "quarantined":
            _log(
                "host: session is QUARANTINED; profile lock is released by "
                "process exit, quarantine record persists for the next Host"
            )
    _log("host: sessions closed, playwright stopping")
    return 0


def main() -> int:
    global _LOG_FILE
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--artifact-root",
        type=Path,
        default=REPO_ROOT / "tests" / "fixtures" / "camoufox",
    )
    parser.add_argument(
        "--profile-root",
        type=Path,
        default=REPO_ROOT / "artifacts" / "camoufox-m2" / "profiles",
    )
    parser.add_argument(
        "--state-root",
        type=Path,
        default=REPO_ROOT / "artifacts" / "camoufox-m2" / "state",
    )
    parser.add_argument(
        "--tree-manifest",
        type=Path,
        default=(
            REPO_ROOT
            / "tests"
            / "fixtures"
            / "camoufox"
            / (
                "browser-tree-manifest-windows.json"
                if IS_WINDOWS
                else "browser-tree-manifest.json"
            )
        ),
    )
    parser.add_argument("--display", default=None)
    parser.add_argument(
        "--probe-port",
        type=int,
        default=0,
        help=(
            "Probe HTTP server port (0 = ephemeral). A fixed port keeps the "
            "cookie/localStorage origin stable across Host restarts."
        ),
    )
    args = parser.parse_args()
    args.state_root.mkdir(parents=True, exist_ok=True)
    _LOG_FILE = (args.state_root / "host-stderr.log").open("ab")
    host = CamoufoxHost(
        artifact_root=args.artifact_root,
        profile_root=args.profile_root,
        state_root=args.state_root,
        tree_manifest=args.tree_manifest,
        display=args.display,
        probe_port=args.probe_port,
    )
    return asyncio.run(run_host(host))


if __name__ == "__main__":
    raise SystemExit(main())
