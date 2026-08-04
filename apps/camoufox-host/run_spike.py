#!/usr/bin/env python3
"""VeriSilo M0 Camoufox compatibility spike.

Launches a pinned Camoufox browser (v152.0.4-beta.28) with an explicit
executable_path and user_data_dir as a persistent context under Xvfb, writes a
cookie and a LocalStorage value, then closes and relaunches three times.

Every run gets its own run-id directory under
artifacts/camoufox-m0/runs/<run-id>/ containing report.json and report.sha256,
and by default uses a fresh per-run profile (the same profile is shared by all
three cycles of that run). The cookie value embeds the run-id, and cycle 1
proves the cookie is absent before writing it.

The JSON report separates what was *observed* on this host from what is *not
yet verified*. Nothing here is release evidence and nothing is called
"verified" by this host.
"""

from __future__ import annotations

import argparse
import asyncio
from collections.abc import Callable
from functools import partial
import hashlib
import json
import os
import platform
import select
import shutil
import signal
import subprocess
import sys
import time
import urllib.parse
import uuid
import zipfile
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib.metadata import version as dist_version
from pathlib import Path
from threading import Thread
from typing import Any, Optional

REPO_ROOT = Path(__file__).resolve().parents[2]
SPIKE_ROOT = Path(__file__).resolve().parent
PROBE_DIR = REPO_ROOT / "tests" / "fingerprint-probe"
ARTIFACT_DIR = REPO_ROOT / "artifacts" / "camoufox-m0"
RUNS_DIR = ARTIFACT_DIR / "runs"
BROWSER_DIR = ARTIFACT_DIR / "browser"
LOCK_DIR = SPIKE_ROOT / "lock"
XDG_CACHE_DIR = ARTIFACT_DIR / "xdg-cache"
CAMOUFOX_INSTALL_DIR = XDG_CACHE_DIR / "camoufox"
SUPERVISOR = SPIKE_ROOT / "exit_supervisor.py"

RELEASE = "v152.0.4-beta.28"
PLATFORM = "linux-x86_64"
ASSET_NAME = "camoufox-152.0.4-beta.28-lin.x86_64.zip"
EXECUTABLE_REL = "camoufox-bin"
EXTRACT_DIR = BROWSER_DIR / "camoufox-152.0.4-beta.28-lin-x86_64"
EXECUTABLE = EXTRACT_DIR / EXECUTABLE_REL

COOKIE_NAME = "verisilo_probe_cookie"
CYCLES = 3
SAMPLE_INTERVAL_SECONDS = 0.25

# Secret-like patterns used to scan the spike's own argv, the browser argv
# snapshots, and the run logs. The per-run cookie value is included as a
# sentinel: it must never appear in argv or logs.
SECRET_PATTERNS = [
    "password=",
    "passwd=",
    "pwd=",
    "token=",
    "secret=",
    "api_key=",
    "apikey=",
    "client_secret=",
    "private_key=",
    "authorization:",
    "bearer ",
    "-----begin",
    "verisilo_",
]

# Environment variable *names* that look secret-like. Values are never read or
# recorded; this is only a scope note for how the spike passes the environment
# through to the browser process.
ENV_NAME_PATTERNS = [
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "private_key",
    "client_secret",
    "authorization",
    "auth",
]


def utcnow() -> str:
    return datetime.now(timezone.utc).isoformat()


def new_run_id() -> str:
    return f"run-{int(time.time())}-{uuid.uuid4().hex[:8]}"


