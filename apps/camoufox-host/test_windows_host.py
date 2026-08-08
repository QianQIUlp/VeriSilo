#!/usr/bin/env python3
"""Manual M2-W Windows Gate driver for the standalone Camoufox Host.

This is deliberately separate from the Linux integration test.  It uses
Windows pipe semantics, the Windows asset fixtures, and Job Object evidence;
it never changes the Linux fixture or evidence manifest.
"""

from __future__ import annotations

import hashlib
import json
import os
import queue
import shutil
import signal
import subprocess
import tempfile
import time
import uuid
from pathlib import Path
from threading import Thread
from typing import Any, Callable, Optional

from host_platform import JobHandle, IS_WINDOWS, process_creation_time, process_identity_alive
from browser_tree import TreeIntegrityError, build_tree_manifest, verify_tree
from identity_policy import compute_artifact_digest, configured_identity_digest
from host_v1 import write_quarantine_record
from run_spike import EXECUTABLE, new_run_id

REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
HOST_PY = HOST_DIR / "host_v1.py"
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"
TREE_MANIFEST = FIXTURES / "browser-tree-manifest-windows.json"
ARTIFACT = FIXTURES / "identity-win-a.json"
ARTIFACT_B = FIXTURES / "identity-win-b.json"
ARTIFACT_C = FIXTURES / "identity-win-c.json"
RUNS_ROOT = REPO_ROOT / "artifacts" / "camoufox-m2-windows-gate" / "runs"


def artifact_sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _readline_with_timeout(stream: Any, timeout: float) -> bytes:
    result: queue.Queue[tuple[str, Any]] = queue.Queue(maxsize=1)

    def read() -> None:
        try:
            result.put(("ok", stream.readline()))
        except BaseException as exc:  # noqa: BLE001
            result.put(("error", exc))

    Thread(target=read, daemon=True).start()
    try:
        kind, value = result.get(timeout=timeout)
    except queue.Empty as exc:
        raise TimeoutError("timed out waiting for Host protocol response") from exc
    if kind == "error":
        raise value
    return value


class HostProc:
    def __init__(
        self,
        profile_root: Path,
        state_root: Path,
        artifact_root: Path = FIXTURES,
        probe_port: int = 0,
    ) -> None:
        self.lines: list[str] = []
        self.cmd = [
            str(__import__("sys").executable),
            str(HOST_PY),
            "--artifact-root",
            str(artifact_root),
            "--profile-root",
            str(profile_root),
            "--state-root",
            str(state_root),
            "--tree-manifest",
            str(TREE_MANIFEST),
        ]
        if probe_port:
            self.cmd += ["--probe-port", str(probe_port)]
        self.proc = subprocess.Popen(
            self.cmd,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            cwd=HOST_DIR,
        )

    def read_response(self, timeout: float = 120.0) -> dict:
        if self.proc.stdout is None:
            raise RuntimeError("Host stdout is unavailable")
        raw = _readline_with_timeout(self.proc.stdout, timeout)
        if not raw:
            stderr = b""
            if self.proc.stderr is not None:
                stderr = self.proc.stderr.read()
            raise RuntimeError(f"Host closed stdout: {stderr[-2000:]!r}")
        line = raw.decode("utf-8").rstrip("\r\n")
        response = json.loads(line)
        self.lines.append(line)
        return response

    def send(self, obj: dict, timeout: float = 120.0, crlf: bool = False) -> dict:
        if self.proc.stdin is None:
            raise RuntimeError("Host stdin is unavailable")
        payload = json.dumps(obj, separators=(",", ":")).encode("utf-8")
        self.proc.stdin.write(payload + (b"\r\n" if crlf else b"\n"))
        self.proc.stdin.flush()
        return self.read_response(timeout)

    def send_raw(self, payload: bytes, timeout: float = 30.0) -> dict:
        if self.proc.stdin is None:
            raise RuntimeError("Host stdin is unavailable")
        self.proc.stdin.write(payload)
        self.proc.stdin.flush()
        return self.read_response(timeout)

    def hello(self) -> dict:
        return self.send({"id": "hello", "command": "hello"})

    def launch(self, artifact: Path, profile: str) -> dict:
        return self.send(
            {
                "id": f"launch-{profile}",
                "command": "launch",
                "params": {
                    "artifactId": artifact.stem,
                    "profileId": profile,
                    "expectedArtifactFileSha256": artifact_sha(artifact),
                },
            }
        )

    def close(self, session_id: str) -> dict:
        return self.send(
            {
                "id": f"close-{session_id}",
                "command": "close",
                "params": {"sessionId": session_id},
            }
        )

    def status(self, session_id: Optional[str] = None) -> dict:
        params = {"sessionId": session_id} if session_id else {}
        return self.send(
            {"id": "status", "command": "status", "params": params}
        )

    def shutdown(self) -> dict:
        response = self.send({"id": "shutdown", "command": "shutdown"})
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
            self.proc.wait(timeout=20)

    def assert_stdout_pure(self) -> None:
        for line in self.lines:
            parsed = json.loads(line)
            assert isinstance(parsed, dict)


