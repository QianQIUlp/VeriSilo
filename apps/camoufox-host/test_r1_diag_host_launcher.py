#!/usr/bin/env python3
"""No-browser tests for the R1 diagnostic host provenance boundary."""

from __future__ import annotations

import inspect
import json
import os
import sys
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-diag-v1"
sys.path.insert(0, str(BUILD_DIR))
import build_host  # noqa: E402


IMAGE_ID = "sha256:" + "1" * 64
QUALIFICATION = {
    "qualificationId": "r1diag-durable-qual-test0001",
    "mountIdentity": {
        "target": "/var/lib/verisilo",
        "source": "/dev/test",
        "filesystemType": "ext4",
        "uuid": "test-uuid",
    },
    "requestSha256": "2" * 64,
    "resultSha256": "3" * 64,
}
BUILD_CONTEXT = {
    "name": build_host.BUILD_CONTEXT_NAME,
    "sha256": "4" * 64,
    "sizeBytes": 10240,
    "members": [
        {"name": name, "sha256": "5" * 64, "sizeBytes": 1}
        for name in build_host.BUILD_CONTEXT_MEMBERS
    ],
}


def _child(code: str) -> list[str]:
    return [sys.executable, "-c", code]


def _fake_buildx(
    command: list[str],
    _cwd: Path,
    log_path: Path,
    _input_path: Path,
    _expected_input: dict,
) -> int:
    if command[-1] != "-" or command[command.index("--file") + 1] != "Dockerfile":
        raise AssertionError("buildx must consume the frozen tar context from stdin")
    log_path.write_text("build-log\n", encoding="utf-8")
    metadata_path = Path(command[command.index("--metadata-file") + 1])
    metadata_path.write_text(
        json.dumps({"containerimage.config.digest": IMAGE_ID}) + "\n",
        encoding="utf-8",
    )
    return 0


def _inspect_json(source: dict) -> str:
    return json.dumps(
        [
            {
                "Id": IMAGE_ID,
                "Config": {
                    "Labels": {
                        "io.verisilo.recipe-source-commit": source["commit"],
                        "io.verisilo.recipe-source-tree": source["tree"],
                        "io.verisilo.recipe-source-lock-sha256": source[
                            "lockSha256"
                        ],
                        "io.verisilo.recipe-dockerfile-sha256": source[
                            "dockerfileSha256"
                        ],
                    }
                },
            }
        ]
    )