def load_asset_lock() -> dict:
    path = LOCK_DIR / f"camoufox-{RELEASE}-{PLATFORM}.json"
    if not path.exists():
        raise SystemExit(
            f"missing asset lock {path}; run `uv run python fetch-browser.py --record` first"
        )
    return json.loads(path.read_text())


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        while True:
            chunk = fh.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def ensure_browser_asset(lock: dict, allow_download: bool = True) -> Path:
    archive = ARTIFACT_DIR / ASSET_NAME
    if not archive.exists():
        if not allow_download:
            raise SystemExit(
                f"browser archive missing ({archive}); no automatic download in "
                "this mode — run `uv run python fetch-browser.py --record` first"
            )
        print("browser archive missing; fetching pinned asset ...")
        subprocess.run(
            [sys.executable, str(SPIKE_ROOT / "fetch-browser.py")],
            check=True,
        )
    actual = sha256_file(archive)
    if actual != lock["sha256"]:
        raise SystemExit(
            f"browser archive SHA-256 mismatch: expected {lock['sha256']}, got {actual}"
        )
    if archive.stat().st_size != lock["sizeBytes"]:
        raise SystemExit("browser archive size mismatch")
    if not EXECUTABLE.exists():
        print("extracting pinned browser archive ...")
        with zipfile.ZipFile(archive) as zf:
            names = zf.namelist()
            if EXECUTABLE_REL not in names:
                raise SystemExit(f"executable {EXECUTABLE_REL} missing from archive")
            zf.extractall(EXTRACT_DIR)
    EXECUTABLE.chmod(0o755)
    return EXECUTABLE


def seed_camoufox_cache(
    lock: dict,
    executable: Path,
    install_dir: Optional[Path] = None,
) -> bool:
    """Seed the camoufox package's cache from the verified extraction so that
    launch_options() finds fontconfig/version metadata without downloading.
    The cache lives under artifacts/ (gitignored) and is derived only from the
    pinned archive that fetch-browser.py verified."""
    sha8 = lock["sha256"][:8]
    folder = f"152.0.4-beta.28-{sha8}"
    install_dir = install_dir or CAMOUFOX_INSTALL_DIR
    dest = install_dir / "browsers" / "official" / folder
    version_json = dest / "version.json"
    if not (dest / "camoufox-bin").exists() or not version_json.exists():
        print(f"seeding camoufox cache from verified archive ({folder}) ...")
        shutil.copytree(EXTRACT_DIR, dest, dirs_exist_ok=True)
        version_json.write_text(
            json.dumps(
                {
                    "version": "152.0.4",
                    "build": "beta.28",
                    "prerelease": True,
                    "sha256": lock["sha256"],
                    "created_at": "2026-07-19T07:20:26Z",
                }
            )
            + "\n"
        )
    config_path = install_dir / "config.json"
    config = {}
    if config_path.exists():
        config = json.loads(config_path.read_text())
    config["active_version"] = f"browsers/official/{folder}"
    config_path.write_text(json.dumps(config, indent=2) + "\n")
    (install_dir / ".0.5_FLAG").touch()
    return True


class DownloadGuard:
    """Raises on any camoufox webdl() call (addon or browser download), so an
    unpinned network fetch fails the spike instead of silently succeeding."""

    tripped = False

    @classmethod
    def reset(cls) -> None:
        cls.tripped = False

    @classmethod
    def guard(cls, *args, **kwargs):
        cls.tripped = True
        raise RuntimeError("camoufox attempted an unpinned download (webdl)")


def install_download_guard() -> bool:
    import camoufox.addons
    import camoufox.pkgman

    camoufox.addons.webdl = DownloadGuard.guard
    camoufox.pkgman.webdl = DownloadGuard.guard
    return True


class ProbeHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802
        path = urllib.parse.urlparse(self.path).path
        if path == "/probe.html":
            body = (PROBE_DIR / "probe.html").read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Cache-Control", "no-store")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, *args: Any) -> None:  # noqa: N802
        pass


