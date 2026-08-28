#!/usr/bin/env python3
"""FP4 native-Windows ordinary-site compatibility discriminator."""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import math
import os
import platform
import queue
import re
import shutil
import subprocess
import sys
import tempfile
import time
import traceback
import uuid
from datetime import datetime, timezone
from pathlib import Path
from threading import Thread
from typing import Any, Awaitable, Callable
from urllib.parse import parse_qs, urlsplit

REPO_ROOT = Path(__file__).resolve().parents[2]
HOST_DIR = Path(__file__).resolve().parent
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

import host_v1
import run_fp3_1b_windows as fp3
from host_platform import process_identity_alive

BRANCH = "codex/camoufox-m3-engine-adapter"
MATRIX_VERSION = "fp4-ordinary-sites-v1"
CONTRACT = REPO_ROOT / "docs/camoufox-fp4-ordinary-site-compatibility-contract.md"
CONTRACT_SHA256 = "5bd350d643b5453a397fdf9c35e65b9ef512c921ef9db3ee78a616c0f07c3826"
FP3_RESULT = HOST_DIR / "lock" / (
    "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp3-result.json"
)
FP3_RESULT_SHA256 = "8a821eca7b9e11716668d6742ac356743b7438ab2b9a7ca8b0d604264be86e62"
SOURCE_ARTIFACT = REPO_ROOT / (
    "artifacts/camoufox-fp3-1b-attempt-7/"
    "identity-fp3-1b-formal-v3-a.json"
)
SOURCE_ARTIFACT_SHA256 = "8a4cd0d10a0a456678d1f3b4beb1515195d5d171742c4695c2d909132a26e722"
SOURCE_ARTIFACT_SIDECAR = SOURCE_ARTIFACT.with_suffix(".json.sha256")
SOURCE_ARTIFACT_SIDECAR_SHA256 = "e027eb101fa2783adbc697fa8b47a339e7d66bf00170eacdca7b71a8983f8b86"
ARTIFACT_ID = "identity-fp3-1b-formal-v3-a"
PROXY_URI = "socks5://127.0.0.1:7897"
SCREENSHOT_ROOT_ENV = "VERISILO_FP4_SCREENSHOT_ROOT"

DOCUMENT_URL = (
    "https://en.wikipedia.org/w/index.php?"
    "search=camouflage+animals+military&title=Special%3ASearch&ns0=1"
)
COMPLEX_JS_URL = "https://github.com/microsoft/playwright/issues"
GRAPHICS_URL = "https://www.openstreetmap.org/#map=12/51.5074/-0.1278"
MEDIA_URL = (
    "https://commons.wikimedia.org/wiki/"
    "File:Big_Buck_Bunny_keyframe_strobing_example.webm"
)
STATE_URL = "https://en.wikipedia.org/wiki/Web_browser"
SELECTED_URLS = {
    "documentNavigation": DOCUMENT_URL,
    "complexJavaScript": COMPLEX_JS_URL,
    "interactiveGraphics": GRAPHICS_URL,
    "audioVideo": MEDIA_URL,
    "formState": STATE_URL,
    "formStateReplay": STATE_URL,
}
NAVIGATION_TIMEOUT_MS = 30_000
PAGE_CLOSE_TIMEOUT_SECONDS = 3.0
TASK_BUDGET_SECONDS = {
    "documentNavigation": 90.0,
    "complexJavaScript": 90.0,
    "interactiveGraphics": 90.0,
    "audioVideo": 120.0,
    "formState": 90.0,
    "formStateReplay": 70.0,
}
PHASE_A_TASK_NAMES = (
    "documentNavigation",
    "complexJavaScript",
    "interactiveGraphics",
    "audioVideo",
    "formState",
)
PHASE_B_TASK_NAMES = ("formStateReplay",)
_ORIGINAL_HOST = host_v1.CamoufoxHost


class FP4Error(RuntimeError):
    pass


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="microseconds").replace(
        "+00:00", "Z"
    )


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_json(path: Path, value: object) -> str:
    raw = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    path.write_bytes(raw)
    digest = hashlib.sha256(raw).hexdigest()
    path.with_suffix(path.suffix + ".sha256").write_text(
        f"{digest}  {path.name}\n", encoding="ascii", newline="\n"
    )
    return digest


def stage_artifact(artifact_root: Path) -> None:
    shutil.copyfile(SOURCE_ARTIFACT, artifact_root / SOURCE_ARTIFACT.name)
    shutil.copyfile(
        SOURCE_ARTIFACT_SIDECAR, artifact_root / SOURCE_ARTIFACT_SIDECAR.name
    )


def response_status(response: Any) -> int | None:
    value = None if response is None else response.status
    return value if type(value) is int else None


def http_ok(value: object) -> bool:
    return type(value) is int and 200 <= value < 300


