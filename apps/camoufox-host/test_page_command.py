#!/usr/bin/env python3
"""Small protocol check for agent page actions; no browser launch required."""

import asyncio
import hashlib
import json
import os
import shutil
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from unittest import mock

import host_v1
from host_v1 import (
    CamoufoxHost,
    ProtocolError,
    _process_path,
    handle_frame,
    validate_page_params,
    verify_browser_public_address,
)
from identity_policy import IDENTITY_EVIDENCE_SCHEMA, reconcile_website_identity

REPO_ROOT = Path(__file__).resolve().parents[2]


class FakeLocator:
    def __init__(self, page):
        self.page = page

    async def aria_snapshot(self, timeout):
        return "- document Fake"

    async def inner_text(self, timeout):
        return self.page.body_text

    async def click(self, timeout):
        self.page.called = "click"

    async def fill(self, value, timeout):
        self.page.called = ("fill", value)

    async def press(self, key, timeout):
        self.page.called = ("press", key)


class FakePage:
    url = "about:blank"
    called = None
    body_text = "Fake body"

    def is_closed(self):
        return False

    def locator(self, selector):
        self.selector = selector
        return FakeLocator(self)

    async def title(self):
        return "Fake"

    async def goto(self, url, wait_until, timeout):
        self.url = url

    async def evaluate(self, script):
        return {"script": script}

    async def screenshot(self, path, full_page):
        Path(path).write_bytes(b"png")


async def main():
    if os.name == "nt":
        assert _process_path(Path(r"\\?\C:\VeriSilo\camoufox.exe")) == (
            r"C:\VeriSilo\camoufox.exe"
        )
        assert _process_path(Path(r"\\?\UNC\server\share\camoufox.exe")) == (
            r"\\server\share\camoufox.exe"
        )

    try:
        validate_page_params({"sessionId": "s", "action": "goto", "url": "file:///x"})
    except ProtocolError as error:
        assert error.code == "bad_url"
    else:
        raise AssertionError("non-HTTP URL must be rejected")

    exit_page = FakePage()
    exit_page.body_text = '{"ip":"1.1.1.1"}'
    assert await verify_browser_public_address(
        exit_page, {"networkIdentity": {"expectedPublicAddress": "1.1.1.1"}}
    ) == "1.1.1.1"
    try:
        await verify_browser_public_address(
            exit_page, {"networkIdentity": {"expectedPublicAddress": "8.8.8.8"}}
        )
    except ProtocolError as error:
        assert error.code == "network_exit_mismatch"
    else:
        raise AssertionError("a mismatched browser exit must fail closed")
    try:
        await verify_browser_public_address(exit_page, {})
    except ProtocolError as error:
        assert error.code == "network_exit_unavailable"
    else:
        raise AssertionError("a missing bound exit must fail closed")

    with tempfile.TemporaryDirectory() as tmp:
        page = FakePage()
        host = object.__new__(CamoufoxHost)
        host.executable = Path(sys.executable)
        host.session = {
            "sessionId": "s",
            "state": "running",
            "page": page,
            "sessionDir": Path(tmp),
        }
        snapshot = await host.page_action(
            {"sessionId": "s", "action": "goto", "url": "https://example.com/"}
        )
        assert snapshot == {
            "url": "https://example.com/",
            "title": "Fake",
            "aria": "- document Fake",
            "text": "Fake body",
        }
        evaluated = await host.page_action(
            {"sessionId": "s", "action": "evaluate", "script": "() => 1"}
        )
        assert evaluated["value"] == {"script": "() => 1"}
        windows = await host.page_action({"sessionId": "s", "action": "windows"})
        assert windows["available"] == (os.name == "nt")
        assert windows["page"]["script"].startswith("() =>")
        screenshot = await host.page_action({"sessionId": "s", "action": "screenshot"})
        assert Path(screenshot["path"]).read_bytes() == b"png"


