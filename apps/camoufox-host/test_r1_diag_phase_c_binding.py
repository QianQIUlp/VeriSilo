#!/usr/bin/env python3
"""No-browser Phase C tests for the accepted R1 diagnostic builder binding."""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
LOCK_PATH = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
sys.path.insert(0, str(BUILD_DIR))
import build_host  # noqa: E402


RUN_ID = "r1diag-builder-20260823t0504z"
PHASE_B_SOURCE_COMMIT = "0600e8922ee67322aaf55c5ea10e7ecde9663307"
PHASE_B_SOURCE_TREE = "46471bbb4ca48cd260019b217f5f2a68cd4dbb6c"
PHASE_B_SOURCE_LOCK_SHA256 = "db55354d8b47c3a5379d6bfd26459fdc440ae216df6ddaefbaa3522cd2044c24"
PROPOSAL_CANONICAL_SHA256 = "66d2dad23fa769e9b3fcf55aff91a615d8e1b19d52bfe2676563dfe1adb2251d"
RESULT_SHA256 = "80df26ccd257789ad0b922f6ea80b7d6dcc66cec22fad05bbd6666dc90f1c636"
HISTORICAL_FAILED_IMAGE_ID = "sha256:e6e61c5d5196957d7d60eadec992cfe84a3d6edc0b930c6969e3857878a1021e"

EXPECTED_PROPOSAL = {
    "baseIndexDigest": "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea",
    "baseLinuxAmd64ManifestDigest": "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467",
    "buildxLogSha256": "5ef77404e15f06d783f8a832358e72e946860f7d985d5ff3baec5c73a5dab2f2",
    "buildxLogSizeBytes": 78095,
    "buildxMetadataSha256": "11fac51a48a1c658d7c1a27bfad91d8b9e4c5a7f8e1c1080a3e45a15a6053691",
    "dockerfileSha256": "6bdd56672de8ad1c12f466aa60f1ceb16613267dff8f7bc3ec3058c1c3f6bfb2",
    "hostToolingSha256": "374ac14473fe269ab2577100dccbe219176db6ea8252c8790310fdd0a78f711e",
    "imageId": "sha256:f46ec076dcde9b3759007c3683c07e5a3c563f9145475b335b6f40a82bb6732c",
    "imageInspectSha256": "2c77ff767eec994bd10d1c943273a633c5cdaa5ae6f4b1d1f5bcfa23fe7c6e03",
    "recipeSourceCommit": PHASE_B_SOURCE_COMMIT,
    "recipeSourceLockSha256": PHASE_B_SOURCE_LOCK_SHA256,
    "recipeSourceTree": PHASE_B_SOURCE_TREE,
    "savedArchiveSha256": "8f1ca52564c6b039351e3cee01894fb3c3d28a6e351ab6b145491abc288d03f2",
    "savedArchiveSizeBytes": 483575296,
}

EXPECTED_EVIDENCE = {
    "bindingProposalCanonicalSha256": PROPOSAL_CANONICAL_SHA256,
    "builderImageResultSha256": RESULT_SHA256,
    "runId": RUN_ID,
    "sourceCommit": PHASE_B_SOURCE_COMMIT,
    "sourceLockSha256": PHASE_B_SOURCE_LOCK_SHA256,
    "sourceTree": PHASE_B_SOURCE_TREE,
}


