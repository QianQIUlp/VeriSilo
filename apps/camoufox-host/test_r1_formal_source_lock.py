#!/usr/bin/env python3
"""No-browser checks for the Formal R1 source lock and strict recipe."""

from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


HOST_DIR = Path(__file__).resolve().parent
REPO_ROOT = HOST_DIR.parents[1]
BUILD_DIR = HOST_DIR / "build" / "r1-formal-v1"
LOCK_PATH = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v1-source.json"
)
SHARED_LOCK_PATH = (
    HOST_DIR
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-r1-diag-v2-source.json"
)
ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005"]
PATCH_BINDINGS = {
    "0000": ("8d407bdc4010f7b2989f206a70909bfa9ad89046ddb9e17fa76092c864433600", 1184),
    "0001": ("4fa6d3bbf203e2385e29a72ec2669ee17a571281be7ee2a73598e38918069b02", 2121),
    "0002": ("efb006d5b2b05756fc310b52eb48e0bdab5e8b23e780fa08534a7fc099c22ce7", 3059),
    "0003": ("3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b", 2774),
    "0003a": ("c2f9a9f88ba8aeb610eb1cb29f2515f1d79fcf582393397a571bc3206889588c", 500),
    "0004": ("5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185", 412),
    "0005": ("998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec", 344),
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


spec = importlib.util.spec_from_file_location(
    "verisilo_r1_formal_strict_build", BUILD_DIR / "strict_build.py"
)
assert spec and spec.loader
strict_build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(strict_build)


class R1FormalSourceLockTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.lock = json.loads(LOCK_PATH.read_text(encoding="utf-8"))

    def validate(self, lock: dict) -> None:
        with mock.patch.object(strict_build, "VERISILO_ROOT", REPO_ROOT):
            strict_build._validate_recipe_and_mode(lock)
            strict_build._validate_patch_contract(lock)

    def test_lock_and_recipe_are_exact(self) -> None:
        lock = self.lock
        self.assertEqual(lock["schema"], "verisilo-r1-formal-source-binding/v1")
        self.assertEqual(lock["buildMode"], "formal")
        self.assertFalse(lock["diagnosticOnly"])
        self.assertTrue(lock["formalSource"])
        self.assertFalse(lock["formalR1Passed"])
        self.assertEqual(lock["browserLaunches"], 0)
        self.assertFalse(lock["windowsRuntimeObserved"])
        self.assertFalse(lock["runtimeVerified"])
        self.validate(lock)

        recipe = lock["buildBinding"]["recipe"]
        self.assertNotIn("runtimeContainer", lock["buildBinding"])
        self.assertEqual(
            recipe["order"][-7:],
            [
                "configure-windows-x86_64-and-bootstrap-toolchains",
                "resolve-bound-windows-toolchain-before-build",
                "build-windows-x86_64",
                "resolve-bound-windows-toolchain-before-package",
                "package-windows-x86_64-with-explicit-bound-crt",
                "resolve-bound-windows-toolchain-after-package",
                "freeze-candidate-archive",
            ],
        )
        self.assertEqual(
            [row["path"] for row in recipe["files"]],
            [
                "apps/camoufox-host/build/r1-formal-v1/Dockerfile",
                "apps/camoufox-host/build/r1-formal-v1/strict_build.py",
            ],
        )
        for row in recipe["files"]:
            path = REPO_ROOT / row["path"]
            self.assertEqual((sha256(path), path.stat().st_size),
                             (row["sha256"], row["sizeBytes"]))

    def test_series_bytes_are_exact_and_exclude_9000(self) -> None:
        series = self.lock["completePatchSeries"]
        self.assertEqual([row["id"] for row in series], ORDER)
        self.assertEqual(self.lock["completeAppliedPatchOrder"], ORDER)
        self.assertNotIn("9000", json.dumps(series, sort_keys=True))
        for row in series:
            path = REPO_ROOT / row["path"]
            self.assertFalse(row["diagnosticOnly"])
            self.assertEqual((sha256(path), path.stat().st_size),
                             PATCH_BINDINGS[row["id"]])
            self.assertNotEqual(
                path.read_bytes().splitlines()[0],
                b"# VERISILO-DIAGNOSTIC-MARKER: v1",
            )

    def test_0005_seam_and_shared_input_evidence_are_exact(self) -> None:
        voice = [row for row in self.lock["patchSeams"] if row["id"] == "0005"]
        self.assertEqual(
            voice,
            [{
                "id": "0005",
                "path": "dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp",
                "postSha256": "c43447ff66ad5b03b21a9c76d0202c23a699904868a282f2d53e63e01227093e",
                "preSha256": "c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f",
            }],
        )
        shared = self.lock["sharedInputEvidence"]
        self.assertEqual(shared["sourceLockSha256"], sha256(SHARED_LOCK_PATH))

    def test_strict_contract_rejects_9000_missing_0005_and_drift(self) -> None:
        added = copy.deepcopy(self.lock)
        added["completePatchSeries"].append({
            "id": "9000",
            "path": "9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch",
            "diagnosticOnly": True,
        })
        added["completeAppliedPatchOrder"].append("9000")
        with self.assertRaises(strict_build.BuildFailure):
            strict_build._validate_patch_contract(added)

        missing = copy.deepcopy(self.lock)
        missing["completePatchSeries"].pop()
        missing["completeAppliedPatchOrder"].pop()
        with self.assertRaises(strict_build.BuildFailure):
            strict_build._validate_patch_contract(missing)

        patch_drift = copy.deepcopy(self.lock)
        patch_drift["completePatchSeries"][-1]["sha256"] = "0" * 64
        with self.assertRaisesRegex(strict_build.BuildFailure, "binding drifted"):
            strict_build._validate_patch_contract(patch_drift)

        seam_drift = copy.deepcopy(self.lock)
        seam_drift["patchSeams"][-1]["postSha256"] = "0" * 64
        with self.assertRaisesRegex(strict_build.BuildFailure, "0005 seam"):
            strict_build._validate_patch_contract(seam_drift)

    def test_bound_windows_toolchain_resolution_is_fail_closed(self) -> None:
        lock = copy.deepcopy(self.lock)
        toolchain = lock["buildBinding"]["windowsToolchain"]
        crt = toolchain["crt"]
        with tempfile.TemporaryDirectory() as directory:
            mozbuild = Path(directory)
            for relative in (
                toolchain["compiler"]["relativePath"],
                toolchain["windowsSdk"]["includeRelativePath"],
                toolchain["windowsSdk"]["libRelativePath"],
                crt["relativePath"],
                "vs/VC/Redist/MSVC/v145",
            ):
                mozbuild.joinpath(*relative.split("/")).mkdir(parents=True, exist_ok=True)
            crt_file = mozbuild.joinpath(*crt["relativePath"].split("/"), "test.dll")
            crt_file.write_bytes(b"bound-crt")
            crt["files"] = [{
                "path": crt_file.name,
                "sha256": sha256(crt_file),
                "size": crt_file.stat().st_size,
            }]

            resolved = strict_build._resolve_bound_windows_toolchain(lock, mozbuild)
            self.assertEqual(
                resolved["evidence"]["compilerVersion"], "14.50.35717"
            )
            (mozbuild / "vs/VC/Tools/MSVC/unexpected").mkdir()
            with self.assertRaisesRegex(strict_build.BuildFailure, "directory versions"):
                strict_build._resolve_bound_windows_toolchain(lock, mozbuild)


if __name__ == "__main__":
    unittest.main()