async def check_frame_correlation():
    """Protocol error correlation contract at the exact handle_frame seam.

    QA-R2-01: a validation failure on a frame whose id was reliably parsed
    must echo that id so the caller attributes the real error instead of
    observing a missing-id (uncorrelated) response.
    """

    def page_host():
        host = object.__new__(CamoufoxHost)
        host.session = None
        return host

    def session_host(session_dir: Path):
        host = page_host()
        host.executable = Path(sys.executable)
        page = FakePage()
        host.session = {
            "sessionId": "s",
            "state": "running",
            "page": page,
            "sessionDir": session_dir,
        }
        return host

    async def respond(host, payload: bytes) -> dict:
        captured: list[dict] = []
        with mock.patch.object(host_v1, "_send", side_effect=captured.append):
            should_shutdown = await handle_frame(host, payload)
        assert should_shutdown is False
        assert len(captured) == 1, captured
        return captured[0]

    with tempfile.TemporaryDirectory() as tmp:
        session_dir = Path(tmp)

        # Healthy page request: response id echoes the request id, ok=true.
        response = await respond(
            session_host(session_dir),
            json.dumps(
                {
                    "id": "m3-4",
                    "command": "page",
                    "params": {"sessionId": "s", "action": "snapshot"},
                }
            ).encode("utf-8"),
        )
        assert response["id"] == "m3-4" and response["ok"] is True, response
        assert set(response["result"]) == {"url", "title", "aria", "text"}, response

        response = await respond(
            session_host(session_dir),
            json.dumps(
                {
                    "id": "m3-5",
                    "command": "page",
                    "params": {"sessionId": "s", "action": "windows"},
                }
            ).encode("utf-8"),
        )
        assert response["id"] == "m3-5" and response["ok"] is True, response

    # Validation error after the id was reliably parsed: echo the id and the
    # exact validation error; never return a missing-id response.
    response = await respond(
        page_host(),
        json.dumps(
            {
                "id": "m3-6",
                "command": "page",
                "params": {"sessionId": "s", "action": "bogus"},
            }
        ).encode("utf-8"),
    )
    assert response["id"] == "m3-6" and response["ok"] is False, response
    assert response["error"]["code"] == "bad_type", response
    assert "page action is unsupported" in response["error"]["message"], response

    # Unknown command with a valid id (the exact wire shape that QA-R2-01
    # masked as a missing-id response on the stale packaged Host binary).
    response = await respond(
        page_host(),
        json.dumps({"id": "m3-7", "command": "pages", "params": {}}).encode("utf-8"),
    )
    assert response["id"] == "m3-7" and response["ok"] is False, response
    assert response["error"]["code"] == "unknown_command", response

    # Unknown param with a valid id must stay correlated too.
    response = await respond(
        page_host(),
        json.dumps(
            {
                "id": "m3-8",
                "command": "page",
                "params": {"sessionId": "s", "action": "snapshot", "extra": 1},
            }
        ).encode("utf-8"),
    )
    assert response["id"] == "m3-8" and response["ok"] is False, response
    assert response["error"]["code"] == "unknown_field", response

    # Frames that carry no trustworthy id keep the fail-closed null id.
    for payload, code in [
        (b"not json", "invalid_json"),
        (b"[1,2]", "frame_not_object"),
        (b'{"id":"m3-9","id":"m3-9","command":"hello","params":{}}', "duplicate_key"),
        (json.dumps({"id": 5, "command": "hello", "params": {}}).encode("utf-8"), "bad_type"),
    ]:
        response = await respond(page_host(), payload)
        assert response["id"] is None and response["ok"] is False, (payload, response)
        assert response["error"]["code"] == code, (payload, response)


LAUNCH_OBSERVED_AT = "2020-01-01T00:00:00Z"
MEDIA_SUCCESS_RESPONSE = {
    "reason": "success",
    "attempts": [[{"kind": "audioinput"}, {"kind": "videoinput"}]],
}


class ReobserveUserPage:
    """The page the user is actually browsing; it must never be touched."""

    def __init__(self):
        self.url = "https://www.google.com/"
        self.closed = False

    def is_closed(self):
        return self.closed


