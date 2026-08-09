#!/usr/bin/env python3
"""Deterministic M3-0 Camoufox Host transport fixture.

This process implements only the bounded JSONL Host contract. It never imports
Camoufox, opens a browser, touches an Artifact, or starts a managed process
tree. It exists solely for desktop transport/lifecycle tests.
"""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

PROTOCOL = "verisilo-camoufox-host/v1"
HOST_VERSION = "0.1.0"
MAX_FRAME_BYTES = 32768
ASSET_SHA256 = "b" * 64
BROWSER_RELEASE = "152.0.4-beta.28"
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


def response(request_id: str | None, result: dict) -> None:
    payload = json.dumps(
        {"id": request_id, "ok": True, "result": result},
        separators=(",", ":"),
    ).encode()
    if len(payload) > MAX_FRAME_BYTES:
        raise SystemExit("fake Host response exceeded frame bound")
    sys.stdout.buffer.write(payload + b"\n")
    sys.stdout.buffer.flush()


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
    profile_id = "silo-22222222222222222222222222222222"
    artifact_sha = "a" * 64
    state = "idle"

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
            response(
                request_id,
                {
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
                    "state": state,
                    "verified": False,
                    "evidenceClass": "observed-on-this-host",
                },
            )
        elif command == "launch":
            if params.get("artifactId") != artifact_id:
                raise SystemExit("unexpected fake artifact ID")
            if params.get("profileId") != profile_id:
                raise SystemExit("unexpected fake profile ID")
            if params.get("expectedArtifactFileSha256") != artifact_sha:
                raise SystemExit("unexpected fake artifact SHA")
            state = "running"
            response(
                request_id,
                {
                    "sessionId": session_id,
                    "state": state,
                    "artifactId": artifact_id,
                    "profileId": profile_id,
                    "artifactFileSha256": artifact_sha,
                    "configuredIdentityDigest": "c" * 64,
                    "observedWebsiteDigest": "d" * 64,
                    "verified": False,
                    "evidenceClass": "observed-on-this-host",
                },
            )
        elif command == "status":
            response(
                request_id,
                {
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
                },
            )
        elif command == "close":
            state = "exited"
            response(
                request_id,
                {
                    "sessionId": session_id,
                    "state": state,
                    "exitStatus": 0,
                    "exitFileObserved": True,
                    "processTreeExit": {"exited": True, "managedIdentities": []},
                    "quarantine": None,
                },
            )
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
