#!/usr/bin/env python3
"""No-browser fault-injection tests for durable builder evidence."""

from __future__ import annotations

import inspect
import io
import json
import os
import sys
import tarfile
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
sys.path.insert(0, str(BUILD_DIR))
import build_host  # noqa: E402


RUN_ID = "r1diag-builder-durable0001"
QUALIFICATION_ID = "r1diag-durable-qual-test0001"
BOOT_A = "11111111-1111-1111-1111-111111111111"
BOOT_B = "22222222-2222-2222-2222-222222222222"
MOUNT_A = {
    "target": "/var/lib/verisilo",
    "source": "/dev/disk/by-uuid/test-a",
    "filesystemType": "ext4",
    "uuid": "test-a",
}
MOUNT_B = {**MOUNT_A, "uuid": "test-b"}


def _write_image_tar(path: Path, config_bytes: bytes = b"{}") -> str:
    import hashlib

    image_hex = hashlib.sha256(config_bytes).hexdigest()
    manifest = json.dumps(
        [{"Config": f"{image_hex}.json", "RepoTags": [], "Layers": []}],
        separators=(",", ":"),
    ).encode("utf-8")
    with tarfile.open(path, "w") as archive:
        for name, data in (
            ("manifest.json", manifest),
            (f"{image_hex}.json", config_bytes),
        ):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return "sha256:" + image_hex


def _write_build_context(path: Path) -> dict:
    with tarfile.open(path, "w") as archive:
        for name in build_host.BUILD_CONTEXT_MEMBERS:
            data = (name + "\n").encode("utf-8")
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    return build_host._observe_build_context_tar(path)


def _sha(path: Path) -> str:
    import hashlib

    return hashlib.sha256(path.read_bytes()).hexdigest()


def _qualification() -> dict:
    return {
        "qualificationId": QUALIFICATION_ID,
        "mountIdentity": MOUNT_A,
        "requestSha256": "1" * 64,
        "resultSha256": "2" * 64,
    }


def _reserve_and_retain(provenance: Path) -> dict:
    qualification = _qualification()
    build_host._reserve_durable_bundle(RUN_ID, qualification)
    return build_host._retain_durable_bundle(
        provenance, RUN_ID, qualification
    )


def _make_locked_recipe(recipe_dir: Path) -> dict:
    import hashlib

    files = []
    for name in build_host.BUILD_CONTEXT_MEMBERS:
        data = ("frozen-" + name + "\n").encode("utf-8")
        path = recipe_dir / name
        path.write_bytes(data)
        files.append(
            {
                "path": f"apps/camoufox-host/build/r1-diag-v1/{name}",
                "sha256": hashlib.sha256(data).hexdigest(),
                "sizeBytes": len(data),
            }
        )
    return {"files": files}