def fresh_roots() -> tuple[tempfile.TemporaryDirectory[str], Path, Path]:
    tmp = tempfile.TemporaryDirectory(prefix="verisilo-m2w-")
    root = Path(tmp.name)
    return tmp, root / "profiles", root / "state"


def cleanup_temp(tmp: tempfile.TemporaryDirectory[str], timeout: float = 60.0) -> None:
    deadline = time.monotonic() + timeout
    last_error: Optional[Exception] = None
    while time.monotonic() < deadline:
        try:
            tmp.cleanup()
            if not Path(tmp.name).exists():
                return
        except OSError as exc:
            last_error = exc
        time.sleep(0.25)
    if last_error is not None:
        raise last_error
    raise OSError(f"temporary Gate root remains: {tmp.name}")


def wait_job_empty(
    job_name: str,
    identities: Optional[list[dict]] = None,
    timeout: float = 30.0,
) -> dict:
    deadline = time.monotonic() + timeout
    last = -1
    observed = False
    while time.monotonic() < deadline:
        try:
            job = JobHandle.open(job_name)
        except OSError:
            if identities is not None and all(
                not process_identity_alive(identity) for identity in identities
            ):
                return {
                    "name": job_name,
                    "activeProcessCount": 0,
                    "jobObjectClosed": True,
                }
            time.sleep(0.1)
            continue
        observed = True
        try:
            last = job.active_process_count()
            if last == 0:
                return {"name": job_name, "activeProcessCount": 0}
        finally:
            job.close()
        time.sleep(0.1)
    return {"name": job_name, "activeProcessCount": last}


def copy_fixture_set(destination: Path) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    for name in (
        "identity-win-a.json",
        "identity-win-a.json.sha256",
        "identity-win-b.json",
        "identity-win-b.json.sha256",
        "identity-win-c.json",
        "identity-win-c.json.sha256",
    ):
        shutil.copy2(FIXTURES / name, destination / name)


def test_protocol_and_integrity() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    host = HostProc(profile_root, state_root)
    try:
        hello = host.send({"id": "hello-crlf", "command": "hello"}, crlf=True)
        assert hello["ok"] is True, hello
        assert hello["result"]["browserRelease"] == "v152.0.4-beta.28"
        duplicate = host.send_raw(b'{"id":"d","command":"hello","id":"x"}\n')
        assert duplicate["error"]["code"] == "duplicate_key", duplicate
        invalid_number = host.send_raw(b'{"id":"n","command":"hello","x":NaN}\n')
        assert invalid_number["error"]["code"] == "invalid_number", invalid_number
        invalid_utf8 = host.send_raw(b'{"id":"u","command":"hello"}\xff\n')
        assert invalid_utf8["error"]["code"] == "invalid_utf8", invalid_utf8
        non_object = host.send_raw(b"[]\n")
        assert non_object["error"]["code"] == "frame_not_object", non_object
        oversized = host.send_raw(
            b'{"id":"big","command":"hello","params":{'
            + b"x" * (32 * 1024)
            + b"}}\n"
        )
        assert oversized["error"]["code"] == "frame_too_large", oversized
        assert host.hello()["ok"] is True
        shutdown = host.shutdown()
        assert shutdown["ok"] is True
        host.assert_stdout_pure()
        return {"status": "passed", "stdoutPure": True}
    finally:
        host.kill()
        cleanup_temp(tmp)