class ReobserveProbePage:
    """The temporary observation page the re-observation creates and closes."""

    def __init__(self, observed: dict):
        self.url = "about:blank"
        self.closed = False
        self.observed = observed
        self.context: ReobserveContext | None = None
        self.goto_calls: list[str] = []
        self.evaluate_scripts: list[str] = []
        self.fail_read = False
        self.fail_goto = False

    def is_closed(self):
        return self.closed

    async def goto(self, url, wait_until=None, timeout=None):
        if self.fail_goto:
            raise RuntimeError("goto failed")
        self.goto_calls.append(url)
        self.url = url

    async def evaluate(self, script, arg=None):
        if self.fail_read and script == "window.__probe.readIdentity()":
            raise RuntimeError("identity probe failed")
        self.evaluate_scripts.append(script)
        if script == "window.__probe.readIdentity()":
            return self.observed
        if "mediaDevices" in script:
            return MEDIA_SUCCESS_RESPONSE
        if script == "window.__probe.readWebGL()":
            return {"webglAvailable": True}
        return None

    async def close(self):
        self.closed = True
        if self.context is not None:
            _remove_closed(self)


class ReobserveContext:
    def __init__(self, user_page: ReobserveUserPage):
        self.pages: list = [user_page]
        self.created: list[ReobserveProbePage] = []

    def _attach(self, page: "ReobserveProbePage") -> "ReobserveProbePage":
        page.context = self
        return page

    async def new_page(self):
        raise AssertionError("reobserve must call new_page on the context")


def _remove_closed(page: "ReobserveProbePage") -> None:
    context = page.context
    context.pages = [entry for entry in context.pages if entry is not page]


def observed_from_artifact(artifact: dict) -> dict:
    """A full observed payload aligned with the Artifact's declared values."""
    config = artifact["resolvedConfig"]
    declared = artifact["stableSignalsDeclared"]
    return {
        "userAgent": declared["userAgent"],
        "language": declared["language"],
        "languages": [declared["language"]],
        "platform": config["navigator.platform"],
        "oscpu": config["navigator.oscpu"],
        "doNotTrack": config.get("navigator.doNotTrack"),
        "globalPrivacyControl": config.get("navigator.globalPrivacyControl"),
        "screen": declared["screen"],
        "devicePixelRatio": declared["devicePixelRatio"],
        "hardwareConcurrency": declared["hardwareConcurrency"],
        "historyLength": config["window.history.length"],
        "mediaDevices": [{"kind": "audioinput"}, {"kind": "videoinput"}],
        "session": {"timezone": config["timezone"], "utcOffsetMinutes": 0},
        "fontNegativeControls": {},
        "webglVendor": declared["webglVendor"],
        "webglRenderer": declared["webglRenderer"],
        "webglSummary": declared["webglVendor"],
        "voices": [
            {
                "name": voice["name"],
                "lang": voice["lang"],
                "voiceURI": voice.get("voiceURI", voice.get("voiceUri")),
                "localService": voice.get(
                    "localService", voice.get("isLocalService")
                ),
                "default": voice.get("isDefault", voice.get("default", False)),
            }
            for voice in declared["voices"]
        ],
        "audioHash": "hash",
    }


class FakeProbeServer:
    def __init__(self, port: int):
        self.server_address = ("127.0.0.1", port)


