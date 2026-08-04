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
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path

from identity_policy import (
    compute_artifact_digest,
    configured_identity_digest,
)

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
    def __init__(
        self,
        artifact_root: Path = FIXTURES,
        state_root: Path = STATE_ROOT,
        profile_root: Path = PROFILE_ROOT,
        probe_port: int = 0,
    ):
        self.lines: list[str] = []
        cmd = [
            str(VENV_PY),
            str(HOST_PY),
            "--artifact-root",
            str(artifact_root),
            "--profile-root",
            str(profile_root),
            "--state-root",
            str(state_root),
        ]
        if probe_port:
            cmd += ["--probe-port", str(probe_port)]
        self.proc = subprocess.Popen(
            cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=Path(__file__).parent,
        )

    def read_response(self, timeout: float = 120.0) -> dict:
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if not ready:
            raise TimeoutError("no protocol response")
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

    def send(self, obj: dict, timeout: float = 120.0) -> dict:
        self.proc.stdin.write(json.dumps(obj).encode("utf-8") + b"\n")
        self.proc.stdin.flush()
        return self.read_response(timeout)

    def send_raw(self, payload: bytes, timeout: float = 30.0) -> dict:
        self.proc.stdin.write(payload)
        self.proc.stdin.flush()
        return self.read_response(timeout)

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


