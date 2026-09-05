#!/usr/bin/env python3
"""Runtime-only helpers for the self-contained Camoufox Host package.

This module deliberately has no repository or evidence-runner dependency.  A
packaged Host receives all immutable browser, supervisor, and probe files from
its package root; the three mutable data roots remain caller-owned paths.
"""

from __future__ import annotations

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
from collections.abc import Callable
from datetime import datetime, timezone
from functools import partial
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from importlib.metadata import version as dist_version
from pathlib import Path
from threading import Thread
from typing import Any, Optional

from browser_asset import (
    BrowserAssetError,
    SELF_BUILT_ASSET_KIND,
    asset_kind as legacy_asset_kind,
    load_asset_lock as load_legacy_asset_lock,
    verify_self_built_browser_root as verify_legacy_browser_root,
)
from browser_tree import load_tree_manifest, verify_tree
from host_platform import IS_WINDOWS, ensure_no_reparse_points
from package_contract import (
    ASSET_LOCK_NAME,
    BROWSER_DIRECTORY,
    BROWSER_TREE_NAME,
    PACKAGE_ASSET_LOCK_SCHEMA,
    PACKAGE_TREE_NAME,
    PackageContractError,
    PackageLayout,
    load_package_asset_lock,
    strict_json_loads,
    verify_package_browser_root,
)

RELEASE = "v152.0.4-beta.28"
PLATFORM = "windows-x86_64" if IS_WINDOWS else "linux-x86_64"
ASSET_NAME = (
    "camoufox-152.0.4-beta.28-win.x86_64.zip"
    if IS_WINDOWS
    else "camoufox-152.0.4-beta.28-lin.x86_64.zip"
)
EXECUTABLE_REL = "camoufox.exe" if IS_WINDOWS else "camoufox-bin"
COOKIE_NAME = "verisilo_probe_cookie"
CHUNK_SIZE = 2047 if IS_WINDOWS else 32767

# Legacy standalone defaults remain available when no package root is passed.
# Product launches must pass --package-root and therefore never use these
# repository-relative defaults.
MODULE_ROOT = Path(__file__).resolve().parent
LEGACY_REPO_ROOT = MODULE_ROOT.parents[1]
LEGACY_ARTIFACT_DIR = LEGACY_REPO_ROOT / "artifacts" / "camoufox-m0"
LEGACY_LOCK_DIR = MODULE_ROOT / "lock"
LEGACY_BROWSER_DIR = LEGACY_ARTIFACT_DIR / "browser"
LEGACY_CACHE_DIR = LEGACY_ARTIFACT_DIR / "xdg-cache"
LEGACY_SUPERVISOR = (
    MODULE_ROOT / "windows-supervisor" / "target" / "release" / "verisilo-camoufox-supervisor.exe"
    if IS_WINDOWS
    else MODULE_ROOT / "exit_supervisor.py"
)
LEGACY_EXTRACT_DIR = LEGACY_BROWSER_DIR / (
    "camoufox-152.0.4-beta.28-win-x86_64"
    if IS_WINDOWS
    else "camoufox-152.0.4-beta.28-lin-x86_64"
)
LEGACY_EXECUTABLE = LEGACY_EXTRACT_DIR / EXECUTABLE_REL
LEGACY_DEFAULT_ASSET_LOCK = LEGACY_LOCK_DIR / f"camoufox-{RELEASE}-{PLATFORM}.json"
LEGACY_PROBE_FILE = LEGACY_REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html"
LEGACY_DEFAULT_TREE_MANIFEST = LEGACY_REPO_ROOT / "tests" / "fixtures" / "camoufox" / (
    "browser-tree-manifest-windows.json"
    if IS_WINDOWS
    else "browser-tree-manifest.json"
)


class DownloadGuard:
    """Prevent Camoufox from fetching an unpinned package at runtime."""

    tripped = False

    @classmethod
    def reset(cls) -> None:
        cls.tripped = False

    @classmethod
    def guard(cls, *args: Any, **kwargs: Any) -> None:
        cls.tripped = True
        raise RuntimeError("camoufox attempted an unpinned download")


class UnclassifiedCandidateIdentityFieldError(RuntimeError):
    """A BrowserForge candidate field is outside the Host's closed policy."""


def install_download_guard() -> bool:
    import camoufox.addons
    import camoufox.pkgman

    camoufox.addons.webdl = DownloadGuard.guard
    camoufox.pkgman.webdl = DownloadGuard.guard
    return True


