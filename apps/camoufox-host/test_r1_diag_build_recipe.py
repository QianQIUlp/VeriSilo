#!/usr/bin/env python3
"""Offline contract checks for the independent R1 diagnostic build family."""

from __future__ import annotations

import hashlib
import json
import sys
import unittest
from pathlib import Path


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parent.parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
V1_LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v1-source.json"
SERIES_DIR = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1-diag"
sys.path.insert(0, str(BUILD_DIR))
import diag_gate  # noqa: E402


COMPLETE_ORDER = ["0000", "0001", "0002", "0003", "0004", "9000"]
INCREMENTAL_ORDER = ["0003", "0004", "9000"]
RECIPE_FILES = [
    BUILD_DIR / "Dockerfile",
    BUILD_DIR / "strict_build.py",
    BUILD_DIR / "diag_gate.py",
    BUILD_DIR / "build_host.py",
]


class R1DiagBuildRecipeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
        cls.v1_lock = json.loads(V1_LOCK_PATH.read_text(encoding="utf-8"))

    def test_v2_is_bound_and_not_the_historical_v1_lock(self) -> None:
        self.assertEqual(self.lock["schema"], "verisilo-r1-diag-source-binding/v2")
        self.assertEqual(
            self.lock["engineRevision"],
            "verisilo-camoufox-152.0.4-beta.28-r1-diag-v2",
        )
        self.assertEqual(
            self.lock["status"], "builder-bound-awaiting-diagnostic-engine-build"
        )
        self.assertIsInstance(self.lock["buildBinding"]["builderImageBinding"], dict)
        self.assertNotIn("builderImageBinding", self.lock)
        self.assertTrue(self.lock["diagnosticOnly"])
        self.assertFalse(self.lock["formalEligible"])
        self.assertEqual(self.lock["browserLaunches"], 0)
        self.assertNotEqual(self.lock["engineRevision"], self.v1_lock["engineRevision"])

    def test_complete_patch_lineage_preserves_base_and_incremental_order(self) -> None:
        self.assertEqual(
            self.lock["sourceInputs"]["baseCarryForwardPatchOrder"],
            ["0000", "0001", "0002"],
        )
        self.assertEqual(
            self.lock["sourceInputs"]["r1IncrementalPatchOrder"], INCREMENTAL_ORDER
        )
        self.assertEqual(self.lock["completeAppliedPatchOrder"], COMPLETE_ORDER)
        self.assertEqual(self.lock["sourceInputs"]["completePatchOrder"], COMPLETE_ORDER)
        self.assertEqual(
            [item["id"] for item in self.lock["completePatchSeries"]], COMPLETE_ORDER
        )
        self.assertEqual(
            [item["id"] for item in self.lock["r1IncrementalPatches"]], INCREMENTAL_ORDER
        )

    def test_all_six_patch_bytes_match_v2_lock(self) -> None:
        for item in self.lock["completePatchSeries"]:
            path = REPO_ROOT / item["path"]
            data = path.read_bytes()
            self.assertEqual(len(data), item["sizeBytes"], item["id"])
            self.assertEqual(hashlib.sha256(data).hexdigest(), item["sha256"], item["id"])

    def test_each_patch_has_pre_and_post_seam_binding(self) -> None:
        seams_by_patch: dict[str, list[dict]] = {}
        for seam in self.lock["patchSeams"]:
            seams_by_patch.setdefault(seam["id"], []).append(seam)
            self.assertRegex(seam["preSha256"], r"^[0-9a-f]{64}$")
            self.assertRegex(seam["postSha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(set(seams_by_patch), set(COMPLETE_ORDER))
        self.assertTrue(all(seams_by_patch[patch_id] for patch_id in COMPLETE_ORDER))

    def test_recipe_file_hashes_are_frozen(self) -> None:
        locked = self.lock["buildBinding"]["recipe"]["files"]
        self.assertEqual([item["path"] for item in locked], [
            "apps/camoufox-host/build/r1-diag-v1/Dockerfile",
            "apps/camoufox-host/build/r1-diag-v1/strict_build.py",
            "apps/camoufox-host/build/r1-diag-v1/diag_gate.py",
            "apps/camoufox-host/build/r1-diag-v1/build_host.py",
        ])
        for path, item in zip(RECIPE_FILES, locked):
            data = path.read_bytes()
            self.assertEqual(len(data), item["sizeBytes"], path.name)
            self.assertEqual(hashlib.sha256(data).hexdigest(), item["sha256"], path.name)

    def test_dockerfile_embeds_independent_driver_and_gate(self) -> None:
        text = (BUILD_DIR / "Dockerfile").read_text(encoding="utf-8")
        self.assertIn("COPY strict_build.py /usr/local/bin/verisilo-r1-diag-strict-build", text)
        self.assertIn("COPY diag_gate.py /usr/local/lib/verisilo-r1-diag/diag_gate.py", text)
        self.assertIn('ENTRYPOINT ["python3", "/usr/local/bin/verisilo-r1-diag-strict-build"]', text)
        self.assertNotIn("canvas-engine-v1", text)

    def test_driver_and_launcher_have_no_injection_or_canvas_route(self) -> None:
        strict = (BUILD_DIR / "strict_build.py").read_text(encoding="utf-8")
        host = (BUILD_DIR / "build_host.py").read_text(encoding="utf-8")
        for text in (strict, host):
            self.assertNotIn("canvas-engine-v1", text)
            self.assertNotIn("docker cp", text)
            self.assertNotIn("--entrypoint", text)
        self.assertNotIn("--mode", strict)
        self.assertNotIn("getenv", strict)
        self.assertIn("diagnostic-gate-result.json", strict)
        self.assertIn("_apply_upstream_patches", strict)
        self.assertIn("apply-upstream-patch-", strict)
        self.assertIn("complete downstream patch order applied", strict)
        self.assertIn('"driverInjection": False', host)
        self.assertIn("dst=/inputs,readonly", host)

    def test_v2_gate_accepts_disk_series_and_uses_lock_series(self) -> None:
        expected = diag_gate.expected_series_from_lock(self.lock)
        self.assertEqual(set(expected), set(INCREMENTAL_ORDER))
        result = diag_gate.evaluate(diag_gate.MODE_DIAGNOSTIC, SERIES_DIR, expected)
        self.assertTrue(result.ok)
        self.assertTrue(result.diagnosticOnly)
        self.assertFalse(result.formalEligible)
        self.assertEqual(result.details["purpose"], self.lock["diagnosticPurpose"])

    def test_diagnostic_mode_rejects_unknown_patch_with_structured_details(self) -> None:
        expected = diag_gate.expected_series_from_lock(self.lock)
        import tempfile
        from shutil import copy2

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for item in self.lock["r1IncrementalPatches"]:
                copy2(REPO_ROOT / item["path"], target / Path(item["path"]).name)
            (target / "9999-stray.patch").write_text("stray\n", encoding="utf-8")
            result = diag_gate.evaluate(diag_gate.MODE_DIAGNOSTIC, target, expected)
        self.assertFalse(result.ok)
        self.assertEqual(result.details["missing"], [])
        self.assertEqual(result.details["extra"], ["9999-stray.patch"])
        self.assertEqual(result.details["drift"], [])


if __name__ == "__main__":
    unittest.main(verbosity=1)
