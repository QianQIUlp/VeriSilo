#!/usr/bin/env python3
"""Integration tests for the M2 standalone Camoufox Host v1 stdio protocol.

Runs without pytest: `uv run python test_host_v1.py`.
"""

from __future__ import annotations

import hashlib
import json
import os
import select
import signal
import subprocess
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_PY = Path(__file__).parent / "host_v1.py"
VENV_PY = Path(__file__).parent / ".venv" / "bin" / "python"
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"
TEST_ART = REPO_ROOT / "artifacts" / "camoufox-m2" / "integration"
PROFILE_ROOT = TEST_ART / "profiles"
STATE_ROOT = TEST_ART / "state"
ARTIFACT_ROOT = TEST_ART / "artifacts"
EXTRACT_DIR = (
    REPO_ROOT
    / "artifacts"
    / "camoufox-m0"
    / "browser"
    / "camoufox-152.0.4-beta.28-lin-x86_64"
)


class HostProc:
    def __init__(self, artifact_root: Path = FIXTURES, state_root: Path = STATE_ROOT):
        self.lines: list[str] = []
        self.proc = subprocess.Popen(
            [
                str(VENV_PY),
                str(HOST_PY),
                "--artifact-root",
                str(artifact_root),
                "--profile-root",
                str(PROFILE_ROOT),
                "--state-root",
                str(state_root),
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=Path(__file__).parent,
        )

    def send(self, obj: dict, timeout: float = 120.0) -> dict:
        self.proc.stdin.write(json.dumps(obj).encode("utf-8") + b"\n")
        self.proc.stdin.flush()
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            raise TimeoutError(f"no response for {obj.get('command')}")
        raw = self.proc.stdout.readline()
        if not raw:
            raise RuntimeError("host closed stdout while waiting for response")
        line = raw.decode("utf-8").rstrip("\n")
        try:
            response = json.loads(line)
        except json.JSONDecodeError as exc:
            raise AssertionError(f"stdout frame is not JSON: {line!r}") from exc
        self.lines.append(line)
        return response

    def hello(self) -> dict:
        return self.send({"id": "h1", "command": "hello"})

    def launch(self, artifact_id: str, profile_id: str, expected_sha: str) -> dict:
        return self.send(
            {
                "id": "l1",
                "command": "launch",
                "params": {
                    "artifactId": artifact_id,
                    "profileId": profile_id,
                    "expectedArtifactFileSha256": expected_sha,
                },
            }
        )

    def status(self, session_id: str | None = None) -> dict:
        params = {"sessionId": session_id} if session_id else {}
        return self.send({"id": "s1", "command": "status", "params": params})

    def close(self, session_id: str) -> dict:
        return self.send(
            {"id": "c1", "command": "close", "params": {"sessionId": session_id}}
        )

    def shutdown(self) -> dict:
        response = self.send({"id": "x1", "command": "shutdown"})
        self.wait_exit()
        return response

    def wait_exit(self, timeout: float = 30.0) -> int:
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            return self.proc.wait(timeout=10)

    def kill(self) -> None:
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(timeout=10)

    def assert_stdout_pure(self) -> None:
        for line in self.lines:
            json.loads(line)  # must not raise


def artifact_sha(name: str) -> str:
    return hashlib.sha256((FIXTURES / f"{name}.json").read_bytes()).hexdigest()


def browser_pid_for_profile(profile_dir: Path) -> int:
    marker = str(profile_dir)
    own = os.getpid()
    best = None
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        pid = int(entry)
        if pid == own:
            continue
        try:
            cmdline = Path(f"/proc/{entry}/cmdline").read_bytes()
        except OSError:
            continue
        text = cmdline.replace(b"\0", b" ").decode(errors="replace")
        if "-profile" in text and marker in text:
            if best is None or pid < best:
                best = pid
    return best or 0


def wait_for_status(host: HostProc, session_id: str, state: str, timeout: float = 30.0) -> dict:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        response = host.status(session_id)
        if response.get("ok") and response["result"].get("state") == state:
            return response
        time.sleep(1)
    raise AssertionError(f"state did not become {state}: {response!r}")


def test_hello_and_version_binding() -> None:
    host = HostProc()
    try:
        response = host.hello()
        assert response["ok"] is True
        result = response["result"]
        assert result["protocol"] == "verisilo-camoufox-host/v1"
        assert result["hostVersion"] == "0.1.0"
        assert result["state"] == "idle"
        assert result["verified"] is False
        assert result["evidenceClass"] == "observed-on-this-host"
        assert result["browserRelease"] == "v152.0.4-beta.28"
        shutdown = host.shutdown()
        assert shutdown["ok"] is True
        assert shutdown["result"]["selfCheck"]["argvMatches"] == []
        assert shutdown["result"]["selfCheck"]["stderrLogMatches"] == []
        host.assert_stdout_pure()
    finally:
        host.kill()


def test_launch_status_close() -> None:
    host = HostProc()
    try:
        launch = host.launch("identity-a", "t-lifecycle", artifact_sha("identity-a"))
        assert launch["ok"] is True, launch
        result = launch["result"]
        assert result["state"] == "running"
        assert result["observedWebsiteDigest"].startswith("sha256:")
        assert result["bootCountBefore"] == 0
        assert result["bootCountAfter"] == 1
        status = host.status(result["sessionId"])
        assert status["result"]["state"] == "running"
        closed = host.close(result["sessionId"])
        assert closed["ok"] is True
        assert closed["result"]["state"] == "exited"
        assert closed["result"]["exitStatus"] == 0
        host.shutdown()
        host.assert_stdout_pure()
    finally:
        host.kill()


def test_restart_persistence() -> None:
    digests: list[str] = []
    for _ in range(2):
        host = HostProc()
        try:
            launch = host.launch(
                "identity-a", "t-persist", artifact_sha("identity-a")
            )
            assert launch["ok"] is True, launch
            digests.append(launch["result"]["observedWebsiteDigest"])
            assert launch["result"]["bootCountAfter"] == launch["result"]["bootCountBefore"] + 1
            host.close(launch["result"]["sessionId"])
            host.shutdown()
        finally:
            host.kill()
    assert digests[0] == digests[1]


def test_three_cold_starts_same_digest() -> None:
    host = HostProc()
    try:
        digests: list[str] = []
        for index in (1, 2, 3):
            launch = host.launch(
                "identity-a", f"t-cold-{index}", artifact_sha("identity-a")
            )
            assert launch["ok"] is True, launch
            digests.append(launch["result"]["observedWebsiteDigest"])
            assert launch["result"]["bootCountBefore"] == 0
            host.close(launch["result"]["sessionId"])
        assert len(set(digests)) == 1
        host.shutdown()
        host.assert_stdout_pure()
    finally:
        host.kill()


def test_profile_in_use() -> None:
    host1 = HostProc()
    host2 = HostProc()
    try:
        first = host1.launch("identity-a", "t-lock", artifact_sha("identity-a"))
        assert first["ok"] is True
        second = host2.launch("identity-a", "t-lock", artifact_sha("identity-a"))
        assert second["ok"] is False
        assert second["error"]["code"] == "profile_in_use"
        host1.close(first["result"]["sessionId"])
        host1.shutdown()
        host2.shutdown()
    finally:
        host1.kill()
        host2.kill()


def test_tamper_rejections() -> None:
    # (a) wrong expectedArtifactFileSha256
    host = HostProc()
    try:
        response = host.launch("identity-a", "t-tamper-sha", "0" * 64)
        assert response["ok"] is False
        assert response["error"]["code"] == "integrity_rejected"

        # (b) missing required config field (sidecar kept valid)
        ARTIFACT_ROOT.mkdir(parents=True, exist_ok=True)
        source = FIXTURES / "identity-a.json"
        tampered = ARTIFACT_ROOT / "identity-a.json"
        artifact = json.loads(source.read_text())
        artifact["resolvedConfig"].pop("screen.availTop")
        tampered.write_text(json.dumps(artifact, indent=2) + "\n")
        (ARTIFACT_ROOT / "identity-a.json.sha256").write_text(
            f"{hashlib.sha256(tampered.read_bytes()).hexdigest()}  identity-a.json\n"
        )
        expected = hashlib.sha256(tampered.read_bytes()).hexdigest()
        host2 = HostProc(artifact_root=ARTIFACT_ROOT)
        try:
            response = host2.launch(
                "identity-a", "t-tamper-field", expected
            )
            assert response["ok"] is False
            assert response["error"]["code"] == "integrity_rejected"
        finally:
            host2.shutdown()
            host2.kill()

        # (c) extraction tree tamper (extra file)
        marker = EXTRACT_DIR / "__m2_tree_tamper_test__"
        marker.write_text("tamper")
        try:
            response = host.launch(
                "identity-a", "t-tamper-tree", artifact_sha("identity-a")
            )
            assert response["ok"] is False
            assert response["error"]["code"] == "integrity_rejected"
        finally:
            marker.unlink()
        host.shutdown()
        host.assert_stdout_pure()
    finally:
        host.kill()


def test_crash_recovery() -> None:
    host = HostProc()
    try:
        launch = host.launch("identity-a", "t-crash", artifact_sha("identity-a"))
        assert launch["ok"] is True, launch
        session_id = launch["result"]["sessionId"]
        profile_dir = PROFILE_ROOT / "t-crash"
        deadline = time.monotonic() + 20
        pid = 0
        while time.monotonic() < deadline:
            pid = browser_pid_for_profile(profile_dir)
            if pid:
                break
            time.sleep(0.5)
        assert pid, "browser pid not found"
        os.kill(pid, signal.SIGKILL)
        wait_for_status(host, session_id, "failed")
        status = host.status(session_id)
        assert status["result"]["failure"] is not None

        relaunch = host.launch("identity-a", "t-crash", artifact_sha("identity-a"))
        assert relaunch["ok"] is True, relaunch
        host.close(relaunch["result"]["sessionId"])
        host.shutdown()
        host.assert_stdout_pure()
    finally:
        host.kill()


def main() -> int:
    tests = [
        (name, fn)
        for name, fn in sorted(globals().items())
        if name.startswith("test_") and callable(fn)
    ]
    failed = 0
    for name, fn in tests:
        print(f"RUN {name}")
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            failed += 1
            print(f"FAIL {name}: {exc}")
    if failed:
        print(f"{failed}/{len(tests)} host integration tests failed")
        return 1
    print(f"all {len(tests)} host integration tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