def utcnow() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace(
        "+00:00", "Z"
    )


def asset_kind(lock: dict) -> str:
    if lock.get("schema") == PACKAGE_ASSET_LOCK_SCHEMA:
        return SELF_BUILT_ASSET_KIND
    return legacy_asset_kind(lock)


def resolve_asset_lock_path(
    path: Path | str | None = None, *, package_root: Path | str | None = None
) -> Path:
    if path is None and package_root is not None:
        selected = PackageLayout.from_root(package_root).asset_lock
    elif path is not None:
        selected = Path(path)
    else:
        selected = LEGACY_DEFAULT_ASSET_LOCK
    try:
        selected = selected.resolve(strict=True)
    except OSError as exc:
        raise SystemExit(f"missing asset lock {selected}: {exc}") from exc
    return selected


def load_asset_lock(path: Path | str | None = None, *, package_root: Path | str | None = None) -> dict:
    selected = resolve_asset_lock_path(path, package_root=package_root)
    try:
        raw = selected.read_bytes()
        parsed = strict_json_loads(raw, selected.name)
        if type(parsed) is dict and parsed.get("schema") == PACKAGE_ASSET_LOCK_SCHEMA:
            return load_package_asset_lock(selected)
    except OSError as exc:
        raise SystemExit(f"asset lock is unreadable: {exc}") from exc
    try:
        return load_legacy_asset_lock(
            selected,
            expected_release=RELEASE,
            expected_platform=PLATFORM,
        )
    except BrowserAssetError as exc:
        raise SystemExit(f"asset lock rejected: {exc}") from exc


def ensure_browser_asset(
    lock: dict,
    allow_download: bool = True,
    *,
    browser_root: Path | str | None = None,
    tree_manifest: Path | str | None = None,
    verify_tree_contents: bool = True,
) -> Path:
    if lock.get("schema") == PACKAGE_ASSET_LOCK_SCHEMA:
        if browser_root is None or tree_manifest is None:
            raise SystemExit("packaged asset lock requires browser root and tree manifest")
        if allow_download:
            raise SystemExit("packaged browser assets cannot use automatic download")
        try:
            executable, _ = verify_package_browser_root(
                lock,
                browser_root,
                tree_manifest,
                verify_tree_contents=verify_tree_contents,
            )
        except PackageContractError as exc:
            raise SystemExit(f"packaged browser root rejected: {exc}") from exc
        return executable
    if browser_root is not None or tree_manifest is not None:
        # Keep the legacy lock's explicit-root boundary intact.
        if lock.get("assetKind") != SELF_BUILT_ASSET_KIND:
            raise SystemExit("explicit browser root requires a self-built lock")
    if lock.get("assetKind") == SELF_BUILT_ASSET_KIND:
        if browser_root is None:
            raise SystemExit("self-built asset lock requires an explicit browser root")
        if allow_download:
            raise SystemExit("self-built browser assets can never use automatic download")
        try:
            executable, _ = verify_legacy_browser_root(
                lock,
                browser_root,
                repo_root=LEGACY_REPO_ROOT,
                tree_manifest_path=tree_manifest,
                verify_tree_contents=verify_tree_contents,
            )
        except BrowserAssetError as exc:
            raise SystemExit(f"self-built browser root rejected: {exc}") from exc
        return executable
    if browser_root is not None or tree_manifest is not None:
        raise SystemExit("explicit browser root injection requires a self-built lock")
    archive = LEGACY_ARTIFACT_DIR / ASSET_NAME
    if not archive.exists():
        if not allow_download:
            raise SystemExit(f"browser archive missing ({archive}); automatic download disabled")
        subprocess.run([sys.executable, str(MODULE_ROOT / "fetch-browser.py")], check=True)
    actual = sha256_file(archive)
    if actual != lock["sha256"] or archive.stat().st_size != lock["sizeBytes"]:
        raise SystemExit("browser archive digest or size mismatch")
    if not LEGACY_EXECUTABLE.exists():
        import zipfile

        with zipfile.ZipFile(archive) as archive_zip:
            for name in archive_zip.namelist():
                normalized = name.replace("\\", "/")
                parts = normalized.rstrip("/").split("/")
                if (
                    normalized.startswith("/")
                    or not normalized.rstrip("/")
                    or any(part in ("", ".", "..") for part in parts)
                ):
                    raise SystemExit(f"unsafe archive path: {name!r}")
                target = (LEGACY_EXTRACT_DIR / normalized).resolve()
                if LEGACY_EXTRACT_DIR.resolve() not in target.parents and target != LEGACY_EXTRACT_DIR.resolve():
                    raise SystemExit(f"archive path escapes extraction root: {name!r}")
                archive_zip.extract(name, LEGACY_EXTRACT_DIR)
    if not IS_WINDOWS:
        LEGACY_EXECUTABLE.chmod(0o755)
    return LEGACY_EXECUTABLE


