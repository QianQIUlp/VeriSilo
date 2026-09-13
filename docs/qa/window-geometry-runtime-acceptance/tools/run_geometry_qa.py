#!/usr/bin/env python3
"""QA evidence driver: real Managed Camoufox window geometry coherence.

Runs the current-source Host (apps/camoufox-host/host_v1.py) against the dev
engine package (frozen Formal-v3 browser tree).  This is evidence tooling only;
it modifies no product source and never touches user Vault data (all Artifact /
profile / state roots live under the QA work directory passed by the caller).

Subcommands:
  provision  one-shot --provision-artifact run (length-prefixed frame)
  legacy     craft a legal legacy-style Artifact whose persisted resolvedConfig
             carries out-of-bounds window.screenX / window.screenY
  runtime    one full JSONL session (hello/launch/page/close/shutdown) with a
             geometry observation on the real visible window
  run-all    the full acceptance matrix + invariant evaluation + summary.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[4]
HOST_DIR = REPO_ROOT / "apps" / "camoufox-host"
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

DEFAULT_PYTHON = HOST_DIR / ".venv" / "Scripts" / "python.exe"
GEOMETRY_EVALUATE_SCRIPT = (
    "() => ({"
    "outerWidth: window.outerWidth, outerHeight: window.outerHeight,"
    "innerWidth: window.innerWidth, innerHeight: window.innerHeight,"
    "screenX: window.screenX, screenY: window.screenY,"
    "screenLeft: window.screenLeft, screenTop: window.screenTop,"
    "devicePixelRatio: window.devicePixelRatio,"
    "screen: {"
    "width: screen.width, height: screen.height,"
    "availWidth: screen.availWidth, availHeight: screen.availHeight,"
    "availLeft: screen.availLeft, availTop: screen.availTop,"
    "colorDepth: screen.colorDepth, pixelDepth: screen.pixelDepth},"
    "url: location.href})"
)


def sha256_file(path: Path) -> str:
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def geometry_invariants(observation: dict[str, Any]) -> dict[str, bool]:
    screen = observation["screen"]
    return {
        "screenX_equals_screenLeft": observation["screenX"] == observation["screenLeft"],
        "screenY_equals_screenTop": observation["screenY"] == observation["screenTop"],
        "availLeft_le_screenX": screen["availLeft"] <= observation["screenX"],
        "screenX_plus_outerWidth_within_avail": (
            observation["screenX"] + observation["outerWidth"]
            <= screen["availLeft"] + screen["availWidth"]
        ),
        "availTop_le_screenY": screen["availTop"] <= observation["screenY"],
        "screenY_plus_outerHeight_within_avail": (
            observation["screenY"] + observation["outerHeight"]
            <= screen["availTop"] + screen["availHeight"]
        ),
    }


def host_work_area() -> dict[str, Any]:
    if os.name != "nt":
        return {"available": False}
    import ctypes

    user32 = ctypes.windll.user32

    class RECT(ctypes.Structure):
        _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                    ("right", ctypes.c_long), ("bottom", ctypes.c_long)]

    rect = RECT()
    ok = user32.SystemParametersInfoW(0x0030, 0, ctypes.byref(rect), 0)  # SPI_GETWORKAREA
    return {
        "available": bool(ok),
        "primaryWorkArea": {
            "left": rect.left, "top": rect.top,
            "width": rect.right - rect.left, "height": rect.bottom - rect.top,
        },
        "primaryScreenWidth": user32.GetSystemMetrics(0),
        "primaryScreenHeight": user32.GetSystemMetrics(1),
        "virtualScreenWidth": user32.GetSystemMetrics(76),
        "virtualScreenHeight": user32.GetSystemMetrics(77),
        "virtualScreenLeft": user32.GetSystemMetrics(76 - 2),
        "virtualScreenTop": user32.GetSystemMetrics(78),
    }


# ---------------------------------------------------------------------------
# Host process wrappers
# ---------------------------------------------------------------------------


class ProvisionClient:
    """One-shot --provision-artifact run over a length-prefixed frame."""

    def __init__(self, python: Path, host_dir: Path, package_root: Path,
                 artifact_root: Path, state_root: Path, log_path: Path) -> None:
        self.log_path = log_path
        self.log_file = log_path.open("wb")
        self.process = subprocess.Popen(
            [
                str(python), str(host_dir / "host_v1.py"),
                "--provision-artifact",
                "--package-root", str(package_root),
                "--artifact-root", str(artifact_root),
                "--state-root", str(state_root),
            ],
            cwd=str(host_dir), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.log_file,
        )

    def _read_frame(self, timeout: float) -> bytes:
        result: list[bytes] = []
        error: list[str] = []

        def runner() -> None:
            try:
                assert self.process.stdout is not None
                header = self.process.stdout.read(4)
                if len(header) != 4:
                    raise RuntimeError("provision host closed stdout before header")
                payload = self.process.stdout.read(int.from_bytes(header, "big"))
                if len(payload) != int.from_bytes(header, "big"):
                    raise RuntimeError("provision host closed stdout before payload")
                result.append(payload)
            except Exception as exc:  # noqa: BLE001
                error.append(f"{type(exc).__name__}: {exc}")

        thread = threading.Thread(target=runner, daemon=True)
        thread.start()
        thread.join(timeout)
        if not result:
            raise RuntimeError(f"provision frame read failed: {error or 'timeout'}")
        return result[0]

    def request(self, payload: dict[str, Any], timeout: float = 900.0) -> dict[str, Any]:
        raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        assert self.process.stdin is not None
        self.process.stdin.write(len(raw).to_bytes(4, "big") + raw)
        self.process.stdin.flush()
        return json.loads(self._read_frame(timeout).decode("utf-8"))

    def close(self) -> int:
        if self.process.stdin:
            try:
                self.process.stdin.close()
            except OSError:
                pass
        try:
            code = self.process.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.process.kill()
            code = self.process.wait(timeout=30)
        self.log_file.close()
        return code


CAMOUFOX_INTERACTIVE_USER_JS = """\
user_pref("browser.startup.page", 0);
user_pref("browser.startup.homepage", "https://www.google.com/");
user_pref("browser.newtabpage.enabled", true);
user_pref("browser.newtabpage.activity-stream.showSearch", true);
user_pref("keyword.enabled", true);
user_pref("browser.urlbar.suggest.searches", true);
user_pref("browser.urlbar.suggest.engines", true);
user_pref("browser.urlbar.maxRichResults", 8);
user_pref("browser.urlbar.autoFill", true);
user_pref("browser.fixup.alternate.enabled", true);
user_pref("webgl.disabled", false);
user_pref("webgl.force-enabled", true);
user_pref("webgl.enable-webgl2", true);
user_pref("webgl.enable-debug-renderer-info", true);
"""


def write_interactive_prefs(profile_root: Path, profile_id: str) -> None:
    """Replicate the Desktop's write_camoufox_interactive_prefs exactly."""
    profile_directory = Path(profile_root) / profile_id
    profile_directory.mkdir(parents=True, exist_ok=True)
    (profile_directory / "user.js").write_bytes(
        CAMOUFOX_INTERACTIVE_USER_JS.encode("utf-8"))