def test_persistence() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    host1 = HostProc(profile_root, state_root)
    port = 0
    try:
        first = host1.launch(ARTIFACT, "windows-persist")
        assert first["ok"] is True, first
        first_result = first["result"]
        port = first_result["probePort"]
        closed = host1.close(first_result["sessionId"])
        assert closed["ok"] is True and closed["result"]["processTreeExit"]["exited"]
        assert closed["result"]["processTreeExit"]["job"]["activeProcessCount"] == 0
        host1.shutdown()
    finally:
        host1.kill()

    host2 = HostProc(profile_root, state_root, probe_port=port)
    try:
        second = host2.launch(ARTIFACT, "windows-persist")
        assert second["ok"] is True, second
        second_result = second["result"]
        assert (first_result["bootCountBefore"], first_result["bootCountAfter"]) == (0, 1)
        assert (second_result["bootCountBefore"], second_result["bootCountAfter"]) == (1, 2)
        assert second_result["cookieEvidence"]["cookieInApi"] is True
        assert second_result["cookieEvidence"]["cookieOnPage"] is True
        assert second_result["observedWebsiteDigest"] == first_result["observedWebsiteDigest"]
        closed = host2.close(second_result["sessionId"])
        assert closed["ok"] is True
        sqlite = closed["result"]["cookieSqlite"]
        assert sqlite["fileExists"] is True and sqlite["cookieNamePresent"] is True
        assert sqlite["cookieRows"] >= 1
        host2.shutdown()
        return {
            "status": "passed",
            "bootCounts": [first_result["bootCountAfter"], second_result["bootCountAfter"]],
            "observedWebsiteDigest": second_result["observedWebsiteDigest"],
            "cookieSqlite": sqlite,
        }
    finally:
        host2.kill()
        cleanup_temp(tmp)


def test_profile_lock_and_crash_recovery() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    host1 = HostProc(profile_root, state_root)
    host2 = HostProc(profile_root, state_root)
    try:
        first = host1.launch(ARTIFACT, "windows-lock")
        assert first["ok"] is True, first
        second = host2.launch(ARTIFACT, "windows-lock")
        assert second["ok"] is False, second
        assert second["error"]["code"] == "profile_in_use", second
        closed = host1.close(first["result"]["sessionId"])
        assert closed["ok"] is True
        host1.shutdown()

        crashed = host2.launch(ARTIFACT, "windows-crash")
        assert crashed["ok"] is True, crashed
        session_id = crashed["result"]["sessionId"]
        meta = json.loads(
            (state_root / session_id / "supervisor.json").read_text(encoding="utf-8")
        )
        job = JobHandle.open(meta["jobName"])
        job.terminate(17)
        job.close()
        deadline = time.monotonic() + 30
        status = None
        while time.monotonic() < deadline:
            status = host2.status(session_id)
            if status.get("ok") and status["result"]["state"] == "failed":
                break
            time.sleep(0.5)
        assert status and status["result"]["state"] == "failed", status
        relaunch = host2.launch(ARTIFACT, "windows-crash")
        assert relaunch["ok"] is True, relaunch
        host2.close(relaunch["result"]["sessionId"])
        host2.shutdown()
        return {
            "status": "passed",
            "profileInUse": second["error"]["code"],
            "crashState": status["result"]["state"],
            "relaunch": True,
            "jobName": meta["jobName"],
        }
    finally:
        host1.kill()
        host2.kill()
        cleanup_temp(tmp)


def test_profile_quarantine_blocks_takeover() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    profile_id = "windows-quarantine"
    record_session = {
        "profileId": profile_id,
        "sessionId": "synthetic-quarantine",
        "artifactId": ARTIFACT.stem,
        "artifactFileSha256": artifact_sha(ARTIFACT),
    }
    creation = process_creation_time(os.getpid())
    assert creation is not None
    write_quarantine_record(
        state_root,
        record_session,
        "manual Windows quarantine test",
        [{"pid": os.getpid(), "creationTime100ns": creation, "role": "browser"}],
    )
    host = HostProc(profile_root, state_root)
    try:
        response = host.launch(ARTIFACT, profile_id)
        assert response["ok"] is False, response
        assert response["error"]["code"] == "profile_quarantined", response
        host.shutdown()
        return {"status": "passed", "errorCode": response["error"]["code"]}
    finally:
        host.kill()
        cleanup_temp(tmp)


