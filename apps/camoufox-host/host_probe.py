#!/usr/bin/env python3
"""Bounded browser-observation helpers used by the packaged Host.

This is intentionally separate from the historical evidence runner.  It
contains only the probe projection and the small media readiness RPC needed
for one Host launch.
"""

from __future__ import annotations

import asyncio
import contextlib
import math
import re
import time
from collections.abc import Awaitable, Callable
from typing import Any

from host_fonts import FONT_UNIVERSE, host_negative_control_families

MEDIA_READINESS_REASONS = frozenset(
    {
        "success",
        "enumerate_timeout",
        "readiness_timeout",
        "count_mismatch",
        "playwright_exception",
        "unavailable",
    }
)


def extract_observed_website_signals(observed: dict, font_mode: str = "inherit") -> dict:
    voices = [
        {key: voice.get(key) for key in ("name", "lang", "localService", "voiceURI")}
        for voice in observed["voices"]
    ]
    signals = {
        "userAgent": observed["userAgent"],
        "language": observed["language"],
        "languages": observed["languages"],
        "platform": observed["platform"],
        "oscpu": observed["oscpu"],
        "doNotTrack": observed["doNotTrack"],
        "globalPrivacyControl": observed["globalPrivacyControl"],
        "screen": observed["screen"],
        "devicePixelRatio": observed["devicePixelRatio"],
        "hardwareConcurrency": observed["hardwareConcurrency"],
        "historyLength": observed["historyLength"],
        "mediaDevices": observed["mediaDevices"],
        "timezone": observed["session"]["timezone"],
        "utcOffsetMinutes": observed["session"]["utcOffsetMinutes"],
        "fontNegativeControls": observed["fontNegativeControls"],
        "webglVendor": observed["webglVendor"],
        "webglRenderer": observed["webglRenderer"],
        "webglSummary": observed["webglSummary"],
        "voices": voices,
        "audioHash": observed["audioHash"],
    }
    if font_mode == "managed":
        signals["fontUniverseWidths"] = observed["fontUniverseWidths"]
    return signals


def expected_media_device_counts(config: dict) -> dict[str, int]:
    if config.get("mediaDevices:enabled") is not True:
        return {"audioinput": 0, "videoinput": 0, "audiooutput": 0}
    return {
        "audioinput": int(config.get("mediaDevices:micros", 0)),
        "videoinput": int(config.get("mediaDevices:webcams", 0)),
        "audiooutput": int(config.get("mediaDevices:speakers", 0)),
    }

def observed_media_device_counts(devices: list[dict]) -> dict[str, int]:
    counts = {"audioinput": 0, "videoinput": 0, "audiooutput": 0}
    for device in devices:
        if device.get("kind") in counts:
            counts[device["kind"]] += 1
    return counts


class MediaDeviceReadinessTimeout(TimeoutError):
    def __init__(self, reason: str):
        if reason not in {"enumerate_timeout", "readiness_timeout"}:
            raise ValueError("invalid media timeout reason")
        self.reason = reason
        super().__init__(f"media readiness failed: {reason}")


class MediaDeviceReadinessError(RuntimeError):
    def __init__(self, exception_class: str):
        self.reason = "playwright_exception"
        self.exception_class = re.sub(r"[^A-Za-z0-9_.-]", "_", exception_class)[:64]
        super().__init__("media readiness failed: playwright_exception")


async def _bounded_media_rpc(
    awaitable: Any,
    timeout_seconds: float,
    timeout_reason: str,
    cancel_settle_seconds: float,
) -> Any:
    def consume_result(done_task: asyncio.Task[Any]) -> None:
        with contextlib.suppress(asyncio.CancelledError, Exception):
            done_task.result()

    task = asyncio.create_task(awaitable)
    try:
        done, _ = await asyncio.wait({task}, timeout=max(0.001, timeout_seconds))
    except asyncio.CancelledError:
        task.cancel()
        task.add_done_callback(consume_result)
        raise
    if task not in done:
        task.cancel()
        done, _ = await asyncio.wait({task}, timeout=max(0.0, cancel_settle_seconds))
        if task not in done:
            task.add_done_callback(consume_result)
        raise MediaDeviceReadinessTimeout(timeout_reason)
    try:
        return task.result()
    except asyncio.CancelledError:
        raise
    except Exception as exc:
        raise MediaDeviceReadinessError(type(exc).__name__) from exc


