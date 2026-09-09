#!/usr/bin/env python3
"""Small protocol check for agent page actions; no browser launch required."""

import asyncio
import json
import os
import sys
import tempfile
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


if __name__ == "__main__":
    asyncio.run(main())
    asyncio.run(check_frame_correlation())
    print("page command check passed")