def start_probe_server(port: int = 0) -> tuple[ThreadingHTTPServer, str]:
    server = ThreadingHTTPServer(("127.0.0.1", port), ProbeHandler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    port = server.server_address[1]
    return server, f"http://127.0.0.1:{port}/probe.html"


def start_xvfb() -> tuple[str, subprocess.Popen]:
    xvfb = shutil.which("Xvfb")
    if not xvfb:
        raise SystemExit("Xvfb not found; install xvfb or run under xvfb-run")
    read_fd, write_fd = os.pipe()
    cmd = [
        xvfb,
        "-displayfd",
        str(write_fd),
        "-screen",
        "0",
        "1280x800x24",
        "-ac",
        "-nolisten",
        "tcp",
        "-extension",
        "RENDER",
        "+extension",
        "GLX",
        "-extension",
        "COMPOSITE",
        "-extension",
        "XVideo",
        "-extension",
        "XINERAMA",
        "-fp",
        "built-ins",
        "-nocursor",
        "-br",
    ]
    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        start_new_session=True,
        pass_fds=(write_fd,),
    )
    os.close(write_fd)
    buf = b""
    deadline = time.monotonic() + 10
    while b"\n" not in buf:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([read_fd], [], [], max(remaining, 0.01))[0]:
            proc.kill()
            os.close(read_fd)
            raise SystemExit("Xvfb did not report a display")
        chunk = os.read(read_fd, 64)
        if not chunk:
            proc.kill()
            os.close(read_fd)
            raise SystemExit(f"Xvfb exited (code {proc.poll()})")
        buf += chunk
    os.close(read_fd)
    display = f":{int(buf.strip())}"
    os.environ["DISPLAY"] = display
    os.environ["GDK_BACKEND"] = "x11"
    os.environ["MOZ_ENABLE_WAYLAND"] = "0"
    os.environ["LIBGL_ALWAYS_SOFTWARE"] = "1"
    os.environ["__GLX_VENDOR_LIBRARY_NAME"] = "mesa"
    return display, proc


def stop_xvfb(proc: subprocess.Popen) -> None:
    if proc.poll() is None:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)


def _proc_children(pid: int) -> list[int]:
    try:
        text = Path(f"/proc/{pid}/task/{pid}/children").read_text()
    except OSError:
        return []
    return [int(part) for part in text.split()]


def tree_rss_kib(pid: int) -> int:
    total = 0
    seen: set[int] = set()
    stack = [pid]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        try:
            status = Path(f"/proc/{current}/status").read_text()
        except OSError:
            continue
        for line in status.splitlines():
            if line.startswith("VmRSS:"):
                total += int(line.split()[1])
        stack.extend(_proc_children(current))
    return total


def find_pid_by_cmdline(marker: str) -> Optional[int]:
    own_pid = os.getpid()
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        pid = int(entry)
        if pid == own_pid:
            continue
        try:
            raw = Path(f"/proc/{entry}/cmdline").read_bytes()
        except OSError:
            continue
        text = raw.replace(b"\0", b" ").decode(errors="replace")
        if "-profile" in text and marker in text:
            return pid
    return None


def browser_process(ctx: Any) -> tuple[Optional[subprocess.Popen], Optional[int]]:
    try:
        proc = ctx.browser.process
        if proc is not None and proc.pid:
            return proc, proc.pid
    except Exception:
        pass
    return None, None


def profile_arg_observed(pid: Optional[int], user_data_dir: Path) -> bool:
    if pid is None:
        return False
    try:
        cmdline = Path(f"/proc/{pid}/cmdline").read_bytes()
    except OSError:
        return False
    text = cmdline.replace(b"\0", b" ").decode(errors="replace")
    return "-profile" in text and str(user_data_dir) in text


def collect_argv_snapshots(root_pid: Optional[int]) -> list[dict]:
    """Snapshot cmdlines of the process tree rooted at root_pid (browser,
    content processes, supervisor). Best-effort: processes that die between
    enumeration steps are simply skipped."""
    snapshots: list[dict] = []
    seen: set[int] = set()
    stack = [root_pid] if root_pid else []
    own_pid = os.getpid()
    while stack:
        pid = stack.pop()
        if pid is None or pid in seen or pid == own_pid:
            continue
        seen.add(pid)
        try:
            raw = Path(f"/proc/{pid}/cmdline").read_bytes()
        except OSError:
            continue
        text = raw.replace(b"\0", b" ").decode(errors="replace").strip()
        if text:
            snapshots.append({"pid": pid, "argv": text})
        stack.extend(_proc_children(pid))
    return snapshots