_MEDIA_ENUMERATE_SCRIPT = """async ({timeoutMs}) => {
    if (!navigator.mediaDevices?.enumerateDevices) return {reason: "unavailable", attempts: []};
    let enumeration;
    try { enumeration = navigator.mediaDevices.enumerateDevices(); }
    catch (_) { return {reason: "unavailable", attempts: []}; }
    return await new Promise((resolve) => {
        let settled = false;
        const finish = (value) => { if (settled) return; settled = true; clearTimeout(timer); resolve(value); };
        const timer = setTimeout(() => finish({reason: "enumerate_timeout", attempts: []}), timeoutMs);
        Promise.resolve(enumeration).then((devices) => {
            try { finish({reason: "success", attempts: [devices.map((device) => ({kind: device.kind}))]}); }
            catch (_) { finish({reason: "unavailable", attempts: []}); }
        }, () => finish({reason: "unavailable", attempts: []}));
    });
}"""


def _parse_attempts(value: Any) -> list[dict]:
    if not isinstance(value, list):
        raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
    attempts = []
    for devices in value:
        if not isinstance(devices, list):
            raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
        normalized = []
        for device in devices:
            if not isinstance(device, dict) or set(device) != {"kind"} or not isinstance(device["kind"], str):
                raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
            normalized.append({"kind": device["kind"]})
        attempts.append({"counts": observed_media_device_counts(normalized), "matched": False})
    return attempts


async def wait_for_configured_media_devices(
    page: Any,
    config: dict,
    timeout_seconds: float = 8.0,
    *,
    clock: Callable[[], float] = time.monotonic,
    poll_interval_ms: int = 250,
    readiness_wait: Callable[[float], Awaitable[Any]] = asyncio.sleep,
) -> dict:
    expected = expected_media_device_counts(config)
    if timeout_seconds <= 0 or not isinstance(poll_interval_ms, int) or poll_interval_ms <= 0:
        raise ValueError("media readiness budget is invalid")
    started = clock()
    deadline = started + timeout_seconds
    margin = poll_interval_ms / 1000
    max_attempts = math.ceil(timeout_seconds / margin) + 1
    attempts: list[dict] = []
    while len(attempts) < max_attempts:
        remaining = deadline - clock()
        if remaining <= margin * 2:
            break
        channel_timeout = remaining - margin
        value = await _bounded_media_rpc(
            page.evaluate(_MEDIA_ENUMERATE_SCRIPT, {"timeoutMs": max(1, int((channel_timeout - margin) * 1000))}),
            channel_timeout,
            "enumerate_timeout",
            margin,
        )
        if not isinstance(value, dict) or set(value) != {"reason", "attempts"}:
            raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
        reason = value["reason"]
        if reason not in MEDIA_READINESS_REASONS - {"readiness_timeout", "playwright_exception"}:
            raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
        current = _parse_attempts(value["attempts"])
        if len(current) != (1 if reason == "success" else 0):
            raise MediaDeviceReadinessError("InvalidMediaDeviceResponse")
        attempts.extend(current)
        if reason != "success":
            return _readiness_result(reason, expected, attempts, started, clock)
        if attempts[-1]["counts"] == expected:
            return _readiness_result("success", expected, attempts, started, clock)
        remaining = deadline - clock()
        if remaining <= margin * 2:
            break
        await _bounded_media_rpc(
            readiness_wait(min(margin, remaining - margin)),
            min(remaining - margin, margin * 2),
            "readiness_timeout",
            margin,
        )
    return _readiness_result("count_mismatch", expected, attempts, started, clock)


def _readiness_result(
    reason: str,
    expected: dict[str, int],
    attempts: list[dict],
    started: float,
    clock: Callable[[], float],
) -> dict:
    for attempt in attempts:
        attempt["matched"] = attempt["counts"] == expected
    return {
        "expectedCounts": expected,
        "attempts": attempts,
        "matched": reason == "success" and bool(attempts) and attempts[-1]["matched"],
        "waitSeconds": round(max(0.0, clock() - started), 3),
        "reason": reason,
    }
