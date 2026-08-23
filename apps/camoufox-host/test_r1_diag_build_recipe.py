#!/usr/bin/env python3
"""Offline contract checks for the independent R1 diagnostic build family."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parent.parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
V1_LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v1-source.json"
SERIES_DIR = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1-diag"
sys.path.insert(0, str(BUILD_DIR))
import diag_gate  # noqa: E402
import strict_build  # noqa: E402


COMPLETE_ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "9000"]
INCREMENTAL_ORDER = ["0003", "0003a", "0004", "9000"]
RECIPE_FILES = [
    BUILD_DIR / "Dockerfile",
    BUILD_DIR / "strict_build.py",
    BUILD_DIR / "diag_gate.py",
    BUILD_DIR / "build_host.py",
]
FROZEN_CONTAINER_RECIPE_SHA256 = {
    "Dockerfile": "6bdd56672de8ad1c12f466aa60f1ceb16613267dff8f7bc3ec3058c1c3f6bfb2",
    "strict_build.py": "19c8ca4450d3192592c80eb06fb5565e36449b173accdaef706207b77bf1e51a",
    "diag_gate.py": "1f5ed796e981cc00596f33f4d0a7f29356febacf330ad8bbeb5e34e3ba5429ff",
}
FROZEN_PATCH_SHA256 = {
    "0000": "8d407bdc4010f7b2989f206a70909bfa9ad89046ddb9e17fa76092c864433600",
    "0001": "4fa6d3bbf203e2385e29a72ec2669ee17a571281be7ee2a73598e38918069b02",
    "0002": "efb006d5b2b05756fc310b52eb48e0bdab5e8b23e780fa08534a7fc099c22ce7",
    "0003": "3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b",
    "0003a": "c2f9a9f88ba8aeb610eb1cb29f2515f1d79fcf582393397a571bc3206889588c",
    "0004": "5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185",
    "9000": "1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8",
}


class R1DiagBuildRecipeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
        cls.v1_lock = json.loads(V1_LOCK_PATH.read_text(encoding="utf-8"))

    def test_v2_is_bound_to_phase_c8_builder(self) -> None:
        self.assertEqual(self.lock["schema"], "verisilo-r1-diag-source-binding/v2")
        self.assertEqual(
            self.lock["engineRevision"],
            "verisilo-camoufox-152.0.4-beta.28-r1-diag-v2",
        )
        self.assertEqual(
            self.lock["status"],
            "builder-bound-durable-evidence-awaiting-diagnostic-engine-build",
        )
        self.assertEqual(
            self.lock["buildBinding"]["status"],
            "builder-bound-durable-evidence-awaiting-diagnostic-engine-build",
        )
        self.assertIsInstance(
            self.lock["buildBinding"]["builderImageBinding"], dict
        )
        self.assertTrue(self.lock["builderImagePreparationEvidence"]["retained"])
        self.assertTrue(self.lock["builderImagePreparationEvidence"]["reReadable"])
        self.assertNotIn("builderImageBinding", self.lock)
        self.assertTrue(self.lock["diagnosticOnly"])
        self.assertFalse(self.lock["formalEligible"])
        self.assertEqual(self.lock["browserLaunches"], 0)
        self.assertEqual(
            self.lock["buildBinding"]["recipe"]["fixedEnvironment"],
            strict_build.EXPECTED_FIXED_ENVIRONMENT,
        )
        self.assertNotEqual(self.lock["engineRevision"], self.v1_lock["engineRevision"])

    def test_durable_evidence_contract_and_superseded_lineage_are_explicit(self) -> None:
        contract = self.lock["durableBuilderEvidenceContract"]
        self.assertEqual(
            contract["schema"],
            "verisilo-r1-diag-durable-builder-evidence-contract/v1",
        )
        self.assertEqual(contract["scratchRoot"], "/mnt/camoufox-build")
        self.assertEqual(
            contract["durableRoot"],
            "/var/lib/verisilo/camoufox-build-evidence",
        )
        self.assertTrue(contract["qualificationRequired"])
        self.assertEqual(contract["imageSaveReference"], "immutable-image-id")
        self.assertEqual(contract["dockerPullPolicy"], "never")
        self.assertIn("durable-manifest.json", contract["bundleRequiredFiles"])
        self.assertIn("retention-receipt.json", contract["bundleRequiredFiles"])
        self.assertIn("retention-preflight.json", contract["bundleRequiredFiles"])
        self.assertIn("builder-build-context.tar", contract["bundleRequiredFiles"])
        self.assertEqual(
            contract["retentionPreflightSchema"],
            "verisilo-r1-diag-durable-retention-preflight/v1",
        )
        self.assertEqual(
            contract["retentionReceiptSchema"],
            "verisilo-r1-diag-durable-builder-retention-receipt/v1",
        )
        self.assertFalse(contract["environmentOverride"])
        lineage = self.lock["builderOperationalLineage"]
        self.assertEqual(lineage["current"]["bindingState"], "bound")
        self.assertEqual(
            lineage["current"]["durableEvidence"],
            "retained-and-reread",
        )
        self.assertEqual(lineage["current"]["reasonCodes"], [])
        superseded = lineage["supersededPhaseC1"]
        self.assertEqual(
            superseded["bindingCheckpointCommit"],
            "f267bb4ff3f00115a37546bbe0649d0db889a7d3",
        )
        self.assertEqual(superseded["bindingCorrectness"], "historically-accepted")
        self.assertEqual(superseded["materialEvidence"], "permanently-lost")
        self.assertFalse(superseded["operationallyConsumable"])

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

    def test_all_seven_patch_bytes_match_v2_lock(self) -> None:
        for item in self.lock["completePatchSeries"]:
            path = REPO_ROOT / item["path"]
            data = path.read_bytes()
            self.assertEqual(len(data), item["sizeBytes"], item["id"])
            self.assertEqual(hashlib.sha256(data).hexdigest(), item["sha256"], item["id"])

    def test_distinct_bind_roots_can_share_one_backing_filesystem(self) -> None:
        mountinfo = "\n".join(
            [
                "100 1 7:0 /runs/test/inputs /inputs ro - ext4 /dev/loop0 rw",
                "101 1 7:0 /runs/test/build-home /build-home rw - ext4 /dev/loop0 rw",
                "102 1 7:0 /runs/test/work /work rw - ext4 /dev/loop0 rw",
                "103 1 7:0 /runs/test/out /out rw - ext4 /dev/loop0 rw",
            ]
        )
        with mock.patch.object(Path, "read_text", return_value=mountinfo):
            selected = strict_build._validate_mounts()
        self.assertEqual(
            {selected[path]["root"] for path in ("/build-home", "/work", "/out")},
            {
                "/runs/test/build-home",
                "/runs/test/work",
                "/runs/test/out",
            },
        )

    def test_duplicate_bind_root_is_not_a_distinct_mount_identity(self) -> None:
        mountinfo = "\n".join(
            [
                "100 1 7:0 /runs/test/inputs /inputs ro - ext4 /dev/loop0 rw",
                "101 1 7:0 /runs/test/shared /build-home rw - ext4 /dev/loop0 rw",
                "102 1 7:0 /runs/test/shared /work rw - ext4 /dev/loop0 rw",
                "103 1 7:0 /runs/test/shared /out rw - ext4 /dev/loop0 rw",
            ]
        )
        with (
            mock.patch.object(Path, "read_text", return_value=mountinfo),
            self.assertRaisesRegex(
                strict_build.BuildFailure,
                "build-home, work and out must be distinct mounts",
            ),
        ):
            strict_build._validate_mounts()

    def test_embedded_gate_loader_executes_actual_diagnostic_contract(self) -> None:
        with (
            tempfile.TemporaryDirectory() as directory,
            mock.patch.object(
                strict_build, "EMBEDDED_GATE", BUILD_DIR / "diag_gate.py"
            ),
            mock.patch.object(strict_build, "VERISILO_ROOT", REPO_ROOT),
        ):
            result = strict_build._run_diagnostic_gate(
                self.lock, Path(directory)
            )
        self.assertTrue(result["ok"])
        self.assertTrue(result["diagnosticOnly"])
        self.assertFalse(result["formalEligible"])

    def test_rust_toolchain_is_lock_pinned_without_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rustup = root / "rustup"
            rustc = root / "rustc"
            rustup.write_bytes(b"rustup")
            rustc.write_bytes(b"rustc")
            environment: dict[str, str] = {}
            log = mock.Mock()
            with (
                mock.patch.object(strict_build, "RUSTUP_BIN", rustup),
                mock.patch.object(strict_build, "RUSTC_BIN", rustc),
            ):
                version = strict_build._pin_rust_toolchain(
                    self.lock, root, environment, log
                )
            self.assertEqual(version, "1.90.0")
            self.assertEqual(environment["RUSTUP_TOOLCHAIN"], "1.90.0")
            self.assertEqual(
                [call.kwargs["label"] for call in log.run.call_args_list],
                [
                    "install-pinned-rust-toolchain",
                    "select-pinned-rust-toolchain",
                    "verify-pinned-rust-toolchain",
                ],
            )

    def test_rust_toolchain_environment_override_is_rejected(self) -> None:
        with self.assertRaisesRegex(
            strict_build.BuildFailure,
            "cannot be overridden by environment",
        ):
            strict_build._pin_rust_toolchain(
                self.lock,
                Path("unused"),
                {"RUSTUP_TOOLCHAIN": "stable"},
                mock.Mock(),
            )

    def test_each_patch_has_pre_and_post_seam_binding(self) -> None:
        seams_by_patch: dict[str, list[dict]] = {}
        for seam in self.lock["patchSeams"]:
            seams_by_patch.setdefault(seam["id"], []).append(seam)
            self.assertRegex(seam["preSha256"], r"^[0-9a-f]{64}$")
            self.assertRegex(seam["postSha256"], r"^[0-9a-f]{64}$")
        self.assertEqual(set(seams_by_patch), set(COMPLETE_ORDER))
        self.assertTrue(all(seams_by_patch[patch_id] for patch_id in COMPLETE_ORDER))
        self.assertEqual(
            seams_by_patch["0003a"],
            [
                {
                    "id": "0003a",
                    "path": "camoucfg/GpcProjection.h",
                    "preSha256": "ab0b4c26e628a74d0ef4bac66d35bc6b0e9aee45cd67ad6bd5e5da91b609cf3f",
                    "postSha256": "364655669418c106f80f030a7a48797dbdbca1030c0d29e4e91c841129999bda",
                }
            ],
        )

    def test_recipe_orders_0003a_between_projection_and_worker_restore(self) -> None:
        order = self.lock["buildBinding"]["recipe"]["order"]
        expected = [
            "apply-0003-gpc",
            "verify-0003-seams",
            "apply-0003a-gpc-namespace-compile-repair",
            "verify-0003a-seams",
            "apply-0004-worker-restore",
            "verify-0004-seams",
        ]
        start = order.index(expected[0])
        self.assertEqual(order[start:start + len(expected)], expected)

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

    def test_container_recipe_and_patch_bytes_remain_at_accepted_hashes(self) -> None:
        for path in RECIPE_FILES[:3]:
            self.assertEqual(
                hashlib.sha256(path.read_bytes()).hexdigest(),
                FROZEN_CONTAINER_RECIPE_SHA256[path.name],
                path.name,
            )
        for item in self.lock["completePatchSeries"]:
            path = REPO_ROOT / item["path"]
            self.assertEqual(
                hashlib.sha256(path.read_bytes()).hexdigest(),
                FROZEN_PATCH_SHA256[item["id"]],
                item["id"],
            )

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

    def test_v2_formal_gate_requires_compile_repair_and_rejects_9000(self) -> None:
        expected = diag_gate.expected_series_from_lock(self.lock)
        formal_ids = ["0003", "0003a", "0004"]
        from shutil import copy2

        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            for patch_id in formal_ids:
                name = expected[patch_id]["filename"]
                copy2(SERIES_DIR / name, target / name)
            result = diag_gate.evaluate(diag_gate.MODE_FORMAL, target, expected)
            self.assertTrue(result.ok)
            self.assertTrue(result.formalEligible)
            self.assertFalse(result.diagnosticOnly)

            (target / expected["0003a"]["filename"]).unlink()
            missing = diag_gate.evaluate(diag_gate.MODE_FORMAL, target, expected)
            self.assertFalse(missing.ok)
            self.assertEqual(missing.details["missing"], ["0003a"])

            copy2(
                SERIES_DIR / expected["0003a"]["filename"],
                target / expected["0003a"]["filename"],
            )
            copy2(
                SERIES_DIR / expected["9000"]["filename"],
                target / expected["9000"]["filename"],
            )
            diagnostic = diag_gate.evaluate(
                diag_gate.MODE_FORMAL, target, expected
            )
            self.assertFalse(diagnostic.ok)
            self.assertIn("HARD FAIL", diagnostic.reason)

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