async def sample_memory(pid_holder: dict, stop: asyncio.Event) -> dict:
    peak_kib = 0
    samples = 0
    while not stop.is_set():
        pid = pid_holder.get("pid")
        if pid:
            rss = tree_rss_kib(pid)
            peak_kib = max(peak_kib, rss)
            samples += 1
        try:
            await asyncio.wait_for(stop.wait(), timeout=SAMPLE_INTERVAL_SECONDS)
        except asyncio.TimeoutError:
            pass
    return {"peakRssKib": peak_kib, "samples": samples}


def capture_exit_code(pid: Optional[int], timeout_seconds: float = 5.0) -> Optional[int]:
    """Best-effort capture of a reaped process's exit code from /proc (zombie
    window). Returns None when the code was not observable."""
    if pid is None:
        return None
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            stat = Path(f"/proc/{pid}/stat").read_text()
        except OSError:
            return None
        fields = stat.rsplit(")", 1)[-1].split()
        if fields and fields[0].strip() == "Z":
            try:
                return int(fields[49])  # exit_code field after state
            except (IndexError, ValueError):
                return None
        time.sleep(0.02)
    return None


def installed_versions() -> dict:
    return {
        "camoufox": dist_version("camoufox"),
        "playwright": dist_version("playwright"),
        "browserforge": dist_version("browserforge"),
    }


def verify_pins(versions: dict) -> list[str]:
    expected = {
        "camoufox": "0.5.4",
        "playwright": "1.60.0",
        "browserforge": "1.2.4",
    }
    return [
        f"{name} expected {want}, got {versions[name]}"
        for name, want in expected.items()
        if versions.get(name) != want
    ]


def scan_text(text: str, patterns: list[str]) -> list[str]:
    lowered = text.lower()
    return sorted({pattern for pattern in patterns if pattern.lower() in lowered})


def secret_like_env_names() -> list[str]:
    """Names (never values) of environment variables that look secret-like.
    The spike passes its environment through to the browser process, so this
    is a scope note, not a claim that the environment is secret-free."""
    return sorted(
        {
            name
            for name in os.environ
            if any(pattern in name.lower() for pattern in ENV_NAME_PATTERNS)
        }
    )


def start_output_tee(log_path: Path) -> Callable[[], None]:
    """Tee this process's stdout/stderr (and therefore the browser process's,
    which inherits them) into log_path while still echoing to the terminal.
    Returns a restore callable."""
    log_fh = log_path.open("ab")
    saved_stdout = os.dup(1)
    saved_stderr = os.dup(2)
    read_stdout, write_stdout = os.pipe()
    read_stderr, write_stderr = os.pipe()
    os.dup2(write_stdout, 1)
    os.dup2(write_stderr, 2)
    os.close(write_stdout)
    os.close(write_stderr)
    try:
        sys.stdout.reconfigure(line_buffering=True)
        sys.stderr.reconfigure(line_buffering=True)
    except Exception:
        pass

    def pump(read_fd: int, dest_fd: int) -> None:
        try:
            with os.fdopen(read_fd, "rb", 0) as src:
                while True:
                    chunk = src.read(65536)
                    if not chunk:
                        break
                    if not log_fh.closed:
                        log_fh.write(chunk)
                        log_fh.flush()
                    try:
                        os.write(dest_fd, chunk)
                    except OSError:
                        pass
        except Exception:
            pass

    threads = [
        Thread(target=pump, args=(read_stdout, saved_stdout), daemon=True),
        Thread(target=pump, args=(read_stderr, saved_stderr), daemon=True),
    ]
    for thread in threads:
        thread.start()

    def restore() -> None:
        os.dup2(saved_stdout, 1)
        os.dup2(saved_stderr, 2)
        for thread in threads:
            thread.join(timeout=5)
        for fd in (saved_stdout, saved_stderr):
            try:
                os.close(fd)
            except OSError:
                pass
        if not log_fh.closed:
            log_fh.close()

    return restore


