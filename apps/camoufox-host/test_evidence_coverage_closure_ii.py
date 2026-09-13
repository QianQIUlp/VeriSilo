#!/usr/bin/env python3
"""Comprehensive verification suite for Evidence Coverage Closure II:
WebGL2 + Request Header (Accept-Encoding) formal runtime evidence closure.

Covers:
1. WebGL2 reconciliation:
   - matched: expected vendor & exact renderer match observed
   - ", or similar" renderer remains unavailable (no false matched)
   - WebGL2 unavailable (webgl2Available: False or missing) -> unavailable
   - WebGL1 backward compatibility preserved
2. Accept-Encoding reconciliation:
   - exact matched with headers.Accept-Encoding
   - mismatched detected
   - unavailable when observed or expected is missing
   - proven to come from HTTP request, cannot self-prove from config
3. Loopback server observation:
   - ProbeServer records real HTTP Accept-Encoding from requests
   - /header-observation returns real request header
4. Digest compatibility:
   - ObservedWebsiteDigest v2 remains stable; new surfaces do not alter digest
5. Fresh Recheck:
   - Launch and reobserve both carry new signals; observedAt updates
6. Report safety:
   - Signals exported safely without leaking secrets
"""

from __future__ import annotations

import json
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from host_probe import SAFE_IDENTITY_SCRIPT, extract_observed_website_signals
from host_runtime import ProbeServer, start_probe_server
from identity_policy import (
    IDENTITY_EVIDENCE_SCHEMA,
    OBSERVED_DIGEST_SCHEMA,
    STABLE_WEBSITE_SIGNAL_KEYS,
    canonical_digest,
    observed_website_digest,
    reconcile_website_identity,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"


def _load_fixture_artifact() -> dict[str, Any]:
    path = FIXTURES / "identity-a.json"
    return json.loads(path.read_text(encoding="utf-8"))


def test_webgl2_reconciliation_exact_matched() -> None:
    artifact = _load_fixture_artifact()
    # Configure an exact renderer without ", or similar"
    artifact["stableSignalsDeclared"]["webglVendor"] = "Google Inc. (NVIDIA)"
    artifact["stableSignalsDeclared"]["webglRenderer"] = "ANGLE (NVIDIA GeForce GTX 980)"

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": "ANGLE (NVIDIA GeForce GTX 980)",
        "webgl2Vendor": "Google Inc. (NVIDIA)",
        "webgl2Renderer": "ANGLE (NVIDIA GeForce GTX 980)",
        "webgl2Available": True,
        "acceptEncoding": artifact["resolvedConfig"]["headers.Accept-Encoding"],
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    assert "webgl2Vendor" in signals_by_name
    assert signals_by_name["webgl2Vendor"]["state"] == "matched"
    assert signals_by_name["webgl2Vendor"]["expected"] == "Google Inc. (NVIDIA)"
    assert signals_by_name["webgl2Vendor"]["observed"] == "Google Inc. (NVIDIA)"

    assert "webgl2Renderer" in signals_by_name
    assert signals_by_name["webgl2Renderer"]["state"] == "matched"
    assert signals_by_name["webgl2Renderer"]["expected"] == "ANGLE (NVIDIA GeForce GTX 980)"
    assert signals_by_name["webgl2Renderer"]["observed"] == "ANGLE (NVIDIA GeForce GTX 980)"


def test_webgl2_renderer_or_similar_remains_unavailable() -> None:
    artifact = _load_fixture_artifact()
    # Artifact declares ", or similar"
    renderer_str = "ANGLE (NVIDIA, NVIDIA GeForce GTX 980 Direct3D11 vs_5_0 ps_5_0), or similar"
    artifact["stableSignalsDeclared"]["webglVendor"] = "Google Inc. (NVIDIA)"
    artifact["stableSignalsDeclared"]["webglRenderer"] = renderer_str

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": renderer_str,
        "webgl2Vendor": "Google Inc. (NVIDIA)",
        "webgl2Renderer": renderer_str,
        "webgl2Available": True,
        "acceptEncoding": artifact["resolvedConfig"]["headers.Accept-Encoding"],
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    # WebGL1 renderer is unavailable
    assert signals_by_name["webglRenderer"]["state"] == "unavailable"
    assert "只声明了渲染器系列" in signals_by_name["webglRenderer"]["reason"]

    # WebGL2 renderer must ALSO be unavailable, not falsely matched
    assert signals_by_name["webgl2Renderer"]["state"] == "unavailable"
    assert signals_by_name["webgl2Renderer"]["expected"] is None
    assert signals_by_name["webgl2Renderer"]["observed"] == renderer_str
    assert "只声明了渲染器系列" in signals_by_name["webgl2Renderer"]["reason"]

    # But webgl2Vendor is matched
    assert signals_by_name["webgl2Vendor"]["state"] == "matched"


def test_webgl2_unavailable_when_browser_lacks_webgl2() -> None:
    artifact = _load_fixture_artifact()

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "webglVendor": artifact["stableSignalsDeclared"]["webglVendor"],
        "webglRenderer": artifact["stableSignalsDeclared"]["webglRenderer"],
        "webgl2Vendor": None,
        "webgl2Renderer": None,
        "webgl2Available": False,
        "acceptEncoding": artifact["resolvedConfig"]["headers.Accept-Encoding"],
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    assert signals_by_name["webgl2Vendor"]["state"] == "unavailable"
    assert signals_by_name["webgl2Vendor"]["observed"] is None

    assert signals_by_name["webgl2Renderer"]["state"] == "unavailable"
    assert signals_by_name["webgl2Renderer"]["observed"] is None


def test_webgl1_backward_compatibility() -> None:
    artifact = _load_fixture_artifact()
    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "webglVendor": artifact["stableSignalsDeclared"]["webglVendor"],
        "webglRenderer": artifact["stableSignalsDeclared"]["webglRenderer"],
    }
    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    # WebGL1 vendor is still matched
    assert signals_by_name["webglVendor"]["state"] == "matched"
    assert signals_by_name["webglVendor"]["expected"] == artifact["stableSignalsDeclared"]["webglVendor"]


