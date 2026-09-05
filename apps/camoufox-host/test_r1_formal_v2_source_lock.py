#!/usr/bin/env python3
"""Focused no-browser checks for the Formal v2 source/build input."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import unittest
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parents[1]
BUILD_DIR = HOST_DIR / "build" / "r1-formal-v2"
LOCK_PATH = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v2-source.json"
)
ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005", "0006"]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


spec = importlib.util.spec_from_file_location(
    "verisilo_r1_formal_v2_strict_build", BUILD_DIR / "strict_build.py"
)
assert spec and spec.loader
strict_build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(strict_build)

host_spec = importlib.util.spec_from_file_location(
    "verisilo_r1_formal_v2_build_host", BUILD_DIR / "build_host.py"
)
assert host_spec and host_spec.loader
build_host = importlib.util.module_from_spec(host_spec)
host_spec.loader.exec_module(build_host)


class R1FormalV2SourceLockTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

    def validate(self, lock: dict) -> None:
        with mock.patch.object(strict_build, "VERISILO_ROOT", REPO_ROOT):
            strict_build._validate_recipe_and_mode(lock)
            strict_build._validate_patch_contract(lock)

    def test_lock_recipe_and_patch_series_are_exact(self) -> None:
        lock = self.lock
        self.assertEqual(
            lock["engineRevision"],
            "verisilo-camoufox-152.0.4-beta.28-r1-formal-v2",
        )
        self.assertEqual(lock["completeAppliedPatchOrder"], ORDER)
        self.assertEqual([row["id"] for row in lock["completePatchSeries"]], ORDER)
        self.assertNotIn("9000", json.dumps(lock["completePatchSeries"], sort_keys=True))
        self.validate(lock)

        for row in lock["completePatchSeries"]:
            path = REPO_ROOT / row["path"]
            self.assertEqual(
                (sha256(path), path.stat().st_size),
                (row["sha256"], row["sizeBytes"]),
            )
        recipe = lock["buildBinding"]["recipe"]
        self.assertEqual(
            [row["path"] for row in recipe["files"]],
            [
                "apps/camoufox-host/build/r1-formal-v2/Dockerfile",
                "apps/camoufox-host/build/r1-formal-v2/strict_build.py",
            ],
        )
        for row in recipe["files"]:
            path = REPO_ROOT / row["path"]
            self.assertEqual(
                (sha256(path), path.stat().st_size),
                (row["sha256"], row["sizeBytes"]),
            )
        host_tool = lock["buildBinding"]["hostTool"]
        host_path = REPO_ROOT / host_tool["path"]
        self.assertEqual(
            (sha256(host_path), host_path.stat().st_size),
            (host_tool["sha256"], host_tool["sizeBytes"]),
        )

    def test_0006_is_single_file_and_moves_the_existing_call(self) -> None:
        seam = [row for row in self.lock["patchSeams"] if row["id"] == "0006"]
        self.assertEqual(
            seam,
            [
                {
                    "id": "0006",
                    "path": "toolkit/xre/nsAppRunner.cpp",
                    "preSha256": "7847e88093beeff74aa8a7e89f5e5f1e3ea0d6b1f9dece21f97387940fbe8b94",
                    "postSha256": "51252fbfa75731f63a5b5a3f1134a252b370c151aaaa1135a6df930ff9ba23b3",
                }
            ],
        )
        patch = (REPO_ROOT / self.lock["completePatchSeries"][-1]["path"]).read_text(
            encoding="utf-8"
        )
        self.assertEqual(patch.count("ProjectGpcPolicyFromMaskConfig();"), 2)
        self.assertLess(
            patch.index("mDirProvider.FinishInitializingUserPrefs();"),
            patch.index("+    camoucfg::ProjectGpcPolicyFromMaskConfig();"),
        )

    def test_host_launcher_binds_v2_order_and_lock(self) -> None:
        self.assertEqual(build_host.ORDER, ORDER)
        self.assertEqual(
            build_host.LOCK_REL.as_posix(),
            "apps/camoufox-host/lock/"
            "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v2-source.json",
        )

    def test_strict_contract_rejects_missing_0006_and_9000(self) -> None:
        missing = copy.deepcopy(self.lock)
        missing["completePatchSeries"].pop()
        missing["completeAppliedPatchOrder"].pop()
        with self.assertRaises(strict_build.BuildFailure):
            self.validate(missing)

        added = copy.deepcopy(self.lock)
        added["completePatchSeries"].append(
            {"id": "9000", "path": "diagnostic", "diagnosticOnly": True}
        )
        added["completeAppliedPatchOrder"].append("9000")
        with self.assertRaises(strict_build.BuildFailure):
            self.validate(added)


if __name__ == "__main__":
    unittest.main(verbosity=1)
