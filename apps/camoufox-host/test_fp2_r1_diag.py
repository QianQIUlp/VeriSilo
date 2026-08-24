#!/usr/bin/env python3
"""No-browser regression for the FP2-R1 diagnostic execution package."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parents[1]
sys.path.insert(0, str(HOST_DIR))

import fp2_r1_diag as diag  # noqa: E402


def line(value: dict) -> str:
    return "VSIDIAG " + json.dumps(value, separators=(",", ":"))


def wrapped(value: str, pid: int = 123) -> str:
    return f"pw:browser [pid={pid}][err] {value}"


def inventory(count: int) -> dict:
    return {"count": count, "uriHashes": [f"{index:012x}" for index in range(count)]}


def observation(
    first_count: int = 58,
    second_count: int = 58,
) -> dict:
    return {
        "configuredIdentityDigest": "a" * 64,
        "expectedConfiguredIdentityDigest": "a" * 64,
        "singleTopObjectSchedule": True,
        "top": {
            "firstAtMonotonicMs": 1.0,
            "secondAtMonotonicMs": 3001.0,
            "waitReason": "bounded-delay",
            "first": inventory(first_count),
            "second": inventory(second_count),
        },
    }


def top_lines(first_count: int = 58, second_count: int = 58, *, delivery: bool = False) -> list[str]:
    values = [
        line({"e": "E7_getvoices", "proc": "C", "seq": 0, "ctx": 0, "n": first_count, "cache": first_count, "first": 1}),
    ]
    if delivery:
        values.append(line({"e": "E6_recv_initial_voices", "proc": "C", "seq": 1, "n": second_count}))
        second_seq = 2
    else:
        second_seq = 1
    values.append(line({"e": "E7_getvoices", "proc": "C", "seq": second_seq, "ctx": 0, "n": second_count, "cache": second_count, "first": 0}))
    return values


class ReadinessTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.readiness = diag.verify_readiness(hash_archive=False)

    def test_exact_1542z_binding_is_diagnostic_only(self) -> None:
        self.assertEqual(self.readiness["status"], "execution-package-ready-no-browser")
        self.assertEqual(self.readiness["runId"], diag.EXPECTED_RUN_ID)
        self.assertTrue(self.readiness["diagnosticOnly"])
        self.assertFalse(self.readiness["formalEligible"])
        self.assertEqual(self.readiness["browserLaunches"], 0)
        self.assertFalse(self.readiness["verified"])
        self.assertEqual(
            self.readiness["claims"],
            {
                "fp2R1Accepted": False,
                "formalR1": False,
                "gpcRuntimeVerified": False,
                "voicesFixed": False,
                "remediationSuccess": False,
            },
        )

    def test_runtime_namespace_cannot_reuse_formal_fp2_claim(self) -> None:
        self.assertEqual(diag.CLAIM_PATH.name, "fp2-r1-diag-v1-one-shot-claim.json")
        self.assertNotEqual(diag.CLAIM_PATH.parent, diag.fp2.FP2_EVIDENCE_ROOT)
        self.assertNotIn("generation", diag.CLAIM_PATH.as_posix().lower())
        parser = diag.build_parser()
        self.assertFalse(parser.parse_args([]).execute_browser_diagnostic)
        self.assertTrue(parser.parse_args(["--execute-browser-diagnostic"]).execute_browser_diagnostic)

    def test_direct_child_requires_parent_authorization(self) -> None:
        run_id = "fp2-r1-diag-20000101T000000Z-0000000000"
        args = diag.build_parser().parse_args(
            [
                "--child-session",
                "--child-run-id",
                run_id,
                "--child-authorization",
                str(diag.EVIDENCE_ROOT / run_id / "child-authorization.json"),
            ]
        )
        with self.assertRaisesRegex(diag.DiagnosticError, "evidence_missing"):
            diag._consume_child_authorization(args)

    def test_child_rejects_authorization_for_other_runner_bytes(self) -> None:
        original_root = diag.EVIDENCE_ROOT
        try:
            with tempfile.TemporaryDirectory() as temp_name:
                diag.EVIDENCE_ROOT = Path(temp_name)
                run_id = "fp2-r1-diag-20000101T000000Z-0000000000"
                authorization = diag.EVIDENCE_ROOT / run_id / "child-authorization.json"
                diag.write_json(
                    authorization,
                    {
                        "schema": diag.CHILD_AUTH_SCHEMA,
                        "runId": run_id,
                        "runnerSha256": "0" * 64,
                    },
                )
                args = diag.build_parser().parse_args(
                    [
                        "--child-session",
                        "--child-run-id",
                        run_id,
                        "--child-authorization",
                        str(authorization),
                    ]
                )
                with self.assertRaisesRegex(
                    diag.DiagnosticError, "child_authorization_invalid: runner"
                ):
                    diag._consume_child_authorization(args)
        finally:
            diag.EVIDENCE_ROOT = original_root

    def test_child_environment_forces_playwright_capture(self) -> None:
        environment = diag.diagnostic_child_environment("token")
        self.assertEqual(environment[diag.CHILD_TOKEN_ENV], "token")
        self.assertEqual(environment["DEBUG"], "pw:browser")
        self.assertEqual(environment["DEBUG_COLORS"], "0")
        self.assertEqual(environment["DEBUG_HIDE_DATE"], "1")
        self.assertNotIn("DEBUG_FILE", environment)

    def test_failed_child_never_claims_zero_launches_without_evidence(self) -> None:
        self.assertEqual(
            diag._browser_launch_evidence({}, child_started=True),
            (None, "unknown-after-child-start"),
        )
        self.assertEqual(
            diag._browser_launch_evidence(
                {
                    "schema": "verisilo-fp2-r1-diag-child/v1",
                    "browserSpawnCalled": False,
                },
                child_started=True,
            ),
            (0, "child-result"),
        )

    def test_process_scanner_failure_is_reportable(self) -> None:
        original = diag.fp2.target_processes
        try:
            diag.fp2.target_processes = lambda: (_ for _ in ()).throw(
                diag.fp2.FP2Failure("process_cleanliness_unverifiable", "test")
            )
            self.assertEqual(
                diag._post_run_process_cleanliness("run"),
                (
                    None,
                    {
                        "code": "process_cleanliness_unverifiable",
                        "detail": "test",
                    },
                ),
            )
        finally:
            diag.fp2.target_processes = original

    def test_child_termination_must_be_confirmed(self) -> None:
        class StuckProcess:
            pid = 123

            def poll(self) -> None:
                return None

            def kill(self) -> None:
                return None

            def wait(self, timeout: int) -> None:
                raise diag.subprocess.TimeoutExpired("child", timeout)

        original = diag.subprocess.run
        try:
            diag.subprocess.run = lambda *_args, **_kwargs: None
            self.assertFalse(diag._terminate_child_bounded(StuckProcess()))
        finally:
            diag.subprocess.run = original

    def test_child_termination_falls_back_when_taskkill_fails(self) -> None:
        class KillableProcess:
            pid = 123
            alive = True
            kill_called = False

            def poll(self) -> int | None:
                return None if self.alive else 1

            def kill(self) -> None:
                self.kill_called = True
                self.alive = False

            def wait(self, timeout: int) -> int:
                return 1

        original = diag.subprocess.run
        try:
            for failure in (OSError("taskkill"), diag.subprocess.TimeoutExpired("taskkill", 10)):
                with self.subTest(failure=type(failure).__name__):
                    process = KillableProcess()

                    def fail(*_args: object, **_kwargs: object) -> None:
                        raise failure

                    diag.subprocess.run = fail
                    self.assertTrue(diag._terminate_child_bounded(process))
                    self.assertTrue(process.kill_called)
        finally:
            diag.subprocess.run = original

    def test_frozen_actual_event_schema_is_explicit(self) -> None:
        patch = diag.PATCH_9000_PATH.read_text(encoding="utf-8")
        self.assertIn('"E7_getvoices"', patch)
        self.assertIn("aCacheSize", patch)
        self.assertNotIn("E8_", patch)
        self.assertNotIn('"tid"', patch)
        self.assertNotIn('"actorTag"', patch)
        self.assertEqual(patch.splitlines()[0], diag.DIAGNOSTIC_MARKER)

    def test_reference_hash_sets_are_disjoint(self) -> None:
        values = diag.reference_voice_hashes()
        self.assertEqual(len(values["managed"]), 53)
        self.assertEqual(len(values["knownNative"]), 5)
        self.assertTrue(values["managed"].isdisjoint(values["knownNative"]))

    def test_pinned_source_model_is_bound(self) -> None:
        lock = json.loads(diag.SOURCE_LOCK_PATH.read_text(encoding="utf-8"))
        seams = {
            item["path"]: item["preSha256"]
            for item in lock["patchSeams"]
            if item["id"] == "9000"
        }
        self.assertEqual(
            seams["dom/media/webspeech/synth/windows/SapiService.cpp"],
            "d316d9f48c6123aed43612b0d21e8d64a2ea388c6bd40cad45b2848f42066274",
        )
        self.assertEqual(
            seams["dom/media/webspeech/synth/nsSynthVoiceRegistry.cpp"],
            "9eac4b53804588d57f4309ed0e6f8d9c971ca876473efd698d30d90a4bf18a3f",
        )
        patch = diag.PATCH_9000_PATH.read_text(encoding="utf-8")
        parent = patch[patch.index("bool SpeechSynthesisParent::SendInit()") : patch.index("--- a/dom/media/webspeech/synth/nsSynthVoiceRegistry.cpp")]
        self.assertLess(parent.index("GetInstance()->GetVoiceCount"), parent.index("E4_sendinit_snapshot"))

        source = (
            REPO_ROOT
            / "artifacts"
            / "camoufox-fp1"
            / "engine-patch-work"
            / "firefox-152.0.4-patched"
            / "dom"
            / "media"
            / "webspeech"
            / "synth"
            / "windows"
            / "SapiService.cpp"
        )
        if source.is_file():
            self.assertEqual(
                diag.sha256_file(source),
                "d316d9f48c6123aed43612b0d21e8d64a2ea388c6bd40cad45b2848f42066274",
            )
            text = source.read_text(encoding="utf-8")
            init = text[text.index("bool SapiService::Init()") : text.index("already_AddRefed<ISpVoice>")]
            self.assertLess(init.index("RegisterVoices()"), init.index("mInitialized = true"))
            register = text[text.index("bool SapiService::RegisterVoices(nsCOMPtr") : text.index("NS_IMETHODIMP\nSapiService::Speak")]
            self.assertIn("while (true)", register)
            self.assertIn("registry->AddVoice", register)

    def test_archive_path_validation_rejects_escape(self) -> None:
        for value in ("../camoufox.exe", "/camoufox.exe", "C:/camoufox.exe", "a\\b"):
            with self.subTest(value=value), self.assertRaises(diag.DiagnosticError):
                diag._zip_member_path(value)


class TimelineTests(unittest.TestCase):
    def parse(self, values: list[str]) -> dict:
        return diag.parse_diagnostic_log("\n".join(wrapped(value) for value in values) + "\n")

    def classify(self, values: list[str], observed: dict | None = None) -> dict:
        refs = diag.reference_voice_hashes()
        return diag.classify_v1_v4(
            self.parse(values),
            observed or observation(),
            managed_hashes=refs["managed"],
            known_native_hashes=refs["knownNative"],
        )

    def parent_lines(self) -> list[str]:
        return [
            line({"e": "E2a_sapi_init_begin", "proc": "P", "seq": 0}),
            line({"e": "E2b_sapi_init_end", "proc": "P", "seq": 1}),
            line({"e": "E1_mvoices_parsed", "proc": "P", "seq": 2, "n": 53}),
            line({"e": "E3a_managed_batch_begin", "proc": "P", "seq": 3, "n": 53}),
            line({"e": "E3b_managed_batch_end", "proc": "P", "seq": 4}),
            line({"e": "E4_sendinit_snapshot", "proc": "P", "seq": 5, "n": 58}),
        ]

    def test_v1_v2_are_source_refuted(self) -> None:
        result = self.classify([*self.parent_lines(), *top_lines()])
        self.assertEqual(result["axes"]["V1"]["status"], "source-refuted-as-written")
        self.assertEqual(result["axes"]["V2"]["status"], "source-refuted-as-written")
        self.assertEqual(result["actualCompensation"]["T1_contentMirrorIncrementalDelivery"]["status"], "not-observed")

    def test_temporal_incremental_delivery_signature(self) -> None:
        values = [
            *self.parent_lines(),
            *top_lines(5, 58, delivery=True),
        ]
        observed = observation(5, 58)
        refs = diag.reference_voice_hashes()
        observed["top"]["first"]["uriHashes"] = sorted(refs["knownNative"])
        observed["top"]["second"]["uriHashes"] = sorted(refs["knownNative"] | refs["managed"])
        result = self.classify(values, observed)
        self.assertEqual(result["actualCompensation"]["T1_contentMirrorIncrementalDelivery"]["status"], "supported")
        self.assertEqual(result["conclusion"], "temporal-incremental-delivery-supported")

    def test_t1_rejects_count_only_inventory_change(self) -> None:
        result = self.classify(
            [*self.parent_lines(), *top_lines(5, 58, delivery=True)],
            observation(5, 58),
        )
        self.assertEqual(
            result["actualCompensation"]["T1_contentMirrorIncrementalDelivery"]["status"],
            "not-observed",
        )
        self.assertEqual(result["conclusion"], "inconclusive")

    def test_t1_rejects_parent_snapshot_contradiction(self) -> None:
        values = self.parent_lines()
        values[-1] = line({"e": "E4_sendinit_snapshot", "proc": "P", "seq": 5, "n": 5})
        observed = observation(5, 58)
        refs = diag.reference_voice_hashes()
        observed["top"]["first"]["uriHashes"] = sorted(refs["knownNative"])
        observed["top"]["second"]["uriHashes"] = sorted(refs["knownNative"] | refs["managed"])
        result = self.classify([*values, *top_lines(5, 58, delivery=True)], observed)
        self.assertEqual(
            result["actualCompensation"]["T1_contentMirrorIncrementalDelivery"]["status"],
            "not-observed",
        )
        self.assertEqual(result["conclusion"], "inconclusive")

    def test_v4_null_config_signature(self) -> None:
        values = [
            line({"e": "E2a_sapi_init_begin", "proc": "P", "seq": 0}),
            line({"e": "E2b_sapi_init_end", "proc": "P", "seq": 1}),
            line({"e": "E1_mvoices_parsed", "proc": "P", "seq": 2, "n": None}),
            line({"e": "E4_sendinit_snapshot", "proc": "P", "seq": 3, "n": 5}),
            *top_lines(),
        ]
        result = self.classify(values)
        self.assertEqual(result["axes"]["V4"]["status"], "suspicion")
        self.assertNotIn("V4", result["supported"])

    def test_count_change_without_e6_is_not_cache_proof(self) -> None:
        values = [*self.parent_lines(), *top_lines(5, 58)]
        observed = observation(5, 58)
        result = self.classify(values, observed)
        self.assertEqual(result["axes"]["V3"]["status"], "inconclusive")
        self.assertEqual(result["conclusion"], "unexplained-content-local-transition")

    def test_overflow_and_malformed_lines_fail_closed(self) -> None:
        with self.assertRaisesRegex(diag.DiagnosticError, "vsidiag_overflow"):
            self.parse([line({"e": "OVERFLOW"})])
        with self.assertRaisesRegex(diag.DiagnosticError, "invalid_json"):
            diag.parse_diagnostic_log(wrapped("VSIDIAG {bad}") + "\n")
        with self.assertRaisesRegex(diag.DiagnosticError, "vsidiag_unknown_event"):
            self.parse([line({"e": "E8_not_frozen", "proc": "P", "seq": 0})])

    def test_event_schema_and_sequence_are_strict(self) -> None:
        invalid = [
            {"e": "E7_getvoices", "proc": "C", "seq": 0, "ctx": 0, "n": -1, "cache": 0, "first": 1},
            {"e": "E7_getvoices", "proc": "C", "seq": 0, "ctx": 0, "n": 0, "cache": "x", "first": 1},
            {"e": "E7_getvoices", "proc": "C", "seq": 0, "ctx": 0, "n": 0, "cache": 0, "first": True},
            {"e": "E6_recv_add_voice", "proc": "C", "seq": 0, "h": "not-a-hash"},
        ]
        for value in invalid:
            with self.subTest(value=value), self.assertRaises(diag.DiagnosticError):
                self.parse([line(value)])
        with self.assertRaisesRegex(diag.DiagnosticError, "vsidiag_sequence_gap"):
            self.parse([line({"e": "E2a_sapi_init_begin", "proc": "P", "seq": 1})])

    def test_missing_config_digests_fail_closed(self) -> None:
        refs = diag.reference_voice_hashes()
        with self.assertRaisesRegex(diag.DiagnosticError, "config_delivery_unproven"):
            diag.classify_v1_v4(
                self.parse([*self.parent_lines(), *top_lines()]),
                {},
                managed_hashes=refs["managed"],
                known_native_hashes=refs["knownNative"],
            )

    def test_top_content_process_reset_fails_closed(self) -> None:
        values = [
            line({"e": "E2a_sapi_init_begin", "proc": "P", "seq": 0}),
            line({"e": "E2b_sapi_init_end", "proc": "P", "seq": 1}),
            line({"e": "E1_mvoices_parsed", "proc": "P", "seq": 2, "n": 53}),
            line({"e": "E3a_managed_batch_begin", "proc": "P", "seq": 3, "n": 53}),
            line({"e": "E3b_managed_batch_end", "proc": "P", "seq": 4}),
            line({"e": "E4_sendinit_snapshot", "proc": "P", "seq": 5, "n": 58}),
            line({"e": "E7_getvoices", "proc": "C", "seq": 0, "ctx": 0, "n": 58, "cache": 58, "first": 1}),
            line({"e": "E6_recv_initial_voices", "proc": "C", "seq": 0, "n": 58}),
            line({"e": "E7_getvoices", "proc": "C", "seq": 1, "ctx": 0, "n": 58, "cache": 58, "first": 0}),
        ]
        with self.assertRaisesRegex(diag.DiagnosticError, "vsidiag_sequence_duplicate"):
            self.classify(values)

    def test_playwright_wrapped_capture_uses_content_sequence(self) -> None:
        refs = diag.reference_voice_hashes()
        observed = observation(5, 58)
        observed["top"]["first"]["uriHashes"] = sorted(refs["knownNative"])
        observed["top"]["second"]["uriHashes"] = sorted(refs["knownNative"] | refs["managed"])
        raw = [
            *[wrapped(value) for value in self.parent_lines()],
            *[wrapped(value) for value in top_lines(5, 58, delivery=True)],
        ]
        timeline = diag.parse_diagnostic_log("\n".join(raw) + "\n")
        result = diag.classify_v1_v4(
            timeline,
            observed,
            managed_hashes=refs["managed"],
            known_native_hashes=refs["knownNative"],
        )
        self.assertEqual(timeline["captureMode"], "playwright-pw-browser-stderr-v1")
        self.assertEqual(timeline["transportPids"], [123])
        self.assertEqual(result["conclusion"], "temporal-incremental-delivery-supported")
        with self.assertRaisesRegex(diag.DiagnosticError, "vsidiag_transport_invalid"):
            diag.parse_diagnostic_log(line({"e": "E2a_sapi_init_begin", "proc": "P", "seq": 0}))


if __name__ == "__main__":
    unittest.main(verbosity=2)