def test_eof_and_forced_host_exit() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    host = HostProc(profile_root, state_root)
    try:
        launch = host.launch(ARTIFACT, "windows-eof")
        assert launch["ok"] is True, launch
        session_id = launch["result"]["sessionId"]
        meta = json.loads(
            (state_root / session_id / "supervisor.json").read_text(encoding="utf-8")
        )
        assert host.proc.stdin is not None
        host.proc.stdin.close()
        assert host.wait_exit(timeout=60) == 0
        eof_job = wait_job_empty(
            meta["jobName"],
            [
                {
                    "pid": meta["supervisorPid"],
                    "creationTime100ns": meta["supervisorCreationTime100ns"],
                },
                {
                    "pid": meta["childPid"],
                    "creationTime100ns": meta["childCreationTime100ns"],
                },
            ],
            timeout=60,
        )
        assert eof_job["activeProcessCount"] == 0, eof_job
    finally:
        host.kill()

    host2 = HostProc(profile_root, state_root)
    try:
        relaunch = host2.launch(ARTIFACT, "windows-eof")
        assert relaunch["ok"] is True, relaunch
        host2.close(relaunch["result"]["sessionId"])
        host2.shutdown()
    finally:
        host2.kill()

    host3 = HostProc(profile_root, state_root)
    try:
        forced = host3.launch(ARTIFACT, "windows-force-exit")
        assert forced["ok"] is True, forced
        forced_session = forced["result"]["sessionId"]
        forced_meta = json.loads(
            (state_root / forced_session / "supervisor.json").read_text(encoding="utf-8")
        )
        host3.proc.kill()
        host3.wait_exit(timeout=30)
    finally:
        host3.kill()
    forced_job = wait_job_empty(
        forced_meta["jobName"],
        [
            {
                "pid": forced_meta["supervisorPid"],
                "creationTime100ns": forced_meta["supervisorCreationTime100ns"],
            },
            {
                "pid": forced_meta["childPid"],
                "creationTime100ns": forced_meta["childCreationTime100ns"],
            },
        ],
        timeout=60,
    )
    assert forced_job["activeProcessCount"] == 0, forced_job

    host4 = HostProc(profile_root, state_root)
    try:
        relaunch = host4.launch(ARTIFACT, "windows-force-exit")
        assert relaunch["ok"] is True, relaunch
        host4.close(relaunch["result"]["sessionId"])
        host4.shutdown()
    finally:
        host4.kill()
        cleanup_temp(tmp)
    return {
        "status": "passed",
        "eofJob": eof_job,
        "forcedJob": forced_job,
        "forcedHostRelaunch": True,
    }


def test_replay_and_separation() -> dict:
    digests: list[str] = []
    raw_shas: list[str] = []
    job_results: list[dict] = []
    tmp, profile_root, state_root = fresh_roots()
    host = HostProc(profile_root, state_root)
    try:
        for index in range(1, 6):
            response = host.launch(ARTIFACT, f"windows-cold-{index}")
            assert response["ok"] is True, response
            result = response["result"]
            digests.append(result["observedWebsiteDigest"])
            raw_shas.append(result["artifactFileSha256"])
            closed = host.close(result["sessionId"])
            assert closed["ok"] is True
            job_results.append(closed["result"]["processTreeExit"]["job"])
        host.shutdown()
    finally:
        host.kill()
        cleanup_temp(tmp)
    assert len(set(digests)) == 1, digests
    assert raw_shas == [artifact_sha(ARTIFACT)] * 5
    assert all(item["activeProcessCount"] == 0 for item in job_results)

    separation: dict[str, str] = {}
    for artifact, profile in (
        (ARTIFACT, "windows-separation-a"),
        (ARTIFACT_B, "windows-separation-b"),
        (ARTIFACT_C, "windows-separation-c"),
    ):
        tmp, profile_root, state_root = fresh_roots()
        host = HostProc(profile_root, state_root)
        try:
            response = host.launch(artifact, profile)
            assert response["ok"] is True, response
            result = response["result"]
            separation[artifact.stem] = result["observedWebsiteDigest"]
            closed = host.close(result["sessionId"])
            assert closed["ok"] is True
            host.shutdown()
        finally:
            host.kill()
            cleanup_temp(tmp)
    assert len(set(separation.values())) == 3, separation
    return {
        "status": "passed",
        "stabilityDigests": digests,
        "expectedArtifactFileSha256": raw_shas,
        "jobResults": job_results,
        "separationDigests": separation,
    }