def verify_self_built_browser_root(
    lock: dict,
    browser_root: Path | str,
    *,
    repo_root: Path | str | None = None,
    tree_manifest_path: Path | str | None = None,
    verify_tree_contents: bool = True,
) -> tuple[Path, dict]:
    if lock.get("schema") == PACKAGE_ASSET_LOCK_SCHEMA:
        if tree_manifest_path is None:
            raise BrowserAssetError("packaged self-built browser root requires a tree manifest")
        try:
            return verify_package_browser_root(
                lock,
                browser_root,
                tree_manifest_path,
                verify_tree_contents=verify_tree_contents,
            )
        except PackageContractError as exc:
            raise BrowserAssetError(str(exc)) from exc
    if repo_root is None:
        repo_root = LEGACY_REPO_ROOT
    return verify_legacy_browser_root(
        lock,
        browser_root,
        repo_root=repo_root,
        tree_manifest_path=tree_manifest_path,
        verify_tree_contents=verify_tree_contents,
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _cache_install_dir(cache_root: Path) -> Path:
    cache_root = Path(cache_root).resolve()
    if IS_WINDOWS:
        os.environ["WIN_PD_OVERRIDE_LOCAL_APPDATA"] = str(cache_root)
        from platformdirs import user_cache_dir

        return Path(user_cache_dir("camoufox")).resolve()
    os.environ["XDG_CACHE_HOME"] = str(cache_root)
    return cache_root / "camoufox"


def configure_camoufox_cache(cache_root: Path) -> Path:
    install_dir = _cache_install_dir(cache_root)
    pkgman = sys.modules.get("camoufox.pkgman")
    if pkgman is not None and Path(pkgman.INSTALL_DIR).resolve() != install_dir:
        raise RuntimeError(
            "Camoufox was imported before the controlled package cache was configured"
        )
    return install_dir


def _link_or_copy_browser_tree(source: Path, destination: Path) -> bool:
    """Prefer a directory junction/symlink so first launch does not copy ~1 GiB."""

    if (destination / EXECUTABLE_REL).exists():
        return False
    destination.parent.mkdir(parents=True, exist_ok=True)
    if destination.exists() and not destination.is_dir():
        destination.unlink()
    if IS_WINDOWS:
        completed = subprocess.run(
            ["cmd", "/c", "mklink", "/J", str(destination), str(source)],
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode == 0 and (destination / EXECUTABLE_REL).exists():
            return True
    try:
        os.symlink(source, destination, target_is_directory=True)
        if (destination / EXECUTABLE_REL).exists():
            return True
    except OSError:
        pass
    shutil.copytree(source, destination, dirs_exist_ok=True)
    return True


def seed_camoufox_cache(lock: dict, executable: Path, install_dir: Optional[Path] = None) -> bool:
    """Point Camoufox at the packaged browser tree without recopying it."""

    install_dir = install_dir or LEGACY_CACHE_DIR / "camoufox"
    sha8 = lock["sha256"][:8]
    namespace = "verisilo" if lock.get("assetKind") == SELF_BUILT_ASSET_KIND else "official"
    destination = install_dir / "browsers" / namespace / f"152.0.4-beta.28-{sha8}"
    seeded = _link_or_copy_browser_tree(executable.parent, destination)
    version_json = destination / "version.json"
    if not version_json.exists():
        version_json.write_text(
            json.dumps(
                {
                    "version": "152.0.4",
                    "build": "beta.28",
                    "prerelease": True,
                    "sha256": lock["sha256"],
                    "created_at": "2026-08-27T00:00:00Z",
                },
                separators=(",", ":"),
            )
            + "\n",
            encoding="utf-8",
        )
    install_dir.mkdir(parents=True, exist_ok=True)
    (install_dir / "config.json").write_text(
        json.dumps({"active_version": f"browsers/{namespace}/{destination.name}"}) + "\n",
        encoding="utf-8",
    )
    (install_dir / ".0.5_FLAG").touch()
    return seeded


# Launch-only Camoufox keys. They are not Identity Artifact fields.
# showcursor defaults to True in Camoufox and draws the lagging red cursor.
INTERACTIVE_LAUNCH_CONFIG = {
    "showcursor": False,
    "humanize": False,
}


def interactive_desktop_launch() -> bool:
    return os.environ.get("VERISILO_INTERACTIVE") == "1"


def clamp_launch_window(requested: tuple[int, int]) -> tuple[int, int]:
    width, height = int(requested[0]), int(requested[1])
    max_width, max_height = _work_area_size()
    if max_width is None or max_height is None:
        return (width, height)
    return (max(800, min(width, max_width)), max(600, min(height, max_height)))


def apply_interactive_window_override(config: dict) -> None:
    """Fit the real HWND to this display. Screen spoof stays on the artifact."""
    if not interactive_desktop_launch():
        return
    outer_w = config.get("window.outerWidth")
    outer_h = config.get("window.outerHeight")
    if type(outer_w) is not int or type(outer_h) is not int:
        return
    new_w, new_h = clamp_launch_window((outer_w, outer_h))
    if (new_w, new_h) == (outer_w, outer_h):
        return
    inner_w = config.get("window.innerWidth")
    inner_h = config.get("window.innerHeight")
    chrome_w = max(0, outer_w - inner_w) if type(inner_w) is int else 0
    chrome_h = max(0, outer_h - inner_h) if type(inner_h) is int else 0
    config["window.outerWidth"] = new_w
    config["window.outerHeight"] = new_h
    if type(inner_w) is int:
        config["window.innerWidth"] = max(1, new_w - chrome_w)
    if type(inner_h) is int:
        config["window.innerHeight"] = max(1, new_h - chrome_h)


def _work_area_size() -> tuple[Optional[int], Optional[int]]:
    if not IS_WINDOWS:
        return (None, None)
    try:
        import ctypes
        from ctypes import wintypes

        class RECT(ctypes.Structure):
            _fields_ = [
                ("left", wintypes.LONG),
                ("top", wintypes.LONG),
                ("right", wintypes.LONG),
                ("bottom", wintypes.LONG),
            ]

        work = RECT()
        ok = ctypes.windll.user32.SystemParametersInfoW(
            0x0030, 0, ctypes.byref(work), 0
        )
        if not ok:
            return (None, None)
        margin = 48
        max_width = int(work.right - work.left) - margin
        max_height = int(work.bottom - work.top) - margin
        if max_width < 800 or max_height < 600:
            return (None, None)
        return (max_width, max_height)
    except Exception:
        return (None, None)


def firefox_user_prefs_for_config(config: Optional[dict] = None) -> dict:
    prefs = {
        "app.update.auto": False,
        "app.update.enabled": False,
        "browser.shell.checkDefaultBrowser": False,
        "browser.sessionhistory.max_entries": 50,
        "browser.startup.page": 0,
        "browser.startup.homepage": "https://www.google.com/",
        "browser.newtabpage.enabled": True,
        "browser.newtabpage.activity-stream.showSearch": True,
        "keyword.enabled": True,
        "browser.urlbar.suggest.searches": True,
        "browser.urlbar.suggest.engines": True,
        "browser.urlbar.maxRichResults": 8,
        "browser.urlbar.autoFill": True,
        "browser.fixup.alternate.enabled": True,
        "webgl.disabled": False,
        "webgl.force-enabled": True,
        "webgl.enable-webgl2": True,
        "webgl.enable-debug-renderer-info": True,
    }
    if IS_WINDOWS and config and config.get("mediaDevices:enabled") is True:
        prefs["media.navigator.streams.fake"] = True
        prefs["media.navigator.permission.disabled"] = True
    return prefs


def normalize_camou_config_env(env: dict, disk_config: dict) -> tuple[dict, dict, dict]:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    if not chunks:
        raise RuntimeError("launch_options returned no CAMOU_CONFIG env chunks")
    sent = json.loads("".join(value for _, value in chunks))
    # These fields are generated by BrowserForge but are intentionally not
    # long-lived Artifact inputs.  Unknown extras fail closed instead of being
    # silently dropped.
    allowed_native = {
        "navigator.maxTouchPoints",
        "navigator.doNotTrack",
        "navigator.globalPrivacyControl",
        "showcursor",
        "humanize",
        "humanize:maxTime",
    }
    extras = sorted(set(sent) - set(disk_config))
    unknown = [key for key in extras if key not in allowed_native]
    if unknown:
        raise UnclassifiedCandidateIdentityFieldError(
            "unclassified candidate identity field(s): " + ", ".join(unknown)
        )
    for key in extras:
        if key == "navigator.maxTouchPoints" and (type(sent[key]) is not int or sent[key] < 0):
            raise RuntimeError("candidate identity field has invalid type")
        if key == "navigator.doNotTrack" and (
            type(sent[key]) is not str or sent[key] not in {"0", "1", "unspecified"}
        ):
            raise RuntimeError("candidate identity field has invalid type")
        if key == "navigator.globalPrivacyControl" and type(sent[key]) is not bool:
            raise RuntimeError("candidate identity field has invalid type")
        if key in {"showcursor", "humanize"} and type(sent[key]) is not bool:
            raise RuntimeError("candidate identity field has invalid type")
        if key == "humanize:maxTime" and type(sent[key]) not in {int, float}:
            raise RuntimeError("candidate identity field has invalid type")
    normalized = dict(sent)
    for key in extras:
        normalized.pop(key, None)
    changed = sorted(
        key for key in set(normalized) & set(disk_config)
        if type(normalized[key]) is not type(disk_config[key]) or normalized[key] != disk_config[key]
    )
    diff = {
        "added": sorted(set(normalized) - set(disk_config)),
        "removed": sorted(set(disk_config) - set(normalized)),
        "changed": changed,
    }
    rewritten = {key: value for key, value in env.items() if not key.startswith("CAMOU_CONFIG_")}
    encoded_config = dict(disk_config)
    encoded_config.update(INTERACTIVE_LAUNCH_CONFIG)
    apply_interactive_window_override(encoded_config)
    encoded = json.dumps(encoded_config, ensure_ascii=False, separators=(",", ":"))
    for index in range(0, len(encoded), CHUNK_SIZE):
        rewritten[f"CAMOU_CONFIG_{index // CHUNK_SIZE + 1}"] = encoded[index : index + CHUNK_SIZE]
    return normalized, diff, rewritten


def installed_versions() -> dict:
    return {
        "camoufox": dist_version("camoufox"),
        "playwright": dist_version("playwright"),
        "browserforge": dist_version("browserforge"),
    }


def verify_pins(versions: dict) -> list[str]:
    expected = {"camoufox": "0.5.4", "playwright": "1.60.0", "browserforge": "1.2.4"}
    return [
        f"{name} expected {want}, got {versions.get(name)}"
        for name, want in expected.items()
        if versions.get(name) != want
    ]


class _ProbeHandler(BaseHTTPRequestHandler):
    probe_file: Path = LEGACY_PROBE_FILE

    def do_GET(self) -> None:  # noqa: N802
        if urllib.parse.urlparse(self.path).path != "/probe.html":
            self.send_response(404)
            self.end_headers()
            return
        try:
            body = self.probe_file.read_bytes()
        except OSError:
            self.send_response(404)
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args: Any) -> None:  # noqa: N802
        pass


def start_probe_server(
    port: int = 0, probe_file: Path | str | None = None
) -> tuple[ThreadingHTTPServer, str]:
    selected = Path(probe_file or LEGACY_PROBE_FILE).resolve(strict=True)
    if not selected.is_file() or selected.is_symlink():
        raise SystemExit("probe asset must be a regular file")
    handler = type("PackageProbeHandler", (_ProbeHandler,), {"probe_file": selected})
    server = ThreadingHTTPServer(("127.0.0.1", port), handler)
    Thread(target=server.serve_forever, daemon=True).start()
    return server, f"http://127.0.0.1:{server.server_address[1]}/probe.html"


def start_xvfb() -> tuple[str, subprocess.Popen]:
    if IS_WINDOWS:
        raise SystemExit("Xvfb is not used on Windows")
    xvfb = shutil.which("Xvfb")
    if not xvfb:
        raise SystemExit("Xvfb not found")
    read_fd, write_fd = os.pipe()
    proc = subprocess.Popen(
        [
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
        ],
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
    os.environ.update(
        {
            "DISPLAY": display,
            "GDK_BACKEND": "x11",
            "MOZ_ENABLE_WAYLAND": "0",
            "LIBGL_ALWAYS_SOFTWARE": "1",
            "__GLX_VENDOR_LIBRARY_NAME": "mesa",
        }
    )
    return display, proc


def stop_xvfb(proc: subprocess.Popen) -> None:
    if proc.poll() is None:
        proc.send_signal(signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=5)
