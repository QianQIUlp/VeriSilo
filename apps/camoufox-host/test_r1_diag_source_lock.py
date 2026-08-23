#!/usr/bin/env python3
"""Self-verification for the r1-diag diagnostic source lock and build gate.

Covers implementation-contract section 10 and the main-brain hard invariants:
- explicit two-mode gate (formal HARD FAIL on marker; diagnostic strict trio)
- no environment-variable bypass exists in the gate module
- lock/record/disk cross-consistency for patches, sections, seams
- builder image binding identical to the frozen canvas builder binding
- GPC startup-order invariant statically proven against patch 0003
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import sys

HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
sys.path.insert(0, str(BUILD_DIR))

import diag_gate  # noqa: E402

SERIES_DIR = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1-diag"
LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v1-source.json"
CANVAS_LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json"
RECORD_PATH = BUILD_DIR / "authoring-record.json"


class R1DiagSourceLockTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
        cls.record = json.loads(RECORD_PATH.read_text(encoding="utf-8"))
        cls.canvas_lock = json.loads(CANVAS_LOCK_PATH.read_text(encoding="utf-8"))
        cls.expected = diag_gate.load_expected_series(RECORD_PATH)

    # ---- mode / purpose consistency ----

    def test_mode_and_purpose_consistency(self) -> None:
        self.assertEqual(self.lock["buildMode"], "diagnostic")
        self.assertEqual(self.lock["diagnosticPurpose"],
                         "fp2-r1-voices-v1-v4-discrimination")
        self.assertTrue(self.lock["diagnosticOnly"])
        self.assertFalse(self.lock["formalEligible"])
        self.assertIn("r1-diag", self.lock["engineRevision"])

    def test_carry_forward_policy(self) -> None:
        patches = {p["id"]: p for p in self.lock["downstreamPatchSeries"]["applyAfterUpstreamOrder"]}
        self.assertEqual(patches["9000"]["formalCarryForward"], "never")
        self.assertTrue(patches["9000"]["diagnosticOnly"])
        for key in ("0003", "0004"):
            self.assertFalse(patches[key]["diagnosticOnly"])
            self.assertEqual(patches[key]["formalCarryForward"],
                             "allowed-after-qualification")

    # ---- disk / record / lock triple consistency ----

    def test_patches_match_disk_record_and_lock(self) -> None:
        import hashlib
        series = {p["id"]: p for p in self.lock["downstreamPatchSeries"]["applyAfterUpstreamOrder"]}
        self.assertEqual(set(series), {"0003", "0004", "9000"})
        order = [p["id"] for p in self.lock["downstreamPatchSeries"]["applyAfterUpstreamOrder"]]
        self.assertEqual(order, ["0003", "0004", "9000"])
        for pid, meta in series.items():
            blob = (SERIES_DIR / meta["path"]).read_bytes()
            digest = hashlib.sha256(blob).hexdigest()
            self.assertEqual(digest, meta["sha256"], pid)
            rec_name = [n for n in self.record["patches"] if n.startswith(pid)][0]
            self.assertEqual(digest, self.record["patches"][rec_name]["sha256"], pid)

    def test_sections_and_seams_match_authoring_record(self) -> None:
        self.assertEqual(self.lock["upstreamSectionsSha256"],
                         self.record["upstreamSectionsSha256"])
        self.assertEqual(self.lock["seams"], self.record["seams"])
        self.assertEqual(len(self.lock["seams"]), 16)

    def test_builder_binding_matches_frozen_canvas_builder(self) -> None:
        canvas = self.canvas_lock["buildBinding"]["builderImageBinding"]
        ours = self.lock["builderImageBinding"]
        for field, value in canvas.items():
            self.assertEqual(ours[field], value, field)

    # ---- GPC startup-order invariant (hard invariant A) ----

    def test_gpc_projection_ordering_within_xre_main_run(self) -> None:
        inv = self.lock["gpcStartupOrderInvariant"]
        self.assertEqual(inv["anchorFunction"], "XREMain::XRE_mainRun")
        seams = self.lock["seams"]
        self.assertEqual(inv["seamBinding"]["pre"], seams["toolkit-xre-nsAppRunner-pre"])
        self.assertEqual(inv["seamBinding"]["post"], seams["toolkit-xre-nsAppRunner-post"])

        text = (SERIES_DIR / "0003-verisilo-gpc-canonical-pref-projection.patch") \
            .read_text(encoding="utf-8")
        # hunk pins the insertion inside the XRE_mainRun head region:
        self.assertIn("@@ -5536,", text)
        pos_rv = text.index(" nsresult rv = NS_OK;")
        pos_assertion = text.index("NS_ASSERTION(mScopedXPCOM")
        pos_comment = text.index("+  // VeriSilo GPC policy-state projection")
        pos_call = text.index("camoucfg::ProjectGpcPolicyFromMaskConfig();")
        pos_next_ctx = text.index("#if defined(XP_WIN)")
        self.assertLess(pos_rv, pos_assertion)
        self.assertLess(pos_assertion, pos_comment)
        self.assertLess(pos_comment, pos_call)
        self.assertLess(pos_call, pos_next_ctx)
        # projection sits in the function-head region (immediately after the
        # entry assertion), i.e. before any window/command-line scope opens.
        self.assertLessEqual(pos_call - pos_rv, 300)

    # ---- build gate behavior (hard invariant B) ----

    def _eval_with_dir(self, mode: str, series_dir: Path, texts=None):
        return diag_gate.evaluate(mode, series_dir, self.expected, patch_texts=texts)

    def test_gate_formal_rejects_diagnostic_series(self) -> None:
        result = self._eval_with_dir(diag_gate.MODE_FORMAL, SERIES_DIR)
        self.assertFalse(result.ok)
        self.assertIn("HARD FAIL", result.reason)

    def test_gate_formal_accepts_clean_pair(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "0004"):
                name = self.expected[pid]["filename"]
                shutil.copy(SERIES_DIR / name, tmp / name)
            result = self._eval_with_dir(diag_gate.MODE_FORMAL, tmp)
        self.assertTrue(result.ok)
        self.assertTrue(result.formalEligible)
        self.assertFalse(result.diagnosticOnly)

    def test_gate_formal_rejects_unknown_file(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "0004"):
                name = self.expected[pid]["filename"]
                shutil.copy(SERIES_DIR / name, tmp / name)
            (tmp / "9999-stray.patch").write_text("junk", encoding="utf-8")
            result = self._eval_with_dir(diag_gate.MODE_FORMAL, tmp)
        self.assertFalse(result.ok)
        self.assertIn("unrecognized", result.reason)

    def test_gate_formal_rejects_missing_patch(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            shutil.copy(
                SERIES_DIR / self.expected["0003"]["filename"],
                tmp / self.expected["0003"]["filename"],
            )
            result = self._eval_with_dir(diag_gate.MODE_FORMAL, tmp)
        self.assertFalse(result.ok)
        self.assertEqual(result.details["missing"], ["0004"])
        self.assertEqual(result.details["extra"], [])
        self.assertEqual(result.details["drift"], [])

    def test_gate_formal_rejects_sha_drift(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "0004"):
                name = self.expected[pid]["filename"]
                data = (SERIES_DIR / name).read_bytes()
                if pid == "0004":
                    data += b"\n"
                (tmp / name).write_bytes(data)
            result = self._eval_with_dir(diag_gate.MODE_FORMAL, tmp)
        self.assertFalse(result.ok)
        self.assertEqual(result.details["drift"], ["0004"])

    def test_gate_diagnostic_accepts_frozen_trio(self) -> None:
        result = self._eval_with_dir(diag_gate.MODE_DIAGNOSTIC, SERIES_DIR)
        self.assertTrue(result.ok)
        self.assertTrue(result.diagnosticOnly)
        self.assertFalse(result.formalEligible)
        self.assertEqual(result.details["purpose"],
                         "fp2-r1-voices-v1-v4-discrimination")

    def test_gate_diagnostic_rejects_sha_drift(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "0004", "9000"):
                name = self.expected[pid]["filename"]
                data = (SERIES_DIR / name).read_bytes()
                if pid == "0003":
                    data += b"\n"
                (tmp / name).write_bytes(data)
            result = self._eval_with_dir(diag_gate.MODE_DIAGNOSTIC, tmp)
        self.assertFalse(result.ok)
        self.assertIn("drift", result.reason)

    def test_gate_diagnostic_rejects_missing_patch(self) -> None:
        for missing in ("0003", "0004", "9000"):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as td:
                tmp = Path(td)
                for pid in ("0003", "0004", "9000"):
                    if pid == missing:
                        continue
                    name = self.expected[pid]["filename"]
                    shutil.copy(SERIES_DIR / name, tmp / name)
                result = self._eval_with_dir(diag_gate.MODE_DIAGNOSTIC, tmp)
                self.assertFalse(result.ok)
                self.assertEqual(result.details["missing"], [missing])
                self.assertEqual(result.details["extra"], [])
                self.assertEqual(result.details["drift"], [])
                self.assertNotIn("Traceback", result.to_json())

    def test_gate_diagnostic_rejects_empty_directory(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            result = self._eval_with_dir(diag_gate.MODE_DIAGNOSTIC, Path(td))
        self.assertFalse(result.ok)
        self.assertEqual(result.details["missing"], ["0003", "0004", "9000"])
        self.assertEqual(result.details["extra"], [])
        self.assertEqual(result.details["drift"], [])

    def test_gate_diagnostic_rejects_marker_drift(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "0004", "9000"):
                name = self.expected[pid]["filename"]
                shutil.copy(SERIES_DIR / name, tmp / name)
            result = self._eval_with_dir(
                diag_gate.MODE_DIAGNOSTIC,
                tmp,
                texts={"9000": "# VERISILO-DIAGNOSTIC-MARKER: v2\n"},
            )
        self.assertFalse(result.ok)
        self.assertEqual(result.details["marker"], "missing-or-drifted")

    def test_gate_cli_missing_patch_is_json_and_exit_two(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            tmp = Path(td)
            for pid in ("0003", "9000"):
                name = self.expected[pid]["filename"]
                shutil.copy(SERIES_DIR / name, tmp / name)
            completed = subprocess.run(
                [
                    sys.executable,
                    str(BUILD_DIR / "diag_gate.py"),
                    "--mode",
                    "diagnostic",
                    "--series-dir",
                    str(tmp),
                    "--authoring-record",
                    str(RECORD_PATH),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
        self.assertEqual(completed.returncode, 2)
        payload = json.loads(completed.stdout)
        self.assertFalse(payload["ok"])
        self.assertEqual(payload["details"]["missing"], ["0004"])
        self.assertNotIn("Traceback", completed.stdout + completed.stderr)

    def test_gate_unknown_mode_rejected(self) -> None:
        result = self._eval_with_dir("formal-with-env-override", SERIES_DIR)
        self.assertFalse(result.ok)

    def test_gate_module_has_no_environment_bypass(self) -> None:
        source = (BUILD_DIR / "diag_gate.py").read_text(encoding="utf-8")
        self.assertNotIn("environ", source)
        self.assertNotIn("getenv", source)


if __name__ == "__main__":
    unittest.main(verbosity=1)