def test_tamper_rejections() -> dict:
    tmp, profile_root, state_root = fresh_roots()
    artifact_root = Path(tmp.name) / "artifacts"
    copy_fixture_set(artifact_root)
    host = HostProc(profile_root, state_root, artifact_root=artifact_root)
    cases: dict[str, str] = {}
    try:
        wrong = host.send(
            {
                "id": "wrong-sha",
                "command": "launch",
                "params": {
                    "artifactId": ARTIFACT.stem,
                    "profileId": "windows-tamper-sha",
                    "expectedArtifactFileSha256": "0" * 64,
                },
            }
        )
        assert wrong["error"]["code"] == "integrity_rejected", wrong
        cases["expectedRawSha"] = wrong["error"]["code"]

        artifact = json.loads((artifact_root / "identity-win-a.json").read_text(encoding="utf-8"))
        artifact["resolvedConfig"].pop("screen.availTop")
        broken = artifact_root / "identity-broken.json"
        artifact["artifactId"] = "identity-broken"
        broken.write_text(json.dumps(artifact, indent=2) + "\n", encoding="utf-8")
        broken.with_suffix(".json.sha256").write_text(
            f"{artifact_sha(broken)}  {broken.name}\n", encoding="utf-8"
        )
        response = host.launch(broken, "windows-tamper-field")
        assert response["error"]["code"] == "integrity_rejected", response
        cases["missingField"] = response["error"]["code"]

        sidecar = artifact_root / "identity-win-b.json.sha256"
        sidecar.unlink()
        response = host.launch(artifact_root / "identity-win-b.json", "windows-tamper-sidecar")
        assert response["error"]["code"] == "integrity_rejected", response
        cases["sidecar"] = response["error"]["code"]

        binding = json.loads(
            (artifact_root / "identity-win-c.json").read_text(encoding="utf-8")
        )
        binding["artifactId"] = "identity-binding"
        binding["browserBinding"]["archiveSha256"] = "0" * 64
        binding_path = artifact_root / "identity-binding.json"
        binding_path.write_text(json.dumps(binding, indent=2) + "\n", encoding="utf-8")
        binding_path.with_suffix(".json.sha256").write_text(
            f"{artifact_sha(binding_path)}  {binding_path.name}\n", encoding="utf-8"
        )
        response = host.launch(binding_path, "windows-tamper-binding")
        assert response["error"]["code"] == "integrity_rejected", response
        cases["browserBinding"] = response["error"]["code"]

        for artifact_id, raw in (
            ("identity-dup", b'{"schema":"x","schema":"y"}'),
            ("identity-nan", b'{"generatedAtUtc":NaN}'),
        ):
            raw_path = artifact_root / f"{artifact_id}.json"
            raw_path.write_bytes(raw)
            raw_path.with_suffix(".json.sha256").write_text(
                f"{artifact_sha(raw_path)}  {raw_path.name}\n", encoding="utf-8"
            )
            response = host.launch(raw_path, f"windows-tamper-{artifact_id[9:]}")
            assert response["error"]["code"] == "integrity_rejected", response
            cases[artifact_id] = response["error"]["code"]

        host.shutdown()
        return {"status": "passed", "cases": cases}
    finally:
        host.kill()
        cleanup_temp(tmp)