async def run_cycle(
    playwright: Any,
    executable: Path,
    user_data_dir: Path,
    probe_url: str,
    cycle: int,
    display: str,
    run_dir: Path,
    cookie_value: str,
) -> dict:
    cycle_started = utcnow()
    exit_file = run_dir / f"cycle-{cycle}-exit.json"
    if exit_file.exists():
        exit_file.unlink()
    os.environ["VERISILO_REAL_EXE"] = str(executable)
    os.environ["VERISILO_EXIT_FILE"] = str(exit_file)

    launch_start = time.perf_counter()
    from camoufox import AsyncNewBrowser
    from camoufox import DefaultAddons
    from camoufox.utils import launch_options

    opts = await asyncio.get_event_loop().run_in_executor(
        None,
        partial(
            launch_options,
            headless=False,
            executable_path=str(executable),
            user_data_dir=str(user_data_dir),
            virtual_display=display,
            ff_version=152,
            os="linux",
            window=(1280, 800),
            firefox_user_prefs={
                "app.update.auto": False,
                "app.update.enabled": False,
                "browser.shell.checkDefaultBrowser": False,
            },
            exclude_addons=[DefaultAddons.UBO],
            i_know_what_im_doing=True,
        ),
    )
    # The spike supervisor observes the real browser's exit code; validation
    # above still ran against the real bundle's properties.json.
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

    pid_holder: dict = {}
    proc, pid = browser_process(ctx)
    if pid is None:
        pid = find_pid_by_cmdline(str(user_data_dir))
    pid_holder["pid"] = pid

    stop_sampling = asyncio.Event()
    sampler = asyncio.create_task(sample_memory(pid_holder, stop_sampling))

    page = await ctx.new_page()
    page_start = time.perf_counter()
    await page.goto(probe_url, wait_until="domcontentloaded", timeout=60_000)
    ready_seconds = time.perf_counter() - page_start

    state_before = await page.evaluate("window.__probe.read()")
    boot_before = int(state_before.get("bootCount", 0))
    expected_before = cycle - 1
    if boot_before != expected_before:
        await ctx.close()
        raise RuntimeError(
            f"cycle {cycle}: bootCount before was {boot_before}, expected {expected_before}"
        )

    api_cookies_before = await ctx.cookies()
    cookie_in_api_before = any(c["name"] == COOKIE_NAME for c in api_cookies_before)
    if cycle == 1 and cookie_in_api_before:
        await ctx.close()
        raise RuntimeError(
            f"cycle 1: {COOKIE_NAME} already present before write "
            "(stale profile would pollute this run)"
        )

    await page.evaluate(
        f"window.__probe.writeBootCount({cycle})"
    )
    if cycle == 1:
        await ctx.add_cookies(
            [
                {
                    "name": COOKIE_NAME,
                    "value": cookie_value,
                    "url": probe_url.rsplit("/", 1)[0] + "/",
                    "expires": int(time.time()) + 30 * 86400,
                }
            ]
        )
    await page.reload(wait_until="domcontentloaded")
    state_after = await page.evaluate("window.__probe.read()")
    page_cookie_value = await page.evaluate("document.cookie")
    api_cookies = await ctx.cookies()
    cookie_in_api = any(c["name"] == COOKIE_NAME for c in api_cookies)
    cookie_on_page = bool(state_after.get("cookiePresent"))
    cookie_value_observed = cookie_value in page_cookie_value

    profile_observed = profile_arg_observed(pid, user_data_dir)
    argv_snapshots = collect_argv_snapshots(pid)

    close_start = time.perf_counter()
    await ctx.close()
    close_seconds = time.perf_counter() - close_start
    stop_sampling.set()
    memory = await sampler

    exit_code = None
    if exit_file.exists():
        try:
            exit_code = int(json.loads(exit_file.read_text())["exitCode"])
        except (OSError, ValueError, KeyError, json.JSONDecodeError):
            exit_code = None
    if exit_code is None:
        exit_code = capture_exit_code(pid)

    return {
        "cycle": cycle,
        "startedAtUtc": cycle_started,
        "spawnSeconds": round(spawn_seconds, 3),
        "pageReadySeconds": round(ready_seconds, 3),
        "closeSeconds": round(close_seconds, 3),
        "exitStatus": exit_code,
        "exitStatusObservable": exit_code is not None,
        "peakRssKib": memory["peakRssKib"],
        "memorySamples": memory["samples"],
        "bootCountBefore": boot_before,
        "bootCountAfter": cycle,
        "cookieInApiBefore": cookie_in_api_before,
        "cookieOnPageBefore": bool(state_before.get("cookiePresent")),
        "cookieAbsentBeforeWrite": (
            not cookie_in_api_before if cycle == 1 else None
        ),
        "pageCookieAbsentBeforeWrite": (
            not bool(state_before.get("cookiePresent")) if cycle == 1 else None
        ),
        "cookieInApi": cookie_in_api,
        "cookieOnPage": cookie_on_page,
        "cookieValueContainsRunId": cookie_value_observed,
        "pageCookieValue": page_cookie_value,
        "profileArgObserved": profile_observed,
        "argvSnapshots": argv_snapshots,
        "pageProbe": state_after,
    }


