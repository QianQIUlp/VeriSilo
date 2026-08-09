#!/usr/bin/env python3
"""Deterministic M3-0 Camoufox Host transport fixture.

This process implements only the bounded JSONL Host contract. It never imports
Camoufox, opens a browser, touches an Artifact, or starts a managed process
tree. It exists solely for desktop transport/lifecycle tests.
"""

from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path

PROTOCOL = "verisilo-camoufox-host/v1"
HOST_VERSION = "0.1.0"
MAX_FRAME_BYTES = 32768
ASSET_SHA256 = "b" * 64
BROWSER_RELEASE = "v152.0.4-beta.28"
PLATFORM = "windows-x64" if sys.platform == "win32" else "linux-x64"


def args_map(argv: list[str]) -> dict[str, str]:
    values: dict[str, str] = {}
    index = 0
    while index < len(argv):
        key = argv[index]
        if not key.startswith("--") or index + 1 >= len(argv):
            raise SystemExit("invalid fake Host arguments")
        values[key] = argv[index + 1]
        index += 2
    return values


def write_frame(payload: bytes, newline: bool = True) -> None:
    if newline and len(payload) > MAX_FRAME_BYTES:
        raise SystemExit("fake Host response exceeded frame bound")
    sys.stdout.buffer.write(payload + (b"\n" if newline else b""))
    sys.stdout.buffer.flush()


def response(
    request_id: str | None,
    result: dict,
    *,
    extra: dict | None = None,
    response_id: str | None = None,
) -> None:
    payload = {"id": request_id if response_id is None else response_id, "ok": True, "result": result}
    if extra:
        payload.update(extra)
    write_frame(json.dumps(payload, separators=(",", ":")).encode())


def error_response(request_id: str | None, code: str, message: str) -> None:
    write_frame(
        json.dumps(
            {"id": request_id, "ok": False, "error": {"code": code, "message": message}},
            separators=(",", ":"),
        ).encode()
    )


def hello_result(
    artifact_root: str,
    profile_root: str,
    state_root: str,
    tree_manifest: str,
    tree_sha: str,
) -> dict:
    return {
        "protocol": PROTOCOL,
        "hostVersion": HOST_VERSION,
        "pythonVersion": "fake",
        "artifactRoot": artifact_root,
        "profileRoot": profile_root,
        "stateRoot": state_root,
        "maxFrameBytes": MAX_FRAME_BYTES,
        "probePortPolicy": "ephemeral",
        "browserRelease": BROWSER_RELEASE,
        "assetSha256": ASSET_SHA256,
        "treeManifest": tree_manifest,
        "treeManifestSha256": tree_sha,
        "platform": PLATFORM,
        "state": "idle",
        "verified": False,
        "evidenceClass": "observed-on-this-host",
    }


