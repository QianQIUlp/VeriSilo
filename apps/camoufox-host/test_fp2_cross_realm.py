#!/usr/bin/env python3
"""No-browser regression tests for the FP2 comparator and evidence gates."""

from __future__ import annotations

import copy
import hashlib
import inspect
import json
import socket
import subprocess
import tempfile
import unittest
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import urlencode
from urllib.request import Request, urlopen
from unittest.mock import patch

import fp2_cross_realm as fp2


def marker_hash(value: str) -> str:
    return f"sha256:{hashlib.sha256(value.encode('utf-8')).hexdigest()}"


class FP2NoBrowserTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.ledger = fp2.load_applicability()
        cls.relation = fp2.load_relation_matrix()
        cls.manifest, cls.manifest_sha256 = fp2.load_probe_manifest()
        cls.artifact_a, cls.artifact_a_info = fp2.load_artifact(fp2.ARTIFACT_A_PATH)
        cls.artifact_b, cls.artifact_b_info = fp2.load_artifact(fp2.ARTIFACT_B_PATH)

    def assertCode(self, code: str, callable_obj, *args, **kwargs) -> None:  # noqa: N802
        with self.assertRaises(fp2.FP2Failure) as context:
            callable_obj(*args, **kwargs)
        self.assertEqual(context.exception.code, code)

    def probe_failure(self, *, stage: str, operation: str, message: str = "synthetic failure") -> dict:
        return {
            "schema": fp2.PROBE_FAILURE_SCHEMA,
            "realm": "top-window",
            "stage": stage,
            "operation": operation,
            "errorName": "Error",
            "errorMessage": message,
            "lastSuccessfulStage": "navigator",
            "probeCompleted": False,
        }

    def make_realm(self, artifact: dict, realm: str, *, artifact_label: str) -> dict:
        config = artifact["resolvedConfig"]
        kind = self.ledger["realms"][realm]["kind"]
        languages = ["en-US", "en"]
        navigator = {
            "userAgent": config["navigator.userAgent"],
            "platform": config["navigator.platform"],
            "hardwareConcurrency": config["navigator.hardwareConcurrency"],
            "language": "en-US",
            "languages": languages,
            "doNotTrack": config["navigator.doNotTrack"],
            "globalPrivacyControl": config["navigator.globalPrivacyControl"],
        }
        identity_headers = {
            "user-agent": config["navigator.userAgent"],
            "accept-language": "en-US,en",
            "accept-encoding": config["headers.Accept-Encoding"],
            "dnt": str(config["navigator.doNotTrack"]),
            "sec-gpc": "1" if config["navigator.globalPrivacyControl"] else "0",
        }
        request_headers = {
            "identityHeaders": identity_headers,
            "contextHeaders": {},
            "requestPolicy": {"method": "GET", "cache": "no-store", "credentials": "omit"},
        }
        digest = marker_hash(f"{artifact_label}:canvas-raw")
        canvas = {
            "apiPresent": True,
            "rawHash": digest,
            "rawRgbaHash": digest,
            "decodedPngPixelsHash": digest,
            "pngBytesHash": marker_hash(f"{artifact_label}:canvas-png"),
            "dataUrlHash": marker_hash(f"{artifact_label}:canvas-data-url"),
            "exportHash": marker_hash(f"{artifact_label}:canvas-export"),
            "png": {
                "signatureValid": True,
                "dataUrlSignatureValid": True,
                "decodeValid": True,
                "width": 240,
                "height": 120,
                "mimeType": "image/png",
            },
        }
        voice_values = [
            {
                "name": value["name"],
                "lang": value["lang"],
                "voiceURI": value["voiceURI"],
                "isDefault": value["isDefault"],
                "localService": value["isLocalService"],
            }
            for value in fp2.expected_voice_projection(config)
        ]
        fonts = {
            "apiPresent": True,
            "injectedFonts": [{"family": value, "available": True} for value in config["fonts"]],
            "fontNegativeControls": {
                "VeriSilo Missing Font 01": False,
                "VeriSilo Missing Font 02": False,
                "VeriSilo Missing Font 03": False,
            },
            "fontUniverseWidths": {value: 100 for value in ("Arial", "Times New Roman", "Courier New")},
        }
        # The declared font universe is a probe surface, not an Artifact input.
        # Keep a deterministic synthetic width map that differs only when the
        # B spacing seed is intentionally modeled by the test fixture.
        fonts["fontUniverseWidths"] = {value: 100 + (1 if artifact_label == "B" else 0) for value in ("Arial", "Times New Roman", "Courier New")}
        webgl = {"apiPresent": True, "vendor": "test-vendor", "renderer": "test-renderer", "supportedExtensions": [], "parameters": {}}
        media = {
            "apiPresent": True,
            "counts": {
                "audioinput": config["mediaDevices:micros"],
                "videoinput": config["mediaDevices:webcams"],
                "audiooutput": config["mediaDevices:speakers"],
            },
            "deviceKinds": [],
        }
        common = {
            "realm": realm,
            "kind": kind,
            "navigator": navigator,
            "locale": {"timeZone": config["timezone"], "utcOffsetMinutes": 0},
            "requestHeaders": request_headers,
            "privacySignals": {
                "doNotTrack": {"apiPresent": True, "value": navigator["doNotTrack"]},
                "globalPrivacyControl": {"apiPresent": True, "value": navigator["globalPrivacyControl"]},
            },
            "maxTouchPoints": {"apiPresent": True, "value": 0},
            "webgl": webgl,
            "webgl2": copy.deepcopy(webgl),
            "fonts": fonts,
            "capabilities": {},
        }
        if kind == "window":
            common.update(
                {
                    "screen": {
                        "width": config["screen.width"],
                        "height": config["screen.height"],
                        "availWidth": config["screen.availWidth"],
                        "availHeight": config["screen.availHeight"],
                        "availTop": config["screen.availTop"],
                        "availLeft": 0,
                        "colorDepth": 24,
                        "pixelDepth": 24,
                    },
                    "devicePixelRatio": 1,
                    "geometry": {
                        "innerWidth": 1280,
                        "innerHeight": 800,
                        "outerWidth": config["window.outerWidth"],
                        "outerHeight": config["window.outerHeight"],
                        "screenX": config["window.screenX"],
                        "screenY": config["window.screenY"],
                        "screenLeft": config["window.screenX"],
                        "screenTop": config["window.screenY"],
                    },
                    "historyLength": config["window.history.length"],
                    "canvas": canvas,
                    "audio": {"apiPresent": True, "audioHash": marker_hash(f"{artifact_label}:audio")},
                    "voices": {"apiPresent": True, "voices": voice_values},
                    "mediaDevices": media,
                }
            )
            common["capabilities"] = {
                surface: {"apiPresent": True}
                for surface in ("navigator", "localeTimezone", "screenDpr", "geometry", "history", "canvas", "audio", "webgl", "webgl2", "fonts", "voices", "mediaDevices", "httpHeaders")
            }
            common["capabilities"].update(
                {
                    "privacySignals": copy.deepcopy(common["privacySignals"]),
                    "maxTouchPoints": copy.deepcopy(common["maxTouchPoints"]),
                }
            )
        else:
            worker_canvas = {"apiPresent": False, "unavailableReason": "test_not_applicable"}
            common["workerCanvas"] = worker_canvas
            common["capabilities"] = {
                "navigator": {"apiPresent": True},
                "localeTimezone": {"apiPresent": True},
                "webgl": {"apiPresent": False, "reason": "test_not_applicable"},
                "webgl2": {"apiPresent": False, "reason": "test_not_applicable"},
                "fonts": {"apiPresent": False, "reason": "test_not_applicable"},
                "privacySignals": {
                    "doNotTrack": {"apiPresent": False, "reason": "test_not_applicable"},
                    "globalPrivacyControl": {"apiPresent": False, "reason": "test_not_applicable"},
                },
                "maxTouchPoints": {"apiPresent": False, "reason": "test_not_applicable"},
                "httpHeaders": {"apiPresent": True},
                "workerCanvas": {"apiPresent": False, "reason": "test_not_applicable"},
            }
            common["webgl"] = {"apiPresent": False, "unavailableReason": "test_not_applicable"}
            common["webgl2"] = {"apiPresent": False, "unavailableReason": "test_not_applicable"}
            common["fonts"] = {"apiPresent": False, "unavailableReason": "test_not_applicable"}
        return common

    def make_session(self, artifact: dict, artifact_info: dict, label: str, artifact_label: str, *, nonce: str = "test-nonce") -> dict:
        realms = {realm: self.make_realm(artifact, realm, artifact_label=artifact_label) for realm in fp2.CANONICAL_REALMS}
        realms["service-worker"] = copy.deepcopy(realms["service-worker"])
        raw = {
            "realmOrder": list(fp2.CANONICAL_REALMS),
            "realms": realms,
            "nonceSha256": marker_hash(nonce),
            "fontInputSha256": marker_hash("fonts"),
            "bundleManifestSha256": self.manifest_sha256,
            "bundleFiles": self.manifest["files"],
            "serviceWorker": {
                "existedBefore": label == "A2",
                "scriptURLPath": "/fp2/service-worker.js",
                "scriptSha256": f"sha256:{fp2.sha256_file(fp2.BUNDLE_DIR / 'service-worker.js')}",
                "scopePath": "/fp2/",
                "activeState": "activated",
                "topController": True,
                "controlledPage": False,
                "workerResult": realms["service-worker"],
            },
            "storage": {
                "boot": {"before": 1 if label == "A2" else 0, "after": 2 if label == "A2" else 1},
                "cookie": {
                    "presentBefore": label == "A2",
                    "presentAfter": True,
                    "valueSha256": marker_hash(f"{artifact_label}:cookie"),
                },
                "localStorage": {
                    "presentBefore": label == "A2",
                    "presentAfter": True,
                    "valueSha256": marker_hash(f"{artifact_label}:local"),
                },
            },
        }
        captures = [{"realm": realm, "identityHeaders": realms[realm]["requestHeaders"]["identityHeaders"]} for realm in fp2.CANONICAL_REALMS]
        validated = fp2.validate_session_result(
            label,
            raw,
            artifact,
            self.ledger,
            captures,
            probe_manifest=self.manifest,
            probe_manifest_sha256=self.manifest_sha256,
        )
        return fp2.comparison_item(
            label=label,
            artifact_sha256=artifact_info["sha256"],
            configured_identity_digest=marker_hash(artifact_label),
            validated=validated,
            raw_result=raw,
            child={"hostPid": 1, "sessionId": label, "profileId": "fp2-a", "boot": [raw["storage"]["boot"]["before"], raw["storage"]["boot"]["after"]]},
        )

    def make_comparisons(self) -> dict[str, dict]:
        return {
            "A1": self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A"),
            "A2": self.make_session(self.artifact_a, self.artifact_a_info, "A2", "A"),
            "B1": self.make_session(self.artifact_b, self.artifact_b_info, "B1", "B"),
        }

    def test_required_realm_missing(self) -> None:
        raw = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")["rawRealms"]
        raw.pop("service-worker")
        self.assertCode("realm_matrix_incomplete", fp2.validate_session_result, "A1", {"realmOrder": list(fp2.CANONICAL_REALMS), "realms": raw}, self.artifact_a, self.ledger, [])

    def test_duplicate_realm(self) -> None:
        values = [{"realm": realm} for realm in fp2.CANONICAL_REALMS[:-1]] + [{"realm": "top-window"}]
        self.assertCode("duplicate_realm", fp2.validate_realm_key_set, values, "duplicate")

    def test_applicability_ledger_hash_mismatch(self) -> None:
        self.assertCode("applicability_hash_mismatch", fp2.validate_bound_file_hash, fp2.APPLICABILITY_PATH, "0" * 64, "applicability_hash_mismatch")

    def test_required_api_missing(self) -> None:
        raw = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")["rawRealms"]["top-window"]
        raw["capabilities"]["canvas"]["apiPresent"] = False
        self.assertCode("realm_capability_missing", fp2.validate_realm_result, "A1", "top-window", raw, self.artifact_a, self.ledger)

    def test_conditional_present_but_uncompared(self) -> None:
        raw = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")["rawRealms"]["dedicated-worker"]
        raw["capabilities"]["workerCanvas"] = {"apiPresent": True}
        raw.pop("workerCanvas")
        self.assertCode("conditional_surface_uncompared", fp2.validate_realm_result, "A1", "dedicated-worker", raw, self.artifact_a, self.ledger)

    def test_same_artifact_realm_identity_mismatch(self) -> None:
        raw = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")["rawRealms"]
        raw["same-origin-iframe"]["locale"]["utcOffsetMinutes"] = 60
        captures = [{"realm": realm, "identityHeaders": raw[realm]["requestHeaders"]["identityHeaders"]} for realm in fp2.CANONICAL_REALMS]
        payload = {"realmOrder": list(fp2.CANONICAL_REALMS), "realms": raw, "serviceWorker": {"scriptURLPath": "/fp2/service-worker.js", "scriptSha256": marker_hash("service-worker-script"), "scopePath": "/fp2/", "activeState": "activated"}, "storage": {}}
        self.assertCode("cross_realm_identity_mismatch", fp2.validate_session_result, "A1", payload, self.artifact_a, self.ledger, captures)

    def test_context_fields_not_required_equal_across_realms(self) -> None:
        session = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")
        session["rawRealms"]["same-origin-iframe"]["geometry"]["innerWidth"] += 1
        # The pure cross-realm identity comparison intentionally excludes geometry.
        left = fp2.identity_projection(session["rawRealms"]["top-window"], "top-window", self.ledger)
        right = fp2.identity_projection(session["rawRealms"]["same-origin-iframe"], "same-origin-iframe", self.ledger)
        self.assertEqual(left["navigator"], right["navigator"])

    def test_identity_field_cannot_be_excluded(self) -> None:
        session = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")
        session["identityProjection"]["same-origin-iframe"]["navigator"]["platform"] = "Other"
        self.assertCode("a1_a2_identity_mismatch", fp2.compare_session_pair, "A1/A2", session, self.make_session(self.artifact_a, self.artifact_a_info, "A2", "A"), True)

    def test_worker_hardware_concurrency_mismatch(self) -> None:
        comparisons = self.make_comparisons()
        comparisons["A2"]["identityProjection"]["dedicated-worker"]["navigator"]["hardwareConcurrency"] = 999
        self.assertCode("a1_a2_identity_mismatch", fp2.compare_session_pair, "A1/A2", comparisons["A1"], comparisons["A2"], True)

    def test_http_user_agent_mismatch(self) -> None:
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["requestHeaders"]["identityHeaders"]["user-agent"] = "wrong"
        self.assertCode("header_js_mismatch", fp2.validate_header_coherence, "A1", "top-window", realm, self.artifact_a)

    def test_accept_language_mismatch(self) -> None:
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["requestHeaders"]["identityHeaders"]["accept-language"] = "en-US,fr"
        self.assertCode("accept_language_mismatch", fp2.validate_header_coherence, "A1", "top-window", realm, self.artifact_a)

    def test_accept_encoding_mismatch(self) -> None:
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["requestHeaders"]["identityHeaders"]["accept-encoding"] = "gzip"
        self.assertCode("accept_encoding_mismatch", fp2.validate_header_coherence, "A1", "top-window", realm, self.artifact_a)

    def test_dnt_gpc_mapping_mismatch(self) -> None:
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["requestHeaders"]["identityHeaders"]["dnt"] = "0"
        self.assertCode("dnt_mapping_mismatch", fp2.validate_header_coherence, "A1", "top-window", realm, self.artifact_a)
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["requestHeaders"]["identityHeaders"]["sec-gpc"] = "0"
        self.assertCode("gpc_mapping_mismatch", fp2.validate_header_coherence, "A1", "top-window", realm, self.artifact_a)

    def test_cross_origin_nonce_is_fail_closed(self) -> None:
        owner = object.__new__(fp2.FP2HTTPServer)
        owner._lock = fp2.Lock()
        owner.active_label = "A1"
        owner.active_nonce = "fresh-nonce"
        owner.captures = []
        headers = {"User-Agent": "ua", "Accept-Language": "en-US,en", "Accept-Encoding": "gzip", "DNT": "1", "Sec-GPC": "1", "X-FP2-Realm": "cross-origin-iframe", "X-FP2-Nonce": "old-nonce"}
        self.assertCode(
            "cross_origin_nonce_mismatch",
            owner.record_header_request,
            method="GET",
            path="/fp2/header-observation",
            headers=headers,
            realm="cross-origin-iframe",
            nonce="old-nonce",
        )

    def test_shared_worker_old_session_evidence_is_rejected(self) -> None:
        owner = object.__new__(fp2.FP2HTTPServer)
        owner._lock = fp2.Lock()
        owner.active_label = "A1"
        owner.active_nonce = "fresh-nonce"
        owner.captures = [{"realm": "shared-worker"}]
        headers = {"User-Agent": "ua", "Accept-Language": "en-US,en", "Accept-Encoding": "gzip", "DNT": "1", "Sec-GPC": "1", "X-FP2-Realm": "shared-worker", "X-FP2-Nonce": "fresh-nonce"}
        self.assertCode(
            "duplicate_header_observation",
            owner.record_header_request,
            method="GET",
            path="/fp2/header-observation",
            headers=headers,
            realm="shared-worker",
            nonce="fresh-nonce",
        )

    def test_real_loopback_handler_captures_http_evidence(self) -> None:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.bind((fp2.PRIMARY_HOST, 0))
            port = probe.getsockname()[1]
        server = fp2.FP2HTTPServer(fp2.BUNDLE_DIR, port, set())
        nonce = "loopback-handler-test-nonce"
        headers = {
            "User-Agent": "fp2-loopback-test-agent",
            "Accept-Language": "en-US,en",
            "Accept-Encoding": "gzip",
            "DNT": "1",
            "Sec-GPC": "1",
            "X-FP2-Realm": "top-window",
            "X-FP2-Nonce": nonce,
        }
        try:
            server.begin_session("A1", nonce)
            query = urlencode({"realm": "top-window", "nonce": nonce})
            request = Request(f"{server.primary_origin}/fp2/header-observation?{query}", headers=headers, method="GET")
            with urlopen(request, timeout=3) as response:
                self.assertEqual(response.status, 200)
                payload = json.load(response)
            self.assertTrue(payload["ok"])
            captures = server.take_captures()
            self.assertEqual(len(captures), 1)
            capture = captures[0]
            self.assertEqual(capture["method"], "GET")
            self.assertEqual(capture["path"], "fp2/header-observation")
            self.assertEqual(capture["realm"], "top-window")
            self.assertEqual(capture["identityHeaders"]["user-agent"], headers["User-Agent"])
            self.assertEqual(capture["identityHeaders"]["accept-language"], headers["Accept-Language"])
            self.assertEqual(capture["identityHeaders"]["accept-encoding"], headers["Accept-Encoding"])
            self.assertTrue(capture["customRealmHeaderMatches"])
            self.assertTrue(capture["customNonceHeaderMatches"])
            self.assertFalse(capture["cookiePresent"])
            fp2.ensure_sanitized(capture, "real-loopback-capture")
        finally:
            server.close()

    def test_real_loopback_handler_malformed_realm_fails_closed(self) -> None:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.bind((fp2.PRIMARY_HOST, 0))
            port = probe.getsockname()[1]
        server = fp2.FP2HTTPServer(fp2.BUNDLE_DIR, port, set())
        nonce = "loopback-malformed-test-nonce"
        try:
            server.begin_session("A1", nonce)
            query = urlencode({"realm": "not-a-realm", "nonce": nonce})
            request = Request(f"{server.primary_origin}/fp2/header-observation?{query}", method="GET")
            with self.assertRaises(HTTPError) as context:
                urlopen(request, timeout=3)
            self.assertEqual(context.exception.code, 409)
            self.assertEqual(json.load(context.exception), {"ok": False, "error": "header_realm_invalid"})
            self.assertEqual(server.take_captures(), [])
        finally:
            server.close()

    def test_loopback_server_shutdown_releases_port(self) -> None:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            probe.bind((fp2.PRIMARY_HOST, 0))
            port = probe.getsockname()[1]
        server = fp2.FP2HTTPServer(fp2.BUNDLE_DIR, port, set())
        server.close()
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as rebound:
            rebound.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
            rebound.bind((fp2.PRIMARY_HOST, port))

    def test_missing_header_capture_fails_closed(self) -> None:
        session = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")
        raw = {"realmOrder": list(fp2.CANONICAL_REALMS), "realms": session["rawRealms"]}
        self.assertCode("header_capture_matrix_incomplete", fp2.validate_session_result, "A1", raw, self.artifact_a, self.ledger, [])

    def test_probe_failure_fields_preserve_each_collector_stage(self) -> None:
        cases = {
            "canvas": "identityCanvas",
            "audio": "audioSnapshot",
            "webgl": "webglSnapshot",
            "headerRequest": "observeHeaders",
        }
        for stage, operation in cases.items():
            with self.subTest(stage=stage):
                failure = fp2.normalize_probe_failure(
                    self.probe_failure(stage=stage, operation=operation),
                )
                safe = fp2.safe_failure({"code": "realm_probe_failed", "detail": "synthetic", **failure})
                self.assertEqual(safe["stage"], stage)
                self.assertEqual(safe["operation"], operation)
                self.assertEqual(safe["errorMessage"], "synthetic failure")
                self.assertFalse(safe["probeCompleted"])

    def test_probe_failure_metadata_is_fail_closed_when_incomplete(self) -> None:
        failure = self.probe_failure(stage="canvas", operation="identityCanvas")
        del failure["operation"]
        self.assertCode("realm_probe_failure_invalid", fp2.normalize_probe_failure, failure)

    def test_probe_failure_message_is_bounded_and_sanitized(self) -> None:
        secret = self.probe_failure(
            stage="canvas",
            operation="identityCanvas",
            message=r"C:\Users\qiu\token.txt token=abc https://example.invalid/private",
        )
        normalized = fp2.normalize_probe_failure(secret)
        self.assertEqual(normalized["errorMessage"], "<redacted>")
        self.assertNotIn("C:\\Users", normalized["errorMessage"])
        path_only = self.probe_failure(
            stage="audio",
            operation="audioSnapshot",
            message=r"C:\Users\qiu\file.txt https://example.invalid/path",
        )
        path_normalized = fp2.normalize_probe_failure(path_only)
        self.assertNotIn("C:\\Users", path_normalized["errorMessage"])
        self.assertNotIn("https://", path_normalized["errorMessage"])
        long_message = self.probe_failure(
            stage="audio",
            operation="audioSnapshot",
            message="x" * (fp2.PROBE_FAILURE_TEXT_LIMIT + 50),
        )
        bounded = fp2.normalize_probe_failure(long_message)
        self.assertEqual(len(bounded["errorMessage"]), fp2.PROBE_FAILURE_TEXT_LIMIT)
        fp2.ensure_sanitized(bounded, "probe-failure")

    def test_child_evidence_retains_structured_probe_failure_and_stage_trace(self) -> None:
        host = object.__new__(fp2.FP2ManagedHost)
        host.session = None
        host.fp2_bundle_manifest_sha256 = "a" * 64
        host.fp2_nonce = "synthetic-nonce"
        host.fp2_stages = [
            {"stage": "navigator", "status": "success", "elapsedSeconds": 0.001},
            {"stage": "canvas", "status": "error", "elapsedSeconds": 0.002},
        ]
        failure = {"code": "realm_probe_failed", "detail": "synthetic", **self.probe_failure(stage="canvas", operation="identityCanvas")}
        summary = fp2.child_result_summary(
            "A1",
            host,
            None,
            None,
            None,
            failure,
            Path("raw-realms.json"),
        )
        self.assertEqual(summary["failure"]["realm"], "top-window")
        self.assertEqual(summary["failure"]["stage"], "canvas")
        self.assertEqual(summary["failure"]["operation"], "identityCanvas")
        self.assertEqual(summary["stageTrace"][-1]["status"], "error")

    def test_protocol_error_mapping_does_not_require_detail_attribute(self) -> None:
        error = fp2.host_module.ProtocolError(
            "realm_probe_failed",
            "top-window:service-worker:serviceWorkerEvidence:Error: service_worker_redundant",
        )
        mapped = fp2.protocol_error_failure(error)
        self.assertEqual(mapped["code"], "realm_probe_failed")
        self.assertIn("service_worker_redundant", mapped["detail"])
        self.assertNotIn("AttributeError", mapped["detail"])

    def test_js_failure_observability_and_top_probe_propagation(self) -> None:
        script = r'''
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const root = process.cwd();
const context = {
  URLSearchParams,
  Promise,
  Error,
  TextEncoder,
  setTimeout,
  clearTimeout,
  console,
  document: { getElementById: () => ({ textContent: "" }) },
  window: {
    location: { search: "?nonce=synthetic-nonce-123456", origin: "http://127.0.0.1", port: "18192" },
    addEventListener() {},
    removeEventListener() {},
  },
};
context.globalThis = context;
vm.createContext(context);
vm.runInContext(fs.readFileSync(path.join(root, "tests/fingerprint-probe/fp2/realm-common.js"), "utf8"), context);
(async () => {
  const stageResults = {};
  for (const [stage, operation] of [["canvas", "identityCanvas"], ["audio", "audioSnapshot"], ["webgl", "webglSnapshot"], ["headerRequest", "observeHeaders"]]) {
    const tracker = context.FP2Realm.createStageTracker("top-window");
    try {
      await context.FP2Realm.observeStage(tracker, stage, operation, () => { throw new Error(`${stage}_synthetic`); });
    } catch (error) {
      stageResults[stage] = error.__fp2Failure;
    }
  }
  const successValue = { unchanged: true };
  const successTracker = context.FP2Realm.createStageTracker("top-window");
  const success = await context.FP2Realm.observeStage(successTracker, "finalize", "identity", () => successValue);
  context.FP2Realm.collectWindowRealm = async () => { throw new Error("abc"); };
  vm.runInContext(fs.readFileSync(path.join(root, "tests/fingerprint-probe/fp2/top.js"), "utf8"), context);
  context.window.__fp2ProvideInput({ fonts: [], fontInputSha256: "sha256:font", bundleManifestSha256: "sha256:bundle", bundleFiles: [] });
  await new Promise((resolve) => setTimeout(resolve, 30));
  process.stdout.write(JSON.stringify({ stageResults, successSameObject: success === successValue, topFailure: context.window.__fp2Error }));
})().catch((error) => { process.stderr.write(String(error)); process.exitCode = 1; });
'''
        result = subprocess.run(
            ["node", "-e", script],
            cwd=fp2.REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertTrue(payload["successSameObject"])
        for stage, operation in {
            "canvas": "identityCanvas",
            "audio": "audioSnapshot",
            "webgl": "webglSnapshot",
            "headerRequest": "observeHeaders",
        }.items():
            failure = payload["stageResults"][stage]
            self.assertEqual(failure["stage"], stage)
            self.assertEqual(failure["operation"], operation)
            self.assertEqual(failure["errorMessage"], f"{stage}_synthetic")
            self.assertFalse(failure["probeCompleted"])
        top_failure = payload["topFailure"]
        self.assertEqual(top_failure["realm"], "top-window")
        self.assertEqual(top_failure["stage"], "top-window")
        self.assertEqual(top_failure["operation"], "collectWindowRealm")
        self.assertEqual(top_failure["errorMessage"], "abc")
        self.assertFalse(top_failure["probeCompleted"])

    def test_service_worker_activation_state_machine_is_event_driven(self) -> None:
        script = r'''
const fs = require("fs");
const path = require("path");
const vm = require("vm");
const { performance } = require("perf_hooks");
const root = process.cwd();
const context = {
  Promise,
  Error,
  TextEncoder,
  setTimeout,
  clearTimeout,
  performance,
  console,
};
context.globalThis = context;
vm.createContext(context);
vm.runInContext(fs.readFileSync(path.join(root, "tests/fingerprint-probe/fp2/realm-common.js"), "utf8"), context);

class FakeWorker {
  constructor(state, onAdd = null) {
    this.state = state;
    this.listeners = new Set();
    this.onAdd = onAdd;
  }

  addEventListener(type, listener) {
    if (type !== "statechange") throw new Error("unexpected_event");
    this.listeners.add(listener);
    if (this.onAdd) {
      const callback = this.onAdd;
      this.onAdd = null;
      callback(this);
    }
  }

  removeEventListener(type, listener) {
    if (type === "statechange") this.listeners.delete(listener);
  }

  setState(state) {
    this.state = state;
    for (const listener of [...this.listeners]) listener();
  }
}

async function capture(factory) {
  try {
    return { ok: true, value: await factory() };
  } catch (error) {
    return {
      ok: false,
      name: error.name,
      message: error.message,
      failure: error.__fp2Failure || null,
    };
  }
}

(async () => {
  const immediate = await context.FP2Realm.waitForServiceWorkerActivation(
    new FakeWorker("activated"),
    context.FP2Realm.deadlineFromNow(100),
  );

  const transitioningWorker = new FakeWorker("activating");
  setTimeout(() => transitioningWorker.setState("activated"), 5);
  const transitioning = await context.FP2Realm.waitForServiceWorkerActivation(
    transitioningWorker,
    context.FP2Realm.deadlineFromNow(100),
  );

  const listenerRace = await context.FP2Realm.waitForServiceWorkerActivation(
    new FakeWorker("activating", (worker) => {
      // Transition exactly during listener installation; the helper's second
      // state read must observe this even if the event itself was missed.
      worker.state = "activated";
    }),
    context.FP2Realm.deadlineFromNow(100),
  );

  const controller = null;
  const activatedWithoutController = await context.FP2Realm.waitForServiceWorkerActivation(
    new FakeWorker("activated"),
    context.FP2Realm.deadlineFromNow(100),
  );

  const redundantWorker = new FakeWorker("activating");
  const redundantPromise = context.FP2Realm.waitForServiceWorkerActivation(
    redundantWorker,
    context.FP2Realm.deadlineFromNow(100),
  );
  setTimeout(() => redundantWorker.setState("redundant"), 5);
  const redundantTransition = await capture(() => redundantPromise);

  const timeout = await capture(() =>
    context.FP2Realm.waitForServiceWorkerActivation(
      new FakeWorker("activating"),
      context.FP2Realm.deadlineFromNow(15),
    ),
  );

  const unexpected = await capture(() =>
    context.FP2Realm.waitForServiceWorkerActivation(
      new FakeWorker("installed"),
      context.FP2Realm.deadlineFromNow(100),
    ),
  );

  const tracker = context.FP2Realm.createStageTracker("top-window");
  const propagated = await capture(() =>
    context.FP2Realm.observeStage(
      tracker,
      "service-worker",
      "serviceWorkerEvidence",
      () =>
        context.FP2Realm.waitForServiceWorkerActivation(
          new FakeWorker("activating"),
          context.FP2Realm.deadlineFromNow(15),
        ),
    ),
  );

  process.stdout.write(JSON.stringify({
    immediate,
    transitioning,
    listenerRace,
    controller,
    activatedWithoutController,
    redundantTransition,
    timeout,
    unexpected,
    propagated,
  }));
})().catch((error) => {
  process.stderr.write(String(error));
  process.exitCode = 1;
});
'''
        result = subprocess.run(
            ["node", "-e", script],
            cwd=fp2.REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertEqual(payload["immediate"], "activated")
        self.assertEqual(payload["transitioning"], "activated")
        self.assertEqual(payload["listenerRace"], "activated")
        self.assertIsNone(payload["controller"])
        self.assertEqual(payload["activatedWithoutController"], "activated")
        redundant = payload["redundantTransition"]
        self.assertFalse(redundant["ok"])
        self.assertEqual(redundant["name"], "ServiceWorkerLifecycleError")
        self.assertIn("service_worker_redundant:redundant", redundant["message"])
        timeout = payload["timeout"]
        self.assertFalse(timeout["ok"])
        self.assertEqual(timeout["name"], "ServiceWorkerActivationTimeout")
        self.assertEqual(timeout["message"], "service_worker_activation_timeout:activating")
        unexpected = payload["unexpected"]
        self.assertFalse(unexpected["ok"])
        self.assertIn("service_worker_unexpected_state:installed", unexpected["message"])
        propagated = payload["propagated"]
        self.assertFalse(propagated["ok"])
        self.assertEqual(propagated["failure"]["stage"], "service-worker")
        self.assertEqual(propagated["failure"]["operation"], "serviceWorkerEvidence")
        self.assertEqual(propagated["failure"]["errorName"], "ServiceWorkerActivationTimeout")
        self.assertEqual(
            propagated["failure"]["errorMessage"],
            "service_worker_activation_timeout:activating",
        )

    def test_service_worker_activation_deadline_is_wired_without_timeout_change(self) -> None:
        top_source = (fp2.BUNDLE_DIR / "top.js").read_text(encoding="utf-8")
        runner_source = (fp2.HOST_DIR / "fp2_cross_realm.py").read_text(encoding="utf-8")
        self.assertEqual(fp2.REALM_STAGE_DEADLINE_SECONDS, 15)
        self.assertIn(
            "serviceWorkerEvidence(input.serviceWorkerActivationDeadlineMs)",
            top_source,
        )
        self.assertIn(
            '"serviceWorkerActivationDeadlineMs": REALM_STAGE_DEADLINE_SECONDS * 1000',
            runner_source,
        )
        self.assertNotIn("service_worker_not_activated", top_source)
        self.assertNotIn('activeState === "activated"', top_source)

    def test_structured_failure_is_closed_by_report_adjudication_and_byte_receipt(self) -> None:
        failure = {"code": "realm_probe_failed", "detail": "synthetic", **self.probe_failure(stage="headerRequest", operation="observeHeaders", message="abc")}
        with tempfile.TemporaryDirectory(prefix="fp2-observability-finalization-") as folder:
            root = Path(folder)
            artifacts = fp2.finalize_report_artifacts(
                run_dir=root,
                report={"schema": fp2.REPORT_SCHEMA, "status": "failed", "verified": False, "failure": failure},
                conclusion="failed",
                checks={"verified": False, "syntheticFailure": True},
            )
            report = json.loads(artifacts["reportPath"].read_text(encoding="utf-8"))
            adjudication = json.loads(artifacts["adjudicationPath"].read_text(encoding="utf-8"))
            self.assertEqual(report["failure"]["stage"], "headerRequest")
            self.assertEqual(report["failure"]["operation"], "observeHeaders")
            self.assertEqual(adjudication["failure"]["errorMessage"], "abc")
            fp2.validate_hash_sidecar(artifacts["reportPath"], root / "run-report.sha256")
            fp2.validate_hash_sidecar(artifacts["adjudicationPath"], root / "final-offline-adjudication.sha256")
            fp2.validate_hash_sidecar(artifacts["closurePath"], root / "byte-closure-receipt.sha256")

    def test_generation4_preserves_all_historical_attempts(self) -> None:
        attempts = fp2.previous_execution_attempts()
        self.assertEqual(set(attempts), {"generation1", "generation2", "generation3"})
        self.assertEqual(attempts["generation1"]["claimSha256"], fp2.PREVIOUS_BLOCKED_CLAIM_SHA256)
        self.assertEqual(attempts["generation2"]["claimSha256"], fp2.GENERATION2_CLAIM_SHA256)
        self.assertEqual(attempts["generation3"]["claimSha256"], fp2.GENERATION3_CLAIM_SHA256)
        self.assertTrue(attempts["generation2"]["browserLaunched"])
        self.assertEqual(attempts["generation2"]["validRealmObservations"], 0)
        self.assertEqual(attempts["generation2"]["headerCaptureCount"], 0)
        self.assertTrue(attempts["generation3"]["browserLaunched"])
        self.assertEqual(attempts["generation3"]["validRealmObservations"], 0)
        self.assertEqual(attempts["generation3"]["headerCaptureCount"], 0)
        self.assertEqual(attempts["generation3"]["classification"], "gen3_failure_evidence_insufficient")

    def test_generation4_package_metadata_and_bindings(self) -> None:
        self.assertEqual(fp2.EXECUTION_GENERATION, 4)
        self.assertEqual(fp2.TASK_VERSION, "fp2-v4")
        self.assertEqual(fp2.GLOBAL_CLAIM_PATH.name, "fp2-v4-one-shot-claim.json")
        self.assertEqual(fp2.GENERATION3_CLAIM_PATH.name, "fp2-v3-one-shot-claim.json")
        self.assertNotEqual(fp2.GLOBAL_CLAIM_PATH, fp2.GENERATION3_CLAIM_PATH)
        self.assertEqual(fp2.CLAIM_SCHEMA, "verisilo-camoufox-fp2-one-shot-claim/v4")
        self.assertEqual(self.manifest_sha256, "d69e61c4da482c8cebaed912a6c24b57b73ac0c465a9fafd6a0be8dc974cfb37")
        self.assertEqual(fp2.sha256_file(fp2.APPLICABILITY_PATH), "f6d51d4e3234fec9677e65c996933131519c96fa4c739890bc67a249cca2ef63")
        self.assertEqual(fp2.sha256_file(fp2.RELATION_PATH), "4741c0c443c4ac3032e634a3e4b7892820843c19b5b6574dbe2973dfb79e9342")

    def test_generation_history_hash_mismatch_fails_closed(self) -> None:
        cases = (
            ("LEGACY_CLAIM_PATH", "previous_blocked_attempt", "previous_blocked_claim_hash_mismatch"),
            ("GENERATION2_CLAIM_PATH", "previous_generation2_attempt", "previous_generation2_claim_hash_mismatch"),
            ("GENERATION3_CLAIM_PATH", "previous_generation3_attempt", "previous_generation3_claim_hash_mismatch"),
        )
        for path_name, function_name, expected_code in cases:
            with self.subTest(path=path_name):
                with tempfile.TemporaryDirectory(prefix="fp2-history-mismatch-") as folder:
                    tampered = Path(folder) / "claim.json"
                    tampered.write_bytes(Path(getattr(fp2, path_name)).read_bytes() + b"\n")
                    with patch.object(fp2, path_name, tampered):
                        self.assertCode(expected_code, getattr(fp2, function_name))

    def test_generation4_claim_path_rejects_second_claim(self) -> None:
        result = self.runtime_preflight_result()
        previous = fp2.previous_execution_attempts()
        with tempfile.TemporaryDirectory(dir=fp2.FP2_EVIDENCE_ROOT) as folder:
            root = Path(folder)
            receipt = root / "runtime-preflight-receipt.json"
            receipt.write_text("{}\n", encoding="utf-8")
            result["receiptPath"] = fp2.relative_repo_path(receipt)
            result["receiptSha256"] = fp2.sha256_file(receipt)
            run_dir = root / "run"
            run_dir.mkdir()
            claim_path = root / fp2.GLOBAL_CLAIM_PATH.name
            claim_path.write_text("existing-gen4-claim\n", encoding="utf-8")
            original = claim_path.read_bytes()
            with patch.object(fp2, "GLOBAL_CLAIM_PATH", claim_path):
                self.assertCode(
                    "one_shot_claim_already_exists",
                    fp2.create_claim,
                    run_id="test-generation4-duplicate",
                    run_dir=run_dir,
                    port=fp2.DEFAULT_RUN_PORT,
                    git={"branch": "test", "head": "0" * 40, "tree": "0" * 40, "upstream": {}, "trackedWorktreeClean": True},
                    candidate={},
                    artifacts={},
                    probe_manifest_sha256="0" * 64,
                    applicability_sha256="0" * 64,
                    relation_sha256="0" * 64,
                    static_diff_sha256="0" * 64,
                    no_browser_test_sha256="0" * 64,
                    runtime_preflight=result,
                    previous_blocked_attempt=previous,
                )
            self.assertEqual(claim_path.read_bytes(), original)

    def test_service_worker_script_scope_mismatch(self) -> None:
        raw = self.make_session(self.artifact_a, self.artifact_a_info, "A1", "A")["rawRealms"]
        payload = {"realmOrder": list(fp2.CANONICAL_REALMS), "realms": raw, "serviceWorker": {"scriptURLPath": "/wrong.js", "scriptSha256": marker_hash("service-worker-script"), "scopePath": "/fp2/", "activeState": "activated"}, "storage": {}}
        captures = [{"realm": realm, "identityHeaders": raw[realm]["requestHeaders"]["identityHeaders"]} for realm in fp2.CANONICAL_REALMS]
        self.assertCode("service_worker_script_url_mismatch", fp2.validate_session_result, "A1", payload, self.artifact_a, self.ledger, captures)

    def test_b_profile_storage_inheritance(self) -> None:
        comparisons = self.make_comparisons()
        comparisons["B1"]["storage"]["cookiePresentBefore"] = True
        self.assertCode("b1_profile_inherits_a_storage", fp2.validate_storage_sequence, comparisons)

    def test_a1_a2_capability_shape_drift(self) -> None:
        comparisons = self.make_comparisons()
        comparisons["A2"]["capabilityShape"]["top-window"]["canvas"]["apiPresent"] = False
        self.assertCode("realm_capability_shape_drift", fp2.compare_session_pair, "A1/A2", comparisons["A1"], comparisons["A2"], True)

    def test_ab_common_field_drift(self) -> None:
        comparisons = self.make_comparisons()
        comparisons["B1"]["identityProjection"]["top-window"]["localeTimezone"]["timeZone"] = "Other/Zone"
        self.assertCode("ab_common_identity_mismatch", fp2.compare_ab, comparisons["A1"], comparisons["A2"], comparisons["B1"], self.artifact_a, self.artifact_b, self.ledger, self.relation)

    def test_full_b_canvas_raw_unexplained_drift(self) -> None:
        raw_a = {"canvas": {"rawHash": marker_hash("a"), "rawRgbaHash": marker_hash("a"), "decodedPngPixelsHash": marker_hash("a")}, "fonts": {"injectedFonts": [], "fontUniverseWidths": {}}}
        raw_b = copy.deepcopy(raw_a)
        raw_b["canvas"]["rawHash"] = marker_hash("b")
        raw_b["canvas"]["rawRgbaHash"] = marker_hash("b")
        raw_b["canvas"]["decodedPngPixelsHash"] = marker_hash("b")
        self.assertCode("full_b_canvas_unexplained_raw_drift", fp2.validate_full_b_canvas_raw_relation, raw_a, raw_b, "top-window")

    def test_png_invalid(self) -> None:
        realm = self.make_realm(self.artifact_a, "top-window", artifact_label="A")
        realm["canvas"]["png"]["signatureValid"] = False
        self.assertCode("png_invalid", fp2.validate_png_surface, "A1", "top-window", realm, "required")

    def test_report_claim_reference_hash_mismatch(self) -> None:
        with tempfile.TemporaryDirectory(prefix="fp2-test-") as folder:
            root = Path(folder)
            path = root / "evidence.json"
            path.write_text("{}\n", encoding="utf-8")
            sidecar = root / "evidence.sha256"
            sidecar.write_text("0" * 64 + "  evidence.json\n", encoding="ascii")
            self.assertCode("evidence_integrity_mismatch", fp2.validate_hash_sidecar, path, sidecar)
            reference = {"path": "evidence.json", "size": path.stat().st_size, "sha256": "0" * 64}
            self.assertCode("evidence_reference_hash_mismatch", fp2.validate_file_reference, root, reference)

    def test_lifecycle_job_lock_failure(self) -> None:
        close = {"state": "exited", "exitStatus": 0, "exitFileObserved": True, "processTreeExit": {"exited": True, "remaining": [], "job": {"activeProcessCount": 1}}, "closeOutcome": {"status": "success", "forcedJobCleanup": {"status": "not_needed"}}}
        self.assertCode("lifecycle_unclean", fp2.validate_close_receipt, close, "A1")

    def test_secret_path_sentinel(self) -> None:
        self.assertCode("secret_path_sentinel_leak", fp2.ensure_sanitized, "C:\\Users\\example\\token.txt", "sanitized")

    def test_reauthorization_reason_is_not_a_secret_sentinel(self) -> None:
        fp2.ensure_sanitized({"reasonForReauthorization": fp2.PREVIOUS_BLOCKED_REASON}, "preflight")

    def test_tasklist_access_denied_uses_independent_fallback(self) -> None:
        with patch.object(fp2, "_enumerate_tasklist_processes", side_effect=fp2.FP2Failure("process_scan_backend_unavailable", "tasklist.exe")), patch.object(
            fp2, "_enumerate_powershell_processes", return_value=[]
        ):
            self.assertEqual(fp2.target_processes(), [])

    def test_process_fallback_target_is_not_treated_as_empty(self) -> None:
        target = [{"imageName": "camoufox.exe", "pid": 4242}]
        with patch.object(fp2, "_enumerate_tasklist_processes", side_effect=fp2.FP2Failure("process_scan_backend_unavailable", "tasklist.exe")), patch.object(
            fp2, "_enumerate_powershell_processes", return_value=target
        ):
            self.assertEqual(fp2.target_processes(), target)
            self.assertCode("target_processes_present", fp2.require_no_target_processes, "fallback target")

    def test_all_process_enumeration_backends_unavailable_is_blocked(self) -> None:
        with patch.object(fp2, "_enumerate_tasklist_processes", side_effect=fp2.FP2Failure("process_scan_backend_unavailable", "tasklist.exe")), patch.object(
            fp2, "_enumerate_powershell_processes", side_effect=fp2.FP2Failure("process_scan_backend_unavailable", "powershell")
        ):
            self.assertCode("process_cleanliness_unverifiable", fp2.target_processes)

    def test_runtime_preflight_source_precedes_claim_creation(self) -> None:
        source = inspect.getsource(fp2.orchestrate)
        self.assertLess(source.index("run_runtime_preflight("), source.index("create_claim("))
        self.assertEqual(source.count("run_runtime_preflight("), 1)
        self.assertIn("generation-4", source)

    def runtime_preflight_result(self, *, boundary: dict | None = None) -> dict:
        interpreter = fp2.resolve_runtime_interpreter()
        browser_boundary = boundary or {
            "ready": True,
            "browserLaunchCalled": False,
            "nextCall": "AsyncNewBrowser(playwright, from_options=opts, persistent_context=True)",
        }
        dependencies = {
            "external": {
                name: {"available": True, "version": version}
                for name, version in fp2.EXPECTED_RUNTIME_DEPENDENCY_VERSIONS.items()
            },
            "project": {name: {"available": True} for name in ("host_v1", "browser_asset", "host_platform", "run_spike")},
            "browserSpawnBoundary": browser_boundary,
        }
        invocation = fp2.runtime_invocation_descriptor(interpreter)
        closure = {
            "interpreterSha256": fp2.sha256_file(interpreter),
            "pythonVersion": fp2.EXPECTED_RUNTIME_PYTHON_VERSION,
            "implementation": fp2.EXPECTED_RUNTIME_IMPLEMENTATION,
            "dependencies": dependencies,
            "childInvocation": invocation,
        }
        binding = {
            "interpreterRelativePath": invocation["interpreterRelativePath"],
            "interpreterSha256": closure["interpreterSha256"],
            "pythonVersion": closure["pythonVersion"],
            "implementation": closure["implementation"],
            "dependencyClosureSha256": fp2.runtime_dependency_closure_sha256(closure),
            "childInvocationSha256": hashlib.sha256(fp2.canonical_json_bytes(invocation)).hexdigest(),
        }
        return {
            "schema": fp2.RUNTIME_PREFLIGHT_CHILD_SCHEMA,
            "status": "passed",
            "verified": False,
            "runtimeBinding": binding,
            "dependencyClosure": closure,
            "browserSpawnBoundary": browser_boundary,
            "claimCreationAllowed": True,
        }

    def test_runtime_missing_dependency_blocks_preflight(self) -> None:
        interpreter = fp2.resolve_runtime_interpreter()
        for module_name in ("camoufox", "playwright", "browserforge"):
            with self.subTest(module=module_name):
                result = {
                    "schema": fp2.RUNTIME_PREFLIGHT_CHILD_SCHEMA,
                    "status": "blocked",
                    "verified": False,
                    "failure": {"code": "runtime_dependency_missing", "detail": module_name},
                }
                self.assertCode("runtime_dependency_missing", fp2.validate_runtime_preflight_result, result, interpreter)

    def test_child_interpreter_mismatch_blocks_preflight(self) -> None:
        result = self.runtime_preflight_result()
        result["runtimeBinding"]["interpreterRelativePath"] = "other/python.exe"
        self.assertCode("runtime_interpreter_mismatch", fp2.validate_runtime_preflight_result, result, fp2.resolve_runtime_interpreter())

    def test_browser_spawn_boundary_failure_blocks_preflight(self) -> None:
        result = self.runtime_preflight_result(boundary={"ready": False, "browserLaunchCalled": False})
        self.assertCode("runtime_browser_spawn_boundary_unavailable", fp2.validate_runtime_preflight_result, result, fp2.resolve_runtime_interpreter())

    def test_blocked_preflight_cannot_create_claim(self) -> None:
        with tempfile.TemporaryDirectory(dir=fp2.FP2_EVIDENCE_ROOT) as folder:
            root = Path(folder)
            claim_path = root / "claim.json"
            blocked = {"status": "blocked", "claimCreationAllowed": False}
            with patch.object(fp2, "GLOBAL_CLAIM_PATH", claim_path):
                self.assertCode(
                    "runtime_preflight_required",
                    fp2.create_claim,
                    run_id="test-blocked",
                    run_dir=root,
                    port=fp2.DEFAULT_RUN_PORT,
                    git={"branch": "test", "head": "0" * 40, "tree": "0" * 40, "upstream": {}, "trackedWorktreeClean": True},
                    candidate={},
                    artifacts={},
                    probe_manifest_sha256="0" * 64,
                    applicability_sha256="0" * 64,
                    relation_sha256="0" * 64,
                    static_diff_sha256="0" * 64,
                    no_browser_test_sha256="0" * 64,
                    runtime_preflight=blocked,
                    previous_blocked_attempt={},
                )
            self.assertFalse(claim_path.exists())

    def test_successful_preflight_is_bound_before_claim_creation(self) -> None:
        result = self.runtime_preflight_result()
        historical_claims = {
            path: fp2.sha256_file(path)
            for path in (fp2.LEGACY_CLAIM_PATH, fp2.GENERATION2_CLAIM_PATH, fp2.GENERATION3_CLAIM_PATH)
        }
        with tempfile.TemporaryDirectory(dir=fp2.FP2_EVIDENCE_ROOT) as folder:
            root = Path(folder)
            receipt = root / "runtime-preflight-receipt.json"
            receipt.write_text("{}\n", encoding="utf-8")
            result["receiptPath"] = fp2.relative_repo_path(receipt)
            result["receiptSha256"] = fp2.sha256_file(receipt)
            claim_path = root / "claim.json"
            previous = fp2.previous_execution_attempts()
            with patch.object(fp2, "GLOBAL_CLAIM_PATH", claim_path):
                claim, _ = fp2.create_claim(
                    run_id="test-success",
                    run_dir=root,
                    port=fp2.DEFAULT_RUN_PORT,
                    git={"branch": "test", "head": "0" * 40, "tree": "0" * 40, "upstream": {}, "trackedWorktreeClean": True},
                    candidate={},
                    artifacts={},
                    probe_manifest_sha256="0" * 64,
                    applicability_sha256="0" * 64,
                    relation_sha256="0" * 64,
                    static_diff_sha256="0" * 64,
                    no_browser_test_sha256="0" * 64,
                    runtime_preflight=result,
                    previous_blocked_attempt=previous,
                )
            self.assertTrue(claim_path.exists())
            self.assertEqual(claim["executionGeneration"], 4)
            self.assertEqual(claim["taskVersion"], "fp2-v4")
            self.assertEqual(claim["schema"], "verisilo-camoufox-fp2-one-shot-claim/v4")
            self.assertEqual(claim["previousBlockedAttempt"]["browserObservations"], 0)
            self.assertEqual(claim["previousAttempts"]["generation1"]["browserObservations"], 0)
            self.assertEqual(claim["previousAttempts"]["generation2"]["claimSha256"], fp2.GENERATION2_CLAIM_SHA256)
            self.assertEqual(claim["previousAttempts"]["generation3"]["claimSha256"], fp2.GENERATION3_CLAIM_SHA256)
            self.assertEqual(claim["runtime"]["interpreterSha256"], result["runtimeBinding"]["interpreterSha256"])
            for path, digest in historical_claims.items():
                self.assertEqual(fp2.sha256_file(path), digest)

    def test_runtime_environment_cannot_switch_after_preflight(self) -> None:
        result = self.runtime_preflight_result()
        result["runtimeBinding"]["interpreterSha256"] = "0" * 64
        self.assertCode("runtime_environment_changed", fp2.validate_runtime_binding, result["runtimeBinding"])

    def test_child_environment_and_invocation_are_shared(self) -> None:
        interpreter = fp2.resolve_runtime_interpreter()
        environment = fp2.child_environment()
        descriptor = fp2.runtime_invocation_descriptor(interpreter)
        self.assertEqual(environment["PYTHONUNBUFFERED"], "1")
        self.assertEqual(Path(environment["PYTHONPATH"]).resolve(), fp2.HOST_DIR.resolve())
        self.assertEqual(descriptor["interpreterRelativePath"], fp2.relative_repo_path(interpreter))
        self.assertEqual(descriptor["entrypoints"]["preflight"], "--runtime-preflight-child")
        self.assertEqual(descriptor["entrypoints"]["session"], "--child-session")

    def test_synthetic_finalization_covers_all_statuses(self) -> None:
        result = fp2.synthetic_report_finalization_test()
        self.assertEqual(result["status"], "passed")
        self.assertFalse(result["claimCreated"])
        self.assertFalse(result["browserLaunchCalled"])
        self.assertEqual(
            set(result["cases"]),
            {"execution-passed-awaiting-main-brain-gate", "failed", "blocked"},
        )

    def test_blocked_report_sidecar_adjudication_and_byte_closure(self) -> None:
        with tempfile.TemporaryDirectory(prefix="fp2-finalization-test-") as folder:
            root = Path(folder)
            artifacts = fp2.finalize_report_artifacts(
                run_dir=root,
                report={"schema": fp2.REPORT_SCHEMA, "status": "blocked", "verified": False, "failure": {"code": "synthetic", "detail": "blocked"}},
                conclusion="blocked",
                checks={"verified": False},
            )
            fp2.validate_hash_sidecar(artifacts["reportPath"], root / "run-report.sha256")
            fp2.validate_hash_sidecar(artifacts["adjudicationPath"], root / "final-offline-adjudication.sha256")
            fp2.validate_hash_sidecar(artifacts["closurePath"], root / "byte-closure-receipt.sha256")


if __name__ == "__main__":
    unittest.main(verbosity=1)
