#!/usr/bin/env python3
"""VeriSilo M2 standalone Camoufox Host v1 (Linux, stdio protocol).

Protocol: JSON Lines on stdin/stdout, one object per line, LF-terminated.
Maximum frame size: 32 KiB (requests and responses). stdout carries ONLY
protocol frames; all logs go to stderr.

Commands: hello, launch, status, close, shutdown.

Launch requests carry artifactId/profileId/expectedArtifactFileSha256 only;
the caller can never pass arbitrary paths. Roots are fixed at process start
(--artifact-root, --profile-root, --state-root).

State machine per session: idle -> starting -> running -> closing ->
exited/failed. Profile directories hold an exclusive flock; a concurrent
launch of the same profile is rejected with profile_in_use.

Every response keeps verified:false / observed-on-this-host.
"""

from __future__ import annotations

import argparse
import asyncio
import copy
import fcntl
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
    build_projection,
    configured_identity_digest,
    diff_configs,
    observed_website_digest,
    verify_artifact_raw,
    verify_browser_binding,
)
from browser_tree import (
    TreeIntegrityError,
    load_tree_manifest,
    verify_tree,
)
from host_fonts import (
    FONT_UNIVERSE,
    host_negative_control_families,
)
from run_identity_spike import extract_observed_website_signals
from run_spike import (
    COOKIE_NAME,
    DownloadGuard,
    EXECUTABLE,
    REPO_ROOT,
    SUPERVISOR,
    XDG_CACHE_DIR,
    ensure_browser_asset,
    install_download_guard,
    installed_versions,
    load_asset_lock,
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
    os.write(_PROTOCOL_FD, frame)


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

    try:
        obj = json.loads(text, object_pairs_hook=reject_duplicates)
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
        self.artifact_root = artifact_root.resolve()
        self.profile_root = profile_root.resolve()
        self.state_root = state_root.resolve()
        self.tree_manifest = tree_manifest
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
        lock = load_asset_lock()
        if lock.get("digestAgreement") is not True:
            raise SystemExit("asset lock digestAgreement is not true")
        executable = ensure_browser_asset(lock, allow_download=False)
        seed_camoufox_cache(lock, executable)
        SUPERVISOR.chmod(0o755)
        os.environ["XDG_CACHE_HOME"] = str(XDG_CACHE_DIR)
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
        verify_tree(EXECUTABLE.parent, load_tree_manifest(self.tree_manifest))

        profile_dir = self.profile_root / profile_id
        profile_dir.mkdir(parents=True, exist_ok=True)
        lock_path = self.profile_root / f"{profile_id}.lock"
        lock_fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            os.close(lock_fd)
            raise ProtocolError(
                "profile_in_use", f"profile {profile_id} is locked by another session"
            )

        session_id = uuid.uuid4().hex
        session_dir = self.state_root / session_id
        session_dir.mkdir(parents=True, exist_ok=False)
        browser_log = session_dir / "browser.log"
        log_fh = browser_log.open("ab")
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
            "lockFd": lock_fd,
            "logFh": log_fh,
            "ctx": None,
            "pid": None,
            "childPid": None,
            "supervisorMeta": None,
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
        }
        self.session = session
        _log(
            f"session {session_id}: launching artifact={artifact_id} "
            f"profile={profile_id}"
        )
        try:
            await self._launch_browser(session, artifact)
        except ProtocolError:
            await self._fail_session(session, "launch rejected")
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
        }

    async def _launch_browser(self, session: dict, artifact: dict) -> None:
        from camoufox import AsyncNewBrowser
        from camoufox import DefaultAddons
        from camoufox.utils import launch_options

        policy = artifact["policy"]
        window = tuple(policy["window"])
        disk_config = copy.deepcopy(artifact["resolvedConfig"])
        disk_digest = configured_identity_digest(disk_config)

        display = self.display_arg or os.environ.get("DISPLAY")
        xvfb = None
        if not display:
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

        launch_start = time.perf_counter()
        opts = await asyncio.get_event_loop().run_in_executor(
            None,
            partial(
                launch_options,
                config=disk_config,
                os=policy["targetOs"],
                window=window,
                locale=policy["locale"],
                ff_version=policy["ffVersion"],
                headless=False,
                executable_path=str(self.executable),
                user_data_dir=str(session["profileDir"]),
                virtual_display=display,
                firefox_user_prefs={
                    "app.update.auto": False,
                    "app.update.enabled": False,
                    "browser.shell.checkDefaultBrowser": False,
                },
                exclude_addons=[DefaultAddons.UBO],
                i_know_what_im_doing=True,
            ),
        )
        sent_config = _reassemble_config(opts["env"])
        sent_digest = configured_identity_digest(sent_config)
        diff = diff_configs(disk_config, sent_config)
        if (
            sent_digest != disk_digest
            or diff["added"]
            or diff["removed"]
            or diff["changed"]
        ):
            raise ProtocolError(
                "config_mutation",
                "launch_options mutated the disk config: " + json.dumps(diff),
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
        # file (supervisor pid, child browser pid, start times, process
        # groups). We never guess a pid by scanning /proc cmdlines.
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
            pids = managed_pids(session)
            if pids and not all(pid_alive(pid) for pid in pids):
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
            try:
                await asyncio.wait_for(ctx.close(), timeout=5)
            except Exception:
                pass
            session["ctx"] = None
        # Confirm the whole managed process tree is gone BEFORE releasing the
        # profile lock, so no second Host can ever touch the same profile
        # while the old browser is still alive.
        session["processTreeExit"] = terminate_managed_tree(session, timeout=6)
        session["pid"] = None
        session["childPid"] = None
        await release_session(self, session)
        write_session_state(session)
        _log(f"session {session['sessionId']} failed: {error}")

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
            "failure": session["failure"],
        }

    async def close(self, session_id: str) -> dict:
        session = self.session
        if session is None or session["sessionId"] != session_id:
            raise ProtocolError("session_not_found", f"no session {session_id}")
        if session["state"] in ("exited", "failed"):
            return {
                "sessionId": session["sessionId"],
                "state": session["state"],
                "exitStatus": session["exitStatus"],
                "exitFileObserved": session.get("exitFileObserved"),
                "processTreeExit": session.get("processTreeExit"),
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
            try:
                await asyncio.wait_for(ctx.close(), timeout=10)
            except Exception:
                _log(f"session {session['sessionId']}: ctx.close() raised, terminating tree")
        session["ctx"] = None
        # close() must CONFIRM the managed process tree is fully gone (and
        # terminate it if not) before the profile lock is released.
        session["processTreeExit"] = terminate_managed_tree(session, timeout=8)
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


def pid_alive(pid: int) -> bool:
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


def process_tree_alive(pid: Optional[int]) -> bool:
    if not isinstance(pid, int) or pid <= 0:
        return False
    return any(pid_alive(candidate) for candidate in process_descendants(pid))


def managed_pids(session: dict) -> list[int]:
    return sorted(
        {
            pid
            for pid in (session.get("pid"), session.get("childPid"))
            if isinstance(pid, int) and pid > 0
        }
    )


def terminate_managed_tree(session: dict, timeout: float = 8.0) -> dict:
    """Terminate every managed supervisor/browser descendant and CONFIRM the
    tree is gone. Returns evidence; the caller only releases the profile lock
    after this returns exited=True."""
    roots = managed_pids(session)
    if not roots:
        return {
            "exited": True,
            "managedPids": [],
            "sigterm": False,
            "sigkill": False,
        }
    targets: list[int] = []
    seen: set[int] = set()
    for root in roots:
        for descendant in process_descendants(root):
            if descendant not in seen:
                seen.add(descendant)
                targets.append(descendant)
    for target in reversed(targets):  # children first, supervisors last
        try:
            os.kill(target, signal.SIGTERM)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        alive = [pid for pid in targets if pid_alive(pid)]
        if not alive:
            return {
                "exited": True,
                "managedPids": roots,
                "sigterm": True,
                "sigkill": False,
            }
        time.sleep(0.05)
    for target in alive:
        try:
            os.kill(target, signal.SIGKILL)
        except ProcessLookupError:
            pass
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        alive = [pid for pid in targets if pid_alive(pid)]
        if not alive:
            return {
                "exited": True,
                "managedPids": roots,
                "sigterm": True,
                "sigkill": True,
            }
        time.sleep(0.05)
    return {
        "exited": False,
        "managedPids": roots,
        "sigterm": True,
        "sigkill": True,
        "remainingPids": alive,
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
        "supervisorMeta": session.get("supervisorMeta"),
        "exitFileObserved": session.get("exitFileObserved"),
        "processTreeExit": session.get("processTreeExit"),
        "cookieEvidence": session.get("cookieEvidence"),
        "cookieSqlite": session.get("cookieSqlite"),
        "probePort": session.get("probePort"),
    }
    (session_dir / "session.json").write_text(json.dumps(payload, indent=2) + "\n")


def release_profile_lock(session: dict) -> None:
    if session.get("lockFd") is not None:
        try:
            fcntl.flock(session["lockFd"], fcntl.LOCK_UN)
            os.close(session["lockFd"])
        except OSError:
            pass
        session["lockFd"] = None


async def release_session(host: CamoufoxHost, session: dict) -> None:
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
    # The profile lock is released LAST, only after the process tree is
    # confirmed gone, the context is closed, and server/Xvfb are cleaned up.
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
            / "browser-tree-manifest.json"
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
