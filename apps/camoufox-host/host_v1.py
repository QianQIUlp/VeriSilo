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
    DownloadGuard,
    EXECUTABLE,
    REPO_ROOT,
    SUPERVISOR,
    XDG_CACHE_DIR,
    browser_process,
    ensure_browser_asset,
    find_pid_by_cmdline,
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
    ) -> None:
        self.artifact_root = artifact_root.resolve()
        self.profile_root = profile_root.resolve()
        self.state_root = state_root.resolve()
        self.tree_manifest = tree_manifest
        self.display_arg = display
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
            "exitFile": session_dir / "exit.json",
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
        server, probe_url = start_probe_server()
        session["server"] = server
        os.environ["VERISILO_REAL_EXE"] = str(self.executable)
        os.environ["VERISILO_EXIT_FILE"] = str(session["exitFile"])

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

        proc, pid = browser_process(ctx)
        if pid is None:
            pid = find_pid_by_cmdline(str(session["profileDir"]))
        session["pid"] = pid

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

        boot_before = int(observed.get("bootCount", 0))
        await page.evaluate(f"window.__probe.writeBootCount({boot_before + 1})")
        session["bootCountBefore"] = boot_before
        session["bootCountAfter"] = boot_before + 1

        signals = extract_observed_website_signals(observed)
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
        pid = session.get("pid")
        while not session["stopMonitor"].is_set():
            if pid is not None and not pid_alive(pid):
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
        session["stopMonitor"].set()
        # Release the profile lock FIRST: the failed state must not block a
        # relaunch of the same profile.
        release_profile_lock(session)
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
                pid = session.get("pid")
                if pid:
                    terminate_exact(pid)
                try:
                    await asyncio.wait_for(ctx.close(), timeout=10)
                except Exception:
                    pass
        session["ctx"] = None
        session["exitStatus"] = read_exit_status(session["exitFile"])
        session["state"] = "exited"
        session["closeSeconds"] = round(time.perf_counter() - close_start, 3)
        await release_session(self, session)
        write_session_state(session)
        return {
            "sessionId": session["sessionId"],
            "state": session["state"],
            "exitStatus": session["exitStatus"],
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


def terminate_exact(pid: int) -> None:
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        if not pid_alive(pid):
            return
        time.sleep(0.05)
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


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
    release_profile_lock(session)
    if session.get("server") is not None:
        try:
            session["server"].shutdown()
        except Exception:
            pass
        session["server"] = None
    if session.get("xvfb") is not None:
        stop_xvfb(session["xvfb"])
        session["xvfb"] = None
        # The host owns the display; clear it so the next launch starts a
        # fresh Xvfb instead of reusing a dead display number.
        os.environ.pop("DISPLAY", None)


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
    stdin_stream = os.fdopen(_STDIN_FD, "rb", buffering=0)

    def reader() -> None:
        _log("host: reader thread started")
        try:
            for line in stdin_stream:
                _log(f"host: reader got {len(line)} bytes")
                loop.call_soon_threadsafe(queue.put_nowait, line.rstrip(b"\n"))
        except Exception:
            _log("host: reader thread exception")
            pass
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
        while not shutdown_event.is_set():
            raw = await queue.get()
            if raw is None:
                _log("host: stdin EOF")
                break
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
    args = parser.parse_args()
    args.state_root.mkdir(parents=True, exist_ok=True)
    _LOG_FILE = (args.state_root / "host-stderr.log").open("ab")
    host = CamoufoxHost(
        artifact_root=args.artifact_root,
        profile_root=args.profile_root,
        state_root=args.state_root,
        tree_manifest=args.tree_manifest,
        display=args.display,
    )
    return asyncio.run(run_host(host))


if __name__ == "__main__":
    raise SystemExit(main())