def main() -> int:
    values = args_map(sys.argv[1:])
    mode = values.get("--mode", "normal")
    artifact_root = str(Path(values["--artifact-root"]).absolute())
    profile_root = str(Path(values["--profile-root"]).absolute())
    state_root = str(Path(values["--state-root"]).absolute())
    tree_manifest = str(Path(values["--tree-manifest"]).absolute())
    tree_sha = hashlib.sha256(Path(tree_manifest).read_bytes()).hexdigest()
    session_id = "11111111-1111-4111-8111-111111111111"
    artifact_id = "identity-m3-fake"
    profile_id = "silo-22222222222242228222222222222222"
    artifact_sha = "a" * 64
    state = "idle"

    def launch_result() -> dict:
        return {
            "sessionId": session_id,
            "state": "running",
            "artifactId": artifact_id,
            "profileId": profile_id,
            "artifactFileSha256": artifact_sha,
            "configuredIdentityDigest": "c" * 64,
            "observedWebsiteDigest": "d" * 64,
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }

    def status_result() -> dict:
        return {
            "state": state,
            "sessionId": session_id,
            "artifactId": artifact_id,
            "profileId": profile_id,
            "artifactFileSha256": artifact_sha,
            "configuredIdentityDigest": "c" * 64,
            "observedWebsiteDigest": "d" * 64,
            "exitStatus": None,
            "exitFileObserved": None,
            "quarantine": None,
            "failure": None,
            "verified": False,
            "evidenceClass": "observed-on-this-host",
        }

    for raw in sys.stdin.buffer:
        if len(raw.rstrip(b"\n")) > MAX_FRAME_BYTES:
            raise SystemExit("fake Host request exceeded frame bound")
        request = json.loads(raw)
        request_id = request.get("id")
        command = request.get("command")
        params = request.get("params", {})
        if mode == "eof":
            return 0
        if mode == "timeout":
            continue
        if command == "hello":
            if mode == "invalid-utf8":
                write_frame(b"\xff")
                return 0
            if mode == "oversized":
                sys.stdout.buffer.write(b"x" * (MAX_FRAME_BYTES + 1) + b"\n")
                sys.stdout.buffer.flush()
                return 0
            if mode == "partial-frame":
                write_frame(b'{"id":"m3-1"', newline=False)
                return 0
            if mode == "duplicate-field":
                payload = json.dumps(hello_result(artifact_root, profile_root, state_root, tree_manifest, tree_sha), separators=(",", ":")).encode()
                write_frame(b'{"id":"m3-1","ok":true,"ok":true,"result":' + payload)
                return 0
            hello = hello_result(artifact_root, profile_root, state_root, tree_manifest, tree_sha)
            if mode == "wrong-protocol":
                hello["protocol"] = "wrong/protocol"
            elif mode == "wrong-host-version":
                hello["hostVersion"] = "9.9.9"
            elif mode == "wrong-platform":
                hello["platform"] = "linux-x64" if PLATFORM == "windows-x64" else "windows-x64"
            elif mode == "wrong-release":
                hello["browserRelease"] = "152.0.4-beta.28"
            elif mode == "wrong-asset":
                hello["assetSha256"] = "e" * 64
            elif mode == "wrong-tree":
                hello["treeManifestSha256"] = "e" * 64
            elif mode == "wrong-root":
                hello["artifactRoot"] = state_root
            elif mode == "wrong-tree-path":
                hello["treeManifest"] = artifact_root
            elif mode == "unknown-field":
                response(request_id, hello, extra={"unexpected": True})
                continue
            elif mode == "wrong-id":
                response(request_id, hello, response_id="m3-wrong")
                continue
            elif mode == "out-of-order-id":
                response(request_id, hello, response_id="m3-2")
                continue
            elif mode == "duplicate-id":
                response(request_id, hello)
                response(request_id, hello)
                continue
            response(request_id, hello)
        elif command == "launch":
            if mode == "profile-in-use":
                error_response(request_id, "profile_in_use", "profile is already owned")
                continue
            if mode == "profile-quarantined":
                error_response(request_id, "profile_quarantined", "profile is quarantined")
                continue
            if params.get("artifactId") != artifact_id:
                raise SystemExit("unexpected fake artifact ID")
            if params.get("profileId") != profile_id:
                raise SystemExit("unexpected fake profile ID")
            if params.get("expectedArtifactFileSha256") != artifact_sha:
                raise SystemExit("unexpected fake artifact SHA")
            state = "running"
            launch = launch_result()
            if mode == "launch-artifact-mismatch":
                launch["artifactId"] = "identity-wrong"
            elif mode == "launch-sha-mismatch":
                launch["artifactFileSha256"] = "e" * 64
            elif mode == "launch-profile-mismatch":
                launch["profileId"] = "silo-wrong"
            elif mode == "launch-unknown-field":
                response(request_id, launch, extra={"unexpected": True})
                continue
            response(request_id, launch)
        elif command == "status":
            status = status_result()
            if mode == "quarantined":
                status["quarantine"] = {"reason": "fake quarantine"}
            elif mode == "status-failure":
                status["failure"] = "fake failure"
            response(request_id, status)
            if mode == "active-session-eof":
                return 0
            if mode == "active-session-crash":
                os._exit(17)
        elif command == "close":
            if mode == "desktop-close-eof":
                return 0
            state = "exited"
            close = {
                "sessionId": session_id,
                "state": state,
                "exitStatus": 0,
                "exitFileObserved": True,
                "processTreeExit": {"exited": True, "managedIdentities": []},
                "contextClose": {
                    "page": {"status": "not_present"},
                    "ctx": {"status": "success"},
                },
                "closeOutcome": {
                    "status": "success",
                    "contextClose": {
                        "page": {"status": "not_present"},
                        "ctx": {"status": "success"},
                    },
                    "gracefulProcessExit": {"status": "success"},
                    "forcedJobCleanup": {"status": "not_needed"},
                    "sqliteEvidence": {"status": "unavailable"},
                },
                "quarantine": None,
            }
            if mode == "quarantined":
                close["state"] = "quarantined"
                close["quarantine"] = {"reason": "fake quarantine"}
                close["closeOutcome"]["status"] = "failed"
                close["closeOutcome"]["gracefulProcessExit"]["status"] = "failed"
                close["closeOutcome"]["forcedJobCleanup"]["status"] = "failed"
            elif mode == "tree-exit-false":
                close["processTreeExit"] = {"exited": False, "managedIdentities": [1234]}
                close["closeOutcome"]["status"] = "failed"
                close["closeOutcome"]["gracefulProcessExit"]["status"] = "failed"
                close["closeOutcome"]["forcedJobCleanup"]["status"] = "failed"
            response(request_id, close)
        elif command == "shutdown":
            response(
                request_id,
                {
                    "state": "shutdown",
                    "sessionsClosed": 1,
                    "selfCheck": {
                        "argvMatches": [],
                        "stderrLogMatches": [],
                        "patternsChecked": 16,
                    },
                },
            )
            return 0
        else:
            raise SystemExit(f"unexpected fake Host command: {command}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
