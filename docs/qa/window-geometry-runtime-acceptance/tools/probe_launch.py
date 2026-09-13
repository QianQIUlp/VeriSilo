"""Launch probe: observe real window creation while the Host launch is in flight.

Evidence tooling only. Starts the current-source Host exactly like
run_geometry_qa.runtime, sends hello+launch on a background thread, and polls
visible top-level windows owned by the engine browser executable every 3s.
"""

from __future__ import annotations

import argparse
import json
import sys
import threading
import time
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
if str(TOOLS_DIR) not in sys.path:
    sys.path.insert(0, str(TOOLS_DIR))

from run_geometry_qa import (  # noqa: E402
    HOST_DIR,
    RuntimeClient,
    write_interactive_prefs,
)

from host_platform import visible_windows_for_executable  # noqa: E402


class ControlClient:
    """JSONL client for a packaged host exe (diagnostic control only)."""

    def __init__(self, host_exe: Path, package_root: Path, artifact_root: Path,
                 profile_root: Path, state_root: Path, log_path: Path) -> None:
        import subprocess
        import os
        self.log_file = log_path.open("wb")
        env = dict(os.environ)
        env["VERISILO_INTERACTIVE"] = "1"
        env["PYTHONUNBUFFERED"] = "1"
        self.process = subprocess.Popen(
            [str(host_exe),
             "--package-root", str(package_root),
             "--artifact-root", str(artifact_root),
             "--profile-root", str(profile_root),
             "--state-root", str(state_root)],
            cwd=str(host_exe.parent), stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.log_file, env=env,
        )
        self._responses: dict[str, list[dict]] = {}
        self._lock = threading.Lock()
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            text = line.decode("utf-8", errors="replace").strip()
            if not text:
                continue
            try:
                parsed = json.loads(text)
                if isinstance(parsed, dict) and isinstance(parsed.get("id"), str):
                    with self._lock:
                        self._responses.setdefault(parsed["id"], []).append(parsed)
            except json.JSONDecodeError:
                pass

    def request(self, command: str, params: dict | None = None,
                timeout: float = 240.0) -> dict:
        request_id = f"probe-{command}-{time.time_ns()}"
        frame: dict = {"id": request_id, "command": command}
        if params is not None:
            frame["params"] = params
        assert self.process.stdin is not None
        self.process.stdin.write((json.dumps(frame) + "\n").encode("utf-8"))
        self.process.stdin.flush()
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            with self._lock:
                pending = self._responses.get(request_id)
            if pending:
                return pending[0]
            time.sleep(0.05)
        raise RuntimeError(f"timeout waiting for response to {command}")

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
            return self.process.wait(timeout=60)
        except Exception:  # noqa: BLE001
            self.process.kill()
            return self.process.wait(timeout=30)
        finally:
            self.log_file.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--python", type=Path)
    parser.add_argument("--host-exe", type=Path,
                        help="spawn a packaged host exe directly (control mode)")
    parser.add_argument("--package-root", type=Path, required=True)
    parser.add_argument("--artifact-file", type=Path, required=True)
    parser.add_argument("--profile-id", default="qa-geom-probe")
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--profile-root", type=Path, required=True)
    parser.add_argument("--state-root", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--poll-seconds", type=float, default=3.0)
    args = parser.parse_args()

    import hashlib
    artifact_path = Path(args.artifact_file)
    artifact = json.loads(artifact_path.read_text(encoding="utf-8"))
    expected_sha = hashlib.sha256(artifact_path.read_bytes()).hexdigest()
    write_interactive_prefs(Path(args.profile_root), args.profile_id)

    browser_exe = Path(args.package_root) / "browser" / "camoufox.exe"
    supervisor_exe = Path(args.package_root) / "host" / "verisilo-camoufox-supervisor.exe"

    if args.host_exe is not None:
        client = ControlClient(
            host_exe=args.host_exe, package_root=args.package_root,
            artifact_root=args.artifact_root, profile_root=args.profile_root,
            state_root=args.state_root,
            log_path=Path(args.out_dir) / "probe.stderr.log",
        )
    else:
        if args.python is None:
            parser.error("--python is required unless --host-exe is given")
        client = RuntimeClient(
            python=args.python, host_dir=HOST_DIR, package_root=args.package_root,
            artifact_root=args.artifact_root, profile_root=args.profile_root,
            state_root=args.state_root,
            log_path=Path(args.out_dir) / "probe.stderr.log", interactive=True,
        )

    launch_outcome: dict = {}

    def do_launch() -> None:
        try:
            launch_outcome["response"] = client.request("launch", {
                "artifactId": artifact["artifactId"],
                "profileId": args.profile_id,
                "expectedArtifactFileSha256": expected_sha,
            }, timeout=260)
        except Exception as exc:  # noqa: BLE001
            launch_outcome["error"] = f"{type(exc).__name__}: {exc}"

    hello = client.request("hello", timeout=60)
    print(f"hello ok={hello.get('ok')}")
    thread = threading.Thread(target=do_launch, daemon=True)
    thread.start()

    polls: list[dict] = []
    deadline = time.monotonic() + 240
    while time.monotonic() < deadline:
        browser_windows = visible_windows_for_executable(browser_exe)
        supervisor_windows = visible_windows_for_executable(supervisor_exe)
        entry = {
            "at": time.strftime("%H:%M:%S"),
            "browserWindows": browser_windows,
            "supervisorWindows": supervisor_windows,
        }
        polls.append(entry)
        print(json.dumps(entry, ensure_ascii=False))
        if "response" in launch_outcome or "error" in launch_outcome:
            break
        thread.join(timeout=args.poll_seconds)
        if not thread.is_alive():
            break

    print(json.dumps({"launchOutcome": launch_outcome}, ensure_ascii=False, default=str))
    code = client.shutdown()
    print(f"host exit code: {code}")
    (Path(args.out_dir) / "probe.result.json").write_text(
        json.dumps({"polls": polls, "launchOutcome": launch_outcome},
                   indent=2, ensure_ascii=False, default=str) + "\n",
        encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