def reobserve_host(roots: Path, observed: dict, context: ReobserveContext):
    fixture = REPO_ROOT / "tests" / "fixtures" / "camoufox"
    artifact_root = roots / "artifacts"
    artifact_root.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(fixture / "identity-a.json", artifact_root / "identity-a.json")
    shutil.copyfile(
        fixture / "identity-a.json.sha256", artifact_root / "identity-a.json.sha256"
    )
    raw = (artifact_root / "identity-a.json").read_bytes()
    artifact = json.loads(raw)
    file_sha = hashlib.sha256(raw).hexdigest()
    launch_evidence = {
        "schema": IDENTITY_EVIDENCE_SCHEMA,
        "observedAt": LAUNCH_OBSERVED_AT,
        "state": "matched",
        "signals": [],
    }
    host = object.__new__(CamoufoxHost)
    host.artifact_root = artifact_root
    host.session = {
        "sessionId": "sess",
        "state": "running",
        "artifactId": "identity-a",
        "profileId": "prof",
        "artifactFileSha256": file_sha,
        "configuredIdentityDigest": "sha256:" + "0" * 64,
        "browserProxyServer": None,
        "fontMode": "inherit",
        "ctx": context,
        "page": context.pages[0],
        "sessionDir": roots / "state",
        "server": FakeProbeServer(47311),
        "identityEvidence": launch_evidence,
        "observedSignals": None,
        "observedWebsiteDigest": "sha256:" + "1" * 64,
    }
    context.new_page = _new_page_factory(observed, context)  # type: ignore[method-assign]
    return host, artifact, file_sha


def _new_page_factory(observed: dict, context: ReobserveContext):
    async def new_page():
        page = context._attach(ReobserveProbePage(observed))
        context.pages.append(page)
        context.created.append(page)
        return page

    return new_page