def test_reparse_point_rejection() -> dict:
    target = Path(tempfile.mkdtemp(prefix="verisilo-reparse-target-"))
    junction = EXECUTABLE.parent / "__m2w_junction__"
    if junction.exists():
        junction.unlink()
    try:
        created = subprocess.run(
            ["cmd.exe", "/c", "mklink", "/J", str(junction), str(target)],
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert created.returncode == 0 and junction.exists(), created.stderr or created.stdout
        tmp, profile_root, state_root = fresh_roots()
        host = HostProc(profile_root, state_root)
        try:
            response = host.launch(ARTIFACT, "windows-reparse")
            assert response["ok"] is False, response
            assert response["error"]["code"] == "integrity_rejected", response
            host.shutdown()
        finally:
            host.kill()
            cleanup_temp(tmp)
        return {"status": "passed", "junctionRejected": True}
    finally:
        if junction.exists():
            junction.unlink()
        target.rmdir()


def test_tree_missing_extra_modified_rejection() -> dict:
    with tempfile.TemporaryDirectory(prefix="verisilo-tree-") as tmp:
        root = Path(tmp) / "bundle"
        root.mkdir()
        file_path = root / "browser.bin"
        file_path.write_bytes(b"stable-browser-tree")
        manifest = build_tree_manifest(root)
        assert verify_tree(root, manifest)["verified"] is True

        extra = root / "extra.bin"
        extra.write_bytes(b"extra")
        try:
            verify_tree(root, manifest)
        except TreeIntegrityError:
            pass
        else:
            raise AssertionError("extra tree entry must be rejected")
        extra.unlink()

        original = file_path.read_bytes()
        file_path.write_bytes(b"modified-browser-tree")
        try:
            verify_tree(root, manifest)
        except TreeIntegrityError:
            pass
        else:
            raise AssertionError("modified tree entry must be rejected")
        file_path.write_bytes(original)
        file_path.unlink()
        try:
            verify_tree(root, manifest)
        except TreeIntegrityError:
            pass
        else:
            raise AssertionError("missing tree entry must be rejected")
    return {"status": "passed", "missing": True, "extra": True, "modified": True}


def test_pid_reuse_creation_time() -> dict:
    creation = process_creation_time(os.getpid())
    assert creation is not None
    from host_platform import process_identity_alive

    assert process_identity_alive({"pid": os.getpid(), "creationTime100ns": creation})
    assert not process_identity_alive(
        {"pid": os.getpid(), "creationTime100ns": creation + 1}
    )
    return {"status": "passed", "creationTime100ns": creation}


TESTS: list[tuple[str, Callable[[], dict]]] = [
    ("protocol", test_protocol_and_integrity),
    ("persistence", test_persistence),
    ("lock-crash", test_profile_lock_and_crash_recovery),
    ("quarantine", test_profile_quarantine_blocks_takeover),
    ("eof-force-exit", test_eof_and_forced_host_exit),
    ("tamper", test_tamper_rejections),
    ("reparse", test_reparse_point_rejection),
    ("tree-integrity", test_tree_missing_extra_modified_rejection),
    ("pid-reuse", test_pid_reuse_creation_time),
]


def main() -> int:
    if not IS_WINDOWS:
        print("M2-W requires a real Windows host")
        return 2
    RUNS_ROOT.mkdir(parents=True, exist_ok=True)
    results: dict[str, dict] = {}
    failed = 0
    for name, test in TESTS:
        run_id = new_run_id()
        run_dir = RUNS_ROOT / run_id
        run_dir.mkdir(parents=True, exist_ok=False)
        print(f"RUN {name} run-id={run_id}")
        try:
            result = test()
            result["runId"] = run_id
            results[name] = result
            (run_dir / "report.json").write_text(
                json.dumps(result, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            print(f"PASS {name} run-id={run_id}")
        except Exception as exc:  # noqa: BLE001
            failed += 1
            result = {"status": "failed", "runId": run_id, "error": f"{type(exc).__name__}: {exc}"}
            results[name] = result
            (run_dir / "report.json").write_text(
                json.dumps(result, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
            print(f"FAIL {name} run-id={run_id}: {result['error']}")
    summary = {
        "schema": "verisilo-camoufox-m2-windows-gate-run/v1",
        "generatedAtUtc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "results": results,
        "status": "passed" if failed == 0 else "failed",
    }
    summary_path = RUNS_ROOT / f"summary-{int(time.time())}.json"
    summary_path.write_text(json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
