#!/usr/bin/env python3
"""Focused Linux package, archive, CMS and crash-ownership regression checks."""
from __future__ import annotations

import importlib.util
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile
from copy import deepcopy
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
HOST = Path(__file__).resolve().parent
sys.path.insert(0, str(HOST))
from browser_tree import build_tree_manifest
import identity_policy
from package_contract import (
    FORMAL_V3_SOURCE_LOCK_SHA256, LINUX_PLATFORM, PackageContractError,
    PackageLayout, build_package_tree, recheck_formal_package, sha256_file,
)


def module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


engine = module("linux_engine_recipe_test", HOST / "build/linux/build.py")
builder = module("linux_package_builder_test", ROOT / "scripts/build-camoufox-host-package.py")


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


class NativePackageTests(unittest.TestCase):
    def test_native_recipe_keeps_the_frozen_rc5_patchset(self) -> None:
        self.assertEqual(engine.pinned_lock()["completeAppliedPatchOrder"], engine.ORDER)

    def test_native_build_record_rejects_changed_archive_and_claims(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            browser = root / "browser"
            browser.mkdir()
            executable = browser / "camoufox-bin"
            executable.write_bytes(b"\x7fELF\x02" + b"\0" * 13 + b"\x3e\0")
            executable.chmod(0o755)
            properties = browser / "properties.json"
            properties.write_bytes(b"{}")
            tree = root / "browser-tree-manifest.json"
            write_json(tree, build_tree_manifest(browser))
            archive = root / engine.ARCHIVE_NAME
            with zipfile.ZipFile(archive, "w") as stream:
                stream.write(executable, "camoufox-bin")
                stream.write(properties, "properties.json")
            record = {
                "recordType": engine.RECORD_TYPE, "platform": "linux-x86_64", "target": "x86_64-pc-linux-gnu",
                "engineRevision": builder.FORMAL_V3_ENGINE_REVISION, "claims": dict(engine.CLAIMS),
                "upstream": engine.pinned_lock()["upstream"], "completeAppliedPatchOrder": engine.ORDER,
                "source": {"commit": "1" * 40, "tree": "2" * 40, "sourceLockSha256": FORMAL_V3_SOURCE_LOCK_SHA256,
                           "recipeSha256": sha256_file(HOST / "build/linux/build.py")},
                "archive": {"name": archive.name, "sha256": sha256_file(archive), "sizeBytes": archive.stat().st_size,
                            "browserExecutableSha256": sha256_file(executable), "propertiesJsonSha256": sha256_file(properties),
                            "buildId": "20260811045234", "sourceStamp": "native-test"},
                "browserTree": {"sha256": sha256_file(tree), "sizeBytes": tree.stat().st_size},
            }
            result = root / "linux-build-result.json"
            write_json(result, record)
            checked = builder._validate_linux_inputs(engine.LOCK, result, browser, tree)
            self.assertFalse(checked["assetLock"]["verified"])
            self._assert_native_canvas_binding(root, checked["assetLock"])
            record["claims"]["runtimeVerified"] = True
            write_json(result, record)
            with self.assertRaisesRegex(PackageContractError, "patched candidate"):
                builder._validate_linux_inputs(engine.LOCK, result, browser, tree)
            record["claims"]["runtimeVerified"] = False
            write_json(result, record)
            archive.write_bytes(b"changed archive")
            with self.assertRaisesRegex(PackageContractError, "browser bytes"):
                builder._validate_linux_inputs(engine.LOCK, result, browser, tree)

    def _assert_native_canvas_binding(self, root: Path, asset: dict) -> None:
        approval = builder._linux_canvas_approval(asset)
        binding = approval["browserBinding"]
        self.assertEqual(binding["archiveSha256"], asset["sha256"])
        self.assertEqual(binding["archiveSizeBytes"], asset["sizeBytes"])
        self.assertEqual(binding["buildId"], asset["buildId"])
        self.assertEqual(binding["sourceStamp"], asset["sourceStamp"])
        self.assertEqual(binding["propertiesJsonSha256"], asset["propertiesJsonSha256"])
        host = root / "host"
        internal = host / "_internal"
        internal.mkdir(parents=True)
        approval_file = internal / identity_policy.LINUX_CANVAS_BINDING_NAME
        write_json(approval_file, approval)
        builder._validate_linux_host_canvas_approval(host, approval)
        with self.assertRaises(identity_policy.ArtifactIntegrityError):
            identity_policy.canvas_policy_variant_for_browser_binding(binding)
        with patch.object(identity_policy.sys, "platform", "linux"), \
             patch.object(identity_policy.sys, "frozen", True, create=True), \
             patch.object(identity_policy, "__file__", str(internal / "identity_policy.py")):
            self.assertEqual(identity_policy.canvas_policy_variant_for_browser_binding(binding),
                             identity_policy.DETERMINISTIC_CANVAS_POLICY_VARIANT)
            artifact = json.loads((ROOT / "tests/fixtures/camoufox/identity-win-canvas-v1-a.json").read_bytes())
            artifact["browserBinding"] = deepcopy(binding)
            artifact["canonicalDigest"] = identity_policy.compute_artifact_digest(artifact)
            identity_policy.validate_artifact_strict(artifact)
            # Every native tuple member participates; no partial/foreign/
            # cross-variant artifact may inherit this package's approval.
            for field in binding:
                changed = deepcopy(binding)
                changed[field] = changed[field] + 1 if type(changed[field]) is int else "0" + changed[field][1:]
                if changed[field] == binding[field]:
                    changed[field] = "changed"
                with self.subTest(field=field), self.assertRaises(identity_policy.ArtifactIntegrityError):
                    identity_policy.canvas_policy_variant_for_browser_binding(changed)
            for changed in ({**binding, "extra": True},
                            {**binding, "archiveSizeBytes": True},
                            {key: value for key, value in binding.items() if key != "sourceStamp"},
                            identity_policy.FORMAL_R1_V3_CANVAS_BROWSER_BINDING):
                with self.assertRaises(identity_policy.ArtifactIntegrityError):
                    identity_policy.canvas_policy_variant_for_browser_binding(changed)
            artifact["policy"]["canvasClassification"] = dict(identity_policy.CANVAS_CLASSIFICATION)
            artifact["canonicalDigest"] = identity_policy.compute_artifact_digest(artifact)
            with self.assertRaises(identity_policy.ArtifactIntegrityError):
                identity_policy.validate_artifact_strict(artifact)
            approval_file.unlink()
            with self.assertRaises(identity_policy.ArtifactIntegrityError):
                identity_policy.canvas_policy_variant_for_browser_binding(binding)
            for changed in ({**approval, "variant": identity_policy.LEGACY_CANVAS_POLICY_VARIANT},
                            {**approval, "browserBinding": {**binding, "archiveSizeBytes": True}},
                            {**approval, "extra": True}):
                write_json(approval_file, changed)
                with self.assertRaises(identity_policy.ArtifactIntegrityError):
                    identity_policy.canvas_policy_variant_for_browser_binding(binding)
        write_json(approval_file, approval)
        changed = deepcopy(approval)
        changed["browserBinding"]["archiveSha256"] = "0" * 64
        write_json(approval_file, changed)
        with self.assertRaisesRegex(PackageContractError, "approval differs"):
            builder._validate_linux_host_canvas_approval(host, approval)
        self.assertEqual(identity_policy.canvas_policy_variant_for_browser_binding(
            identity_policy.FORMAL_R1_V3_CANVAS_BROWSER_BINDING), identity_policy.DETERMINISTIC_CANVAS_POLICY_VARIANT)

    def test_native_package_bytes_and_platform_are_bound(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            layout = PackageLayout.from_root(root, platform=LINUX_PLATFORM)
            layout.browser_root.mkdir()
            layout.host.parent.mkdir()
            layout.probe.parent.mkdir()
            for path, raw in ((layout.host, b"host"), (layout.supervisor, b"supervisor"),
                              (layout.browser_root / "camoufox-bin", b"browser")):
                path.write_bytes(raw)
                path.chmod(0o755)
            layout.probe.write_bytes(b"probe")
            (layout.browser_root / "properties.json").write_bytes(b"{}")
            (layout.browser_root / "application.ini").write_text("BuildID=20260811045234\nSourceStamp=native-test\n")
            write_json(layout.browser_tree, build_tree_manifest(layout.browser_root))
            asset = {
                "schema": "verisilo-camoufox-package-asset/v1", "assetKind": "self-built", "verified": False,
                "evidenceClass": "compiled-not-runtime-verified", "package": "camoufox", "release": "v152.0.4-beta.28",
                "platform": "linux-x86_64", "pythonPackage": "camoufox==0.5.4", "engineRevision": builder.FORMAL_V3_ENGINE_REVISION,
                "sha256": "a" * 64, "sizeBytes": 12, "executableRelativePath": "camoufox-bin",
                "browserExecutableSha256": sha256_file(layout.browser_root / "camoufox-bin"),
                "buildId": "20260811045234", "sourceStamp": "native-test", "propertiesJsonSha256": sha256_file(layout.browser_root / "properties.json"),
                "sourceBinding": {"commit": "1" * 40, "tree": "2" * 40, "sourceLockSha256": FORMAL_V3_SOURCE_LOCK_SHA256,
                                  "completeAppliedPatchOrder": engine.ORDER},
                "buildResultSha256": "b" * 64, "browserTreeManifestSha256": sha256_file(layout.browser_tree),
            }
            write_json(layout.asset_lock, asset)
            write_json(layout.package_tree, build_package_tree(root))
            host_sha = sha256_file(layout.host)
            manifest = {
                "schemaVersion": 3, "engineId": "camoufox", "engineVersion": builder.FORMAL_V3_ENGINE_VERSION,
                "channel": "experimental", "platform": LINUX_PLATFORM, "artifactSha256": host_sha,
                "signature": {"algorithm": "cms-detached-sha256", "keyId": "0" * 64, "value": ""},
                "capabilities": builder.CAPABILITIES,
                "entrypoint": {"kind": "camoufox-host-v1", "relativePath": "host/camoufox-host", "protocol": "verisilo-camoufox-host/v1", "sha256": host_sha},
                "treeManifest": {"relativePath": "package-tree.json", "sha256": sha256_file(layout.package_tree)},
                "browserTreeManifest": {"relativePath": "browser-tree-manifest.json", "sha256": sha256_file(layout.browser_tree)},
                "hostVersion": "0.1.0", "browserRelease": "v152.0.4-beta.28", "browserAssetSha256": asset["sha256"],
            }
            self.assertEqual(recheck_formal_package(root, manifest)["browserFileCount"], 3)
            manifest["browserAssetSha256"] = "c" * 64
            with self.assertRaisesRegex(PackageContractError, "binding differ"):
                recheck_formal_package(root, manifest)

    def test_zip_rejects_escaping_members_and_preserves_exec(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "browser.zip"
            elf = b"\x7fELF\x02" + b"\0" * 13 + b"\x3e\0"
            with zipfile.ZipFile(archive, "w") as stream:
                item = zipfile.ZipInfo("camoufox-bin")
                item.external_attr = 0o100755 << 16
                stream.writestr(item, elf)
            engine.extract_browser(archive, root / "browser")
            self.assertEqual((root / "browser/camoufox-bin").read_bytes(), elf)
            if sys.platform == "linux":
                self.assertTrue(os.access(root / "browser/camoufox-bin", os.X_OK))
            with zipfile.ZipFile(archive, "w") as stream:
                stream.writestr("../outside", b"bad")
            with self.assertRaisesRegex(ValueError, "irregular"):
                engine.extract_browser(archive, root / "rejected")
            self.assertFalse((root / "outside").exists())

    @unittest.skipUnless(shutil.which("openssl"), "OpenSSL native CMS signing")
    def test_linux_cms_signs_the_exact_key_id_payload(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cert, key = root / "cert.pem", root / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
                            "-subj", "/CN=VeriSilo Linux package test", "-keyout", str(key), "-out", str(cert)],
                           check=True, capture_output=True)
            manifest = {"signature": {"algorithm": "cms-detached-sha256", "keyId": "0" * 64, "value": ""}}
            manifest_path, payload = root / "manifest.json", root / "payload.bin"
            builder._sign_linux_manifest(manifest, manifest_path, payload, cert, key, "VERISILO_TEST_NO_PASSWORD")
            self.assertNotEqual(manifest["signature"]["keyId"], "0" * 64)
            self.assertEqual(payload.read_bytes(), builder.manifest_signing_payload(manifest))
            self.assertGreater(len(manifest["signature"]["value"]), 256)


@unittest.skipUnless(sys.platform == "linux", "native Linux process ownership")
class SupervisorCrashTests(unittest.TestCase):
    def test_pyinstaller_internal_aliases_are_materialized_without_escapes(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "host"
            source.mkdir()
            library = source / "library.so.1"
            library.write_bytes(b"bundled library")
            (source / "library.so").symlink_to("library.so.1")
            builder._copy_host_directory(source, root / "copied", linux=True)
            self.assertFalse((root / "copied/library.so").is_symlink())
            self.assertEqual((root / "copied/library.so").read_bytes(), b"bundled library")
            (source / "escape").symlink_to(root)
            with self.assertRaises(PackageContractError):
                builder._copy_host_directory(source, root / "rejected", linux=True)

    def test_host_death_reclaims_signal_ignoring_browser_descendants(self) -> None:
        from exit_supervisor import starttime_ticks
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pid_file = root / "pids.json"
            supervisor_file = root / "supervisor.json"
            browser_code = (
                "import os,subprocess,signal,time,json; "
                "signal.signal(signal.SIGTERM,signal.SIG_IGN); "
                "child=subprocess.Popen([os.environ['PYTHON_EXE'],'-c','import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)']); "
                f"open({str(pid_file)!r},'w').write(json.dumps([os.getpid(),child.pid]));time.sleep(60)"
            )
            host_code = (
                "import os,subprocess,time; from pathlib import Path; "
                "os.environ['VERISILO_HOST_PID']=str(os.getpid()); "
                "os.environ['VERISILO_HOST_START_TICKS']=Path('/proc/self/stat').read_text().rsplit(')',1)[1].split()[19]; "
                f"subprocess.Popen([{sys.executable!r},{str(HOST / 'exit_supervisor.py')!r},'-c',{browser_code!r}]);time.sleep(60)"
            )
            env = dict(os.environ, VERISILO_REAL_EXE=sys.executable, PYTHON_EXE=sys.executable,
                       VERISILO_EXIT_FILE=str(root / "exit.json"), VERISILO_SUPERVISOR_FILE=str(supervisor_file),
                       VERISILO_PROFILE_LOCK_PATH=str(root / "profile.lock"))
            host = subprocess.Popen([sys.executable, "-c", host_code], env=env)
            supervisor_pid = None
            try:
                deadline = time.monotonic() + 10
                while not (pid_file.exists() and supervisor_file.exists()) and time.monotonic() < deadline:
                    time.sleep(0.05)
                pids = json.loads(pid_file.read_text())
                supervisor_pid = json.loads(supervisor_file.read_text())["supervisorPid"]
                from host_platform import probe_supervisor_lock
                self.assertFalse(probe_supervisor_lock(root / "profile.lock"))
                host.kill()
                host.wait(timeout=5)
                deadline = time.monotonic() + 6
                while any(starttime_ticks(pid) > 0 for pid in [supervisor_pid, *pids]) and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(all(starttime_ticks(pid) < 0 for pid in [supervisor_pid, *pids]))
                self.assertTrue(probe_supervisor_lock(root / "profile.lock"))
            finally:
                if host.poll() is None:
                    host.kill()
                    host.wait()
                if supervisor_pid and starttime_ticks(supervisor_pid) > 0:
                    os.killpg(supervisor_pid, signal.SIGKILL)


if __name__ == "__main__":
    unittest.main()