class RuntimeClient:
    """JSONL request/response client for one Host runtime process."""

    def __init__(self, python: Path, host_dir: Path, package_root: Path,
                 artifact_root: Path, profile_root: Path, state_root: Path,
                 log_path: Path, interactive: bool = True) -> None:
        self.log_path = log_path
        self.frame_log_path = log_path.with_suffix(".frames.jsonl")
        self.log_file = log_path.open("wb")
        self.frame_log = self.frame_log_path.open("wb")
        env = dict(os.environ)
        env.pop("VERISILO_INTERACTIVE", None)
        if interactive:
            env["VERISILO_INTERACTIVE"] = "1"
        env["PYTHONUNBUFFERED"] = "1"
        self.process = subprocess.Popen(
            [
                str(python), str(host_dir / "host_v1.py"),
                "--package-root", str(package_root),
                "--artifact-root", str(artifact_root),
                "--profile-root", str(profile_root),
                "--state-root", str(state_root),
            ],
            cwd=str(host_dir), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=self.log_file, env=env,
        )
        self._lock = threading.Lock()
        self._lines: list[dict[str, Any]] = []
        self._responses: dict[str, list[dict[str, Any]]] = {}
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            text = line.decode("utf-8", errors="replace").strip()
            if not text:
                continue
            entry: dict[str, Any] = {"direction": "in", "at": time.time(), "frame": text}
            try:
                parsed = json.loads(text)
                if isinstance(parsed, dict) and isinstance(parsed.get("id"), str):
                    self._responses.setdefault(parsed["id"], []).append(parsed)
                    entry["id"] = parsed["id"]
            except json.JSONDecodeError:
                pass
            with self._lock:
                self._lines.append(entry)

    def request(self, command: str, params: dict[str, Any] | None = None,
                timeout: float = 240.0) -> dict[str, Any]:
        request_id = f"qa-{command}-{time.time_ns()}"
        frame: dict[str, Any] = {"id": request_id, "command": command}
        if params is not None:
            frame["params"] = params
        raw = json.dumps(frame, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
        with self._lock:
            self._lines.append(
                {"direction": "out", "at": time.time(), "frame": raw.decode("utf-8")})
        assert self.process.stdin is not None
        self.process.stdin.write(raw + b"\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            pending = self._responses.get(request_id)
            if pending:
                return pending[0]
            time.sleep(0.05)
        raise RuntimeError(f"timeout waiting for response to {command} ({request_id})")

    def flush_frame_log(self) -> None:
        with self._lock:
            snapshot = list(self._lines)
        for entry in snapshot:
            self.frame_log.write(
                (json.dumps(entry, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
            )
        self.frame_log.flush()

    def shutdown(self) -> int:
        if self.process.poll() is None:
            try:
                self.request("shutdown", timeout=90)
            except RuntimeError:
                pass
        try:
            if self.process.stdin:
                self.process.stdin.close()
        except OSError:
            pass
        try:
            code = self.process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            self.process.kill()
            code = self.process.wait(timeout=30)
        self.flush_frame_log()
        self.frame_log.close()
        self.log_file.close()
        return code


# ---------------------------------------------------------------------------
# Stages
# ---------------------------------------------------------------------------


def run_provision(python: Path, host_dir: Path, package_root: Path, artifact_root: Path,
                  state_root: Path, out_dir: Path, seed: str, preset: str,
                  window: tuple[int, int]) -> dict[str, Any]:
    out_dir.mkdir(parents=True, exist_ok=True)
    client = ProvisionClient(
        python=python, host_dir=host_dir, package_root=package_root,
        artifact_root=artifact_root, state_root=state_root,
        log_path=out_dir / "provision.stderr.log",
    )
    try:
        response = client.request({"seed": seed, "preset": preset, "window": list(window)})
    finally:
        client.close()
    (out_dir / "provision.response.json").write_text(
        json.dumps(response, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if not response.get("ok"):
        raise SystemExit(f"provision failed: {response.get('error')}")
    return response


def build_legacy_artifact(source: Path, screen_x: int, screen_y: int,
                          out_dir: Path) -> dict[str, Any]:
    """Craft a legal legacy-style artifact with out-of-bounds screen coords.

    The input is a freshly provisioned QA artifact (never user data).  Only the
    session-variable screenX/screenY values are pushed out of bounds; digests
    are recomputed with the Host's own strict validators so the file is a legal
    Artifact input for launch (pre-fix artifacts looked exactly like this).
    """
    from identity_policy import (  # noqa: PLC0415 - QA tool import
        assert_artifact_clean,
        compute_artifact_digest,
        configured_identity_digest,
        validate_artifact_strict,
    )
    from provision_artifact import _declared_stable_signals  # noqa: PLC0415

    artifact = json.loads(Path(source).read_text(encoding="utf-8"))
    config = artifact["resolvedConfig"]
    max_x = config["screen.availWidth"] - config["window.outerWidth"]
    max_y = config["screen.availHeight"] - config["window.outerHeight"]
    if screen_x <= max_x and screen_y <= max_y:
        raise SystemExit("requested legacy coordinates are not out of bounds")
    config["window.screenX"] = screen_x
    config["window.screenY"] = screen_y
    artifact["stableSignalsDeclared"] = _declared_stable_signals(config)
    artifact["configuredIdentityDigest"] = configured_identity_digest(config)
    artifact.pop("canonicalDigest", None)
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    validate_artifact_strict(artifact)
    assert_artifact_clean(artifact)

    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{artifact['artifactId']}.json"
    raw = (json.dumps(artifact, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    path.write_bytes(raw)
    sidecar = path.with_suffix(path.suffix + ".sha256")
    sidecar.write_bytes(f"{hashlib.sha256(raw).hexdigest()}  {path.name}\n".encode("ascii"))
    record = {
        "artifactId": artifact["artifactId"],
        "path": str(path),
        "fileSha256": hashlib.sha256(raw).hexdigest(),
        "injectedScreenX": screen_x,
        "injectedScreenY": screen_y,
        "maxInBoundsX": max_x,
        "maxInBoundsY": max_y,
    }
    (out_dir / "legacy-artifact.json").write_text(
        json.dumps(record, indent=2) + "\n", encoding="utf-8")
    return record


def observe_geometry(client: RuntimeClient, session_id: str) -> dict[str, Any]:
    evaluate = client.request("page", {
        "sessionId": session_id, "action": "evaluate", "script": GEOMETRY_EVALUATE_SCRIPT,
    })
    if not evaluate.get("ok"):
        raise RuntimeError(f"geometry evaluate failed: {evaluate.get('error')}")
    observation = evaluate["result"]["value"]
    windows = client.request("page", {"sessionId": session_id, "action": "windows"})
    if not windows.get("ok"):
        raise RuntimeError(f"page windows failed: {windows.get('error')}")
    return {
        "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "pageUrl": observation.get("url"),
        "observation": observation,
        "nativeWindows": windows["result"]["windows"],
        "nativeWindowCount": windows["result"]["count"],
    }


def run_session(python: Path, host_dir: Path, package_root: Path, artifact_root: Path,
                profile_root: Path, state_root: Path, artifact_file: Path,
                profile_id: str, label: str, out_dir: Path) -> dict[str, Any]:
    artifact_path = Path(artifact_file)
    artifact = json.loads(artifact_path.read_text(encoding="utf-8"))
    expected_sha = sha256_file(artifact_path)
    write_interactive_prefs(Path(profile_root), profile_id)
    client = RuntimeClient(
        python=python, host_dir=host_dir, package_root=package_root,
        artifact_root=artifact_root, profile_root=profile_root,
        state_root=state_root, log_path=out_dir / f"{label}.stderr.log",
        interactive=True,
    )
    record: dict[str, Any] = {
        "label": label,
        "artifactId": artifact["artifactId"],
        "artifactFile": str(artifact_path),
        "artifactFileSha256Before": expected_sha,
        "interactive": True,
        "steps": {},
    }
    try:
        hello = client.request("hello", timeout=60)
        record["steps"]["hello"] = hello
        if not hello.get("ok"):
            raise RuntimeError(f"hello failed: {hello.get('error')}")
        launch = client.request("launch", {
            "artifactId": artifact["artifactId"],
            "profileId": profile_id,
            "expectedArtifactFileSha256": expected_sha,
        }, timeout=420)
        record["steps"]["launch"] = launch
        if not launch.get("ok"):
            raise RuntimeError(f"launch failed: {launch.get('error')}")
        session_id = launch["result"]["sessionId"]
        record["sessionId"] = session_id
        record["geometry"] = observe_geometry(client, session_id)
        record["geometry"]["invariants"] = geometry_invariants(
            record["geometry"]["observation"])
        shot = client.request("page", {"sessionId": session_id, "action": "screenshot"})
        if shot.get("ok"):
            source = Path(shot["result"]["path"])
            target = out_dir / f"{label}-page.png"
            shutil.copyfile(source, target)
            record["screenshot"] = str(target)
        close = client.request("close", {"sessionId": session_id}, timeout=180)
        record["steps"]["close"] = close
        record["artifactFileSha256After"] = sha256_file(artifact_path)
    finally:
        record["hostExitCode"] = client.shutdown()
    (out_dir / f"{label}.result.json").write_text(
        json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return record


def cmd_run_all(args: argparse.Namespace) -> dict[str, Any]:
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    summary: dict[str, Any] = {
        "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "hostSourceBinding": {},
        "enginePackage": {},
        "python": sys.version.split()[0],
        "workArea": host_work_area(),
        "provision": {},
        "legacy": {},
        "sessions": {},
    }
    git_head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=str(REPO_ROOT), capture_output=True, text=True)
    summary["hostSourceBinding"] = {
        "head": git_head.stdout.strip(),
        "hostEntry": str(HOST_DIR / "host_v1.py"),
    }
    summary["enginePackage"] = {
        "packageRoot": str(args.package_root),
        "enginePackageJsonSha256": sha256_file(Path(args.package_root) / "engine-package.json"),
        "runtimeAssetLockSha256": sha256_file(
            Path(args.package_root) / "runtime-asset-lock.json"),
    }

    artifact_root = Path(args.artifact_root)
    for label, window in (("artifact-1280x800", (1280, 800)),
                          ("artifact-second-size", tuple(args.second_window))):
        response = run_provision(
            python=args.python, host_dir=args.host_dir, package_root=args.package_root,
            artifact_root=artifact_root, state_root=args.state_root, out_dir=out_dir,
            seed=args.seed, preset="balanced-en-us", window=window,
        )
        summary["provision"][label] = response["result"]
    artifact_a = artifact_root / f"{summary['provision']['artifact-1280x800']['artifactId']}.json"
    artifact_b = artifact_root / f"{summary['provision']['artifact-second-size']['artifactId']}.json"

    legacy_record = build_legacy_artifact(
        artifact_a, args.legacy_screen_x, args.legacy_screen_y, out_dir / "legacy")
    summary["legacy"] = legacy_record

    summary["sessions"]["first-1280x800"] = run_session(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=artifact_root, profile_root=args.profile_root,
        state_root=args.state_root, artifact_file=artifact_a,
        profile_id="qa-geom-a", label="first-1280x800", out_dir=out_dir)
    summary["sessions"]["cold-restart-1280x800"] = run_session(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=artifact_root, profile_root=args.profile_root,
        state_root=args.state_root, artifact_file=artifact_a,
        profile_id="qa-geom-a", label="cold-restart-1280x800", out_dir=out_dir)
    summary["sessions"]["second-size"] = run_session(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=artifact_root, profile_root=args.profile_root,
        state_root=args.state_root, artifact_file=artifact_b,
        profile_id="qa-geom-b", label="second-size", out_dir=out_dir)
    summary["sessions"]["legacy-artifact"] = run_session(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=Path(legacy_record["path"]).parent,
        profile_root=args.profile_root, state_root=args.state_root,
        artifact_file=Path(legacy_record["path"]), profile_id="qa-geom-legacy",
        label="legacy-artifact", out_dir=out_dir)

    verdicts: dict[str, Any] = {}
    for name, record in summary["sessions"].items():
        geometry = record.get("geometry")
        if not geometry:
            verdicts[name] = {"pass": False, "reason": "missing geometry observation"}
            continue
        invariants = geometry["invariants"]
        verdicts[name] = {
            "pass": all(invariants.values()),
            "invariants": invariants,
            "artifactBytesUnchanged": (
                record.get("artifactFileSha256Before")
                == record.get("artifactFileSha256After")
            ),
        }
    summary["invariantChecks"] = verdicts
    all_pass = all(item["pass"] for item in verdicts.values())
    summary["legacyArtifactBytesUnchanged"] = (
        verdicts.get("legacy-artifact", {}).get("artifactBytesUnchanged", False))
    summary["verdict"] = (
        "WINDOW_GEOMETRY_REAL_RUNTIME_ACCEPTED"
        if all_pass
        else "WINDOW_GEOMETRY_SOURCE_FIX_NOT_REFLECTED_IN_RUNTIME"
    )
    (out_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return summary


def cmd_provision(args: argparse.Namespace) -> None:
    run_provision(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=args.artifact_root, state_root=args.state_root,
        out_dir=args.out_dir, seed=args.seed, preset=args.preset,
        window=tuple(args.window),
    )


def cmd_legacy(args: argparse.Namespace) -> None:
    build_legacy_artifact(
        Path(args.source), args.screen_x, args.screen_y, Path(args.out_dir))


def cmd_runtime(args: argparse.Namespace) -> None:
    run_session(
        python=args.python, host_dir=args.host_dir, package_root=args.package_root,
        artifact_root=args.artifact_root, profile_root=args.profile_root,
        state_root=args.state_root, artifact_file=args.artifact_file,
        profile_id=args.profile_id, label=args.label, out_dir=args.out_dir,
    )


def parse_window(text: str) -> tuple[int, int]:
    match = re.fullmatch(r"(\d+)[xX](\d+)", text)
    if not match:
        raise argparse.ArgumentTypeError("window must look like 1280x800")
    return int(match.group(1)), int(match.group(2))


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path, default=DEFAULT_PYTHON)
    parser.add_argument("--host-dir", type=Path, default=HOST_DIR)
    parser.add_argument("--package-root", type=Path, required=True)
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("provision")
    p.add_argument("--seed", required=True)
    p.add_argument("--preset", default="balanced-en-us")
    p.add_argument("--window", type=parse_window, default=(1280, 800))
    p.add_argument("--artifact-root", type=Path, required=True)
    p.add_argument("--state-root", type=Path, required=True)
    p.add_argument("--out-dir", type=Path, required=True)

    p = sub.add_parser("legacy")
    p.add_argument("--source", type=Path, required=True)
    p.add_argument("--screen-x", type=int, default=2232)
    p.add_argument("--screen-y", type=int, default=140)
    p.add_argument("--out-dir", type=Path, required=True)

    p = sub.add_parser("runtime")
    p.add_argument("--artifact-file", type=Path, required=True)
    p.add_argument("--profile-id", required=True)
    p.add_argument("--label", required=True)
    p.add_argument("--artifact-root", type=Path, required=True)
    p.add_argument("--profile-root", type=Path, required=True)
    p.add_argument("--state-root", type=Path, required=True)
    p.add_argument("--out-dir", type=Path, required=True)

    p = sub.add_parser("run-all")
    p.add_argument("--seed", required=True)
    p.add_argument("--artifact-root", type=Path, required=True)
    p.add_argument("--profile-root", type=Path, required=True)
    p.add_argument("--state-root", type=Path, required=True)
    p.add_argument("--second-window", type=parse_window, default=(1024, 768))
    p.add_argument("--legacy-screen-x", type=int, default=2232)
    p.add_argument("--legacy-screen-y", type=int, default=140)
    p.add_argument("--out-dir", type=Path, required=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    if args.command == "provision":
        cmd_provision(args)
    elif args.command == "legacy":
        cmd_legacy(args)
    elif args.command == "runtime":
        cmd_runtime(args)
    elif args.command == "run-all":
        summary = cmd_run_all(args)
        print(json.dumps({key: summary[key] for key in ("verdict", "invariantChecks")},
                         indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
