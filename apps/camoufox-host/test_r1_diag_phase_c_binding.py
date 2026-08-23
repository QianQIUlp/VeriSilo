#!/usr/bin/env python3
"""No-browser tests for superseded Phase C-1 and current durable binding."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
LOCK_PATH = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
)
sys.path.insert(0, str(BUILD_DIR))
import build_host  # noqa: E402


HISTORICAL_RUN_ID = "r1diag-builder-20260823t0504z"
HISTORICAL_IMAGE_ID = (
    "sha256:f46ec076dcde9b3759007c3683c07e5a3c563f9145475b335b6f40a82bb6732c"
)
HISTORICAL_FAILED_IMAGE_ID = (
    "sha256:e6e61c5d5196957d7d60eadec992cfe84a3d6edc0b930c6969e3857878a1021e"
)
HISTORICAL_PROPOSAL_SHA256 = (
    "66d2dad23fa769e9b3fcf55aff91a615d8e1b19d52bfe2676563dfe1adb2251d"
)
SUPERSEDED_RECIPE_IMAGE_ID = (
    "sha256:b30686aefb44a6162d1ef5527a42920873c2faeae80527c7306e1beafa3c4706"
)
SUPERSEDED_GATE_LOADER_IMAGE_ID = (
    "sha256:271885058636c5390ad8d6e6ffe66f43ea0377c6abf9d4b7e0adb57382581da4"
)
SUPERSEDED_UNPINNED_RUST_IMAGE_ID = (
    "sha256:610c93044112e0330854b9765f19637c1d3edd51dd0a3ab792df72fccabd4301"
)


def _proposal() -> dict:
    return {
        "baseIndexDigest": build_host.EXPECTED_BASE_INDEX_DIGEST,
        "baseLinuxAmd64ManifestDigest": (
            build_host.EXPECTED_BASE_AMD64_MANIFEST_DIGEST
        ),
        "buildxLogSha256": "1" * 64,
        "buildxLogSizeBytes": 17,
        "buildxMetadataSha256": "2" * 64,
        "dockerfileSha256": "3" * 64,
        "hostToolingSha256": "4" * 64,
        "imageId": "sha256:" + "5" * 64,
        "imageInspectSha256": "6" * 64,
        "recipeSourceCommit": "7" * 40,
        "recipeSourceLockSha256": "8" * 64,
        "recipeSourceTree": "9" * 40,
        "savedArchiveSha256": "a" * 64,
        "savedArchiveSizeBytes": 1234,
    }


def _evidence(run_id: str = "r1diag-builder-future0001") -> dict:
    return {
        "bindingProposalCanonicalSha256": "b" * 64,
        "buildContextSha256": "1" * 64,
        "buildContextSizeBytes": 10240,
        "builderImageResultSha256": "c" * 64,
        "durableManifestCanonicalSha256": "d" * 64,
        "durableManifestSha256": "e" * 64,
        "durableQualificationId": "r1diag-durable-qual-future01",
        "durableQualificationResultSha256": "f" * 64,
        "reReadable": True,
        "retained": True,
        "retentionReceiptCanonicalSha256": "a" * 64,
        "retentionReceiptSha256": "b" * 64,
        "runId": run_id,
        "sourceCommit": "7" * 40,
        "sourceLockSha256": "8" * 64,
        "sourceTree": "9" * 40,
    }


def _bound_preparation(run_id: str, proposal: dict, evidence: dict) -> dict:
    return {
        "recordType": "verisilo-r1-diag-bound-image-preparation/v3",
        "runId": run_id,
        "sourceRunId": evidence["runId"],
        "owner": {
            "recordType": "verisilo-r1-diag-build-owner/v1",
            "runId": run_id,
            "createdAtUtc": "2026-08-23T00:00:00Z",
            "pid": 123,
        },
        "source": {
            "commit": "1" * 40,
            "tree": "2" * 40,
            "lockPath": build_host.LOCK_REL.as_posix(),
            "lockSha256": "3" * 64,
            "dockerfileSha256": "4" * 64,
        },
        "bindingProposal": proposal,
        "durableEvidence": evidence,
        "rehydration": {
            "action": "already-present",
            "exactImageIdVerified": True,
            "imageId": proposal["imageId"],
        },
        "retained": True,
        "status": "prepared-from-durable-builder-binding",
    }


def _bound_lock(base: dict, proposal: dict, evidence: dict) -> dict:
    lock = copy.deepcopy(base)
    lock["buildBinding"]["builderImageBinding"] = proposal
    lock["builderImagePreparationEvidence"] = evidence
    lock["status"] = build_host.BOUND_LOCK_STATUS
    lock["buildBinding"]["status"] = build_host.BOUND_LOCK_STATUS
    lock["builderOperationalLineage"]["current"] = copy.deepcopy(
        build_host.BOUND_LINEAGE_CURRENT
    )
    return lock


class R1DiagPhaseCBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

    def test_current_lock_is_unbound_after_rust_toolchain_pin(
        self,
    ) -> None:
        self.assertIsNone(self.lock["buildBinding"]["builderImageBinding"])
        self.assertIsNone(self.lock["builderImagePreparationEvidence"])
        self.assertEqual(self.lock["status"], build_host.UNBOUND_LOCK_STATUS)
        self.assertEqual(
            self.lock["buildBinding"]["status"], build_host.UNBOUND_LOCK_STATUS
        )
        current = self.lock["builderOperationalLineage"]["current"]
        self.assertEqual(current, build_host.UNBOUND_LINEAGE_CURRENT)
        self.assertIn(
            "unbounded_rustup_stable_broke_firefox_152_triplet_detection",
            current["reasonCodes"],
        )
        historical = self.lock["builderOperationalLineage"]["supersededPhaseC1"]
        self.assertEqual(historical["runId"], HISTORICAL_RUN_ID)
        self.assertEqual(historical["imageId"], HISTORICAL_IMAGE_ID)
        self.assertEqual(
            historical["bindingProposalCanonicalSha256"],
            HISTORICAL_PROPOSAL_SHA256,
        )
        self.assertEqual(historical["bindingCorrectness"], "historically-accepted")
        self.assertFalse(historical["operationallyConsumable"])

    def test_lost_and_failed_images_exist_only_in_historical_lineage(self) -> None:
        encoded = json.dumps(self.lock, sort_keys=True)
        self.assertEqual(encoded.count(HISTORICAL_IMAGE_ID), 1)
        self.assertEqual(encoded.count(HISTORICAL_FAILED_IMAGE_ID), 1)
        self.assertNotIn(SUPERSEDED_RECIPE_IMAGE_ID, encoded)
        self.assertNotIn(SUPERSEDED_GATE_LOADER_IMAGE_ID, encoded)
        self.assertNotIn(SUPERSEDED_UNPINNED_RUST_IMAGE_ID, encoded)
        self.assertNotEqual(
            self.lock["builderOperationalLineage"]["current"].get("imageId"),
            HISTORICAL_IMAGE_ID,
        )

    def test_current_unbound_lock_is_eligible_for_prepare_image(self) -> None:
        git_values = iter(["", "1" * 40, "2" * 40])
        with (
            mock.patch.object(
                build_host, "_git", side_effect=lambda *_: next(git_values)
            ),
            mock.patch.object(build_host, "_strict_json", return_value=self.lock),
            mock.patch.object(build_host, "_sha", return_value="3" * 64),
            mock.patch.object(build_host, "_validate_recipe", return_value={}),
            mock.patch.object(build_host, "_validate_patch_contract"),
        ):
            source, _ = build_host._validate_verisilo(
                Path("unused-checkout"), binding_state="unbound"
            )
        self.assertEqual(source["commit"], "1" * 40)

    def test_current_unbound_lock_rejects_bound_consumption(self) -> None:
        git_values = iter(["", "1" * 40, "2" * 40])
        with (
            mock.patch.object(
                build_host, "_git", side_effect=lambda *_: next(git_values)
            ),
            mock.patch.object(build_host, "_strict_json", return_value=self.lock),
            mock.patch.object(build_host, "_sha", return_value="3" * 64),
            mock.patch.object(build_host, "_validate_recipe", return_value={}),
            mock.patch.object(build_host, "_validate_patch_contract"),
            self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "operational status is not exact",
            ),
        ):
            build_host._validate_verisilo(
                Path("unused-checkout"), binding_state="bound"
            )

    def test_current_unbound_lock_is_not_engine_eligible(self) -> None:
        with self.assertRaises(build_host.HostBuildFailure):
            build_host._validate_bound_binding(
                self.lock, {}, "r1diag-engine-future0001"
            )

    def test_historical_raw_phase_b_result_is_not_durably_acceptable(self) -> None:
        historical = {
            "recordType": "verisilo-r1-diag-builder-image-result/v2",
            "runId": HISTORICAL_RUN_ID,
            "status": "prepared-awaiting-source-lock-binding",
            "bindingProposal": {"imageId": HISTORICAL_IMAGE_ID},
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "builder-image-result.json"
            path.write_text(json.dumps(historical), encoding="utf-8")
            with self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "not pending durable retention",
            ):
                build_host._validate_prepared_result(path, HISTORICAL_RUN_ID)

    def test_future_binding_requires_all_durable_evidence_fields(self) -> None:
        proposal = _proposal()
        lock = _bound_lock(
            self.lock,
            proposal,
            {"bindingProposalCanonicalSha256": "b" * 64},
        )
        with self.assertRaisesRegex(
            build_host.HostBuildFailure, "durable evidence is malformed"
        ):
            build_host._validate_bound_binding(
                lock,
                _bound_preparation(
                    "r1diag-engine-future0001",
                    proposal,
                    _evidence(),
                ),
                "r1diag-engine-future0001",
            )

    def test_exact_future_durable_binding_can_pass_offline_validator(self) -> None:
        proposal = _proposal()
        evidence = _evidence()
        lock = _bound_lock(self.lock, proposal, evidence)
        run_id = "r1diag-engine-future0001"
        self.assertEqual(
            build_host._validate_bound_binding(
                lock,
                _bound_preparation(run_id, proposal, evidence),
                run_id,
            ),
            proposal,
        )

    def test_bound_status_with_unbound_operational_lineage_is_rejected(self) -> None:
        proposal = _proposal()
        evidence = _evidence()
        lock = _bound_lock(self.lock, proposal, evidence)
        lock["builderOperationalLineage"]["current"] = copy.deepcopy(
            build_host.UNBOUND_LINEAGE_CURRENT
        )
        with self.assertRaisesRegex(
            build_host.HostBuildFailure,
            "operational lineage disagrees with binding state",
        ):
            build_host._validate_bound_binding(
                lock,
                _bound_preparation(
                    "r1diag-engine-future0001", proposal, evidence
                ),
                "r1diag-engine-future0001",
            )

    def test_historical_builder_run_ids_cannot_be_reused(self) -> None:
        unbound = copy.deepcopy(self.lock)
        unbound["buildBinding"]["builderImageBinding"] = None
        unbound["builderImagePreparationEvidence"] = None
        unbound["status"] = build_host.UNBOUND_LOCK_STATUS
        unbound["buildBinding"]["status"] = build_host.UNBOUND_LOCK_STATUS
        unbound["builderOperationalLineage"]["current"] = copy.deepcopy(
            build_host.UNBOUND_LINEAGE_CURRENT
        )
        for run_id in (
            build_host.HISTORICAL_FAILED_RUN_ID,
            build_host.HISTORICAL_SUPERSEDED_RUN_ID,
        ):
            with self.subTest(run_id=run_id), self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "historical R1 builder run-id cannot be reused",
            ):
                build_host._reject_historical_preparation_run_id(
                    run_id, unbound
                )

    def test_historical_proposal_digest_remains_auditably_exact(self) -> None:
        historical = self.lock["builderOperationalLineage"]["supersededPhaseC1"]
        self.assertEqual(
            historical["bindingProposalCanonicalSha256"],
            HISTORICAL_PROPOSAL_SHA256,
        )
        self.assertEqual(
            historical["sourceLockSha256"],
            "fc9635500a8667520a1cd4b28ebd9fea31c8bf23a341c4bba2df42c575a84b46",
        )


if __name__ == "__main__":
    unittest.main(verbosity=1)
