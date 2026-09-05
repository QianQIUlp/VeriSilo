#!/usr/bin/env python3
"""Small protocol check for agent page actions; no browser launch required."""

import asyncio
import os
import sys
import tempfile
from pathlib import Path

from host_v1 import (
    CamoufoxHost,
    ProtocolError,
    _process_path,
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


if __name__ == "__main__":
    asyncio.run(main())
    print("page command check passed")