class R1DiagHostLauncherTests(unittest.TestCase):
    def test_save_command_has_no_path_redirection_or_o_option(self) -> None:
        command = build_host._docker_image_save_command(IMAGE_ID)
        self.assertEqual(command, [*build_host.DOCKER, "image", "save", IMAGE_ID])
        self.assertNotIn("-o", command)
        self.assertNotIn("builder-image.tar", command)

    def test_success_stream_is_binary_and_stderr_is_separate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            save_log = root / "docker-save.log"
            command = _child(
                "import sys; sys.stdout.buffer.write(b'\\x00archive\\xff'); "
                "sys.stdout.buffer.flush(); print('stderr-marker', file=sys.stderr)"
            )

            self.assertEqual(
                build_host._save_binary_stdout(command, archive, save_log, root),
                0,
            )
            self.assertEqual(archive.read_bytes(), b"\x00archive\xff")
            log_text = save_log.read_text(encoding="utf-8")
            self.assertIn("stderr-marker", log_text)
            self.assertNotIn(archive.name, log_text)
            details = build_host._archive_provenance(archive)
            self.assertTrue(details["launcherReadable"])
            self.assertRegex(details["archiveMode"], r"^0o[0-7]{3,4}$")
            owner_uid = details["archiveOwnerUid"]
            launcher_uid = getattr(os, "getuid", lambda: None)()
            if owner_uid is not None and launcher_uid is not None:
                self.assertEqual(owner_uid, launcher_uid)

    def test_preexisting_archive_is_rejected_without_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            save_log = root / "docker-save.log"
            archive.write_bytes(b"frozen-existing-bytes")

            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "refusing to overwrite builder image archive"
            ):
                build_host._save_binary_stdout(
                    _child("import sys; sys.stdout.buffer.write(b'new')"),
                    archive,
                    save_log,
                    root,
                )
            self.assertEqual(archive.read_bytes(), b"frozen-existing-bytes")
            self.assertIn("save-output-failed", save_log.read_text(encoding="utf-8"))

    def test_nonzero_save_preserves_partial_archive_and_separate_stderr(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            save_log = root / "docker-save.log"
            command = _child(
                "import sys; sys.stdout.buffer.write(b'partial'); "
                "sys.stdout.buffer.flush(); print('save-failed-marker', file=sys.stderr); "
                "raise SystemExit(7)"
            )

            self.assertEqual(
                build_host._save_binary_stdout(command, archive, save_log, root),
                7,
            )
            self.assertEqual(archive.read_bytes(), b"partial")
            self.assertIn("save-failed-marker", save_log.read_text(encoding="utf-8"))

    def test_successful_zero_byte_save_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "builder-image.tar"
            save_log = root / "docker-save.log"

            with self.assertRaisesRegex(
                build_host.HostBuildFailure, "not a non-empty regular file"
            ):
                build_host._save_binary_stdout(
                    _child("pass"), archive, save_log, root
                )
            self.assertEqual(archive.stat().st_size, 0)

    def test_archive_read_permission_error_is_typed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "builder-image.tar"
            archive.write_bytes(b"archive")
            with mock.patch.object(os, "open", side_effect=PermissionError("denied")):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure,
                    "builder image archive .* is unavailable",
                ):
                    build_host._archive_provenance(archive)

    def test_save_helper_has_no_filesystem_repair_path(self) -> None:
        source = inspect.getsource(build_host._save_binary_stdout)
        self.assertNotIn("chmod", source)
        self.assertNotIn("chown", source)

    def test_prepare_image_does_not_write_result_after_save_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            owner = {"createdAtUtc": "2026-08-23T00:00:00Z", "runId": "r1diag-test-run"}
            source = {
                "commit": "a" * 40,
                "tree": "b" * 40,
                "lockSha256": "c" * 64,
                "dockerfileSha256": "d" * 64,
            }
            locked = {"lock": {}, "recipe": {}}
            args = Namespace(
                run_root=str(root),
                run_id="r1diag-test-run",
                qualification_id=QUALIFICATION["qualificationId"],
            )

            with (
                mock.patch.object(build_host, "_run_root", return_value=(root, inputs, owner)),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=QUALIFICATION,
                ),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(build_host, "_validate_verisilo", return_value=(source, locked)),
                mock.patch.object(build_host, "_validate_upstream", return_value={}),
                mock.patch.object(
                    build_host, "_reject_historical_preparation_run_id"
                ),
                mock.patch.object(build_host, "_reserve_durable_bundle"),
                mock.patch.object(
                    build_host, "_create_build_context", return_value=BUILD_CONTEXT
                ),
                mock.patch.object(
                    build_host,
                    "_run_logged_with_binary_stdin",
                    side_effect=_fake_buildx,
                ),
                mock.patch.object(
                    build_host,
                    "_capture",
                    return_value=_inspect_json(source),
                ),
                mock.patch.object(build_host, "_save_binary_stdout", return_value=7),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "builder image save failed"
                ):
                    build_host.prepare_image(args)

            provenance = root / "provenance"
            self.assertFalse((provenance / "builder-image-result.json").exists())
            failure = json.loads(
                (provenance / "builder-image-failure.json").read_text(encoding="utf-8")
            )
            self.assertEqual(failure["runId"], "r1diag-test-run")

    def test_prepare_image_does_not_write_result_before_hash_size_closure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            owner = {"createdAtUtc": "2026-08-23T00:00:00Z", "runId": "r1diag-test-run"}
            source = {
                "commit": "a" * 40,
                "tree": "b" * 40,
                "lockSha256": "c" * 64,
                "dockerfileSha256": "d" * 64,
            }
            locked = {"lock": {}, "recipe": {}}
            args = Namespace(
                run_root=str(root),
                run_id="r1diag-test-run",
                qualification_id=QUALIFICATION["qualificationId"],
            )

            with (
                mock.patch.object(build_host, "_run_root", return_value=(root, inputs, owner)),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=QUALIFICATION,
                ),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(build_host, "_validate_verisilo", return_value=(source, locked)),
                mock.patch.object(build_host, "_validate_upstream", return_value={}),
                mock.patch.object(
                    build_host, "_reject_historical_preparation_run_id"
                ),
                mock.patch.object(build_host, "_reserve_durable_bundle"),
                mock.patch.object(
                    build_host, "_create_build_context", return_value=BUILD_CONTEXT
                ),
                mock.patch.object(
                    build_host,
                    "_run_logged_with_binary_stdin",
                    side_effect=_fake_buildx,
                ),
                mock.patch.object(
                    build_host,
                    "_capture",
                    return_value=_inspect_json(source),
                ),
                mock.patch.object(build_host, "_save_binary_stdout", return_value=0),
            ):
                with self.assertRaisesRegex(
                    build_host.HostBuildFailure, "builder image archive .* is unavailable"
                ):
                    build_host.prepare_image(args)

            self.assertFalse((root / "provenance" / "builder-image-result.json").exists())

    def test_prepare_image_records_launcher_owned_archive_provenance_on_success(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            inputs = root / "inputs"
            inputs.mkdir()
            owner = {"createdAtUtc": "2026-08-23T00:00:00Z", "runId": "r1diag-test-run"}
            source = {
                "commit": "a" * 40,
                "tree": "b" * 40,
                "lockSha256": "c" * 64,
                "dockerfileSha256": "d" * 64,
            }
            locked = {"lock": {}, "recipe": {}}
            args = Namespace(
                run_root=str(root),
                run_id="r1diag-test-run",
                qualification_id=QUALIFICATION["qualificationId"],
            )

            def fake_save(_command: list[str], archive: Path, save_log: Path, _cwd: Path) -> int:
                self.assertEqual(
                    _command,
                    [*build_host.DOCKER, "image", "save", IMAGE_ID],
                )
                archive.write_bytes(b"builder-image-archive")
                save_log.write_text("save-log\n", encoding="utf-8")
                return 0

            with (
                mock.patch.object(build_host, "_run_root", return_value=(root, inputs, owner)),
                mock.patch.object(
                    build_host,
                    "_validate_durable_qualification",
                    return_value=QUALIFICATION,
                ),
                mock.patch.object(build_host, "_validate_input_names"),
                mock.patch.object(build_host, "_validate_verisilo", return_value=(source, locked)),
                mock.patch.object(build_host, "_validate_upstream", return_value={}),
                mock.patch.object(
                    build_host, "_reject_historical_preparation_run_id"
                ),
                mock.patch.object(build_host, "_reserve_durable_bundle"),
                mock.patch.object(
                    build_host, "_create_build_context", return_value=BUILD_CONTEXT
                ),
                mock.patch.object(
                    build_host,
                    "_run_logged_with_binary_stdin",
                    side_effect=_fake_buildx,
                ),
                mock.patch.object(
                    build_host,
                    "_capture",
                    return_value=_inspect_json(source),
                ) as capture,
                mock.patch.object(build_host, "_save_binary_stdout", side_effect=fake_save),
                mock.patch.object(
                    build_host,
                    "_validate_saved_image_tar",
                    return_value={"imageId": IMAGE_ID},
                ),
                mock.patch.object(build_host, "_retain_durable_bundle") as retain,
            ):
                self.assertEqual(build_host.prepare_image(args), 0)

            result = json.loads(
                (root / "provenance" / "builder-image-result.json").read_text(
                    encoding="utf-8"
                )
            )
            self.assertEqual(result["status"], "prepared-awaiting-durable-retention")
            self.assertEqual(result["archiveProvenance"]["launcherReadable"], True)
            self.assertEqual(
                result["bindingProposal"]["savedArchiveSizeBytes"],
                len(b"builder-image-archive"),
            )
            self.assertEqual(
                result["bindingProposal"]["hostToolingSha256"],
                build_host._tooling_sha_from_build_context(BUILD_CONTEXT),
            )
            retain.assert_called_once_with(
                root / "provenance", "r1diag-test-run", QUALIFICATION
            )
            capture.assert_called_once_with(
                [*build_host.DOCKER, "image", "inspect", IMAGE_ID],
                environment=build_host.DOCKER_ENV,
            )

    def test_failed_run_without_result_cannot_be_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            failure = root / "builder-image-failure.json"
            failure.write_text(
                json.dumps(
                    {
                        "recordType": "verisilo-r1-diag-builder-image-failure/v2",
                        "runId": "r1diag-test-run",
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(build_host.HostBuildFailure):
                build_host._validate_prepared_result(
                    root / "builder-image-result.json", "r1diag-test-run"
                )


if __name__ == "__main__":
    unittest.main(verbosity=1)