def _canonical_proposal_sha(proposal: dict) -> str:
    encoded = json.dumps(
        proposal, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def _prepared_record(proposal: dict | None = None) -> dict:
    return {
        "recordType": "verisilo-r1-diag-builder-image-result/v2",
        "runId": RUN_ID,
        "status": "prepared-awaiting-source-lock-binding",
        "bindingProposal": dict(proposal or EXPECTED_PROPOSAL),
    }


class R1DiagPhaseCBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

    def test_lock_contains_exact_phase_b_proposal_and_evidence_lineage(self) -> None:
        self.assertEqual(
            self.lock["buildBinding"]["builderImageBinding"], EXPECTED_PROPOSAL
        )
        self.assertEqual(
            self.lock["builderImagePreparationEvidence"], EXPECTED_EVIDENCE
        )
        self.assertEqual(
            self.lock["status"], "builder-bound-awaiting-diagnostic-engine-build"
        )
        self.assertEqual(
            self.lock["buildBinding"]["status"],
            "builder-bound-awaiting-diagnostic-engine-build",
        )

    def test_proposal_required_fields_are_exact(self) -> None:
        self.assertEqual(
            set(EXPECTED_PROPOSAL), set(build_host.REQUIRED_BINDING_FIELDS)
        )
        self.assertEqual(
            set(self.lock["buildBinding"]["builderImageBinding"]),
            set(self.lock["buildBinding"]["builderImageBindingRequiredFields"]),
        )

    def test_missing_or_unknown_binding_field_is_rejected(self) -> None:
        for mutation in ("missing", "unknown"):
            proposal = dict(EXPECTED_PROPOSAL)
            if mutation == "missing":
                proposal.pop("imageId")
            else:
                proposal["unexpectedField"] = "reject-me"
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "builder-image-result.json"
                path.write_text(json.dumps(_prepared_record(proposal)), encoding="utf-8")
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure,
                    "builder image binding proposal fields are not exact",
                ):
                    build_host._validate_prepared_result(path, RUN_ID)

    def test_proposal_sha_and_result_record_sha_are_pinned(self) -> None:
        self.assertEqual(_canonical_proposal_sha(EXPECTED_PROPOSAL), PROPOSAL_CANONICAL_SHA256)
        self.assertEqual(
            self.lock["builderImagePreparationEvidence"]["builderImageResultSha256"],
            RESULT_SHA256,
        )
        drifted = dict(EXPECTED_PROPOSAL)
        drifted["imageId"] = "sha256:" + "0" * 64
        self.assertNotEqual(_canonical_proposal_sha(drifted), PROPOSAL_CANONICAL_SHA256)

    def test_source_commit_tree_and_lock_mismatch_are_rejected(self) -> None:
        for field in ("recipeSourceCommit", "recipeSourceTree", "recipeSourceLockSha256"):
            proposal = dict(EXPECTED_PROPOSAL)
            proposal[field] = "0" * len(proposal[field])
            with self.subTest(field=field):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure,
                    "prepared builder image proposal differs from v2 lock",
                ):
                    build_host._validate_bound_binding(
                        self.lock, _prepared_record(proposal), RUN_ID
                    )

    def test_historical_failed_image_and_run_are_not_current_binding(self) -> None:
        binding = self.lock["buildBinding"]["builderImageBinding"]
        evidence = self.lock["builderImagePreparationEvidence"]
        self.assertNotEqual(binding["imageId"], HISTORICAL_FAILED_IMAGE_ID)
        self.assertNotEqual(evidence["runId"], "r1diag-builder-20260823t0435z")
        self.assertEqual(evidence["runId"], RUN_ID)

    def test_failed_preparation_record_cannot_be_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "builder-image-failure.json").write_text(
                json.dumps(
                    {
                        "recordType": "verisilo-r1-diag-builder-image-failure/v2",
                        "runId": RUN_ID,
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(build_host.HostBuildFailure):
                build_host._validate_prepared_result(
                    root / "builder-image-result.json", RUN_ID
                )

    def test_prepare_image_rejects_bound_lock(self) -> None:
        git_outputs = iter(["", PHASE_B_SOURCE_COMMIT, PHASE_B_SOURCE_TREE])
        with (
            mock.patch.object(build_host, "_git", side_effect=lambda *_: next(git_outputs)),
            mock.patch.object(build_host, "_strict_json", return_value=self.lock),
            mock.patch.object(build_host, "_sha", return_value="f" * 64),
            mock.patch.object(build_host, "_validate_recipe", return_value={}),
            mock.patch.object(build_host, "_validate_patch_contract"),
        ):
            with self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "prepare-image requires builderImageBinding=null",
            ):
                build_host._validate_verisilo(Path("unused-checkout"), binding_state="unbound")

    def test_exact_bound_proposal_is_eligible_for_later_consumption_only(self) -> None:
        prepared = _prepared_record()
        self.assertEqual(
            build_host._validate_bound_binding(self.lock, prepared, RUN_ID),
            EXPECTED_PROPOSAL,
        )
        self.assertIsNone(self.lock["buildBinding"]["binaryBinding"])
        self.assertEqual(self.lock["browserLaunches"], 0)
        self.assertTrue(self.lock["diagnosticOnly"])
        self.assertFalse(self.lock["formalEligible"])


if __name__ == "__main__":
    unittest.main(verbosity=1)