@contextmanager
def fresh_roots():
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        yield root / "profiles", root / "state"


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
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(profile_root=profile_root, state_root=state_root)
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
    """Prove real cross-process persistence: same probe origin (fixed port),
    fresh temp roots, bootCount 0->1 then 1->2, cookie present via API/page,
    and cookie rows present in cookies.sqlite after close."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp_root = Path(tmp)
        profile_root = tmp_root / "profiles"
        state_root = tmp_root / "state"
        digests: list[str] = []
        boot_counts: list[tuple[int, int]] = []
        cookie_api: list[bool] = []
        cookie_page: list[bool] = []
        cookie_absent: list[bool | None] = []
        sqlite_rows: list[int | None] = []
        port = 0
        for index in range(2):
            host = HostProc(
                profile_root=profile_root,
                state_root=state_root,
                probe_port=port,
            )
            try:
                launch = host.launch(
                    "identity-a", "t-persist", artifact_sha("identity-a")
                )
                assert launch["ok"] is True, launch
                result = launch["result"]
                if index == 0:
                    port = result["probePort"]
                digests.append(result["observedWebsiteDigest"])
                boot_counts.append(
                    (result["bootCountBefore"], result["bootCountAfter"])
                )
                assert result["cookieEvidence"]["cookieValueLooksManaged"] is True
                cookie_api.append(result["cookieEvidence"]["cookieInApi"])
                cookie_page.append(result["cookieEvidence"]["cookieOnPage"])
                cookie_absent.append(
                    result["cookieEvidence"]["cookieAbsentBeforeWrite"]
                )
                closed = host.close(result["sessionId"])
                assert closed["ok"] is True, closed
                assert closed["result"]["exitFileObserved"] is True
                assert closed["result"]["processTreeExit"]["exited"] is True
                sqlite = closed["result"]["cookieSqlite"]
                assert sqlite["fileExists"] is True, sqlite
                assert sqlite["cookieNamePresent"] is True, sqlite
                assert sqlite["cookieRows"] >= 1, sqlite
                sqlite_rows.append(sqlite["cookieRows"])
                host.shutdown()
            finally:
                host.kill()
    assert boot_counts == [(0, 1), (1, 2)], boot_counts
    assert cookie_api == [True, True]
    assert cookie_page == [True, True]
    assert cookie_absent == [True, False]
    assert digests[0] == digests[1]
    assert sqlite_rows[0] >= 1 and sqlite_rows[1] >= 1


def test_three_cold_starts_same_digest() -> None:
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(profile_root=profile_root, state_root=state_root)
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
    with fresh_roots() as (profile_root, state_root):
        host1 = HostProc(profile_root=profile_root, state_root=state_root)
        host2 = HostProc(profile_root=profile_root, state_root=state_root)
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

        # (b2) missing NESTED required field (policy.canonicalJsonRule), with
        # all self-digests and the sidecar recomputed: strict schema must
        # still reject before any browser starts.
        nested = json.loads(source.read_text())
        nested["artifactId"] = "identity-missing"
        nested["policy"].pop("canonicalJsonRule")
        nested["configuredIdentityDigest"] = configured_identity_digest(
            nested["resolvedConfig"]
        )
        nested["canonicalDigest"] = compute_artifact_digest(nested)
        nested_path = ARTIFACT_ROOT / "identity-missing.json"
        nested_path.write_text(json.dumps(nested, indent=2) + "\n")
        (ARTIFACT_ROOT / "identity-missing.json.sha256").write_text(
            f"{hashlib.sha256(nested_path.read_bytes()).hexdigest()}  identity-missing.json\n"
        )
        expected_nested = hashlib.sha256(nested_path.read_bytes()).hexdigest()

        host2 = HostProc(artifact_root=ARTIFACT_ROOT)
        try:
            response = host2.launch(
                "identity-a", "t-tamper-field", expected
            )
            assert response["ok"] is False
            assert response["error"]["code"] == "integrity_rejected"
            response = host2.launch(
                "identity-missing", "t-tamper-nested", expected_nested
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
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(profile_root=profile_root, state_root=state_root)
        try:
            launch = host.launch("identity-a", "t-crash", artifact_sha("identity-a"))
            assert launch["ok"] is True, launch
            session_id = launch["result"]["sessionId"]
            supervisor_path = state_root / session_id / "supervisor.json"
            deadline = time.monotonic() + 20
            child_pid = 0
            while time.monotonic() < deadline:
                if supervisor_path.exists():
                    try:
                        meta = json.loads(supervisor_path.read_text())
                    except (OSError, json.JSONDecodeError):
                        meta = None
                    if isinstance(meta, dict) and isinstance(meta.get("childPid"), int):
                        child_pid = meta["childPid"]
                        break
                time.sleep(0.5)
            assert child_pid, "supervisor childPid metadata not found"
            os.kill(child_pid, signal.SIGKILL)
            wait_for_status(host, session_id, "failed")
            status = host.status(session_id)
            assert status["result"]["failure"] is not None
            assert status["result"]["exitFileObserved"] is True

            relaunch = host.launch("identity-a", "t-crash", artifact_sha("identity-a"))
            assert relaunch["ok"] is True, relaunch
            host.close(relaunch["result"]["sessionId"])
            host.shutdown()
            host.assert_stdout_pure()
        finally:
            host.kill()


def test_oversized_frame_rejected_bounded() -> None:
    host = HostProc()
    try:
        payload = (
            b'{"id":"big","command":"hello","params":{}'
            + b" " * (32 * 1024 + 64)
            + b"}\n"
        )
        response = host.send_raw(payload)
        assert response["ok"] is False
        assert response["error"]["code"] == "frame_too_large"
        # The Host must stay alive and usable after the oversized frame.
        hello = host.hello()
        assert hello["ok"] is True
        host.shutdown()
        host.assert_stdout_pure()
    finally:
        host.kill()


def _assert_shutdown_cleanup(profile_id: str, trigger) -> None:
    """launch -> trigger (SIGTERM/SIGINT/stdin EOF) -> Host exits -> the same
    profile relaunches (proving the lock was released only after the managed
    process tree was gone)."""
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(
            profile_root=profile_root,
            state_root=state_root,
        )
        try:
            launch = host.launch("identity-a", profile_id, artifact_sha("identity-a"))
            assert launch["ok"] is True, launch
            port = launch["result"]["probePort"]
            trigger(host)
            code = host.wait_exit(timeout=25)
            assert code == 0, f"host exit code after cleanup: {code}"
        finally:
            host.kill()
        host2 = HostProc(
            profile_root=profile_root,
            state_root=state_root,
            probe_port=port,
        )
        try:
            relaunch = host2.launch(
                "identity-a", profile_id, artifact_sha("identity-a")
            )
            assert relaunch["ok"] is True, relaunch
            host2.close(relaunch["result"]["sessionId"])
            host2.shutdown()
            host2.assert_stdout_pure()
        finally:
            host2.kill()


def test_sigterm_cleanup_active_session() -> None:
    _assert_shutdown_cleanup(
        "t-sigterm", lambda host: host.proc.send_signal(signal.SIGTERM)
    )


def test_sigint_cleanup_active_session() -> None:
    _assert_shutdown_cleanup(
        "t-sigint", lambda host: host.proc.send_signal(signal.SIGINT)
    )


def test_eof_cleanup_active_session() -> None:
    _assert_shutdown_cleanup("t-eof", lambda host: host.proc.stdin.close())


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