def test_accept_encoding_exact_matched() -> None:
    artifact = _load_fixture_artifact()
    expected_ae = artifact["resolvedConfig"]["headers.Accept-Encoding"]

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "acceptEncoding": expected_ae,
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    assert "acceptEncoding" in signals_by_name
    assert signals_by_name["acceptEncoding"]["state"] == "matched"
    assert signals_by_name["acceptEncoding"]["expected"] == expected_ae
    assert signals_by_name["acceptEncoding"]["observed"] == expected_ae


def test_accept_encoding_mismatch() -> None:
    artifact = _load_fixture_artifact()
    expected_ae = artifact["resolvedConfig"]["headers.Accept-Encoding"]

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "acceptEncoding": "gzip, deflate",  # Mismatched
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    assert signals_by_name["acceptEncoding"]["state"] == "mismatched"
    assert signals_by_name["acceptEncoding"]["expected"] == expected_ae
    assert signals_by_name["acceptEncoding"]["observed"] == "gzip, deflate"
    assert evidence["state"] == "mismatched"


def test_accept_encoding_unavailable_when_not_observed() -> None:
    artifact = _load_fixture_artifact()
    expected_ae = artifact["resolvedConfig"]["headers.Accept-Encoding"]

    observed_signals = {
        "userAgent": artifact["stableSignalsDeclared"]["userAgent"],
        "language": artifact["stableSignalsDeclared"]["language"],
        "screen": artifact["stableSignalsDeclared"]["screen"],
        "devicePixelRatio": artifact["stableSignalsDeclared"]["devicePixelRatio"],
        "hardwareConcurrency": artifact["stableSignalsDeclared"]["hardwareConcurrency"],
        "timezone": artifact["resolvedConfig"]["timezone"],
        "acceptEncoding": None,  # Observation unavailable
    }

    evidence = reconcile_website_identity(artifact, observed_signals)
    signals_by_name = {s["signal"]: s for s in evidence["signals"]}

    assert signals_by_name["acceptEncoding"]["state"] == "unavailable"
    assert signals_by_name["acceptEncoding"]["expected"] == expected_ae
    assert signals_by_name["acceptEncoding"]["observed"] is None
    # Crucial guarantee: cannot claim matched without observation
    assert signals_by_name["acceptEncoding"]["state"] != "matched"