def _make_provenance(root: Path, run_id: str = RUN_ID) -> tuple[Path, dict]:
    provenance = root / "provenance"
    provenance.mkdir()
    image_tar = provenance / "builder-image.tar"
    image_id = _write_image_tar(image_tar)
    source = {
        "commit": "3" * 40,
        "tree": "4" * 40,
        "lockPath": build_host.LOCK_REL.as_posix(),
        "lockSha256": "5" * 64,
        "dockerfileSha256": "6" * 64,
    }
    labels = {
        "io.verisilo.recipe-source-commit": source["commit"],
        "io.verisilo.recipe-source-tree": source["tree"],
        "io.verisilo.recipe-source-lock-sha256": source["lockSha256"],
        "io.verisilo.recipe-dockerfile-sha256": source["dockerfileSha256"],
    }
    inspect_path = provenance / "builder-image-inspect.json"
    inspect_path.write_text(
        json.dumps([{"Id": image_id, "Config": {"Labels": labels}}]) + "\n",
        encoding="utf-8",
    )
    build_log = provenance / "buildx.log"
    build_log.write_text("build-log\n", encoding="utf-8")
    metadata = provenance / "buildx-metadata.json"
    metadata.write_text(
        json.dumps({"containerimage.config.digest": image_id}) + "\n",
        encoding="utf-8",
    )
    save_log = provenance / "docker-save.log"
    save_log.write_text("save-log\n", encoding="utf-8")
    build_context = _write_build_context(
        provenance / build_host.BUILD_CONTEXT_NAME
    )
    proposal = {
        "imageId": image_id,
        "savedArchiveSha256": _sha(image_tar),
        "savedArchiveSizeBytes": image_tar.stat().st_size,
        "recipeSourceCommit": source["commit"],
        "recipeSourceTree": source["tree"],
        "recipeSourceLockSha256": source["lockSha256"],
        "dockerfileSha256": source["dockerfileSha256"],
        "baseIndexDigest": build_host.EXPECTED_BASE_INDEX_DIGEST,
        "baseLinuxAmd64ManifestDigest": (
            build_host.EXPECTED_BASE_AMD64_MANIFEST_DIGEST
        ),
        "buildxLogSha256": _sha(build_log),
        "buildxLogSizeBytes": build_log.stat().st_size,
        "buildxMetadataSha256": _sha(metadata),
        "imageInspectSha256": _sha(inspect_path),
        "hostToolingSha256": build_host._tooling_sha_from_build_context(
            build_context
        ),
    }
    tar_identity = build_host._validate_saved_image_tar(image_tar, image_id)
    result = {
        "recordType": "verisilo-r1-diag-builder-image-result/v3",
        "runId": run_id,
        "startedAtUtc": "2026-08-23T00:00:00Z",
        "completedAtUtc": "2026-08-23T00:01:00Z",
        "owner": {
            "recordType": "verisilo-r1-diag-build-owner/v1",
            "runId": run_id,
            "createdAtUtc": "2026-08-23T00:00:00Z",
            "pid": 123,
        },
        "source": source,
        "upstream": {
            "commit": "8" * 40,
            "tree": "9" * 40,
            "tag": "v152.0.4-beta.28",
            "archiveSha512": "a" * 128,
            "archiveSizeBytes": 100,
        },
        "archiveProvenance": {
            "archiveOwnerUid": getattr(image_tar.stat(), "st_uid", None),
            "archiveMode": "0o0644",
            "launcherReadable": True,
            "tarIdentity": tar_identity,
        },
        "buildContext": build_context,
        "bindingProposal": proposal,
        "status": "prepared-awaiting-durable-retention",
    }
    (provenance / "builder-image-result.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return provenance, proposal


class _Process:
    def __init__(self, returncode: int = 0, mutate=None) -> None:
        self.returncode = returncode
        self.mutate = mutate

    def wait(self) -> int:
        if self.mutate is not None:
            self.mutate()
        return self.returncode


class _LoggedProcess(_Process):
    def __init__(self, returncode: int = 0, mutate=None) -> None:
        super().__init__(returncode=returncode, mutate=mutate)
        self.stdout = io.StringIO("child-output\n")


class R1DiagDurableEvidenceTests(unittest.TestCase):
    def test_build_context_is_deterministic_and_consumed_via_same_fd(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            recipe_dir = root / "recipe"
            recipe_dir.mkdir()
            recipe = _make_locked_recipe(recipe_dir)
            first = build_host._create_build_context(
                recipe_dir, recipe, root / "context-1.tar"
            )
            second = build_host._create_build_context(
                recipe_dir, recipe, root / "context-2.tar"
            )
            self.assertEqual(first["sha256"], second["sha256"])
            self.assertEqual(first["members"], second["members"])
            frozen_tooling_sha = build_host._tooling_sha_from_build_context(
                first
            )
            observed_fds = []

            def fake_popen(*_args, **kwargs):
                observed_fds.append(kwargs["stdin"].fileno())
                return _LoggedProcess()

            with mock.patch.object(
                build_host.subprocess,
                "Popen",
                side_effect=fake_popen,
            ):
                self.assertEqual(
                    build_host._run_logged_with_binary_stdin(
                        ["fake-buildx"],
                        root,
                        root / "build.log",
                        root / "context-1.tar",
                        first,
                    ),
                    0,
                )
            self.assertEqual(len(observed_fds), 1)
            (recipe_dir / "strict_build.py").write_text(
                "drift\n", encoding="utf-8"
            )
            self.assertEqual(
                build_host._tooling_sha_from_build_context(first),
                frozen_tooling_sha,
            )
            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "changed before context freeze"
            ):
                build_host._create_build_context(
                    recipe_dir, recipe, root / "context-drift.tar"
                )

    def test_build_metadata_digest_and_inspect_labels_are_cross_bound(self) -> None:
        source = {
            "commit": "1" * 40,
            "tree": "2" * 40,
            "lockSha256": "3" * 64,
            "dockerfileSha256": "4" * 64,
        }
        image_id = "sha256:" + "5" * 64
        labels = {
            "io.verisilo.recipe-source-commit": source["commit"],
            "io.verisilo.recipe-source-tree": source["tree"],
            "io.verisilo.recipe-source-lock-sha256": source["lockSha256"],
            "io.verisilo.recipe-dockerfile-sha256": source["dockerfileSha256"],
        }
        inspect_value = [{"Id": image_id, "Config": {"Labels": labels}}]
        with tempfile.TemporaryDirectory() as directory:
            metadata = Path(directory) / "buildx-metadata.json"
            metadata.write_text(
                json.dumps({"containerimage.config.digest": image_id}),
                encoding="utf-8",
            )
            self.assertEqual(
                build_host._image_id_from_build_metadata(metadata), image_id
            )
            self.assertEqual(
                build_host._validate_built_image_inspect(
                    inspect_value, image_id, source
                )["Id"],
                image_id,
            )
            metadata.write_text("{}", encoding="utf-8")
            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "no immutable config digest"
            ):
                build_host._image_id_from_build_metadata(metadata)
        drifted = json.loads(json.dumps(inspect_value))
        drifted[0]["Config"]["Labels"][
            "io.verisilo.recipe-source-lock-sha256"
        ] = "0" * 64
        with self.assertRaisesRegex(
            build_host.HostBuildFailure, "metadata/recipe labels"
        ):
            build_host._validate_built_image_inspect(
                drifted, image_id, source
            )

    def test_prepared_result_rejects_tooling_digest_not_from_frozen_context(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provenance, _ = _make_provenance(Path(directory))
            result_path = provenance / "builder-image-result.json"
            result = json.loads(result_path.read_text(encoding="utf-8"))
            result["bindingProposal"]["hostToolingSha256"] = "0" * 64
            result_path.write_text(
                json.dumps(result, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "tooling digest differs from frozen build context",
            ):
                build_host._validate_prepared_result(result_path, RUN_ID)

    def test_prepared_result_rejects_internal_tar_identity_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provenance, _ = _make_provenance(Path(directory))
            result_path = provenance / "builder-image-result.json"
            result = json.loads(result_path.read_text(encoding="utf-8"))
            result["archiveProvenance"]["tarIdentity"][
                "manifestMember"
            ] = "different.json"
            result_path.write_text(
                json.dumps(result, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "result lineage is malformed"
            ):
                build_host._validate_prepared_result(result_path, RUN_ID)

    def test_prepared_result_accepts_docker_29_oci_tar_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            provenance, _ = _make_provenance(Path(directory))
            result_path = provenance / "builder-image-result.json"
            result = json.loads(result_path.read_text(encoding="utf-8"))
            image_hex = result["bindingProposal"]["imageId"].removeprefix(
                "sha256:"
            )
            result["archiveProvenance"]["tarIdentity"][
                "configMember"
            ] = f"blobs/sha256/{image_hex}"
            result_path.write_text(
                json.dumps(result, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            accepted = build_host._validate_prepared_result(result_path, RUN_ID)
            self.assertEqual(
                accepted["archiveProvenance"]["tarIdentity"]["configMember"],
                f"blobs/sha256/{image_hex}",
            )

    def test_prepare_image_rejects_unqualified_storage_before_docker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            args = Namespace(
                run_root=str(root),
                run_id=RUN_ID,
                qualification_id=QUALIFICATION_ID,
            )
            with (
                mock.patch.object(
                    build_host,
                    "_run_root",
                    return_value=(
                        root,
                        inputs,
                        {"createdAtUtc": "2026-08-23T00:00:00Z"},
                    ),
                ),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    side_effect=build_host.HostBuildFailure("not qualified"),
                ),
                mock.patch.object(build_host, "_run_logged") as docker_build,
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "not qualified"
                ):
                    build_host.prepare_image(args)
            docker_build.assert_not_called()

    def test_retention_preflight_failure_blocks_docker_build(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            args = Namespace(
                run_root=str(root),
                run_id=RUN_ID,
                qualification_id=QUALIFICATION_ID,
            )
            source = {
                "commit": "1" * 40,
                "tree": "2" * 40,
                "lockSha256": "3" * 64,
                "dockerfileSha256": "4" * 64,
            }
            with (
                mock.patch.object(
                    build_host,
                    "_run_root",
                    return_value=(
                        root,
                        inputs,
                        {"createdAtUtc": "2026-08-23T00:00:00Z"},
                    ),
                ),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(
                    build_host,
                    "_validate_verisilo",
                    return_value=(source, {"lock": {}, "recipe": {}}),
                ),
                mock.patch.object(build_host, "_validate_upstream", return_value={}),
                mock.patch.object(
                    build_host, "_reject_historical_preparation_run_id"
                ),
                mock.patch.object(
                    build_host,
                    "_reserve_durable_bundle",
                    side_effect=build_host.HostBuildFailure(
                        "durable preflight unavailable"
                    ),
                ),
                mock.patch.object(
                    build_host, "_run_logged_with_binary_stdin"
                ) as docker_build,
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure,
                    "durable preflight unavailable",
                ):
                    build_host.prepare_image(args)
            docker_build.assert_not_called()

    def test_durable_root_rejects_scratch_filesystem_and_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory).resolve()
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            with (
                mock.patch.object(build_host, "DATA_MOUNT", scratch),
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "shares the scratch filesystem"
                ):
                    build_host._validate_durable_root()
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(Path, "is_symlink", return_value=True),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "must not be a symlink"
                ):
                    build_host._validate_durable_root()

    def test_qualification_rejects_same_boot_and_sentinel_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            durable = Path(directory).resolve()
            args = Namespace(qualification_id=QUALIFICATION_ID)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(build_host, "_read_boot_id", return_value=BOOT_A),
            ):
                build_host.stage_durable_root_qualification(args)
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "requires a different boot"
                ):
                    build_host.verify_durable_root_qualification(args)
            sentinel = durable / QUALIFICATION_ID / build_host.QUALIFICATION_SENTINEL_NAME
            sentinel.write_bytes(b"drift")
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(build_host, "_read_boot_id", return_value=BOOT_B),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "sentinel drifted"
                ):
                    build_host.verify_durable_root_qualification(args)

    def test_qualification_rejects_mount_and_result_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            durable = Path(directory).resolve()
            args = Namespace(qualification_id=QUALIFICATION_ID)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(build_host, "_read_boot_id", side_effect=[BOOT_A, BOOT_B]),
            ):
                build_host.stage_durable_root_qualification(args)
                build_host.verify_durable_root_qualification(args)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_B),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "mount identity drifted"
                ):
                    build_host._validate_durable_qualification(QUALIFICATION_ID)
            result_path = durable / QUALIFICATION_ID / build_host.QUALIFICATION_RESULT_NAME
            result = json.loads(result_path.read_text(encoding="utf-8"))
            result["status"] = "drifted"
            result_path.write_text(json.dumps(result), encoding="utf-8")
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "result drifted"
                ):
                    build_host._validate_durable_qualification(QUALIFICATION_ID)

    def test_retention_copies_exact_bundle_and_revalidates_after_scratch_scope(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, proposal = _make_provenance(scratch)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
            ):
                retained = _reserve_and_retain(provenance)
                reread = build_host._validate_durable_bundle(RUN_ID)
            self.assertTrue(retained["retained"])
            self.assertEqual(reread["proposal"], proposal)
            self.assertEqual(
                set(path.name for path in (durable / RUN_ID).iterdir()),
                set(build_host.DURABLE_BUNDLE_FILES)
                | {
                    build_host.DURABLE_MANIFEST_NAME,
                    build_host.RETENTION_RECEIPT_NAME,
                },
            )

    def test_copy_failure_leaves_no_accepted_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, _ = _make_provenance(scratch)
            original = build_host._copy_file_exclusive_fsync
            calls = 0

            def fail_second(source: Path, target: Path) -> dict:
                nonlocal calls
                calls += 1
                if calls == 2:
                    raise build_host.HostBuildFailure("injected durable copy failure")
                return original(source, target)

            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
                mock.patch.object(
                    build_host, "_copy_file_exclusive_fsync", side_effect=fail_second
                ),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "injected durable copy failure"
                ):
                    _reserve_and_retain(provenance)
            self.assertFalse(
                (durable / RUN_ID / build_host.DURABLE_MANIFEST_NAME).exists()
            )

    def test_fsync_or_manifest_reread_failure_cannot_create_receipt(self) -> None:
        for mutation in ("fsync", "reread"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                scratch = base / "scratch"
                durable = base / "durable"
                scratch.mkdir()
                durable.mkdir()
                provenance, _ = _make_provenance(scratch)
                common = (
                    mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                    mock.patch.object(
                        build_host, "_validate_durable_root", return_value=MOUNT_A
                    ),
                    mock.patch.object(
                        build_host,
                        "_validate_durable_qualification",
                        return_value=_qualification(),
                    ),
                )
                failure = (
                    mock.patch.object(
                        build_host.os,
                        "fsync",
                        side_effect=OSError("injected fsync failure"),
                    )
                    if mutation == "fsync"
                    else mock.patch.object(
                        build_host,
                        "_validate_durable_bundle",
                        side_effect=build_host.HostBuildFailure(
                            "injected durable reread failure"
                        ),
                    )
                )
                with common[0], common[1], common[2], failure:
                    with self.assertRaises(build_host.HostBuildFailure):
                        _reserve_and_retain(provenance)
                self.assertFalse(
                    (durable / RUN_ID / build_host.RETENTION_RECEIPT_NAME).exists()
                )

    def test_bundle_preexistence_and_partial_bundle_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, _ = _make_provenance(scratch)
            bundle = durable / RUN_ID
            bundle.mkdir()
            (bundle / "partial").write_text("partial", encoding="utf-8")
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "bundle already exists"
                ):
                    _reserve_and_retain(provenance)
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "file set is not exact"
                ):
                    build_host._validate_durable_bundle(RUN_ID)
            self.assertEqual((bundle / "partial").read_text(), "partial")

    def test_manifest_missing_unknown_and_hash_drift_are_rejected(self) -> None:
        for mutation in ("missing", "unknown", "drift"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                base = Path(directory)
                scratch = base / "scratch"
                durable = base / "durable"
                scratch.mkdir()
                durable.mkdir()
                provenance, _ = _make_provenance(scratch)
                with (
                    mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                    mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                    mock.patch.object(
                        build_host,
                        "_validate_durable_qualification",
                        return_value=_qualification(),
                    ),
                ):
                    _reserve_and_retain(provenance)
                    bundle = durable / RUN_ID
                    if mutation == "missing":
                        (bundle / "buildx.log").rename(bundle / "buildx.log.missing")
                    elif mutation == "unknown":
                        (bundle / "unknown").write_text("x", encoding="utf-8")
                    else:
                        (bundle / "docker-save.log").write_text(
                            "drift\n", encoding="utf-8"
                        )
                    with self.assertRaises(build_host.HostBuildFailure):
                        build_host._validate_durable_bundle(RUN_ID)

    def test_manifest_without_final_retention_receipt_is_not_acceptable(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, _ = _make_provenance(scratch)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
            ):
                _reserve_and_retain(provenance)
                receipt = durable / RUN_ID / build_host.RETENTION_RECEIPT_NAME
                receipt.rename(durable / RUN_ID / "receipt.missing")
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "file set is not exact"
                ):
                    build_host._validate_durable_bundle(RUN_ID)

    def test_retention_receipt_drift_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, _ = _make_provenance(scratch)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
            ):
                _reserve_and_retain(provenance)
                receipt_path = durable / RUN_ID / build_host.RETENTION_RECEIPT_NAME
                receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
                receipt["reReadable"] = False
                receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "retention receipt"
                ):
                    build_host._validate_durable_bundle(RUN_ID)

    def test_manifest_path_traversal_and_duplicate_json_key_are_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "duplicate.json"
            path.write_text('{"a":1,"a":2}', encoding="utf-8")
            with self.assertRaisesRegex(build_host.HostBuildFailure, "invalid JSON"):
                build_host._strict_json(path)
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            scratch = base / "scratch"
            durable = base / "durable"
            scratch.mkdir()
            durable.mkdir()
            provenance, _ = _make_provenance(scratch)
            with (
                mock.patch.object(build_host, "DURABLE_EVIDENCE_ROOT", durable),
                mock.patch.object(build_host, "_validate_durable_root", return_value=MOUNT_A),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=_qualification(),
                ),
            ):
                _reserve_and_retain(provenance)
                manifest_path = durable / RUN_ID / build_host.DURABLE_MANIFEST_NAME
                manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
                manifest["files"][0]["name"] = "../builder-image.tar"
                manifest["manifestCanonicalSha256"] = (
                    build_host._durable_manifest_canonical_sha(manifest)
                )
                manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
                with self.assertRaises(build_host.HostBuildFailure):
                    build_host._validate_durable_bundle(RUN_ID)

    def test_saved_archive_requires_config_digest_equal_to_image_id(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "builder-image.tar"
            actual_id = _write_image_tar(archive)
            build_host._validate_saved_image_tar(archive, actual_id)
            with self.assertRaisesRegex(
                build_host.HostBuildFailure,
                "config does not name the proposed image ID",
            ):
                build_host._validate_saved_image_tar(
                    archive, "sha256:" + "f" * 64
                )
            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "immutable image ID"
            ):
                build_host._docker_image_save_command("mutable:tag")

    def test_already_present_exact_image_does_not_load(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            image_id = _write_image_tar(archive)
            binding = {
                "imageId": image_id,
                "savedArchiveSha256": _sha(archive),
                "savedArchiveSizeBytes": archive.stat().st_size,
            }
            with (
                mock.patch.object(
                    build_host, "_docker_image_inspect", return_value={"Id": image_id}
                ),
                mock.patch.object(build_host, "_load_binary_stdin") as load,
            ):
                result = build_host._ensure_bound_image(
                    binding, archive, root / "load.log", root
                )
            self.assertEqual(result["action"], "already-present")
            load.assert_not_called()

    def test_absent_image_loads_verified_tar_then_requires_exact_id(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            image_id = _write_image_tar(archive)
            binding = {
                "imageId": image_id,
                "savedArchiveSha256": _sha(archive),
                "savedArchiveSizeBytes": archive.stat().st_size,
            }
            load_evidence = {
                "loadLogSha256": "1" * 64,
                "loadLogSizeBytes": 10,
                "verifiedArchiveSha256": binding["savedArchiveSha256"],
                "verifiedArchiveSizeBytes": binding["savedArchiveSizeBytes"],
            }
            with (
                mock.patch.object(
                    build_host,
                    "_docker_image_inspect",
                    side_effect=[None, {"Id": image_id}],
                ),
                mock.patch.object(
                    build_host, "_load_binary_stdin", return_value=load_evidence
                ) as load,
            ):
                result = build_host._ensure_bound_image(
                    binding, archive, root / "load.log", root
                )
            self.assertEqual(result["action"], "loaded-from-durable-archive")
            load.assert_called_once()
            with (
                mock.patch.object(
                    build_host, "_docker_image_inspect", side_effect=[None, None]
                ),
                mock.patch.object(
                    build_host, "_load_binary_stdin", return_value=load_evidence
                ),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "loaded Docker image ID differs"
                ):
                    build_host._ensure_bound_image(
                        binding, archive, root / "load-2.log", root
                    )

    def test_prepare_bound_image_consumes_only_validated_durable_run_id(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            source = {
                "commit": "1" * 40,
                "tree": "2" * 40,
                "lockPath": build_host.LOCK_REL.as_posix(),
                "lockSha256": "3" * 64,
                "dockerfileSha256": "4" * 64,
            }
            owner = {
                "recordType": "verisilo-r1-diag-build-owner/v1",
                "runId": "r1diag-engine-durable0001",
                "createdAtUtc": "2026-08-23T00:00:00Z",
                "pid": 123,
            }
            scratch = root / "source"
            scratch.mkdir()
            _, proposal = _make_provenance(scratch)
            evidence = {
                "bindingProposalCanonicalSha256": "1" * 64,
                "buildContextSha256": "1" * 64,
                "buildContextSizeBytes": 10240,
                "builderImageResultSha256": "2" * 64,
                "durableManifestCanonicalSha256": "3" * 64,
                "durableManifestSha256": "4" * 64,
                "durableQualificationId": QUALIFICATION_ID,
                "durableQualificationResultSha256": "5" * 64,
                "reReadable": True,
                "retained": True,
                "retentionReceiptCanonicalSha256": "6" * 64,
                "retentionReceiptSha256": "7" * 64,
                "runId": RUN_ID,
                "sourceCommit": "8" * 40,
                "sourceLockSha256": "9" * 64,
                "sourceTree": "a" * 40,
            }
            lock = {
                "buildBinding": {"builderImageBinding": proposal},
                "builderImagePreparationEvidence": evidence,
            }
            durable = {
                "proposal": proposal,
                "bundle": root,
            }
            args = Namespace(
                run_root=str(root),
                run_id=owner["runId"],
                source_run_id=RUN_ID,
            )
            rehydration = {
                "action": "already-present",
                "exactImageIdVerified": True,
                "imageId": proposal["imageId"],
            }
            with (
                mock.patch.object(
                    build_host, "_run_root", return_value=(root, inputs, owner)
                ),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(
                    build_host,
                    "_validate_verisilo",
                    return_value=(source, {"lock": lock}),
                ),
                mock.patch.object(build_host, "_validate_upstream"),
                mock.patch.object(
                    build_host, "_validate_durable_bundle", return_value=durable
                ) as validate_bundle,
                mock.patch.object(
                    build_host,
                    "_validate_bound_durable_evidence",
                    return_value=evidence,
                ),
                mock.patch.object(
                    build_host, "_ensure_bound_image", return_value=rehydration
                ),
            ):
                self.assertEqual(build_host.prepare_bound_image(args), 0)
            validate_bundle.assert_called_once_with(RUN_ID)
            record = json.loads(
                (root / "provenance" / "builder-image-result.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(
                record["status"], "prepared-from-durable-builder-binding"
            )
            self.assertEqual(record["sourceRunId"], RUN_ID)
            self.assertTrue(record["retained"])

    def test_build_engine_rereads_durable_bundle_before_container_run(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            runs_root = Path(directory).resolve()
            run_id = "r1diag-engine-durable0001"
            root = runs_root / run_id
            inputs = root / "inputs"
            provenance = root / "provenance"
            inputs.mkdir(parents=True)
            provenance.mkdir()
            owner = {
                "recordType": "verisilo-r1-diag-build-owner/v1",
                "runId": run_id,
                "createdAtUtc": "2026-08-23T00:00:00Z",
                "pid": 123,
            }
            source = {
                "commit": "1" * 40,
                "tree": "2" * 40,
                "lockPath": build_host.LOCK_REL.as_posix(),
                "lockSha256": "3" * 64,
                "dockerfileSha256": "4" * 64,
            }
            proposal = {
                "imageId": "sha256:" + "5" * 64,
                "savedArchiveSha256": "6" * 64,
            }
            prepared = {
                "runId": run_id,
                "sourceRunId": RUN_ID,
                "owner": owner,
                "source": source,
                "rehydration": {
                    "action": "already-present",
                    "imageId": proposal["imageId"],
                },
            }
            (root / build_host.OWNER_NAME).write_text(
                json.dumps(owner), encoding="utf-8"
            )
            (provenance / "builder-image-result.json").write_text(
                json.dumps(prepared), encoding="utf-8"
            )
            lock = {
                "buildBinding": {
                    "recipe": {
                        "fixedEnvironment": {"MOZ_BUILD_DATE": "20260811045234"}
                    }
                }
            }

            def fake_container(_command: list[str], _cwd: Path, log: Path) -> int:
                log.write_text("container-not-run\n", encoding="utf-8")
                return 1

            args = Namespace(run_root=str(root), run_id=run_id)
            with (
                mock.patch.object(build_host, "RUNS_ROOT", runs_root),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(
                    build_host,
                    "_validate_verisilo",
                    return_value=(source, {"lock": lock}),
                ),
                mock.patch.object(
                    build_host, "_validate_bound_binding", return_value=proposal
                ),
                mock.patch.object(
                    build_host,
                    "_validate_durable_bundle",
                    return_value={"proposal": proposal},
                ) as durable,
                mock.patch.object(
                    build_host, "_validate_bound_durable_evidence"
                ) as bound_evidence,
                mock.patch.object(build_host, "_validate_upstream", return_value={}),
                mock.patch.object(
                    build_host,
                    "_require_exact_image_present",
                    return_value={"Id": proposal["imageId"]},
                ),
                mock.patch.object(
                    build_host, "_run_logged", side_effect=fake_container
                ),
            ):
                self.assertEqual(build_host.build_engine(args), 1)
            durable.assert_called_once_with(RUN_ID)
            bound_evidence.assert_called_once()

    def test_load_nonzero_and_same_fd_archive_mutation_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            image_id = _write_image_tar(archive)
            binding = {
                "imageId": image_id,
                "savedArchiveSha256": _sha(archive),
                "savedArchiveSizeBytes": archive.stat().st_size,
            }
            observed_stdin = []

            def successful_popen(*_args, **kwargs):
                observed_stdin.append(kwargs["stdin"])
                self.assertFalse(kwargs["stdin"].closed)
                return _Process(returncode=0)

            with mock.patch.object(
                build_host.subprocess, "Popen", side_effect=successful_popen
            ):
                success = build_host._load_binary_stdin(
                    binding, archive, root / "load-success.log", root
                )
            self.assertEqual(
                success["verifiedArchiveSha256"], binding["savedArchiveSha256"]
            )
            self.assertEqual(len(observed_stdin), 1)
            with mock.patch.object(
                build_host.subprocess, "Popen", return_value=_Process(returncode=7)
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "exit code 7"
                ):
                    build_host._load_binary_stdin(
                        binding, archive, root / "load-fail.log", root
                    )
            with mock.patch.object(
                build_host.subprocess,
                "Popen",
                return_value=_Process(
                    mutate=lambda: archive.write_bytes(b"mutated-in-place")
                ),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "changed during Docker load"
                ):
                    build_host._load_binary_stdin(
                        binding, archive, root / "load-mutate.log", root
                    )

    def test_tar_drift_is_rejected_before_any_load(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            image_id = _write_image_tar(archive)
            binding = {
                "imageId": image_id,
                "savedArchiveSha256": _sha(archive),
                "savedArchiveSizeBytes": archive.stat().st_size,
            }
            archive.write_bytes(b"drift")
            with mock.patch.object(build_host.subprocess, "Popen") as popen:
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "differs from binding"
                ):
                    build_host._ensure_bound_image(
                        binding, archive, root / "load.log", root
                    )
            popen.assert_not_called()

    def test_hostile_docker_environment_is_ignored_and_commands_are_absolute(self) -> None:
        hostile = {
            "DOCKER_HOST": "tcp://attacker:2375",
            "DOCKER_CONTEXT": "attacker",
            "HTTP_PROXY": "http://attacker",
            "PATH": "attacker",
        }
        with mock.patch.dict(os.environ, hostile, clear=True):
            self.assertEqual(build_host._docker_environment(), build_host.DOCKER_ENV)
        self.assertEqual(build_host.DOCKER[0], "/usr/bin/sudo")
        self.assertIn("/usr/bin/env", build_host.DOCKER)
        self.assertIn("-i", build_host.DOCKER)
        self.assertEqual(build_host.DOCKER[-1], "/usr/bin/docker")

    def test_cli_and_engine_source_have_no_injection_or_implicit_recovery(self) -> None:
        main_source = inspect.getsource(build_host.main)
        bound_source = inspect.getsource(build_host.prepare_bound_image)
        engine_source = inspect.getsource(build_host.build_engine)
        combined = bound_source + engine_source
        self.assertIn("stage-durable-root-qualification", main_source)
        self.assertIn("verify-durable-root-qualification", main_source)
        self.assertNotIn("source-run-root", main_source)
        self.assertNotIn("chmod", combined)
        self.assertNotIn("chown", combined)
        self.assertNotIn("docker cp", combined)
        self.assertNotIn("--entrypoint", combined)
        self.assertIn('"--pull=never"', engine_source)
        self.assertIn("_require_exact_image_present", engine_source)
        self.assertIn("_validate_durable_bundle", engine_source)
        self.assertIn("_validate_bound_durable_evidence", engine_source)
        self.assertNotIn("_ensure_bound_image", engine_source)


if __name__ == "__main__":
    unittest.main(verbosity=1)
