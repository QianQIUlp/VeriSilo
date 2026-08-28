#!/usr/bin/env python3
"""Focused checks for the FP3-1b staged browser observation."""

from __future__ import annotations

import asyncio
import json
import tempfile
import unittest
from pathlib import Path
from typing import Any

import run_fp3_1b_windows as fp3


class FakePage:
    def __init__(self, value: dict[str, Any] | None = None, *, never: bool = False):
        self.value = value
        self.never = never
        self.closed = False

    async def goto(self, *_args: Any, **_kwargs: Any) -> None:
        return None

    async def evaluate(self, *_args: Any, **_kwargs: Any) -> dict[str, Any]:
        if self.never:
            await asyncio.Event().wait()
        assert self.value is not None
        return self.value

    async def close(self) -> None:
        self.closed = True


class FakeContext:
    def __init__(self, pages: list[FakePage]):
        self.pages = pages
        self.created: list[FakePage] = []

    async def new_page(self) -> FakePage:
        page = self.pages.pop(0)
        self.created.append(page)
        return page


class FP3StagedObservationTests(unittest.TestCase):
    def test_timeout_isolated_and_later_stages_are_preserved(self) -> None:
        pages = [
            FakePage(
                {
                    "success": True,
                    "ip": "23.128.188.29",
                    "countryCode": "SG",
                }
            ),
            FakePage(never=True),
            FakePage(
                {
                    "completed": True,
                    "timedOut": False,
                    "candidateCount": 1,
                    "candidates": [{"address": "23.128.188.29"}],
                }
            ),
        ]
        context = FakeContext(pages)
        payload = {
            "observedFull": {
                "language": "en-US",
                "languages": ["en-US", "en"],
                "session": {"timezone": "Asia/Singapore"},
            }
        }
        with tempfile.TemporaryDirectory() as directory:
            observed_path = Path(directory) / "observed.json"
            observation = asyncio.run(
                fp3.collect_network_observation(
                    context,
                    "http://127.0.0.1:49152/probe.html",
                    observed_path,
                    payload,
                    {"status": "granted", "origin": "http://127.0.0.1:49152"},
                    fp3.STUN_URL,
                    setup_timeout=0.05,
                    evaluation_timeouts={
                        "publicExit": 0.01,
                        "geolocation": 0.05,
                        "ice": 0.05,
                    },
                    close_timeout=0.05,
                )
            )
            persisted = json.loads(observed_path.read_text(encoding="utf-8"))

        self.assertTrue(observation["publicExit"]["success"])
        self.assertIsNone(observation["geolocation"])
        self.assertEqual(observation["stages"]["geolocation"]["status"], "failed")
        self.assertEqual(
            observation["stages"]["geolocation"]["errorType"], "TimeoutError"
        )
        self.assertEqual(
            observation["stages"]["geolocation"]["pageClose"]["status"],
            "success",
        )
        self.assertTrue(observation["ice"]["completed"])
        self.assertEqual(len(context.created), 3)
        self.assertTrue(all(page.closed for page in context.created))
        self.assertEqual(persisted["fp3NetworkObservation"], observation)


if __name__ == "__main__":
    unittest.main()