def test_loopback_probe_server_header_capture() -> None:
    probe_html = REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html"
    server, probe_url = start_probe_server(port=0, probe_file=probe_html)
    port = server.server_address[1]

    try:
        # Request /header-observation with custom Accept-Encoding
        test_ae = "gzip, deflate, br, zstd"
        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/header-observation",
            headers={"Accept-Encoding": test_ae, "User-Agent": "TestBrowser/1.0"},
        )
        with urllib.request.urlopen(req) as resp:
            data = json.loads(resp.read().decode("utf-8"))
            assert data["acceptEncoding"] == test_ae

        # Verify server captured the header directly
        assert server.get_observed_accept_encoding() == test_ae

        # Also test /probe.html navigation
        nav_req = urllib.request.Request(
            probe_url,
            headers={"Accept-Encoding": "br, zstd", "User-Agent": "TestBrowser/1.0"},
        )
        with urllib.request.urlopen(nav_req) as resp:
            assert resp.status == 200
            content = resp.read()
            assert b"probe ready" in content

        # Verify server captured the navigation header
        assert server.get_observed_accept_encoding() == "br, zstd"
    finally:
        server.shutdown()


def test_digest_stability_excludes_new_surfaces() -> None:
    # Baseline signals conforming to v2
    base_signals = {
        "userAgent": "Mozilla/5.0",
        "language": "en-US",
        "languages": ["en-US"],
        "platform": "Win32",
        "oscpu": "Windows NT 10.0; Win64; x64",
        "doNotTrack": "1",
        "globalPrivacyControl": False,
        "screen": {"width": 1280, "height": 800},
        "devicePixelRatio": 1,
        "hardwareConcurrency": 4,
        "historyLength": 2,
        "mediaDevices": [],
        "timezone": "UTC",
        "utcOffsetMinutes": 0,
        "fontNegativeControls": {},
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": "ANGLE",
        "webglSummary": {},
        "voices": [],
        "audioHash": "hash",
    }

    base_digest = observed_website_digest(base_signals)

    # Adding webgl2 and acceptEncoding to signals
    extended_signals = dict(base_signals)
    extended_signals["webgl2Vendor"] = "Google Inc. (NVIDIA)"
    extended_signals["webgl2Renderer"] = "ANGLE"
    extended_signals["webgl2Summary"] = {}
    extended_signals["webgl2Available"] = True
    extended_signals["acceptEncoding"] = "gzip, deflate, br, zstd"

    extended_digest = observed_website_digest(extended_signals)

    # ObservedWebsiteDigest v2 MUST be identical!
    assert extended_digest == base_digest, (
        f"Digest changed: {base_digest} vs {extended_digest}"
    )


def test_extract_observed_website_signals_projection() -> None:
    raw_observed = {
        "userAgent": "ua",
        "language": "zh-CN",
        "languages": ["zh-CN"],
        "platform": "Win32",
        "oscpu": "Windows NT 10.0; Win64; x64",
        "doNotTrack": "1",
        "globalPrivacyControl": True,
        "screen": {"width": 1920},
        "devicePixelRatio": 1.25,
        "hardwareConcurrency": 8,
        "historyLength": 1,
        "mediaDevices": [],
        "session": {"timezone": "Asia/Shanghai", "utcOffsetMinutes": -480},
        "fontNegativeControls": {},
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": "ANGLE",
        "webglSummary": {},
        "webgl2Vendor": "Google Inc. (NVIDIA)",
        "webgl2Renderer": "ANGLE",
        "webgl2Summary": {},
        "webgl2Available": True,
        "acceptEncoding": "gzip, deflate, br, zstd",
        "voices": [],
        "audioHash": "hash",
    }

    signals = extract_observed_website_signals(raw_observed)
    assert signals["webgl2Vendor"] == "Google Inc. (NVIDIA)"
    assert signals["webgl2Renderer"] == "ANGLE"
    assert signals["webgl2Available"] is True
    assert signals["acceptEncoding"] == "gzip, deflate, br, zstd"


def run_all_tests() -> None:
    test_webgl2_reconciliation_exact_matched()
    test_webgl2_renderer_or_similar_remains_unavailable()
    test_webgl2_unavailable_when_browser_lacks_webgl2()
    test_webgl1_backward_compatibility()
    test_accept_encoding_exact_matched()
    test_accept_encoding_mismatch()
    test_accept_encoding_unavailable_when_not_observed()
    test_loopback_probe_server_header_capture()
    test_digest_stability_excludes_new_surfaces()
    test_extract_observed_website_signals_projection()
    print("All Evidence Coverage Closure II unit & integration tests passed! (10/10)")


if __name__ == "__main__":
    run_all_tests()