async def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--display", default=None, help="Existing X display, e.g. :99")
    parser.add_argument(
        "--profile-dir",
        default=None,
        help=(
            "Persistent user_data_dir used for all three cycles "
            "(default: a fresh per-run profile under runs/<run-id>/profile)"
        ),
    )
    args = parser.parse_args()

    lock = load_asset_lock()
    if lock.get("digestAgreement") is not True:
        raise SystemExit(
            "asset lock digestAgreement is not true; refresh with "
            "`uv run python fetch-browser.py --record --force`"
        )

    run_id = new_run_id()
    run_dir = RUNS_DIR / run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    restore_tee = start_output_tee(run_dir / "run.log")
    print(f"run-id={run_id}")
    print(f"run-dir={run_dir}")

    user_data_dir = (run_dir / "profile").resolve()
    executable: Optional[Path] = None
    versions: dict = {}
    profile_files_before: list[str] = []
    profile_files_after: list[str] = []
    profile_pre_existed = False
    cycles: list[dict] = []
    failure: Optional[str] = None
    server: Optional[ThreadingHTTPServer] = None
    xvfb_proc: Optional[subprocess.Popen] = None
    guard_installed = False
    cache_seeded = False
    display = ""

    try:
        executable = ensure_browser_asset(lock)
        cache_seeded = seed_camoufox_cache(lock, executable)
        SUPERVISOR.chmod(0o755)
        os.environ["XDG_CACHE_HOME"] = str(XDG_CACHE_DIR)
        guard_installed = install_download_guard()
        DownloadGuard.reset()

        versions = installed_versions()
        pin_issues = verify_pins(versions)
        if pin_issues:
            raise SystemExit("dependency pin mismatch: " + "; ".join(pin_issues))

        user_data_dir = (
            Path(args.profile_dir).resolve()
            if args.profile_dir
            else (run_dir / "profile").resolve()
        )
        profile_pre_existed = user_data_dir.exists() and any(user_data_dir.iterdir())
        user_data_dir.mkdir(parents=True, exist_ok=True)
        profile_files_before = sorted(p.name for p in user_data_dir.iterdir())

        display = args.display or os.environ.get("DISPLAY")
        if not display:
            display, xvfb_proc = start_xvfb()
        print(f"using display {display}")

        cookie_value = f"m0-{run_id}-cookie"
        server, probe_url = start_probe_server()

        from playwright.async_api import async_playwright

        async with async_playwright() as playwright:
            for cycle in range(1, CYCLES + 1):
                print(f"cycle {cycle}/{CYCLES}: launching persistent context ...")
                cycle_result = await run_cycle(
                    playwright,
                    executable,
                    user_data_dir,
                    probe_url,
                    cycle,
                    display,
                    run_dir,
                    cookie_value,
                )
                cycles.append(cycle_result)
                print(
                    f"cycle {cycle}: bootCount {cycle_result['bootCountBefore']} -> "
                    f"{cycle_result['bootCountAfter']}, cookie "
                    f"{'ok' if cycle_result['cookieOnPage'] else 'MISSING'}, "
                    f"peakRssKib={cycle_result['peakRssKib']}, "
                    f"exit={cycle_result['exitStatus']}"
                )
    except Exception as exc:  # keep the report even when a cycle fails
        failure = f"{type(exc).__name__}: {exc}"
        print(f"spike failed: {failure}", file=sys.stderr)
    finally:
        if server is not None:
            server.shutdown()
        if xvfb_proc is not None:
            stop_xvfb(xvfb_proc)
        if user_data_dir is not None:
            profile_files_after = sorted(p.name for p in user_data_dir.iterdir())
        restore_tee()

    persisted_files = [
        name for name in profile_files_after if name not in profile_files_before
    ]

    # Evidence: secret scans over argv and log files.
    secret_patterns = SECRET_PATTERNS + [
        COOKIE_NAME + "=",
        f"m0-{run_id}-cookie",
    ]
    spike_argv_entry = {"pid": os.getpid(), "argv": " ".join(sys.argv)}
    argv_entries = [spike_argv_entry]
    argv_matches: list[str] = []
    for cycle_result in cycles:
        for snapshot in cycle_result.get("argvSnapshots", []):
            argv_entries.append(snapshot)
            argv_matches.extend(scan_text(snapshot["argv"], secret_patterns))
    argv_matches = sorted(set(argv_matches))

    log_entries: list[dict] = []
    for log_path in [run_dir / "run.log", *sorted(run_dir.glob("cycle-*.json"))]:
        if not log_path.exists():
            continue
        text = log_path.read_text(errors="replace")
        log_entries.append(
            {
                "path": str(log_path),
                "relativePath": str(log_path.relative_to(run_dir)),
                "bytes": log_path.stat().st_size,
                "secretPatternMatches": scan_text(text, secret_patterns),
            }
        )
    log_matches = sorted(
        {
            pattern
            for entry in log_entries
            for pattern in entry["secretPatternMatches"]
        }
    )
    secrets_in_argv_or_logs = bool(argv_matches or log_matches)

    success = (
        failure is None
        and len(cycles) == CYCLES
        and not DownloadGuard.tripped
        and not secrets_in_argv_or_logs
        and all(
            c["bootCountBefore"] == c["cycle"] - 1
            and c["bootCountAfter"] == c["cycle"]
            and c["cookieInApi"]
            and c["cookieOnPage"]
            and c["cookieValueContainsRunId"]
            and c["profileArgObserved"]
            and c["exitStatus"] == 0
            for c in cycles
        )
        and (len(cycles) == 0 or cycles[0]["cookieAbsentBeforeWrite"] is True)
    )

    evidence = {
        "downloads": {
            "camoufoxWebdlAttempted": bool(DownloadGuard.tripped),
            "runtimeDownloadGuardInstalled": guard_installed,
            "guardInstallMethod": (
                "patched camoufox.addons.webdl and camoufox.pkgman.webdl to "
                "raise on any call"
            ),
            "outboundNetworkFullyObserved": False,
            "outboundObservationScope": (
                "Only camoufox webdl calls are guarded; there is no "
                "socket/eBPF/proxy-level observation of the browser process "
                "tree, so outbound network was not fully observed"
            ),
        },
        "secrets": {
            "secretInputsSupplied": False,
            "secretInputBasis": (
                "run_spike CLI defines no secret options; no secret values "
                "were passed via launch options, probe URL, or cookie"
            ),
            "envSecretLikeNames": secret_like_env_names(),
            "envValueInspection": "values never read or recorded",
            "patterns": secret_patterns,
            "argvScanned": {
                "entries": len(argv_entries),
                "pids": sorted({entry["pid"] for entry in argv_entries}),
                "secretPatternMatches": argv_matches,
            },
            "logFilesScanned": log_entries,
        },
    }

    report = {
        "schema": "verisilo-camoufox-m0-report/v2",
        "runId": run_id,
        "generatedAtUtc": utcnow(),
        "host": {
            "machine": platform.machine(),
            "cores": os.cpu_count(),
            "python": platform.python_version(),
            "platform": platform.platform(),
            "memTotalKib": _mem_total_kib(),
        },
        "pins": {
            "python": "3.12.11",
            "camoufox": versions["camoufox"],
            "playwright": versions["playwright"],
            "browserforge": versions["browserforge"],
            "browserRelease": RELEASE,
            "assetSha256": lock["sha256"],
            "assetUrl": lock["url"],
            "assetSizeBytes": lock["sizeBytes"],
            "officialAssetId": lock["githubAsset"]["assetId"],
            "officialDigest": lock["githubAsset"]["officialDigest"],
            "digestAgreement": lock["digestAgreement"],
        },
    "launch": {
            "mode": "persistent-context",
            "display": display,
            "xvfbOwnedBySpike": xvfb_proc is not None,
            "executablePath": str(executable) if executable else None,
            "userDataDir": str(user_data_dir),
            "runId": run_id,
            "runDir": str(run_dir),
            "profilePreExisted": profile_pre_existed,
            "profileFilesBeforeCount": len(profile_files_before),
            "camoufoxCache": str(CAMOUFOX_INSTALL_DIR),
            "cacheSeededFromVerifiedAsset": cache_seeded,
            "downloadGuard": "webdl patched to raise; any unpinned download fails the run",
        },
        "cycles": cycles,
        "profile": {
            "userDataDir": str(user_data_dir),
            "filesBeforeRun": profile_files_before,
            "filesAfterRun": profile_files_after,
            "filesCreatedDuringRun": persisted_files,
            "cookiesSqlite": (user_data_dir / "cookies.sqlite").exists(),
            "prefsJs": (user_data_dir / "prefs.js").exists(),
            "storageDir": (user_data_dir / "storage").is_dir(),
        },
        "evidence": evidence,
        "failure": failure,
        "acceptance": {
            "threeStartsAllSucceeded": len(cycles) == CYCLES and failure is None,
            "statePersistedAcrossRestarts": all(
                c["bootCountBefore"] == c["cycle"] - 1
                and c["cookieInApi"]
                and c["cookieOnPage"]
                for c in cycles
            ),
            "noCamoufoxWebdlAttemptObserved": not evidence["downloads"]["camoufoxWebdlAttempted"],
            "noSecretsInArgvOrLogs": not secrets_in_argv_or_logs,
            "m0AcceptanceMet": success,
        },
        "conclusion": {
            "compatibility": "compatible" if success else "incompatible",
            "evidenceClass": "observed-on-this-host",
            "verified": False,
            "summary": (
                "All three persistent-context launches succeeded and Cookie/"
                "LocalStorage survived across normal closes on this 2C/8GB "
                "Linux host. These are host observations, not release-grade "
                "verification."
                if success
                else "Spike did not meet M0 acceptance."
            ),
            "notYetVerified": [
                "Authenticity and integrity of the Camoufox release signing/chain",
                "Extracted browser run tree was not verified file-by-file "
                "(archive hash/size and executable presence only)",
                "Outbound network traffic of the browser process tree was not "
                "fully observed (webdl guard only)",
                "First pin of this browser asset is trust-on-first-use; GitHub's "
                "official digest now agrees with the locally computed digest, "
                "but there is no out-of-band attestation of the initial pin",
                "Canvas/WebGL/font/media-device output truthfulness",
                "TLS ClientHello and QUIC behavior",
                "Windows platform and production EngineAdapter protocol",
                "Long-run memory stability under real site workloads",
            ],
            "m1Gate": {
                "allowed": success,
                "reason": (
                    "Candidate pins are mutually compatible and the persistent "
                    "profile lifecycle works on this host. M1 must still add "
                    "independent artifact provenance, protocol evidence, and "
                    "release-grade verification."
                    if success
                    else "M0 acceptance failed; do not enter M1 with these pins."
                ),
            },
        },
    }

    report_bytes = json.dumps(report, indent=2) + "\n"
    report_path = run_dir / "report.json"
    report_path.write_text(report_bytes)
    report_sha256 = hashlib.sha256(report_bytes.encode("utf-8")).hexdigest()
    (run_dir / "report.sha256").write_text(report_sha256 + "\n")
    print(f"report written to {report_path}")
    print(f"report.sha256={report_sha256}")
    return 0 if success else 1


def _mem_total_kib() -> int:
    try:
        for line in Path("/proc/meminfo").read_text().splitlines():
            if line.startswith("MemTotal:"):
                return int(line.split()[1])
    except OSError:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
