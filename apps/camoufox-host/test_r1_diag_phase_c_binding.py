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
SUPERSEDED_FIXED_ENV_IMAGE_ID = (
    "sha256:2cc52edb2e93e4a52437e0098326612ff7b05d5f3978111cf06c15e65002a323"
)
SUPERSEDED_WINEPREFIX_IMAGE_ID = (
    "sha256:beaa23e93cd17af49a5b97dae4997a84448f61cd65679e90d579a1a1db7366d3"
)
SUPERSEDED_GPC_NAMESPACE_IMAGE_ID = (
    "sha256:706c07f499240f76bc5fcc7ffbc4e9b66e819acf9a54dbc8b57353faae93138b"
)
CURRENT_RUN_ID = "r1diag-builder-20260823t1521z"
CURRENT_IMAGE_ID = (
    "sha256:2742be96b8160fc9206585c8af6abcc95a9bc73d13a39fc2faa1c18c0bb2ea92"
)
CURRENT_PROPOSAL_SHA256 = (
    "fbe82a129e247646d74fd36b808ffb76cc22a7334a271b959379fa6700566e46"
)
CLOSED_STATUS = "diagnostic-engine-build-provenance-closed"
ENGINE_RUN_ID = "r1diag-engine-20260823t1542z"


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
    lock["buildBinding"]["binaryBinding"] = None
    lock["buildBinding"]["builderImageBinding"] = proposal
    lock["builderImagePreparationEvidence"] = evidence
    lock["status"] = build_host.BOUND_LOCK_STATUS
    lock["buildBinding"]["status"] = build_host.BOUND_LOCK_STATUS
    lock["builderOperationalLineage"]["current"] = copy.deepcopy(
        build_host.BOUND_LINEAGE_CURRENT
    )
    return lock


def _preclosure_lock(base: dict) -> dict:
    lock = copy.deepcopy(base)
    lock["buildBinding"]["binaryBinding"] = None
    lock["status"] = build_host.BOUND_LOCK_STATUS
    lock["buildBinding"]["status"] = build_host.BOUND_LOCK_STATUS
    return lock


class R1DiagPhaseCBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

    def test_closed_lock_retains_phase_c8_builder_binding(
        self,
    ) -> None:
        binding = self.lock["buildBinding"]["builderImageBinding"]
        evidence = self.lock["builderImagePreparationEvidence"]
        self.assertEqual(self.lock["status"], CLOSED_STATUS)
        self.assertEqual(self.lock["buildBinding"]["status"], CLOSED_STATUS)
        self.assertEqual(
            self.lock["buildBinding"]["binaryBinding"]["runId"],
            ENGINE_RUN_ID,
        )
        self.assertEqual(binding["imageId"], CURRENT_IMAGE_ID)
        self.assertEqual(
            binding["savedArchiveSha256"],
            "ad48844404013c97acf162d4dd2502007636e30070ae31bb1573387eb99137b2",
        )
        self.assertEqual(binding["savedArchiveSizeBytes"], 1471564288)
        self.assertEqual(
            build_host._canonical_json_sha(binding), CURRENT_PROPOSAL_SHA256
        )
        self.assertEqual(
            evidence["bindingProposalCanonicalSha256"], CURRENT_PROPOSAL_SHA256
        )
        self.assertEqual(evidence["runId"], CURRENT_RUN_ID)
        self.assertEqual(
            evidence["builderImageResultSha256"],
            "79b4c186b33624d9213a2282133331f2eebee5adf85da9988ba51c1c0925b93d",
        )
        self.assertEqual(
            evidence["durableManifestSha256"],
            "ee46c04e52a1f6deedbd9e01c2a50a750d4d9bfca358e21f8ccc8b8a5ec73000",
        )
        self.assertEqual(
            evidence["durableManifestCanonicalSha256"],
            "bb79e9890f2ffade10c940a2c924b8e7bb918d71f9fd4c2d7bf1042f0c252c12",
        )
        self.assertEqual(
            evidence["retentionReceiptSha256"],
            "4f633d82dc532aa52bef229f60c0949f935edbba3d6e0be710ba42861532002e",
        )
        self.assertEqual(
            evidence["retentionReceiptCanonicalSha256"],
            "99cec9c2041b035962d92840afdb098e2eab917e124956bbe51f2d980cb8e934",
        )
        self.assertEqual(
            evidence["sourceCommit"],
            "1579564e703d4b11dae590e1f1bb910f9a3c319d",
        )
        self.assertEqual(
            evidence["sourceTree"],
            "b113cee594f94ab07b663217c6c47908a2055efa",
        )
        self.assertEqual(
            evidence["sourceLockSha256"],
            "1086f39a212a73ceac4c17e771a402751a1912e0a6c913dba64c252a7fead68f",
        )
        self.assertTrue(evidence["retained"])
        self.assertTrue(evidence["reReadable"])
        current = self.lock["builderOperationalLineage"]["current"]
        self.assertEqual(current, build_host.BOUND_LINEAGE_CURRENT)
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
        self.assertNotIn(SUPERSEDED_FIXED_ENV_IMAGE_ID, encoded)
        self.assertNotIn(SUPERSEDED_WINEPREFIX_IMAGE_ID, encoded)
        self.assertNotIn(SUPERSEDED_GPC_NAMESPACE_IMAGE_ID, encoded)
        self.assertEqual(encoded.count(CURRENT_IMAGE_ID), 1)
        self.assertNotEqual(
            self.lock["builderOperationalLineage"]["current"].get("imageId"),
            HISTORICAL_IMAGE_ID,
        )

    def test_preclosure_phase_c8_shape_is_eligible_for_bound_consumption(self) -> None:
        lock = _preclosure_lock(self.lock)
        git_values = iter(["", "1" * 40, "2" * 40])
        with (
            mock.patch.object(
                build_host, "_git", side_effect=lambda *_: next(git_values)
            ),
            mock.patch.object(build_host, "_strict_json", return_value=lock),
            mock.patch.object(build_host, "_sha", return_value="3" * 64),
            mock.patch.object(build_host, "_validate_recipe", return_value={}),
            mock.patch.object(build_host, "_validate_patch_contract"),
        ):
            source, _ = build_host._validate_verisilo(
                Path("unused-checkout"), binding_state="bound"
            )
        self.assertEqual(source["commit"], "1" * 40)

    def test_closed_lock_rejects_prepare_image(self) -> None:
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
                Path("unused-checkout"), binding_state="unbound"
            )

    def test_preclosure_bound_lock_still_requires_prepared_record(self) -> None:
        with self.assertRaises(build_host.HostBuildFailure):
            build_host._validate_bound_binding(
                _preclosure_lock(self.lock), {}, "r1diag-engine-future0001"
            )

    def test_closed_lock_rejects_repeat_bound_consumption(self) -> None:
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

    def test_closed_binary_binding_is_not_a_build_input(self) -> None:
        with self.assertRaisesRegex(
            build_host.HostBuildFailure,
            "diagnostic binary binding must remain null",
        ):
            build_host._validate_durable_contract(self.lock)

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
        unbound["buildBinding"]["binaryBinding"] = None
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
