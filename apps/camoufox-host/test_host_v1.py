#!/usr/bin/env python3
"""Integration tests for the M2 standalone Camoufox Host v1 stdio protocol.

Runs without pytest: `uv run python test_host_v1.py`.
"""

from __future__ import annotations

import asyncio
import fcntl
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
from host_v1 import (
    clear_quarantine_if_stale,
    proc_identity_alive,
    proc_starttime_ticks,
    process_descendants,
    quarantine_processes_alive,
    release_session,
    terminate_managed_tree,
    write_quarantine_record,
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


def test_old_v2_artifact_unsupported_schema_version() -> None:
    ARTIFACT_ROOT.mkdir(parents=True, exist_ok=True)
    source = FIXTURES / "identity-a.json"
    artifact = json.loads(source.read_text())
    artifact["artifactId"] = "identity-v2"
    artifact["schema"] = "verisilo-camoufox-resolved-identity/v2"
    artifact["policy"]["schema"] = "verisilo-camoufox-identity-policy/v2"
    artifact["policy"]["version"] = 2
    artifact["policy"].pop("fontMode")
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    path = ARTIFACT_ROOT / "identity-v2.json"
    path.write_text(json.dumps(artifact, indent=2) + "\n")
    (ARTIFACT_ROOT / "identity-v2.json.sha256").write_text(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  identity-v2.json\n"
    )
    expected = hashlib.sha256(path.read_bytes()).hexdigest()
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(
            artifact_root=ARTIFACT_ROOT,
            profile_root=profile_root,
            state_root=state_root,
        )
        try:
            response = host.launch("identity-v2", "t-v2", expected)
            assert response["ok"] is False
            assert response["error"]["code"] == "unsupported_schema_version"
            host.shutdown()
            host.assert_stdout_pure()
        finally:
            host.kill()


def test_managed_font_failure_never_running() -> None:
    ARTIFACT_ROOT.mkdir(parents=True, exist_ok=True)
    source = FIXTURES / "identity-a.json"
    artifact = json.loads(source.read_text())
    artifact["artifactId"] = "identity-managed"
    artifact["policy"]["fontMode"] = "managed"
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    path = ARTIFACT_ROOT / "identity-managed.json"
    path.write_text(json.dumps(artifact, indent=2) + "\n")
    (ARTIFACT_ROOT / "identity-managed.json.sha256").write_text(
        f"{hashlib.sha256(path.read_bytes()).hexdigest()}  identity-managed.json\n"
    )
    expected = hashlib.sha256(path.read_bytes()).hexdigest()
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(
            artifact_root=ARTIFACT_ROOT,
            profile_root=profile_root,
            state_root=state_root,
        )
        try:
            response = host.launch("identity-managed", "t-managed", expected)
            assert response["ok"] is False
            assert response["error"]["code"] == "host_font_masking_failed"
            status = host.status()
            assert status["result"]["state"] == "failed"
            assert "font" in (status["result"]["failure"] or "").lower()
            # The failed session was cleaned: same profile relaunches.
            # ARTIFACT_ROOT may hold stale artifacts from earlier runs, so
            # refresh identity-a from the tracked fixture first.
            (ARTIFACT_ROOT / "identity-a.json").write_bytes(
                (FIXTURES / "identity-a.json").read_bytes()
            )
            (ARTIFACT_ROOT / "identity-a.json.sha256").write_bytes(
                (FIXTURES / "identity-a.json.sha256").read_bytes()
            )
            relaunch = host.launch(
                "identity-a", "t-managed", artifact_sha("identity-a")
            )
            assert relaunch["ok"] is True, relaunch
            host.close(relaunch["result"]["sessionId"])
            host.shutdown()
            host.assert_stdout_pure()
        finally:
            host.kill()


def test_profile_quarantined_blocks_takeover_until_process_gone() -> None:
    dummy = subprocess.Popen(
        [str(VENV_PY), "-c", "import time; time.sleep(60)"]
    )
    try:
        starttime = proc_starttime_ticks(dummy.pid)
        assert starttime is not None
        with fresh_roots() as (profile_root, state_root):
            fake_session = {
                "profileId": "t-quarantine",
                "sessionId": "fake-session",
                "artifactId": "identity-a",
                "artifactFileSha256": "0" * 64,
            }
            write_quarantine_record(
                state_root,
                fake_session,
                "test quarantine",
                [
                    {
                        "pid": dummy.pid,
                        "startTimeTicks": starttime,
                        "processGroup": None,
                        "role": "browser",
                    }
                ],
            )
            host = HostProc(profile_root=profile_root, state_root=state_root)
            try:
                response = host.launch(
                    "identity-a", "t-quarantine", artifact_sha("identity-a")
                )
                assert response["ok"] is False
                assert response["error"]["code"] == "profile_quarantined"
                host.shutdown()
            finally:
                host.kill()
            dummy.kill()
            dummy.wait()
            host2 = HostProc(profile_root=profile_root, state_root=state_root)
            try:
                launch = host2.launch(
                    "identity-a", "t-quarantine", artifact_sha("identity-a")
                )
                assert launch["ok"] is True, launch
                host2.close(launch["result"]["sessionId"])
                host2.shutdown()
                host2.assert_stdout_pure()
            finally:
                host2.kill()
    finally:
        if dummy.poll() is None:
            dummy.kill()
            dummy.wait()


def test_proc_identity_and_quarantine_logic() -> None:
    dummy = subprocess.Popen(
        [str(VENV_PY), "-c", "import time; time.sleep(60)"]
    )
    try:
        starttime = proc_starttime_ticks(dummy.pid)
        assert starttime is not None
        # Same PID with a different starttime is NOT the original process.
        assert proc_identity_alive({"pid": dummy.pid, "startTimeTicks": starttime})
        assert not proc_identity_alive(
            {"pid": dummy.pid, "startTimeTicks": starttime + 1}
        )

        with tempfile.TemporaryDirectory() as tmp:
            state_root = Path(tmp)
            fake_session = {
                "profileId": "p-unit",
                "sessionId": "s-unit",
                "artifactId": "identity-a",
                "artifactFileSha256": "0" * 64,
            }
            record_path = write_quarantine_record(
                state_root,
                fake_session,
                "unit test",
                [
                    {
                        "pid": dummy.pid,
                        "startTimeTicks": starttime,
                        "processGroup": None,
                        "role": "browser",
                    }
                ],
            )
            assert record_path.exists()
            record = json.loads(record_path.read_text())
            assert quarantine_processes_alive(record), "dummy must be alive"
            check = clear_quarantine_if_stale(state_root, "p-unit")
            assert check["cleared"] is False
            assert check["alive"], "still-alive identity must block takeover"

            # Quarantined session keeps the profile lock when release_lock=False.
            lock_path = Path(tmp) / "p-unit.lock"
            lock_fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o600)
            fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            session_with_lock = dict(fake_session)
            session_with_lock["lockFd"] = lock_fd

            class FakeHost:
                pass

            asyncio.run(
                release_session(FakeHost(), session_with_lock, release_lock=False)
            )
            probe_fd = os.open(lock_path, os.O_RDWR)
            retained = False
            try:
                try:
                    fcntl.flock(probe_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except OSError:
                    retained = True
                else:
                    fcntl.flock(probe_fd, fcntl.LOCK_UN)
            finally:
                os.close(probe_fd)
            assert retained, "quarantine must retain the profile lock"

            asyncio.run(
                release_session(FakeHost(), session_with_lock, release_lock=True)
            )
            probe_fd = os.open(lock_path, os.O_RDWR)
            released = False
            try:
                fcntl.flock(probe_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                released = True
                fcntl.flock(probe_fd, fcntl.LOCK_UN)
            finally:
                os.close(probe_fd)
            assert released, "release_lock=True must free the profile lock"
    finally:
        if dummy.poll() is None:
            dummy.kill()
            dummy.wait()


def test_terminate_waits_for_ignored_sigterm_descendant() -> None:
    """Regression: the managed ROOT exits on SIGTERM while a captured
    descendant ignores SIGTERM. exited=true must NOT be returned until the
    descendant is gone too (before the fix this returned exited=true with
    sigkill=false and leaked the descendant + released the lock)."""
    with tempfile.TemporaryDirectory() as tmp:
        marker = Path(tmp) / "child-ready"
        child_code = (
            "import signal,time\n"
            "signal.signal(signal.SIGTERM, signal.SIG_IGN)\n"
            f"open({str(marker)!r}, 'w').write('ok')\n"
            "time.sleep(60)\n"
        )
        root_code = (
            "import subprocess, sys, time\n"
            f"child = subprocess.Popen([sys.executable, '-c', {child_code!r}])\n"
            "print(child.pid, flush=True)\n"
            "time.sleep(60)\n"
        )
        root = subprocess.Popen(
            [str(VENV_PY), "-c", root_code],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        child_pid = 0
        try:
            line = root.stdout.readline().strip()
            child_pid = int(line)
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                if marker.exists() and child_pid in process_descendants(root.pid):
                    break
                time.sleep(0.05)
            assert marker.exists(), "child never installed SIGTERM ignore"
            assert child_pid in process_descendants(root.pid), (
                "child never became a managed descendant"
            )
            root_start = proc_starttime_ticks(root.pid)
            assert root_start is not None
            session = {
                "supervisorMeta": {
                    "supervisorPid": root.pid,
                    "supervisorStartTimeTicks": root_start,
                }
            }
            result = terminate_managed_tree(session, timeout=1.5)
            assert result["exited"] is True, result
            assert result["sigkill"] is True, (
                "root exited but a SIGTERM-ignoring descendant survived: "
                f"{result}"
            )
            assert result["remaining"] == [], result
            assert proc_starttime_ticks(child_pid) is None, "descendant still alive"
        finally:
            for pid in (child_pid, root.pid):
                if pid and proc_starttime_ticks(pid) is not None:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
            if root.poll() is None:
                root.kill()
            root.wait()


def test_corrupt_quarantine_blocks_takeover() -> None:
    with fresh_roots() as (profile_root, state_root):
        quarantine_dir = state_root / "quarantine"
        quarantine_dir.mkdir(parents=True, exist_ok=True)
        record = quarantine_dir / "t-corrupt.json"
        record.write_text('{"schema": "verisilo-camoufox-profile-quarantine/v1"')
        check = clear_quarantine_if_stale(state_root, "t-corrupt")
        assert check["recordPresent"] is True
        assert check["cleared"] is False
        assert check["invalid"], "corrupt record must be fail-closed"

        # Missing required fields (no processes) must also block.
        (quarantine_dir / "t-corrupt.json").write_text(
            json.dumps(
                {
                    "schema": "verisilo-camoufox-profile-quarantine/v1",
                    "profileId": "t-corrupt",
                }
            )
        )
        check = clear_quarantine_if_stale(state_root, "t-corrupt")
        assert check["cleared"] is False
        assert check["invalid"], "schema-invalid record must block takeover"

        host = HostProc(profile_root=profile_root, state_root=state_root)
        try:
            response = host.launch(
                "identity-a", "t-corrupt", artifact_sha("identity-a")
            )
            assert response["ok"] is False
            assert response["error"]["code"] == "profile_quarantined"
            host.shutdown()
        finally:
            host.kill()


def test_quarantine_atomic_write_and_invalid_path() -> None:
    from host_v1 import read_quarantine_record

    dummy = subprocess.Popen(
        [str(VENV_PY), "-c", "import time; time.sleep(60)"]
    )
    try:
        starttime = proc_starttime_ticks(dummy.pid)
        with tempfile.TemporaryDirectory() as tmp:
            state_root = Path(tmp)
            session = {
                "profileId": "p-atomic",
                "sessionId": "s-atomic",
                "artifactId": "identity-a",
                "artifactFileSha256": "0" * 64,
            }
            record_path = write_quarantine_record(
                state_root,
                session,
                "atomic test",
                [
                    {
                        "pid": dummy.pid,
                        "startTimeTicks": starttime,
                        "processGroup": None,
                        "role": "browser",
                    }
                ],
            )
            assert record_path.exists()
            assert not list(record_path.parent.glob("*.tmp-*")), "tmp leftover"
            status, record, error = read_quarantine_record(state_root, "p-atomic")
            assert status == "valid" and error is None, (status, error)
            assert record["processes"][0]["pid"] == dummy.pid

            # An unreadable path (directory in place of the record) is
            # invalid, not absent.
            record_path.unlink()
            record_path.mkdir()
            (record_path / "junk").write_text("x")
            status, _, error = read_quarantine_record(state_root, "p-atomic")
            assert status == "invalid" and error
            check = clear_quarantine_if_stale(state_root, "p-atomic")
            assert check["cleared"] is False and check["invalid"]

            # A write into an impossible location raises (fail-closed signal).
            bad_root = Path(tmp) / "blocked"
            bad_root.write_text("not a dir")
            try:
                write_quarantine_record(
                    bad_root,
                    session,
                    "atomic test",
                    [{"pid": dummy.pid, "startTimeTicks": starttime, "processGroup": None, "role": "browser"}],
                )
            except OSError:
                pass
            else:
                raise AssertionError("write failure must raise (fail-closed)")
    finally:
        if dummy.poll() is None:
            dummy.kill()
            dummy.wait()


def test_artifact_non_object_and_duplicate_key_integrity_rejected() -> None:
    ARTIFACT_ROOT.mkdir(parents=True, exist_ok=True)
    cases = {
        "identity-array.json": b"[1, 2, 3]",
        "identity-dup.json": b'{"artifactId":"a","artifactId":"b"}',
        "identity-nan.json": b'{"generatedAtUtc": NaN}',
    }
    for name, raw in cases.items():
        path = ARTIFACT_ROOT / name
        path.write_bytes(raw)
        (ARTIFACT_ROOT / f"{name}.sha256").write_text(
            f"{hashlib.sha256(raw).hexdigest()}  {name}\n"
        )
    with fresh_roots() as (profile_root, state_root):
        host = HostProc(
            artifact_root=ARTIFACT_ROOT,
            profile_root=profile_root,
            state_root=state_root,
        )
        try:
            for name in cases:
                response = host.launch(
                    name[:-5], "t-strict-json", hashlib.sha256((ARTIFACT_ROOT / name).read_bytes()).hexdigest()
                )
                assert response["ok"] is False, name
                assert response["error"]["code"] == "integrity_rejected", (name, response)
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
