#!/usr/bin/env python3
"""No-Docker tests for the one-shot Formal R1 host launcher."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import tarfile
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
BUILD_DIR = HOST_DIR / "build" / "r1-formal-v1"
IMAGE_ID = "sha256:" + "1" * 64
RUN_ID = "r1formal-test0001"

spec = importlib.util.spec_from_file_location(
    "verisilo_r1_formal_build_host", BUILD_DIR / "build_host.py"
)
assert spec and spec.loader
build_host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build_host)


def record(path: Path) -> dict:
    data = path.read_bytes()
    return {
        "path": path.as_posix(),
        "sha256": hashlib.sha256(data).hexdigest(),
        "sizeBytes": len(data),
    }


class R1FormalHostLauncherTests(unittest.TestCase):
    def test_context_is_deterministic_and_exact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dockerfile = root / "recipe" / "Dockerfile"
            driver = root / "recipe" / "strict_build.py"
            dockerfile.parent.mkdir()
            dockerfile.write_bytes(b"FROM pinned\n")
            driver.write_bytes(b"#!/usr/bin/env python3\n")
            rows = []
            for path in (dockerfile, driver):
                row = record(path)
                rows.append({
                    "path": path.relative_to(root).as_posix(),
                    "sha256": row["sha256"],
                    "sizeBytes": row["sizeBytes"],
                })
            lock = {"buildBinding": {"recipe": {"files": rows}}}
            first = build_host._create_context(root, lock, root / "first.tar")
            second = build_host._create_context(root, lock, root / "second.tar")
            self.assertEqual(first["sha256"], second["sha256"])
            self.assertEqual(first["members"], ["Dockerfile", "strict_build.py"])
            with tarfile.open(root / "first.tar") as bundle:
                members = bundle.getmembers()
            self.assertEqual([item.name for item in members], list(build_host.CONTEXT_NAMES))
            self.assertEqual([item.mtime for item in members], [0, 0])

    def test_strict_result_rejects_9000_and_runtime_claims(self) -> None:
        result = {
            "recordType": "verisilo-camoufox-r1-formal-build-run/v1",
            "runId": RUN_ID,
            "buildMode": "formal",
            "diagnosticOnly": False,
            "formalSource": True,
            "formalR1Passed": False,
            "browserLaunches": 0,
            "windowsRuntimeObserved": False,
            "runtimeVerified": False,
            "completeAppliedPatchOrder": build_host.ORDER,
            "claims": {"compiled": True},
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "result.json"
            path.write_text(json.dumps(result), encoding="utf-8")
            build_host._validate_strict_result(path, RUN_ID)
            result["completeAppliedPatchOrder"] = [*build_host.ORDER, "9000"]
            path.write_text(json.dumps(result), encoding="utf-8")
            with self.assertRaises(build_host.HostBuildFailure):
                build_host._validate_strict_result(path, RUN_ID)
            result["completeAppliedPatchOrder"] = build_host.ORDER
            result["runtimeVerified"] = True
            path.write_text(json.dumps(result), encoding="utf-8")
            with self.assertRaises(build_host.HostBuildFailure):
                build_host._validate_strict_result(path, RUN_ID)

    def test_execute_uses_exact_image_without_driver_injection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / RUN_ID
            inputs = root / "inputs"
            inputs.mkdir(parents=True)
            lock = {
                "buildBinding": {
                    "recipe": {
                        "fixedEnvironment": {"MOZ_BUILD_DATE": "20260811045234"}
                    }
                }
            }
            source = {
                "commit": "2" * 40,
                "tree": "3" * 40,
                "lockSha256": "4" * 64,
            }
            commands: list[list[str]] = []

            def fake_context(_verisilo, _lock, target):
                target.write_bytes(b"context")
                return {"name": target.name, "sha256": "5" * 64,
                        "sizeBytes": 7, "members": list(build_host.CONTEXT_NAMES)}

            def fake_run(command, _cwd, log, _env, stdin=None):
                commands.append(command)
                log.write_bytes(b"log\n")
                if "buildx" in command:
                    metadata = Path(command[command.index("--metadata-file") + 1])
                    metadata.write_text(
                        json.dumps({"containerimage.config.digest": IMAGE_ID}),
                        encoding="utf-8",
                    )
                else:
                    result_dir = root / "out" / RUN_ID
                    result_dir.mkdir()
                    (result_dir / "build-result.json").write_text("{}", encoding="utf-8")
                return 0

            with (
                mock.patch.object(build_host, "_validate_run_root", return_value=(root, inputs)),
                mock.patch.object(build_host, "_validate_inputs", return_value=(lock, source)),
                mock.patch.object(build_host, "_validate_host", return_value=("/usr/bin/docker", {})),
                mock.patch.object(build_host, "_create_context", side_effect=fake_context),
                mock.patch.object(build_host, "_run_logged", side_effect=fake_run),
                mock.patch.object(build_host, "_inspect_image", return_value={"name": "inspect.json"}),
                mock.patch.object(build_host, "_validate_strict_result"),
            ):
                self.assertEqual(
                    build_host.execute(Namespace(run_id=RUN_ID, run_root=str(root))), 0
                )

            build_command, run_command = commands
            self.assertIn("--no-cache", build_command)
            self.assertIn("--pull=never", run_command)
            self.assertIn(f"type=bind,src={inputs},dst=/inputs,readonly", run_command)
            self.assertNotIn("--entrypoint", run_command)
            self.assertEqual(run_command[run_command.index("--pull=never") + 1], "--read-only")
            provenance = json.loads(
                (root / "provenance" / "host-provenance.json").read_text(encoding="utf-8")
            )
            self.assertFalse(provenance["container"]["driverInjection"])
            self.assertEqual(
                provenance["claims"],
                {
                    "browserLaunches": 0,
                    "formalR1Passed": False,
                    "runtimeVerified": False,
                    "windowsRuntimeObserved": False,
                },
            )


if __name__ == "__main__":
    unittest.main()