def _parse_rfc3339(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


async def check_reobserve_identity():
    """Fresh identity re-observation contract at the CamoufoxHost seam."""
    fixture = REPO_ROOT / "tests" / "fixtures" / "camoufox"
    observed = observed_from_artifact(
        json.loads((fixture / "identity-a.json").read_text(encoding="utf-8"))
    )

    with tempfile.TemporaryDirectory() as tmp:
        roots = Path(tmp)
        (roots / "state").mkdir()
        user_page = ReobserveUserPage()
        context = ReobserveContext(user_page)
        host, artifact, _file_sha = reobserve_host(roots, observed, context)

        with mock.patch.object(host_v1, "interactive_desktop_launch", return_value=False):
            result = await host.reobserve_identity("sess")

        # Fresh evidence: new observation bound to the same session/Artifact.
        evidence = result["identityEvidence"]
        assert evidence["schema"] == IDENTITY_EVIDENCE_SCHEMA, evidence
        assert evidence["state"] == "matched", evidence
        assert evidence["observedAt"] == result["reobservedAt"]
        assert _parse_rfc3339(evidence["observedAt"]) > _parse_rfc3339(
            LAUNCH_OBSERVED_AT
        ), "the re-observation must carry a new observedAt"
        assert result["sessionId"] == "sess"
        assert result["artifactId"] == "identity-a"
        assert result["state"] == "running"

        # The session now carries the fresh evidence, not the launch evidence.
        assert host.session["identityEvidence"] is evidence
        assert host.session["observedWebsiteDigest"].startswith("sha256:")

        # Reconciled against the exact Artifact bound at launch.
        expected_evidence = reconcile_website_identity(
            artifact,
            host.session["observedSignals"],
            evidence["observedAt"],
        )
        assert evidence == expected_evidence

        # The temporary probe page was created, navigated to the Host's own
        # loopback probe URL, and closed; the user's page was never touched.
        assert len(context.created) == 1, context.created
        probe_page = context.created[0]
        assert probe_page.closed is True
        assert probe_page.url == "http://127.0.0.1:47311/probe.html", probe_page.url
        assert context.pages == [user_page], context.pages
        assert user_page.url == "https://www.google.com/", user_page.url
        assert host.session["page"] is user_page
        assert user_page.closed is False

        # reobserved.json records the fresh observation next to the launch
        # observed.json without overwriting it.
        assert not (roots / "state" / "observed.json").exists()
        reobserved = json.loads(
            (roots / "state" / "reobserved.json").read_text(encoding="utf-8")
        )
        assert reobserved["generatedAtUtc"] == evidence["observedAt"]
        assert reobserved["identityEvidence"] == evidence
        assert reobserved["reobserved"] is True

    with tempfile.TemporaryDirectory() as tmp:
        roots = Path(tmp)
        (roots / "state").mkdir()
        context = ReobserveContext(ReobserveUserPage())
        host, _artifact, _file_sha = reobserve_host(roots, observed, context)

        # No active session / wrong session / not running: explicit rejection.
        with mock.patch.object(host_v1, "interactive_desktop_launch", return_value=False):
            try:
                await host.reobserve_identity("other")
            except ProtocolError as error:
                assert error.code == "session_not_found", error.code
            else:
                raise AssertionError("wrong session must be rejected")
            host.session["state"] = "starting"
            try:
                await host.reobserve_identity("sess")
            except ProtocolError as error:
                assert error.code == "session_not_running", error.code
            else:
                raise AssertionError("a non-running session must be rejected")
            host.session["state"] = "running"
            host.session["ctx"] = None
            try:
                await host.reobserve_identity("sess")
            except ProtocolError as error:
                assert error.code == "session_not_running", error.code
            else:
                raise AssertionError("a session without a context must be rejected")
            host.session["ctx"] = context

            # A mutated Artifact file fails closed; no fresh evidence is
            # fabricated and the previous evidence stays bound.
            (roots / "artifacts" / "identity-a.json").write_text("{}\n", encoding="utf-8")
            previous_evidence = host.session["identityEvidence"]
            try:
                await host.reobserve_identity("sess")
            except ProtocolError as error:
                assert error.code == "artifact_integrity", error.code
            else:
                raise AssertionError("a mutated artifact must fail closed")
            assert host.session["identityEvidence"] is previous_evidence
            assert context.created == [], "no probe page may be created"

            # An observation failure leaves the previous evidence in place and
            # still closes the temporary probe page.
            user_page = ReobserveUserPage()
            context = ReobserveContext(user_page)
            host2, _a, _s = reobserve_host(roots, observed, context)
            host2.session["identityEvidence"] = previous_evidence

            async def failing_new_page():
                page = ReobserveProbePage(observed)
                page.context = context
                page.fail_read = True
                context.pages.append(page)
                context.created.append(page)
                return page

            context.new_page = failing_new_page  # type: ignore[method-assign]
            with mock.patch.object(
                host_v1, "interactive_desktop_launch", return_value=False
            ):
                try:
                    await host2.reobserve_identity("sess")
                except RuntimeError:
                    pass
                else:
                    raise AssertionError("a failing identity probe must raise")
            assert host2.session["identityEvidence"] is previous_evidence
            assert host2.session["observedSignals"] is None
            assert context.pages == [user_page], "probe page must still be closed"


async def check_reobserve_frame_validation():
    """Protocol-level validation for the reobserve_identity command."""

    async def respond(host, payload: bytes) -> dict:
        captured: list[dict] = []
        with mock.patch.object(host_v1, "_send", side_effect=captured.append):
            should_shutdown = await handle_frame(host, payload)
        assert should_shutdown is False
        assert len(captured) == 1, captured
        return captured[0]

    idle_host = object.__new__(CamoufoxHost)
    idle_host.session = None
    response = await respond(
        idle_host,
        json.dumps(
            {"id": "r1", "command": "reobserve_identity", "params": {}}
        ).encode("utf-8"),
    )
    assert response["id"] == "r1" and response["ok"] is False, response
    assert response["error"]["code"] == "bad_type", response

    response = await respond(
        idle_host,
        json.dumps(
            {
                "id": "r2",
                "command": "reobserve_identity",
                "params": {"sessionId": "missing", "extra": 1},
            }
        ).encode("utf-8"),
    )
    assert response["error"]["code"] == "unknown_field", response

    response = await respond(
        idle_host,
        json.dumps(
            {
                "id": "r3",
                "command": "reobserve_identity",
                "params": {"sessionId": "missing"},
            }
        ).encode("utf-8"),
    )
    assert response["id"] == "r3" and response["ok"] is False, response
    assert response["error"]["code"] == "session_not_found", response


if __name__ == "__main__":
    asyncio.run(main())
    asyncio.run(check_frame_correlation())
    asyncio.run(check_reobserve_identity())
    asyncio.run(check_reobserve_frame_validation())
    print("page command check passed")