async def document_navigation(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        DOCUMENT_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    initial_heading = (await page.locator("h1").first.inner_text()).strip()
    await page.get_by_role(
        "link", name="Military camouflage", exact=True
    ).first.click()
    await page.wait_for_url(
        re.compile(r"^https://en\.wikipedia\.org/wiki/Military_camouflage(?:[#?].*)?$"),
        wait_until="domcontentloaded",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    article_url = page.url
    article_heading = (await page.locator("h1").first.inner_text()).strip()
    history = page.get_by_role("heading", name="History", exact=True).first
    await history.scroll_into_view_if_needed()
    history_visible = await history.is_visible()
    back = await page.go_back(
        wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    back_url = page.url
    back_heading = (await page.locator("h1").first.inner_text()).strip()
    forward = await page.go_forward(
        wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    return {
        "initialHttpStatus": response_status(initial),
        "initialHeading": initial_heading,
        "articleUrl": article_url,
        "articleHeading": article_heading,
        "historyVisible": history_visible,
        "backHttpStatus": response_status(back),
        "backUrl": back_url,
        "backHeading": back_heading,
        "forwardHttpStatus": response_status(forward),
        "forwardUrl": page.url,
        "forwardHeading": (await page.locator("h1").first.inner_text()).strip(),
        "title": await page.title(),
        "finalUrl": page.url,
    }


async def complex_javascript(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        COMPLEX_JS_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    await page.get_by_role(
        "button", name="Filter by labels", exact=True
    ).first.click()
    dialog = page.get_by_role("dialog").first
    await dialog.wait_for(state="visible")
    label_filter = dialog.locator('input[aria-label="Filter labels"]').first
    await label_filter.fill("browser-chromium")
    await dialog.get_by_role(
        "option", name=re.compile(r"^browser-chromium(?:\s|$)")
    ).first.click()
    expected_query = "is:issue state:open label:browser-chromium"
    await page.wait_for_function(
        """expected => {
          const input = document.querySelector('#repository-input');
          return input && input.value === expected;
        }""",
        arg=expected_query,
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    await page.wait_for_function(
        r"""() => [...document.querySelectorAll(
          'a[href^="/microsoft/playwright/issues/"]'
        )].some(a =>
          /^\/microsoft\/playwright\/issues\/\d+$/.test(
            new URL(a.href, location.href).pathname
          ) && a.getClientRects().length > 0 &&
          getComputedStyle(a).visibility !== 'hidden'
        )""",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    final_url = page.url
    issue_count = await page.locator(
        'a[href^="/microsoft/playwright/issues/"]'
    ).evaluate_all(
        r"""links => new Set(links.filter(a =>
          a.getClientRects().length > 0 &&
          getComputedStyle(a).visibility !== 'hidden'
        ).map(a => new URL(a.href, location.href).pathname)
          .filter(path => /^\/microsoft\/playwright\/issues\/\d+$/.test(path))
        ).size"""
    )
    return {
        "initialHttpStatus": response_status(initial),
        "queryInputValue": await page.locator("#repository-input").first.input_value(),
        "decodedQuery": parse_qs(urlsplit(final_url).query).get("q", [""])[0],
        "issueLinkCount": issue_count,
        "noResultsVisible": await page.get_by_text(
            "No results", exact=True
        ).is_visible(),
        "title": await page.title(),
        "finalUrl": final_url,
    }


async def completed_tile_sources(page: Any) -> list[str]:
    return await page.locator("img.leaflet-tile").evaluate_all(
        """tiles => tiles
          .filter(tile => tile.complete && tile.naturalWidth === 256)
          .map(tile => tile.currentSrc || tile.src)"""
    )


def map_zoom(fragment: str) -> int | None:
    match = re.match(r"^map=(\d+)/", fragment)
    return int(match.group(1)) if match else None


async def interactive_graphics(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        GRAPHICS_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    map_view = page.locator("#map")
    await map_view.wait_for(state="visible")
    await page.wait_for_function(
        """() => [...document.querySelectorAll('img.leaflet-tile')]
          .some(tile => tile.complete && tile.naturalWidth === 256)""",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    initial_sources = await completed_tile_sources(page)
    initial_hash = urlsplit(page.url).fragment
    place_query = "Hong Kong"
    search = page.locator("#query")
    await search.fill(place_query)
    await search.press("Enter")
    search_results = page.locator(".search_results_entry")
    await search_results.first.wait_for(state="visible")
    search_result_count = await search_results.count()
    pre_pan_hash = urlsplit(page.url).fragment
    await map_view.focus()
    await map_view.press("ArrowRight")
    await page.wait_for_function(
        "before => location.hash.slice(1) !== before",
        arg=pre_pan_hash,
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    pan_hash = urlsplit(page.url).fragment
    await page.locator(".zoom .plus-lg").first.click()
    await page.wait_for_function(
        "() => location.hash.startsWith('#map=13/')",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    zoom_hash = urlsplit(page.url).fragment
    await page.wait_for_function(
        """() => [...document.querySelectorAll('img.leaflet-tile')]
          .some(tile => tile.complete && tile.naturalWidth === 256)""",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    before_layer_sources = await completed_tile_sources(page)
    await page.locator(".control-layers > .control-button").first.click()
    cyclosm = page.locator("#map-ui-layer-cyclosm")
    await cyclosm.check()
    await page.wait_for_function(
        """before => {
          const selected = document.querySelector('#map-ui-layer-cyclosm');
          return selected && selected.checked &&
            [...document.querySelectorAll('img.leaflet-tile')].some(tile =>
              tile.complete && tile.naturalWidth === 256 &&
              !before.includes(tile.currentSrc || tile.src));
        }""",
        arg=before_layer_sources,
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    final_sources = await completed_tile_sources(page)
    return {
        "initialHttpStatus": response_status(initial),
        "initialHash": initial_hash,
        "initialCompletedTileCount": len(initial_sources),
        "placeQuery": place_query,
        "searchResultCount": search_result_count,
        "prePanHash": pre_pan_hash,
        "panHash": pan_hash,
        "zoomHash": zoom_hash,
        "cyclosmChecked": await cyclosm.is_checked(),
        "newLayerTileCount": len(set(final_sources) - set(before_layer_sources)),
        "title": await page.title(),
        "finalUrl": page.url,
    }


async def media_state(video: Any) -> dict[str, Any]:
    return await video.evaluate(
        """video => ({
          readyState: video.readyState,
          duration: Number.isFinite(video.duration) ? video.duration : null,
          currentTime: Number.isFinite(video.currentTime) ? video.currentTime : null,
          paused: video.paused
        })"""
    )


async def audio_video(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        MEDIA_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    await page.locator(
        ".mw-tmh-player:first-of-type a.mw-tmh-play"
    ).first.click()
    video = page.locator("video.vjs-tech").first
    await video.wait_for(state="attached")
    await page.wait_for_function(
        """() => {
          const video = document.querySelector('video.vjs-tech');
          return video && video.readyState >= 2 &&
            Number.isFinite(video.duration);
        }""",
        timeout=NAVIGATION_TIMEOUT_MS,
    )
    started = await media_state(video)
    await page.wait_for_function(
        """start => {
          const video = document.querySelector('video.vjs-tech');
          return video && video.currentTime >= start + 1;
        }""",
        arg=started["currentTime"],
        timeout=3_000,
    )
    progressed = await media_state(video)
    await page.get_by_role("button", name="Pause", exact=True).first.click()
    await page.wait_for_function(
        "() => document.querySelector('video.vjs-tech')?.paused === true",
        timeout=5_000,
    )
    paused = await media_state(video)
    progress = page.get_by_role("slider", name="Progress Bar", exact=True).first
    await progress.press("Home")
    await progress.press("ArrowRight")
    await page.wait_for_function(
        """() => {
          const video = document.querySelector('video.vjs-tech');
          return video && video.currentTime >= 4 && video.currentTime <= 6;
        }""",
        timeout=5_000,
    )
    sought = await media_state(video)
    return {
        "initialHttpStatus": response_status(initial),
        "readyState": started["readyState"],
        "durationSeconds": started["duration"],
        "startTimeSeconds": started["currentTime"],
        "progressedTimeSeconds": progressed["currentTime"],
        "paused": paused["paused"],
        "pausedTimeSeconds": paused["currentTime"],
        "seekTimeSeconds": sought["currentTime"],
        "title": await page.title(),
        "finalUrl": page.url,
    }


LARGE_RADIO = "#skin-client-pref-vector-feature-custom-font-size-value-2"
LARGE_CLASS = "vector-feature-custom-font-size-clientpref-2"


async def form_state(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        STATE_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    large = page.locator(LARGE_RADIO)
    await large.check()
    checked_before_reload = await large.is_checked()
    reloaded = await page.reload(
        wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    large = page.locator(LARGE_RADIO)
    await large.wait_for(state="attached")
    root_class = await page.locator("html").get_attribute("class") or ""
    return {
        "initialHttpStatus": response_status(initial),
        "reloadHttpStatus": response_status(reloaded),
        "largeCheckedBeforeReload": checked_before_reload,
        "largeCheckedAfterReload": await large.is_checked(),
        "largeClassAfterReload": LARGE_CLASS in root_class.split(),
        "title": await page.title(),
        "finalUrl": page.url,
    }


async def form_state_replay(page: Any) -> dict[str, Any]:
    initial = await page.goto(
        STATE_URL, wait_until="domcontentloaded", timeout=NAVIGATION_TIMEOUT_MS
    )
    large = page.locator(LARGE_RADIO)
    await large.wait_for(state="attached")
    persisted = await large.is_checked()
    root_class = await page.locator("html").get_attribute("class") or ""
    standard = page.get_by_role("radio", name="Standard", exact=True).first
    await standard.check()
    restored_root_class = await page.locator("html").get_attribute("class") or ""
    return {
        "initialHttpStatus": response_status(initial),
        "stateControlAvailable": True,
        "largeCheckedBeforeMutation": persisted,
        "largeClassBeforeMutation": LARGE_CLASS in root_class.split(),
        "standardCheckedAfterRestore": await standard.is_checked(),
        "largeCheckedAfterRestore": await large.is_checked(),
        "largeClassAfterRestore": LARGE_CLASS in restored_root_class.split(),
        "title": await page.title(),
        "finalUrl": page.url,
    }


def document_markers_passed(task: dict[str, Any]) -> bool:
    return (
        http_ok(task.get("initialHttpStatus"))
        and task.get("initialHeading") == "Search results"
        and urlsplit(task.get("articleUrl", "")).path == "/wiki/Military_camouflage"
        and task.get("articleHeading") == "Military camouflage"
        and task.get("historyVisible") is True
        and http_ok(task.get("backHttpStatus"))
        and task.get("backHeading") == "Search results"
        and http_ok(task.get("forwardHttpStatus"))
        and urlsplit(task.get("forwardUrl", "")).path == "/wiki/Military_camouflage"
        and task.get("forwardHeading") == "Military camouflage"
        and task.get("finalUrl") == task.get("forwardUrl")
    )


def complex_markers_passed(task: dict[str, Any]) -> bool:
    query = "is:issue state:open label:browser-chromium"
    return (
        http_ok(task.get("initialHttpStatus"))
        and task.get("queryInputValue") == query
        and task.get("decodedQuery") == query
        and type(task.get("issueLinkCount")) is int
        and task["issueLinkCount"] >= 1
        and task.get("noResultsVisible") is False
        and urlsplit(task.get("finalUrl", "")).hostname == "github.com"
    )


def graphics_markers_passed(task: dict[str, Any]) -> bool:
    return (
        http_ok(task.get("initialHttpStatus"))
        and type(task.get("initialCompletedTileCount")) is int
        and task["initialCompletedTileCount"] >= 1
        and map_zoom(task.get("initialHash", "")) == 12
        and task.get("placeQuery") == "Hong Kong"
        and type(task.get("searchResultCount")) is int
        and task["searchResultCount"] >= 1
        and map_zoom(task.get("prePanHash", "")) == 12
        and task.get("panHash") != task.get("prePanHash")
        and map_zoom(task.get("panHash", "")) == 12
        and map_zoom(task.get("zoomHash", "")) == 13
        and task.get("cyclosmChecked") is True
        and type(task.get("newLayerTileCount")) is int
        and task["newLayerTileCount"] >= 1
        and urlsplit(task.get("finalUrl", "")).hostname == "www.openstreetmap.org"
    )


def media_markers_passed(task: dict[str, Any]) -> bool:
    numbers = (
        task.get("durationSeconds"),
        task.get("startTimeSeconds"),
        task.get("progressedTimeSeconds"),
        task.get("seekTimeSeconds"),
    )
    return (
        http_ok(task.get("initialHttpStatus"))
        and type(task.get("readyState")) is int
        and task["readyState"] >= 2
        and all(type(value) in (int, float) and math.isfinite(value) for value in numbers)
        and 19 <= task["durationSeconds"] <= 21
        and task["progressedTimeSeconds"] - task["startTimeSeconds"] >= 1
        and task.get("paused") is True
        and 4 <= task["seekTimeSeconds"] <= 6
        and urlsplit(task.get("finalUrl", "")).hostname
        == "commons.wikimedia.org"
    )


def form_markers_passed(task: dict[str, Any]) -> bool:
    return (
        http_ok(task.get("initialHttpStatus"))
        and http_ok(task.get("reloadHttpStatus"))
        and task.get("largeCheckedBeforeReload") is True
        and task.get("largeCheckedAfterReload") is True
        and task.get("largeClassAfterReload") is True
        and urlsplit(task.get("finalUrl", "")).path == "/wiki/Web_browser"
    )


def replay_markers_passed(task: dict[str, Any]) -> bool:
    return (
        http_ok(task.get("initialHttpStatus"))
        and task.get("stateControlAvailable") is True
        and task.get("largeCheckedBeforeMutation") is True
        and task.get("largeClassBeforeMutation") is True
        and task.get("standardCheckedAfterRestore") is True
        and task.get("largeCheckedAfterRestore") is False
        and task.get("largeClassAfterRestore") is False
        and urlsplit(task.get("finalUrl", "")).path == "/wiki/Web_browser"
    )


TASK_PREDICATES: dict[str, Callable[[dict[str, Any]], bool]] = {
    "documentNavigation": document_markers_passed,
    "complexJavaScript": complex_markers_passed,
    "interactiveGraphics": graphics_markers_passed,
    "audioVideo": media_markers_passed,
    "formState": form_markers_passed,
    "formStateReplay": replay_markers_passed,
}
TASK_WORKERS: dict[str, Callable[[Any], Awaitable[dict[str, Any]]]] = {
    "documentNavigation": document_navigation,
    "complexJavaScript": complex_javascript,
    "interactiveGraphics": interactive_graphics,
    "audioVideo": audio_video,
    "formState": form_state,
    "formStateReplay": form_state_replay,
}


def bounded_error(error: object) -> dict[str, str]:
    return {
        "type": type(error).__name__[:64],
        "message": str(error).replace("\r", " ").replace("\n", " ")[:300],
    }


def disconnect_error(error: BaseException) -> bool:
    message = str(error).lower()
    return any(
        marker in message
        for marker in (
            "target page, context or browser has been closed",
            "browser has been closed",
            "browser closed",
            "connection closed",
            "pipe closed",
        )
    )


def screenshot_receipt(path: Path) -> dict[str, Any]:
    return {
        "path": path.relative_to(REPO_ROOT).as_posix(),
        "sha256": sha256_file(path),
        "sizeBytes": path.stat().st_size,
    }


def screenshot_valid(value: object) -> bool:
    return (
        type(value) is dict
        and type(value.get("path")) is str
        and re.fullmatch(r"[0-9a-f]{64}", value.get("sha256", "")) is not None
        and type(value.get("sizeBytes")) is int
        and value["sizeBytes"] > 0
    )


def screenshot_files_verified(evidence: dict[str, Any]) -> bool:
    tasks = [
        *observation_tasks(evidence, "phaseA").values(),
        *observation_tasks(evidence, "phaseB").values(),
    ]
    if len(tasks) != 6:
        return False
    artifacts_root = (REPO_ROOT / "artifacts").resolve()
    for task in tasks:
        receipt = task.get("screenshot")
        if not screenshot_valid(receipt):
            return False
        try:
            path = (REPO_ROOT / receipt["path"]).resolve(strict=True)
        except OSError:
            return False
        if (
            not path.is_relative_to(artifacts_root)
            or path.stat().st_size != receipt["sizeBytes"]
            or sha256_file(path) != receipt["sha256"]
        ):
            return False
    return True


async def observe_task(context: Any, phase: str, name: str, ordinal: int) -> dict[str, Any]:
    page = None
    crashed = False
    page_errors: list[dict[str, str]] = []
    result: dict[str, Any] = {
        "name": name,
        "phase": phase,
        "url": SELECTED_URLS[name],
        "startedAtUtc": utc_now(),
        "status": "inconclusive",
        "verified": False,
    }

    def on_crash() -> None:
        nonlocal crashed
        crashed = True

    def on_page_error(error: object) -> None:
        if len(page_errors) < 3:
            page_errors.append(bounded_error(error))

    started = time.monotonic()
    error: BaseException | None = None
    try:
        page = await context.new_page()
        page.set_default_timeout(NAVIGATION_TIMEOUT_MS)
        page.set_default_navigation_timeout(NAVIGATION_TIMEOUT_MS)
        page.on("crash", on_crash)
        page.on("pageerror", on_page_error)
        result.update(
            await asyncio.wait_for(
                TASK_WORKERS[name](page), timeout=TASK_BUDGET_SECONDS[name]
            )
        )
    except BaseException as exc:  # noqa: BLE001 - bounded evidence only
        error = exc
        result["error"] = bounded_error(exc)
    result["elapsedMs"] = round((time.monotonic() - started) * 1000)
    result["budgetMs"] = round(TASK_BUDGET_SECONDS[name] * 1000)
    result["crashed"] = crashed
    result["pageErrors"] = page_errors
    result["unexpectedPageClose"] = page is not None and page.is_closed()

    if page is not None and not page.is_closed():
        screenshot_root = Path(os.environ[SCREENSHOT_ROOT_ENV]).resolve()
        if not screenshot_root.is_relative_to((REPO_ROOT / "artifacts").resolve()):
            raise FP4Error("screenshot root escaped repository artifacts")
        screenshot_path = screenshot_root / f"{phase}-{ordinal:02d}-{name}.png"
        try:
            await page.screenshot(path=str(screenshot_path), timeout=10_000)
            result["screenshot"] = screenshot_receipt(screenshot_path)
        except BaseException as exc:  # noqa: BLE001
            result["screenshotError"] = bounded_error(exc)

    if page is None:
        result["pageClose"] = {"status": "not_present"}
    elif page.is_closed():
        result["pageClose"] = {"status": "already_closed"}
    else:
        result["pageClose"] = (
            await host_v1.close_context_bounded(page, PAGE_CLOSE_TIMEOUT_SECONDS)
        ).as_dict()

    direct_failure = (
        crashed
        or result["unexpectedPageClose"]
        or (error is not None and disconnect_error(error))
        or result["pageClose"].get("status") not in {"success", "not_present"}
    )
    semantic_pass = (
        error is None
        and result["elapsedMs"] <= result["budgetMs"]
        and TASK_PREDICATES[name](result)
        and screenshot_valid(result.get("screenshot"))
    )
    if direct_failure:
        result["status"] = "failed"
        result["failureClass"] = "direct-browser-or-lifecycle"
    elif semantic_pass:
        result["status"] = "passed"
    else:
        result["failureClass"] = "external-or-ambiguous"
    result["completedAtUtc"] = utc_now()
    return result


def persist_observation(observed_path: Path, observation: dict[str, Any]) -> None:
    payload = fp3.strict_json(observed_path)
    payload["fp4CompatibilityObservation"] = observation
    raw = (json.dumps(payload, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    temporary = observed_path.with_name("observed.fp4.tmp")
    if temporary.exists():
        raise FP4Error("stale FP4 observation temporary file")
    temporary.write_bytes(raw)
    os.replace(temporary, observed_path)


async def collect_phase_observation(
    context: Any, observed_path: Path, phase: str
) -> dict[str, Any]:
    base = fp3.strict_json(observed_path)
    if type(base.get("observedFull")) is not dict:
        raise FP4Error("base Host observation is unavailable")
    names = PHASE_A_TASK_NAMES if phase == "phaseA" else PHASE_B_TASK_NAMES
    observation: dict[str, Any] = {
        "schema": "verisilo-camoufox-fp4-compatibility-observation/v1",
        "matrixVersion": MATRIX_VERSION,
        "phase": phase,
        "status": "running",
        "startedAtUtc": utc_now(),
        "tasks": [],
        "verified": False,
    }
    persist_observation(observed_path, observation)
    for ordinal, name in enumerate(names, 1):
        observation["tasks"].append(
            await observe_task(context, phase, name, ordinal)
        )
        persist_observation(observed_path, observation)
    observation["status"] = "completed"
    observation["completedAtUtc"] = utc_now()
    persist_observation(observed_path, observation)
    return observation


class FP4ManagedHost(fp3.FP3ManagedHost):
    async def _launch_browser(self, session: dict, artifact: dict) -> None:
        # Reuse production launch and FP3's exact asset binding without rerunning
        # FP3's already-closed exit/Geo/ICE observation.
        await _ORIGINAL_HOST._launch_browser(self, session, artifact)
        observed_path = Path(session["sessionDir"]) / "observed.json"
        phase = {0: "phaseA", 1: "phaseB"}.get(session.get("bootCountBefore"))
        try:
            context = session.get("ctx")
            if context is None or phase is None:
                raise FP4Error("live context or expected boot phase is unavailable")
            await collect_phase_observation(context, observed_path, phase)
        except Exception as exc:  # noqa: BLE001 - preserve bounded evidence
            try:
                payload = fp3.strict_json(observed_path)
                existing = payload.get("fp4CompatibilityObservation")
                observation = existing if type(existing) is dict else {}
                observation.update(
                    {
                        "schema": "verisilo-camoufox-fp4-compatibility-observation/v1",
                        "matrixVersion": MATRIX_VERSION,
                        "phase": phase,
                        "status": "failed",
                        "error": bounded_error(exc),
                        "completedAtUtc": utc_now(),
                        "verified": False,
                    }
                )
                persist_observation(observed_path, observation)
            except Exception:
                pass


def run_child_host() -> int:
    if os.name != "nt":
        raise FP4Error("FP4 requires native Windows")
    if SCREENSHOT_ROOT_ENV not in os.environ:
        raise FP4Error("FP4 screenshot root is required")
    fp3.patch_host()
    host_v1.CamoufoxHost = FP4ManagedHost
    return host_v1.main()


def protocol_result(response: object) -> dict[str, Any]:
    if type(response) is not dict or response.get("ok") is not True:
        return {}
    result = response.get("result")
    return result if type(result) is dict else {}


def phase_result(evidence: dict[str, Any], phase: str, command: str) -> dict[str, Any]:
    return protocol_result(
        evidence.get("responses", {}).get(phase, {}).get(command)
    )


def clean_close(result: dict[str, Any]) -> bool:
    process_tree = result.get("processTreeExit", {})
    job = process_tree.get("job", {}) if type(process_tree) is dict else {}
    return (
        result.get("state") == "exited"
        and result.get("exitStatus") == 0
        and result.get("exitFileObserved") is True
        and result.get("closeOutcome", {}).get("status") == "success"
        and process_tree.get("exited") is True
        and process_tree.get("remaining") == []
        and job.get("available") is True
        and job.get("activeProcessCount") == 0
    )


def task_passed(task: dict[str, Any]) -> bool:
    name = task.get("name")
    return (
        name in TASK_PREDICATES
        and task.get("status") == "passed"
        and task.get("verified") is False
        and task.get("phase")
        == ("phaseB" if name == "formStateReplay" else "phaseA")
        and task.get("url") == SELECTED_URLS[name]
        and type(task.get("elapsedMs")) is int
        and task["elapsedMs"] <= round(TASK_BUDGET_SECONDS[name] * 1000)
        and task.get("budgetMs") == round(TASK_BUDGET_SECONDS[name] * 1000)
        and task.get("crashed") is False
        and task.get("unexpectedPageClose") is False
        and task.get("pageClose", {}).get("status") == "success"
        and screenshot_valid(task.get("screenshot"))
        and TASK_PREDICATES[name](task)
    )


def phase_observation(evidence: dict[str, Any], phase: str) -> dict[str, Any]:
    value = evidence.get("observations", {}).get(phase, {}).get(
        "fp4CompatibilityObservation", {}
    )
    return value if type(value) is dict else {}


def observation_tasks(
    evidence: dict[str, Any], phase: str
) -> dict[str, dict[str, Any]]:
    observation = phase_observation(evidence, phase)
    return {
        task.get("name"): task
        for task in observation.get("tasks", [])
        if type(task) is dict and type(task.get("name")) is str
    }


def observation_complete(evidence: dict[str, Any], phase: str) -> bool:
    observation = phase_observation(evidence, phase)
    expected = set(PHASE_A_TASK_NAMES if phase == "phaseA" else PHASE_B_TASK_NAMES)
    tasks = observation_tasks(evidence, phase)
    return (
        observation.get("schema")
        == "verisilo-camoufox-fp4-compatibility-observation/v1"
        and observation.get("matrixVersion") == MATRIX_VERSION
        and observation.get("phase") == phase
        and observation.get("status") == "completed"
        and observation.get("verified") is False
        and set(tasks) == expected
        and len(observation.get("tasks", [])) == len(expected)
    )


def adjudicate_native(evidence: dict[str, Any]) -> dict[str, Any]:
    hello = protocol_result(evidence.get("responses", {}).get("hello"))
    launch_a = phase_result(evidence, "phaseA", "launch")
    launch_b = phase_result(evidence, "phaseB", "launch")
    status_a = phase_result(evidence, "phaseA", "status")
    status_b = phase_result(evidence, "phaseB", "status")
    close_a = phase_result(evidence, "phaseA", "close")
    close_b = phase_result(evidence, "phaseB", "close")
    shutdown = protocol_result(evidence.get("responses", {}).get("shutdown"))
    self_check = shutdown.get("selfCheck", {})
    fixed = evidence.get("fixedInputs", {})
    tasks_a = observation_tasks(evidence, "phaseA")
    tasks_b = observation_tasks(evidence, "phaseB")
    expected_fixed = {
        "matrixVersion": MATRIX_VERSION,
        "artifactId": ARTIFACT_ID,
        "artifactFileSha256": SOURCE_ARTIFACT_SHA256,
        "profileId": fixed.get("profileId"),
        "requiredProxy": PROXY_URI,
        "runtimeExecutableSha256": fp3.EXECUTABLE_SHA256,
        "selectedUrls": SELECTED_URLS,
    }
    checks: dict[str, Any] = {
        "fixedInputsExact": fixed == expected_fixed,
        "formalBindingExact": (
            hello.get("platform") == "windows-x64"
            and hello.get("browserRelease") == fp3.RELEASE
            and hello.get("assetSha256") == fp3.ARCHIVE_SHA256
            and hello.get("treeManifestSha256") == fp3.TREE_MANIFEST_SHA256
            and hello.get("verified") is False
        ),
        "phaseAArtifactBindingExact": (
            launch_a.get("artifactId") == ARTIFACT_ID
            and launch_a.get("artifactFileSha256") == SOURCE_ARTIFACT_SHA256
            and launch_a.get("profileId") == fixed.get("profileId")
            and launch_a.get("verified") is False
        ),
        "phaseBArtifactBindingExact": (
            launch_b.get("artifactId") == ARTIFACT_ID
            and launch_b.get("artifactFileSha256") == SOURCE_ARTIFACT_SHA256
            and launch_b.get("profileId") == fixed.get("profileId")
            and launch_b.get("verified") is False
        ),
        "routeBindingExactBothPhases": (
            launch_a.get("browserProxyServer") == PROXY_URI
            and status_a.get("browserProxyServer") == PROXY_URI
            and launch_b.get("browserProxyServer") == PROXY_URI
            and status_b.get("browserProxyServer") == PROXY_URI
        ),
        "bootTransitionsExact": (
            launch_a.get("bootCountBefore") == 0
            and launch_a.get("bootCountAfter") == 1
            and launch_b.get("bootCountBefore") == 1
            and launch_b.get("bootCountAfter") == 2
        ),
        "hostRunningBothPhases": (
            status_a.get("state") == "running"
            and status_b.get("state") == "running"
        ),
        "phaseAObservationComplete": observation_complete(evidence, "phaseA"),
        "phaseBObservationComplete": observation_complete(evidence, "phaseB"),
        "phaseACleanClose": clean_close(close_a),
        "phaseBCleanClose": clean_close(close_b),
        "shutdownSelfCheckClean": (
            shutdown.get("state") == "shutdown"
            and self_check.get("argvMatches") == []
            and self_check.get("stderrLogMatches") == []
        ),
        "exactHostChildExit": evidence.get("childExitCode") == 0,
        "residualProcessTreeEmpty": evidence.get("residualOwnedPids") == [],
        "runtimeRemovedAfterCleanExit": evidence.get("runtimeCleanup", {}).get(
            "status"
        )
        == "removed",
        "screenshotFilesExact": evidence.get("screenshotFilesVerified") is True,
        "evidenceReadsComplete": evidence.get("readErrors", []) == [],
        "runnerCompletedWithoutError": "failure" not in evidence,
    }
    task_checks = {
        name: task_passed(tasks_a.get(name, {})) for name in PHASE_A_TASK_NAMES
    }
    task_checks["formStateReplay"] = task_passed(
        tasks_b.get("formStateReplay", {})
    )
    checks["tasks"] = task_checks

    replay = tasks_b.get("formStateReplay", {})
    profile_state_lost = (
        task_checks.get("formState") is True
        and replay.get("stateControlAvailable") is True
        and (
            replay.get("largeCheckedBeforeMutation") is False
            or replay.get("largeClassBeforeMutation") is False
        )
    )
    direct_task_failure = any(
        task.get("status") == "failed"
        for task in (*tasks_a.values(), *tasks_b.values())
    )
    direct_checks = (
        "fixedInputsExact",
        "formalBindingExact",
        "phaseAArtifactBindingExact",
        "phaseBArtifactBindingExact",
        "routeBindingExactBothPhases",
        "bootTransitionsExact",
        "hostRunningBothPhases",
        "phaseACleanClose",
        "phaseBCleanClose",
        "shutdownSelfCheckClean",
        "exactHostChildExit",
        "residualProcessTreeEmpty",
        "runtimeRemovedAfterCleanExit",
        "runnerCompletedWithoutError",
    )
    independent_failure = profile_state_lost or any(
        checks[name] is not True for name in direct_checks
    )
    direct_failure = independent_failure or direct_task_failure
    if direct_failure:
        status = "failed"
    elif (
        not checks["phaseAObservationComplete"]
        or not checks["phaseBObservationComplete"]
        or not checks["screenshotFilesExact"]
        or not checks["evidenceReadsComplete"]
        or not all(task_checks.values())
    ):
        status = "inconclusive"
    else:
        status = "passed"
    return {
        "status": status,
        "checks": checks,
        "profileStateLost": profile_state_lost,
        "upstreamControlRequired": direct_task_failure and not independent_failure,
        "terminal": not (direct_task_failure and not independent_failure),
        "precedence": "failed>inconclusive>passed",
    }


class HostProcess:
    def __init__(self, command: list[str], environment: dict[str, str], stderr: Path):
        self.frames: list[dict[str, Any]] = []
        self._counter = 0
        self._stdout: queue.Queue[tuple[str, Any]] = queue.Queue()
        self._stderr = stderr.open("wb")
        try:
            self.process = subprocess.Popen(
                command,
                cwd=HOST_DIR,
                env=environment,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=self._stderr,
            )
        except BaseException:
            self._stderr.close()
            raise
        self._stdout_thread = Thread(target=self._read_stdout, daemon=True)
        self._stdout_thread.start()

    def _read_stdout(self) -> None:
        try:
            if self.process.stdout is None:
                raise FP4Error("Host stdout is unavailable")
            for raw in iter(self.process.stdout.readline, b""):
                self._stdout.put(("ok", raw))
        except BaseException as exc:  # noqa: BLE001
            self._stdout.put(("error", exc))
        finally:
            self._stdout.put(("eof", b""))

    def send(self, name: str, params: dict[str, Any], timeout: float) -> dict[str, Any]:
        if self.process.stdin is None:
            raise FP4Error("Host stdio is unavailable")
        self._counter += 1
        request = {
            "id": f"fp4-{self._counter}-{name}",
            "command": name,
            "params": params,
        }
        self.process.stdin.write(
            json.dumps(request, separators=(",", ":")).encode("utf-8") + b"\n"
        )
        self.process.stdin.flush()
        try:
            kind, value = self._stdout.get(timeout=timeout)
        except queue.Empty as exc:
            raise TimeoutError("timed out waiting for Host protocol response") from exc
        if kind == "error":
            raise value
        if kind == "eof" or not value:
            raise FP4Error("Host closed stdout")
        response = json.loads(value.decode("utf-8"))
        if type(response) is not dict or response.get("id") != request["id"]:
            raise FP4Error(f"invalid Host response for {name}")
        self.frames.append({"request": request, "response": response})
        return response

    def wait(self, timeout: float = 30.0) -> int:
        exit_code = self.process.wait(timeout=timeout)
        self._stdout_thread.join(timeout=2.0)
        self._stderr.close()
        return exit_code

    def kill(self) -> int:
        if self.process.poll() is None:
            self.process.kill()
        return self.wait(20.0)


def require_ok(response: dict[str, Any], command: str) -> dict[str, Any]:
    if response.get("ok") is not True or type(response.get("result")) is not dict:
        raise FP4Error(f"Host {command} failed")
    return response["result"]


def git(*arguments: str) -> str:
    return subprocess.run(
        ["git", *arguments],
        cwd=REPO_ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
    ).stdout.strip()


def output_receipts(paths: list[Path]) -> dict[str, dict[str, Any]]:
    return {
        path.relative_to(REPO_ROOT).as_posix(): {
            "sha256": sha256_file(path),
            "sizeBytes": path.stat().st_size,
        }
        for path in paths
        if path.is_file()
    }


def safe_json(
    path: Path, label: str, read_errors: list[dict[str, str]]
) -> dict[str, Any]:
    try:
        return fp3.strict_json(path)
    except BaseException as exc:  # noqa: BLE001
        read_errors.append({"label": label, **bounded_error(exc)})
        return {}


def phase_identities(evidence: dict[str, Any]) -> list[dict[str, Any]]:
    identities: list[dict[str, Any]] = []
    for phase in ("phaseA", "phaseB"):
        close = phase_result(evidence, phase, "close")
        value = close.get("processTreeExit", {}).get("managedIdentities", [])
        if type(value) is list:
            identities.extend(item for item in value if type(item) is dict)
    return identities


def execute_attempt(attempt_root: Path, attempt: int) -> str:
    artifacts_root = (REPO_ROOT / "artifacts").resolve()
    attempt_root = attempt_root.resolve()
    if not attempt_root.is_relative_to(artifacts_root):
        raise FP4Error("attempt root must be inside repository artifacts")
    if attempt_root.exists():
        raise FP4Error(f"immutable attempt already exists: {attempt_root}")
    if os.name != "nt":
        raise FP4Error("FP4 requires native Windows")
    if Path(git("rev-parse", "--show-toplevel")).resolve() != REPO_ROOT.resolve():
        raise FP4Error("Git root differs from repository root")
    if git("branch", "--show-current") != BRANCH:
        raise FP4Error("wrong Git branch")
    if git("status", "--porcelain=v1", "--untracked-files=all"):
        raise FP4Error("worktree is not clean")
    revision = git("rev-parse", "HEAD")
    if revision != git("rev-parse", f"origin/{BRANCH}"):
        raise FP4Error("runner commit is not synchronized with origin")
    try:
        for path, digest in (
            (CONTRACT, CONTRACT_SHA256),
            (FP3_RESULT, FP3_RESULT_SHA256),
            (SOURCE_ARTIFACT, SOURCE_ARTIFACT_SHA256),
            (SOURCE_ARTIFACT_SIDECAR, SOURCE_ARTIFACT_SIDECAR_SHA256),
            (fp3.ASSET_LOCK, fp3.ASSET_LOCK_SHA256),
            (fp3.TREE_MANIFEST, fp3.TREE_MANIFEST_SHA256),
            (fp3.EXECUTABLE, fp3.EXECUTABLE_SHA256),
        ):
            fp3.require_file(path, digest)
    except Exception as exc:
        raise FP4Error(
            f"frozen input unavailable: {type(exc).__name__}"
        ) from exc

    started_at = utc_now()
    run_id = f"fp4-{uuid.uuid4().hex}"
    profile_id = f"fp4-attempt-{attempt}"
    attempt_root.mkdir(parents=True)
    screenshot_root = attempt_root / "screenshots"
    screenshot_root.mkdir()
    runtime_root = Path(
        tempfile.mkdtemp(prefix=f"verisilo-fp4-{attempt}-")
    ).resolve()
    artifact_root = runtime_root / "artifacts"
    profile_root = runtime_root / "profiles"
    state_root = runtime_root / "state"
    cache_root = runtime_root / "cache"
    for path in (artifact_root, profile_root, state_root, cache_root):
        path.mkdir()
    stage_artifact(artifact_root)

    stderr_path = attempt_root / "host-stderr.txt"
    native_path = attempt_root / "native-evidence.json"
    report_path = attempt_root / "run-report.json"
    command = [
        sys.executable,
        "-u",
        str(Path(__file__).resolve()),
        "--child-host",
        "--artifact-root",
        str(artifact_root),
        "--profile-root",
        str(profile_root),
        "--state-root",
        str(state_root),
        "--tree-manifest",
        str(fp3.TREE_MANIFEST),
        "--asset-lock",
        str(fp3.ASSET_LOCK),
        "--browser-root",
        str(fp3.BROWSER_ROOT),
    ]
    environment = os.environ.copy()
    environment["VERISILO_CAMOUFOX_CACHE_DIR"] = str(cache_root)
    environment[SCREENSHOT_ROOT_ENV] = str(screenshot_root)
    responses: dict[str, Any] = {"phaseA": {}, "phaseB": {}}
    evidence: dict[str, Any] = {
        "schema": "verisilo-camoufox-fp4-native-evidence/v1",
        "status": "failed",
        "evidenceClass": "attempted-on-this-native-windows-host",
        "verified": False,
        "fixedInputs": {
            "matrixVersion": MATRIX_VERSION,
            "artifactId": ARTIFACT_ID,
            "artifactFileSha256": SOURCE_ARTIFACT_SHA256,
            "profileId": profile_id,
            "requiredProxy": PROXY_URI,
            "runtimeExecutableSha256": sha256_file(fp3.EXECUTABLE),
            "selectedUrls": SELECTED_URLS,
        },
        "responses": responses,
        "observations": {},
        "sessions": {},
        "readErrors": [],
        "childExitCode": None,
        "residualOwnedPids": None,
    }
    host: HostProcess | None = None
    active_session_id: str | None = None
    active_phase: str | None = None
    session_ids: dict[str, str] = {}
    failure: BaseException | None = None

    def capture_session(phase: str, session_id: str, suffix: str) -> None:
        session_dir = state_root / session_id
        observed = safe_json(
            session_dir / "observed.json",
            f"{phase}.observed.{suffix}",
            evidence["readErrors"],
        )
        session = safe_json(
            session_dir / "session.json",
            f"{phase}.session.{suffix}",
            evidence["readErrors"],
        )
        if observed:
            evidence["observations"][phase] = observed
        if session:
            evidence["sessions"][f"{phase}.{suffix}"] = session

    try:
        host = HostProcess(command, environment, stderr_path)
        responses["hello"] = host.send("hello", {}, 300.0)
        require_ok(responses["hello"], "hello")
        for phase, launch_timeout in (("phaseA", 900.0), ("phaseB", 360.0)):
            active_phase = phase
            phase_responses = responses[phase]
            phase_responses["launch"] = host.send(
                "launch",
                {
                    "artifactId": ARTIFACT_ID,
                    "profileId": profile_id,
                    "expectedArtifactFileSha256": SOURCE_ARTIFACT_SHA256,
                    "browserProxyServer": PROXY_URI,
                },
                launch_timeout,
            )
            active_session_id = require_ok(
                phase_responses["launch"], f"{phase} launch"
            )["sessionId"]
            session_ids[phase] = active_session_id
            capture_session(phase, active_session_id, "running")
            phase_responses["status"] = host.send(
                "status", {"sessionId": active_session_id}, 30.0
            )
            require_ok(phase_responses["status"], f"{phase} status")
            phase_responses["close"] = host.send(
                "close", {"sessionId": active_session_id}, 150.0
            )
            require_ok(phase_responses["close"], f"{phase} close")
            capture_session(phase, active_session_id, "stopped")
            active_session_id = None
            active_phase = None
    except BaseException as exc:  # noqa: BLE001 - immutable failure lineage
        failure = exc
    finally:
        if (
            host is not None
            and host.process.poll() is None
            and active_session_id is not None
            and active_phase is not None
        ):
            try:
                responses[active_phase]["close"] = host.send(
                    "close", {"sessionId": active_session_id}, 150.0
                )
                capture_session(active_phase, active_session_id, "stopped")
            except BaseException as exc:  # noqa: BLE001
                failure = failure or exc
        if host is not None and host.process.poll() is None:
            try:
                responses["shutdown"] = host.send("shutdown", {}, 60.0)
                require_ok(responses["shutdown"], "shutdown")
                evidence["childExitCode"] = host.wait(30.0)
            except BaseException as exc:  # noqa: BLE001
                failure = failure or exc
                try:
                    evidence["childExitCode"] = host.kill()
                except BaseException as cleanup_exc:  # noqa: BLE001
                    evidence["childExitCode"] = None
                    evidence["cleanupFailure"] = bounded_error(cleanup_exc)
        elif host is not None:
            try:
                evidence["childExitCode"] = host.wait(1.0)
            except BaseException as exc:  # noqa: BLE001
                failure = failure or exc

    for phase, session_id in session_ids.items():
        capture_session(phase, session_id, "postmortem")

    evidence["residualOwnedPids"] = sorted(
        {
            identity["pid"]
            for identity in phase_identities(evidence)
            if type(identity.get("pid")) is int and process_identity_alive(identity)
        }
    )
    if host is not None:
        evidence["protocolFrames"] = host.frames
    if failure is not None:
        evidence["failure"] = {
            "type": type(failure).__name__[:64],
            "message": str(failure)[:500],
        }
        (attempt_root / "runner-error.txt").write_text(
            "".join(traceback.format_exception(failure)),
            encoding="utf-8",
            newline="\n",
        )

    close_results = [
        phase_result(evidence, "phaseA", "close"),
        phase_result(evidence, "phaseB", "close"),
    ]
    removable = (
        all(clean_close(result) for result in close_results)
        and evidence.get("childExitCode") == 0
        and evidence.get("residualOwnedPids") == []
    )
    if removable:
        expected_temp_parent = Path(tempfile.gettempdir()).resolve()
        if (
            runtime_root.parent != expected_temp_parent
            or not runtime_root.name.startswith(f"verisilo-fp4-{attempt}-")
        ):
            evidence["runtimeCleanup"] = {
                "status": "failed",
                "path": str(runtime_root),
                "error": {"type": "UnsafeCleanupTarget", "message": "target rejected"},
            }
        else:
            try:
                shutil.rmtree(runtime_root)
                evidence["runtimeCleanup"] = {"status": "removed"}
            except BaseException as exc:  # noqa: BLE001
                evidence["runtimeCleanup"] = {
                    "status": "failed",
                    "path": str(runtime_root),
                    "error": bounded_error(exc),
                }
    else:
        evidence["runtimeCleanup"] = {
            "status": "preserved-dirty-boundary",
            "path": str(runtime_root),
        }

    evidence["screenshotFilesVerified"] = screenshot_files_verified(evidence)
    adjudication = adjudicate_native(evidence)
    evidence["adjudication"] = adjudication
    evidence["status"] = adjudication["status"]
    if evidence["status"] == "passed":
        evidence["evidenceClass"] = "observed-on-this-native-windows-host"
    native_sha256 = write_json(native_path, evidence)

    output_paths = [native_path, stderr_path]
    output_paths.extend(sorted(screenshot_root.glob("*.png")))
    runner_error = attempt_root / "runner-error.txt"
    if runner_error.exists():
        output_paths.append(runner_error)
    report = {
        "schema": "verisilo-camoufox-fp4-run-report/v1",
        "attempt": attempt,
        "runId": run_id,
        "startedAtUtc": started_at,
        "completedAtUtc": utc_now(),
        "status": evidence["status"],
        "terminal": adjudication["terminal"],
        "upstreamControlRequired": adjudication["upstreamControlRequired"],
        "evidenceClass": evidence["evidenceClass"],
        "verified": False,
        "host": {"platform": platform.platform(), "python": platform.python_version()},
        "code": {
            "branch": BRANCH,
            "revision": revision,
            "tree": git("rev-parse", "HEAD^{tree}"),
            "originRevision": revision,
            "runnerSha256": sha256_file(Path(__file__).resolve()),
            "contractSha256": CONTRACT_SHA256,
        },
        "inputs": {
            **evidence["fixedInputs"],
            "fp3Result": {
                "path": FP3_RESULT.relative_to(REPO_ROOT).as_posix(),
                "sha256": FP3_RESULT_SHA256,
            },
            "sourceArtifact": {
                "path": SOURCE_ARTIFACT.relative_to(REPO_ROOT).as_posix(),
                "sha256": SOURCE_ARTIFACT_SHA256,
                "sidecarPath": SOURCE_ARTIFACT_SIDECAR.relative_to(
                    REPO_ROOT
                ).as_posix(),
                "sidecarSha256": SOURCE_ARTIFACT_SIDECAR_SHA256,
            },
            "runtimeAssetLock": {
                "path": fp3.ASSET_LOCK.relative_to(REPO_ROOT).as_posix(),
                "sha256": fp3.ASSET_LOCK_SHA256,
            },
            "runtimeTree": {
                "path": fp3.TREE_MANIFEST.relative_to(REPO_ROOT).as_posix(),
                "rawSha256": fp3.TREE_MANIFEST_SHA256,
                "canonicalSha256": fp3.TREE_MANIFEST_CANONICAL_SHA256,
            },
        },
        "execution": {"command": command, "childExitCode": evidence["childExitCode"]},
        "nativeEvidence": {
            "path": native_path.relative_to(REPO_ROOT).as_posix(),
            "sha256": native_sha256,
        },
        "adjudication": adjudication,
        "boundaries": {
            "universalCompatibility": "not_claimed",
            "loginPaymentCaptcha": "not_tested",
            "dnsTlsQuic": "unavailable",
            "crossHostReplay": "not_tested",
            "productionPackageRelease": "not_claimed",
            "nextGate": "clean-m3-wi-definition-refreeze-only-after-fp4-pass",
        },
        "outputs": output_receipts(output_paths),
    }
    write_json(report_path, report)
    print(f"{report['status']}: {report_path}")
    return report["status"]


def main() -> int:
    if "--child-host" in sys.argv:
        sys.argv.remove("--child-host")
        return run_child_host()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--attempt-root", type=Path, required=True)
    parser.add_argument("--attempt", type=int, required=True)
    args = parser.parse_args()
    if args.attempt < 1:
        raise FP4Error("attempt must be positive")
    status = execute_attempt(args.attempt_root, args.attempt)
    return {"passed": 0, "failed": 1, "inconclusive": 2}[status]


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except FP4Error as exc:
        raise SystemExit(f"FP4 blocked before execution: {exc}") from exc
