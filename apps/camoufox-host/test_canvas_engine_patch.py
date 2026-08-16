#!/usr/bin/env python3
"""Focused source-binding tests for the FP1 Canvas Engine Patch.

The default run requires every exact source input. ``--tracked-only`` is an
explicit reduced mode for text self-consistency checks; it is not source,
compile, binary, or runtime evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile
from pathlib import Path
from unittest import SkipTest
from unittest import mock


REPO_ROOT = Path(__file__).resolve().parents[2]
LOCK_PATH = (
    REPO_ROOT
    / "apps"
    / "camoufox-host"
    / "lock"
    / "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json"
)
PATCH_PATH = (
    REPO_ROOT
    / "apps"
    / "camoufox-host"
    / "patches"
    / "camoufox"
    / "v152.0.4-beta.28"
    / "0001-verisilo-canvas-export-key.patch"
)
MIDL_COMPAT_PATCH_PATH = (
    REPO_ROOT
    / "apps"
    / "camoufox-host"
    / "patches"
    / "camoufox"
    / "v152.0.4-beta.28"
    / "0000-verisilo-ff152-midl-cross-build-input.patch"
)
CLOSE_BOUND_PATCH_PATH = (
    REPO_ROOT
    / "apps"
    / "camoufox-host"
    / "patches"
    / "camoufox"
    / "v152.0.4-beta.28"
    / "0002-verisilo-juggler-bounded-close.patch"
)
JUGGLER_CLOSE_SEAM_PATH = "juggler/protocol/BrowserHandler.js"
BUILD_RECIPE_ROOT = (
    REPO_ROOT / "apps" / "camoufox-host" / "build" / "canvas-engine-v1"
)
DOCKERFILE_PATH = BUILD_RECIPE_ROOT / "Dockerfile"
STRICT_BUILD_PATH = BUILD_RECIPE_ROOT / "strict_build.py"
BUILD_HOST_PATH = BUILD_RECIPE_ROOT / "build_host.py"
BASE_INDEX_DIGEST = (
    "sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
)
BASE_AMD64_MANIFEST_DIGEST = (
    "sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
)
EXPECTED_HOST_RUNTIME = {
    "policy": "closed",
    "requiredDpkgPackages": [
        {
            "name": "libc6",
            "architecture": "amd64",
            "version": "2.39-0ubuntu8.8",
        },
        {
            "name": "libc6-i386",
            "architecture": "amd64",
            "version": "2.39-0ubuntu8.8",
        },
    ],
    "requiredExecutablePaths": ["/lib/ld-linux.so.2"],
    "foreignArchitectures": [],
}
EXPECTED_WINE_PREFIX = {
    "policy": "closed",
    "environmentVariable": "WINEPREFIX",
    "pathTemplate": "/work/{runId}/.wine-prefix",
    "defaultHomeFallbackAllowed": False,
}
EXPECTED_VS_MANIFEST_SHA256 = (
    "ffeef9c51797082dbfef5f6608d75638d3ca3cb11c17cef9f8deea9fde58c188"
)
EXPECTED_CRT_TREE_SHA256 = (
    "97fd9b9e690301e9e066b40aef96f980ed195b226312a016cccb96ef64db73cd"
)
EXPECTED_CRT_FILES = {
    "concrt140.dll": (321696, "b2faf3b85b23c840b654e57d5497a0ad31acd02fb01856cad4725a1715d5f78e"),
    "msvcp140.dll": (553552, "def46aa6a8f72f27bafac0c43334419486a4d1dcdb6c479a8ef7034b3e1fa4cb"),
    "msvcp140_1.dll": (35488, "2dd670f874562fbdca5b022df1943d70a57ba91fde559280e3a1daebe4db2380"),
    "msvcp140_2.dll": (278608, "1d60da3ac2b06482912ca852fa7047436e6e474b4cfffa3bf77f4598cfbf454c"),
    "msvcp140_atomic_wait.dll": (
        48800,
        "e7963645e0d1db08e300614d4c5fa7194bd8173e9ab7a5558859e6b232ed3241",
    ),
    "msvcp140_codecvt_ids.dll": (
        31392,
        "ae8d922b00cdd93e3ebecc37beb46c800f383ebdeb9f9e5b84e04a72428b6fb3",
    ),
    "vccorlib140.dll": (
        350880,
        "6b8d8a76c3e6664293407553650e60b94df9aaafc7c92057ea83032bd228e44f",
    ),
    "vcruntime140.dll": (
        123472,
        "184146852727a9db4eea06178716bec3cdbb1015c911f6b0f915b184ad7775b2",
    ),
    "vcruntime140_1.dll": (
        47264,
        "e6bfb3662ab4b1969a73441dbe35c96d51441b6bff8cf1fe7430bd5b246ca605",
    ),
    "vcruntime140_threads.dll": (
        37456,
        "a6222020b500a9a86b36e040c2dbd0e459716db1bf2810a11cd7512ea9b8d89b",
    ),
}
EXPECTED_BUILDER_IMAGE_BINDING = {
    "baseIndexDigest": BASE_INDEX_DIGEST,
    "baseLinuxAmd64ManifestDigest": BASE_AMD64_MANIFEST_DIGEST,
    "buildxLogSha256": (
        "9c6e348eb9ad2164b00daf96bdc5ea7baedb48e44ced46247485357d6a5dcd48"
    ),
    "buildxLogSizeBytes": 78272,
    "buildxMetadataSha256": (
        "351cae9f7eb1c4b1ea60f828aba5380818264369a8d2bf4478d3fceb5a6a6b93"
    ),
    "dockerfileSha256": (
        "ae3e28d39c2c996d0c70f6fbd4b182f4e1585eb79fd7fc662e50b158300e5e6d"
    ),
    "hostToolingSha256": (
        "a6857ad0855755e1c3fa70fbbfd42c8ed84abf4697510846f76b3c5c8127ad5b"
    ),
    "imageId": (
        "sha256:5183cb1c8475b4bf263d78294ce06d553ab08d0f2684aab317f8df89bc0ad964"
    ),
    "imageInspectSha256": (
        "11500c030374e33572db41488c6809015cd40812e598aee0eea8545d83631d35"
    ),
    "recipeSourceCommit": "8b8294fafd5d5d68088c05bc31def6044d6e0e69",
    "recipeSourceLockSha256": (
        "6ac8e8ce00e9912dc439b20a5608ed854ef0f41fadcc2e50aa13fcf1c401e3b0"
    ),
    "recipeSourceTree": "99d16659c7e247875076c0e47df3f8f7be3a4564",
    "savedArchiveSha256": (
        "e16596f3257ef38f3fcec5f2e8370995cccd184d59b2335e17048fe3f1aa3bfc"
    ),
    "savedArchiveSizeBytes": 483575808,
}

UPSTREAM_REPO: Path | None = None
FIREFOX_SOURCE_ARCHIVE: Path | None = None
PREIMAGE_SOURCE_TREE: Path | None = None
PATCHED_SOURCE_TREE: Path | None = None
EXACT_FIREFOX_COMPATIBILITY_PREIMAGES: dict[str, bytes] = {}


def _load_lock() -> dict:
    return json.loads(LOCK_PATH.read_text(encoding="utf-8"))


def _builder_binding_for_tests() -> dict:
    active = _load_lock()["buildBinding"]["builderImageBinding"]
    return dict(active or EXPECTED_BUILDER_IMAGE_BINDING)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _sha512(path: Path) -> str:
    digest = hashlib.sha512()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _load_python_file(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _canonical_tree(root: Path) -> tuple[int, int, str]:
    rows: list[dict] = []
    total = 0
    files = sorted(
        (path for path in root.rglob("*") if path.is_file()),
        key=lambda path: path.relative_to(root).as_posix(),
    )
    for path in files:
        data = path.read_bytes()
        total += len(data)
        rows.append(
            {
                "path": path.relative_to(root).as_posix(),
                "size": len(data),
                "sha256": hashlib.sha256(data).hexdigest(),
            }
        )
    encoded = json.dumps(
        rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return len(rows), total, hashlib.sha256(encoded).hexdigest()


def _git(repo: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo), *args],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return result.stdout.strip()


def _expect_build_failure(driver, action, message: str) -> None:
    try:
        action()
    except driver.BuildFailure:
        return
    raise AssertionError(message)


def _synthetic_toolchain_fixture(root: Path) -> tuple[dict, Path, Path, Path]:
    source = root / "source"
    mozbuild = root / "mozbuild"
    manifest = source / "build" / "vs" / "vs2026.yaml"
    manifest.parent.mkdir(parents=True)
    manifest.write_bytes(b"synthetic-vs2026-manifest\n")

    compiler = mozbuild / "vs" / "VC" / "Tools" / "MSVC" / "1.2.3"
    sdk_include = mozbuild / "vs" / "Windows Kits" / "10" / "Include" / "4.5.6"
    sdk_lib = mozbuild / "vs" / "Windows Kits" / "10" / "Lib" / "4.5.6"
    crt = (
        mozbuild
        / "vs"
        / "VC"
        / "Redist"
        / "MSVC"
        / "1.2.2"
        / "x64"
        / "Microsoft.VC145.CRT"
    )
    for directory in (compiler, sdk_include, sdk_lib, crt):
        directory.mkdir(parents=True)
    (crt.parent.parent.parent / "v145").mkdir()
    contents = {
        "msvcp140.dll": b"synthetic-msvcp",
        "vcruntime140.dll": b"synthetic-vcruntime",
    }
    for name, data in contents.items():
        (crt / name).write_bytes(data)
    rows = [
        {
            "path": name,
            "sha256": hashlib.sha256(data).hexdigest(),
            "size": len(data),
        }
        for name, data in sorted(contents.items())
    ]
    count, total, digest = _canonical_tree(crt)
    lock = {
        "buildBinding": {
            "windowsToolchain": {
                "selectionManifest": {
                    "path": "build/vs/vs2026.yaml",
                    "sha256": _sha256(manifest),
                    "size": manifest.stat().st_size,
                },
                "compiler": {
                    "version": "1.2.3",
                    "relativePath": "vs/VC/Tools/MSVC/1.2.3",
                    "versionDirectoryNames": ["1.2.3"],
                },
                "windowsSdk": {
                    "version": "4.5.6",
                    "includeRelativePath": "vs/Windows Kits/10/Include/4.5.6",
                    "includeVersionDirectoryNames": ["4.5.6"],
                    "libRelativePath": "vs/Windows Kits/10/Lib/4.5.6",
                    "libVersionDirectoryNames": ["4.5.6"],
                },
                "crt": {
                    "redistVersion": "1.2.2",
                    "architecture": "x64",
                    "family": "Microsoft.VC145.CRT",
                    "relativePath": (
                        "vs/VC/Redist/MSVC/1.2.2/x64/Microsoft.VC145.CRT"
                    ),
                    "redistDirectoryNames": ["1.2.2", "v145"],
                    "files": rows,
                    "tree": {
                        "fileCount": count,
                        "totalBytes": total,
                        "canonicalTreeSha256": digest,
                    },
                },
            }
        }
    }
    return lock, source, mozbuild, crt


def test_source_lock_contract() -> None:
    lock = _load_lock()
    assert lock["schema"] == "verisilo-camoufox-source-binding/v1"
    assert lock["engineRevision"] == (
        "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1"
    )
    assert lock["status"] == "source-patch-only"
    assert lock["verified"] is False
    build = lock["buildBinding"]
    assert build["status"] == "not-built"
    assert build["binaryBinding"] is None
    assert build["supportedBuildExecutionEnvironment"] == "linux"
    assert "Docker" in build["supportedPhysicalHostWrapper"]
    assert build["unsupportedBuildRoutes"] == ["direct native-Windows", "WSL"]
    assert build["ociBase"] == {
        "reference": "docker.io/library/ubuntu:24.04",
        "indexDigest": BASE_INDEX_DIGEST,
        "linuxAmd64ManifestDigest": BASE_AMD64_MANIFEST_DIGEST,
        "resolvedAtUtc": "2026-08-11T05:46:36Z",
        "resolutionSource": "Docker Hub registry v2 manifest response",
    }
    recipe = build["recipe"]
    assert recipe["name"] == "camoufox-152.0.4-beta.28-canvas-engine-v1"
    assert recipe["fixedEnvironment"] == {
        "BUILD_TARGET": "windows,x86_64",
        "CARGO_BUILD_JOBS": "1",
        "LANG": "C.UTF-8",
        "LC_ALL": "C.UTF-8",
        "MOZ_BUILD_DATE": "20260811045234",
        "TZ": "Etc/UTC",
    }
    assert recipe["sharedMozbuildCacheAllowed"] is False
    assert recipe["oneShotRunDirectoryRequired"] is True
    assert build["resourceGate"]["minimumFreeBytes"] == 80 * 1024**3
    assert build["resourceGate"]["recommendedFreeBytes"] == 100 * 1024**3
    assert build["resourceGate"]["configuredNominalSwapBytes"] == 24 * 1024**3
    assert build["resourceGate"]["minimumSwapBytes"] == 24 * 1024**3 - 4096
    assert build["builderImageBinding"] is None
    assert set(build["builderImageBindingRequiredFields"]) == {
        "imageId",
        "savedArchiveSha256",
        "savedArchiveSizeBytes",
        "recipeSourceCommit",
        "recipeSourceTree",
        "recipeSourceLockSha256",
        "dockerfileSha256",
        "baseIndexDigest",
        "baseLinuxAmd64ManifestDigest",
        "buildxLogSha256",
        "buildxLogSizeBytes",
        "buildxMetadataSha256",
        "imageInspectSha256",
        "hostToolingSha256",
    }
    assert build["runtimeContainer"] == {
        "readOnlyRoot": True,
        "inputMount": "/inputs read-only",
        "distinctEmptyReadWriteMounts": ["/build-home", "/work", "/out"],
        "dpkgClosureMustRemainStable": True,
        "hostRuntime": EXPECTED_HOST_RUNTIME,
        "winePrefix": EXPECTED_WINE_PREFIX,
        "tmpfs": {
            "path": "/tmp",
            "options": [
                "rw",
                "nosuid",
                "nodev",
                "exec",
                "mode=1777",
                "size=4g",
            ],
            "purpose": (
                "execute checksum-verified Mozilla bootstrap temporary tools"
            ),
        },
    }
    toolchain = build["windowsToolchain"]
    assert toolchain["selectionManifest"] == {
        "path": "build/vs/vs2026.yaml",
        "sha256": EXPECTED_VS_MANIFEST_SHA256,
        "size": 136448,
    }
    assert toolchain["compiler"] == {
        "version": "14.50.35717",
        "relativePath": "vs/VC/Tools/MSVC/14.50.35717",
        "versionDirectoryNames": ["14.50.35717"],
    }
    assert toolchain["windowsSdk"] == {
        "version": "10.0.26100.0",
        "includeRelativePath": "vs/Windows Kits/10/Include/10.0.26100.0",
        "includeVersionDirectoryNames": ["10.0.26100.0"],
        "libRelativePath": "vs/Windows Kits/10/Lib/10.0.26100.0",
        "libVersionDirectoryNames": ["10.0.26100.0"],
    }
    crt = toolchain["crt"]
    assert crt["redistVersion"] == "14.50.35710"
    assert crt["architecture"] == "x64"
    assert crt["family"] == "Microsoft.VC145.CRT"
    assert crt["relativePath"] == (
        "vs/VC/Redist/MSVC/14.50.35710/x64/Microsoft.VC145.CRT"
    )
    assert crt["redistDirectoryNames"] == ["14.50.35710", "v145"]
    assert {
        item["path"]: (item["size"], item["sha256"]) for item in crt["files"]
    } == EXPECTED_CRT_FILES
    assert crt["tree"] == {
        "fileCount": 10,
        "totalBytes": 1828608,
        "canonicalTreeSha256": EXPECTED_CRT_TREE_SHA256,
    }
    assert any(
        "SHA-512 verification" in requirement
        for requirement in build["requiredBeforeRuntime"]
    )
    assert any(
        "upstream-patches -> FF152-MIDL-patch -> Canvas-patch -> build -> package"
        in requirement
        for requirement in build["requiredBeforeRuntime"]
    )
    assert lock["compatibility"]["historicalOfficialBindingModified"] is False
    assert lock["compatibility"]["artifactTopLevelSchema"].endswith("/v3")

    upstream = lock["upstream"]
    assert upstream["tag"] == "v152.0.4-beta.28"
    assert upstream["commit"] == "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d"
    assert upstream["tree"] == "1435d544d9b61dee7fcf74cf92462952ca43d38e"

    firefox = lock["firefoxSource"]
    assert firefox["version"] == "152.0.4"
    assert firefox["sizeBytes"] == 799102676
    assert len(firefox["sha512"]) == 128


def test_windows_toolchain_resolution_and_manifest_are_fail_closed() -> None:
    driver = _load_python_file("canvas_toolchain_driver", STRICT_BUILD_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        lock, source, mozbuild, crt = _synthetic_toolchain_fixture(Path(temporary))
        manifest = source / "build" / "vs" / "vs2026.yaml"
        expected_manifest = driver.verify_windows_toolchain_manifest(lock, source)
        assert expected_manifest == lock["buildBinding"]["windowsToolchain"][
            "selectionManifest"
        ]
        resolved = driver.resolve_bound_windows_toolchain(lock, mozbuild)
        assert [path.name for path in resolved["crtFiles"]] == [
            "msvcp140.dll",
            "vcruntime140.dll",
        ]

        original_manifest = manifest.read_bytes()
        manifest.write_bytes(original_manifest + b"drift")
        _expect_build_failure(
            driver,
            lambda: driver.verify_windows_toolchain_manifest(lock, source),
            "toolchain manifest drift was accepted",
        )
        manifest.write_bytes(original_manifest)

        extra_version = mozbuild / "vs" / "VC" / "Tools" / "MSVC" / "9.9.9"
        extra_version.mkdir()
        _expect_build_failure(
            driver,
            lambda: driver.resolve_bound_windows_toolchain(lock, mozbuild),
            "an unexpected compiler version was accepted",
        )
        extra_version.rmdir()

        target = crt / "msvcp140.dll"
        original = target.read_bytes()
        target.write_bytes(original + b"drift")
        _expect_build_failure(
            driver,
            lambda: driver.resolve_bound_windows_toolchain(lock, mozbuild),
            "altered CRT bytes were accepted",
        )
        target.write_bytes(original)

        extra_crt = crt / "msvcp140_9.dll"
        extra_crt.write_bytes(b"unexpected")
        _expect_build_failure(
            driver,
            lambda: driver.resolve_bound_windows_toolchain(lock, mozbuild),
            "an extra CRT member was accepted",
        )
        extra_crt.unlink()

        missing = crt / "vcruntime140.dll"
        missing_bytes = missing.read_bytes()
        missing.unlink()
        _expect_build_failure(
            driver,
            lambda: driver.resolve_bound_windows_toolchain(lock, mozbuild),
            "a missing CRT member was accepted",
        )
        missing.write_bytes(missing_bytes)

        nested = crt / "nested"
        nested.mkdir()
        _expect_build_failure(
            driver,
            lambda: driver.resolve_bound_windows_toolchain(lock, mozbuild),
            "a non-flat CRT tree was accepted",
        )


def test_windows_toolchain_manifest_rejects_symlink_ancestor() -> None:
    driver = _load_python_file("canvas_toolchain_symlink_driver", STRICT_BUILD_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        lock, source, _, _ = _synthetic_toolchain_fixture(Path(temporary))
        symlink_ancestor = source / "build" / "vs"
        real_lstat = Path.lstat

        def lstat_with_symlink_ancestor(path: Path):
            metadata = real_lstat(path)
            if path == symlink_ancestor:
                values = list(metadata)
                values[0] = driver.stat.S_IFLNK | 0o777
                return driver.os.stat_result(values)
            return metadata

        with mock.patch.object(Path, "lstat", lstat_with_symlink_ancestor):
            _expect_build_failure(
                driver,
                lambda: driver.verify_windows_toolchain_manifest(lock, source),
                "a selection manifest below a symlink ancestor was accepted",
            )


def test_windows_package_command_is_explicit_and_glob_free() -> None:
    driver = _load_python_file("canvas_package_driver", STRICT_BUILD_PATH)
    absolute_root = Path(tempfile.gettempdir()).resolve() / "bound-msvc-crt"
    crt_files = [
        absolute_root / "concrt140.dll",
        absolute_root / "vcruntime140.dll",
    ]
    command = driver.windows_package_command(crt_files)
    assert command[:3] == ["python3", "scripts/package.py", "windows"]
    assert command[command.index("--includes") + 1 : command.index("--version")] == [
        "settings/chrome.css",
        "settings/camoucfg.jvv",
        "settings/properties.json",
        *(str(path) for path in crt_files),
    ]
    assert command[command.index("--version") :] == [
        "--version",
        "152.0.4",
        "--release",
        "beta.28",
        "--arch",
        "x86_64",
        "--fonts",
        "macos",
        "linux",
    ]
    assert not any("*" in argument for argument in command)
    assert not any("14.38" in argument or "VC143" in argument for argument in command)


def test_bound_toolchain_is_checked_before_and_after_packaging() -> None:
    driver = STRICT_BUILD_PATH.read_text(encoding="utf-8")
    before = driver.index("bound_toolchain = resolve_bound_windows_toolchain")
    build = driver.index('label="build-windows-x86_64"', before)
    after_compile = driver.index(
        "toolchain_after_build = resolve_bound_windows_toolchain", build
    )
    package = driver.index("windows_package_command", after_compile)
    after_package = driver.index(
        "toolchain_after_package = resolve_bound_windows_toolchain", package
    )
    archive = driver.index("**_validate_zip(", after_package)
    assert before < build < after_compile < package < after_package < archive
    result = driver[driver.index("        result = {", archive) :]
    assert '"toolchainAfterCompile": toolchain_after_build["evidence"]' in result
    assert '"toolchainAfterPackage": toolchain_after' in result
    assert '"toolchainAfterBuild"' not in result


def test_candidate_zip_requires_exact_bound_crt() -> None:
    driver = _load_python_file("canvas_zip_driver", STRICT_BUILD_PATH)
    crt_contents = {
        "msvcp140.dll": b"locked-msvcp",
        "vcruntime140.dll": b"locked-vcruntime",
    }
    rows = [
        {
            "path": name,
            "sha256": hashlib.sha256(data).hexdigest(),
            "size": len(data),
        }
        for name, data in sorted(crt_contents.items())
    ]
    common = {
        "camoufox.exe": b"exe",
        "application.ini": (
            b"[App]\nBuildID=20260811045234\nSourceStamp=" + b"a" * 40 + b"\n"
        ),
        "platform.ini": b"[Build]\n",
        "properties.json": b"{}\n",
        "camoufox.cfg": b"cfg\n",
    }

    def write_zip(path: Path, crt: dict[str, bytes]) -> None:
        with zipfile.ZipFile(path, "w") as bundle:
            for name, data in {**common, **crt}.items():
                bundle.writestr(name, data)

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        exact = root / "exact.zip"
        write_zip(exact, crt_contents)
        result = driver._validate_zip(exact, rows)
        assert result["packagedCrtMemberSha256"] == {
            row["path"]: row["sha256"] for row in rows
        }

        missing = root / "missing.zip"
        write_zip(missing, {"msvcp140.dll": crt_contents["msvcp140.dll"]})
        _expect_build_failure(
            driver,
            lambda: driver._validate_zip(missing, rows),
            "a ZIP missing a bound CRT member was accepted",
        )

        altered = root / "altered.zip"
        write_zip(altered, {**crt_contents, "msvcp140.dll": b"altered"})
        _expect_build_failure(
            driver,
            lambda: driver._validate_zip(altered, rows),
            "a ZIP with altered CRT bytes was accepted",
        )

        extra = root / "extra.zip"
        write_zip(extra, {**crt_contents, "msvcp140_9.dll": b"unexpected"})
        _expect_build_failure(
            driver,
            lambda: driver._validate_zip(extra, rows),
            "a ZIP with an unexpected CRT member was accepted",
        )

        extra_version = root / "extra-version.zip"
        write_zip(
            extra_version,
            {
                **crt_contents,
                "msvcp150.dll": b"unexpected-version",
                "vcruntime150_1.dll": b"unexpected-version-suffix",
            },
        )
        _expect_build_failure(
            driver,
            lambda: driver._validate_zip(extra_version, rows),
            "a ZIP with unexpected non-140 CRT members was accepted",
        )


SYNTHETIC_MIDL_PREIMAGE = """\
import os
import subprocess

def relativize(path, base=None):
    if path.startswith("/"):
        return os.path.relpath(path, base)
    if os.path.isabs(path) or path.startswith("-"):
        return path
    return os.path.relpath(path, base)

def preprocess(base, input, flags):
    preprocessor = ["ccache", "clang-cl", "-E"]
    for _ in (0,):
        if True:
            try:
                raise RuntimeError
            except RuntimeError:
                pass
        command = preprocessor + [input]
        preprocessed = os.path.join(base, os.path.basename(input))
        subprocess.run(command, stdout=open(preprocessed, "wb"), check=True)
        # Read the resulting file, and search for imports, that we'll want to
        return command
"""

SYNTHETIC_RULES_PREIMAGE = """\
$(COBJS):
	$(REPORT_BUILD_VERBOSE)
	$(call BUILDSTATUS,OBJECT_FILE $@)
	$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) $($(notdir $<)_FLAGS) $<
	$(call BUILDSTATUS,END_Object $@)

$(CWASMOBJS):
	$(WASM_CC) -o $@ -c $(WASM_CFLAGS) $($(notdir $<)_FLAGS) $<

$(CPPOBJS):
	$(REPORT_BUILD_VERBOSE)
	$(call BUILDSTATUS,OBJECT_FILE $@)
	$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) $($(notdir $<)_FLAGS) $<
	$(call BUILDSTATUS,END_Object $@)

$(CPPWASMOBJS):
	$(WASM_CXX) -o $@ -c $(WASM_CXXFLAGS) $($(notdir $<)_FLAGS) $<

%.res: $(or $(RCFILE),%.rc) $(MOZILLA_DIR)/config/create_res.py
	$(REPORT_BUILD)
	$(call BUILDSTATUS,START_Res $@)
	$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py $(DEFINES) $(INCLUDES) -o $@ $<
	$(call BUILDSTATUS,END_Res $@)
"""


def _apply_compatibility_patch_to_synthetic_tree(root: Path) -> tuple[Path, Path]:
    midl = root / "build" / "midl.py"
    rules = root / "config" / "rules.mk"
    midl.parent.mkdir(parents=True)
    rules.parent.mkdir(parents=True)
    midl.write_text(SYNTHETIC_MIDL_PREIMAGE, encoding="utf-8", newline="\n")
    rules.write_text(SYNTHETIC_RULES_PREIMAGE, encoding="utf-8", newline="\n")
    applied = subprocess.run(
        [
            "git",
            "-c",
            "core.autocrlf=false",
            "apply",
            "--unidiff-zero",
            "--whitespace=error-all",
            str(MIDL_COMPAT_PATCH_PATH),
        ],
        cwd=root,
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    assert applied.returncode == 0, applied.stderr or applied.stdout
    return midl, rules


def test_ff152_compatibility_patch_is_narrow_and_independently_bound() -> None:
    patch = MIDL_COMPAT_PATCH_PATH.read_text(encoding="utf-8")
    assert re.findall(r"^diff --git a/(\S+) b/(\S+)$", patch, re.MULTILINE) == [
        ("build/midl.py", "build/midl.py"),
        ("config/rules.mk", "config/rules.mk"),
    ]
    deleted = [
        line
        for line in patch.splitlines()
        if line.startswith("-") and not line.startswith("---")
    ]
    added = [
        line
        for line in patch.splitlines()
        if line.startswith("+") and not line.startswith("+++")
    ]
    assert deleted == [
        "-        command = preprocessor + [input]",
        "-\t$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) "
        "$($(notdir $<)_FLAGS) $<",
        "-\t$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) "
        "$($(notdir $<)_FLAGS) $<",
        "-\t$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py "
        "$(DEFINES) $(INCLUDES) -o $@ $<",
    ]
    assert added == [
        "+        command = preprocessor + [relativize(input)]",
        "+\t$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) "
        "$($(notdir $<)_FLAGS) $(call relativize,$<)",
        "+\t$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) "
        "$($(notdir $<)_FLAGS) $(call relativize,$<)",
        "+\t$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py "
        "$(DEFINES) $(INCLUDES) -o $@ $(call relativize,$<)",
    ]

    seams = _load_lock()["midlCompatibilitySeamFiles"]
    assert seams == [
        {
            "path": "build/midl.py",
            "postUpstreamPatchSha256": (
                "411d59bd795d2517367fdec4c26921c1d257415e71f10d98f10046752c23f248"
            ),
            "postCompatibilityPatchSha256": (
                "c4091b253f215a08cd229a2bbefba1875ec1c68e02b2970978aeee53f6789b14"
            ),
        },
        {
            "path": "config/rules.mk",
            "postUpstreamPatchSha256": (
                "739dfaecfe48f9bc9d4d7727d57e2c16aafef47983ab8efae3bca5e439d09639"
            ),
            "postCompatibilityPatchSha256": (
                "b41f128e28829e5550197611d40f37ea563804cca35ea56374c502c5787d41f9"
            ),
        },
    ]


def test_midl_compatibility_patch_relativizes_only_the_subprocess_input() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        source, _ = _apply_compatibility_patch_to_synthetic_tree(root)
        module = _load_python_file("ff152_midl_compat_fixture", source)
        unix_absolute = "/work/run/source/AccessibleEventId.idl"
        base = root / "generated"
        base.mkdir()

        calls: list[list[str]] = []

        def successful_run(command, *, stdout, check):
            calls.append(command)
            stdout.close()
            assert check is True
            return subprocess.CompletedProcess(command, 0)

        with mock.patch.object(
            module.subprocess,
            "run",
            side_effect=successful_run,
        ):
            command = module.preprocess(str(base), unix_absolute, ())
        operand = command[-1]
        assert operand == module.os.path.relpath(unix_absolute)
        assert not operand.startswith("/")
        assert operand != module.relativize(unix_absolute, str(base))
        assert calls == [command]

        relative = "imports/oaidl.idl"
        assert module.relativize(relative) == module.os.path.relpath(relative)
        windows_absolute = r"C:\Windows Kits\Include\oaidl.idl"
        real_isabs = module.os.path.isabs
        with mock.patch.object(
            module.os.path,
            "isabs",
            side_effect=lambda value: value == windows_absolute or real_isabs(value),
        ):
            assert module.relativize(windows_absolute) == windows_absolute

        def failed_run(command, *, stdout, check):
            stdout.close()
            raise subprocess.CalledProcessError(73, command)

        with mock.patch.object(module.subprocess, "run", side_effect=failed_run):
            try:
                module.preprocess(str(base), unix_absolute, ())
            except subprocess.CalledProcessError as exc:
                assert exc.returncode == 73
            else:
                raise AssertionError("the MIDL preprocessor subprocess error was swallowed")


def test_ff152_compatibility_patch_relativizes_synthetic_make_and_resource_rules() -> None:
    with tempfile.TemporaryDirectory() as temporary:
        _, rules = _apply_compatibility_patch_to_synthetic_tree(Path(temporary))
        implementation = rules.read_text(encoding="utf-8")

    def recipe(target: str) -> list[str]:
        match = re.search(
            rf"^\$\({re.escape(target)}\):\n((?:\t.*\n)+)",
            implementation,
            re.MULTILINE,
        )
        assert match is not None
        return match.group(1).splitlines()

    assert recipe("COBJS") == [
        "\t$(REPORT_BUILD_VERBOSE)",
        "\t$(call BUILDSTATUS,OBJECT_FILE $@)",
        "\t$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) "
        "$($(notdir $<)_FLAGS) $(call relativize,$<)",
        "\t$(call BUILDSTATUS,END_Object $@)",
    ]
    assert recipe("CPPOBJS") == [
        "\t$(REPORT_BUILD_VERBOSE)",
        "\t$(call BUILDSTATUS,OBJECT_FILE $@)",
        "\t$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) "
        "$($(notdir $<)_FLAGS) $(call relativize,$<)",
        "\t$(call BUILDSTATUS,END_Object $@)",
    ]
    assert recipe("CWASMOBJS") == [
        "\t$(WASM_CC) -o $@ -c $(WASM_CFLAGS) $($(notdir $<)_FLAGS) $<"
    ]
    assert recipe("CPPWASMOBJS") == [
        "\t$(WASM_CXX) -o $@ -c $(WASM_CXXFLAGS) $($(notdir $<)_FLAGS) $<"
    ]
    assert (
        "\t$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py "
        "$(DEFINES) $(INCLUDES) -o $@ $(call relativize,$<)"
        in implementation
    )
    assert implementation.count("$(call relativize,$<)") == 3


def test_canvas_contract_golden_vectors() -> None:
    contract = _load_lock()["canvasExportContract"]
    assert contract["identityScope"] == "artifact-silo"
    assert contract["managedConfigKey"] == "canvas:seed"
    assert contract["configuredZeroIsValid"] is True
    assert contract["seedEncoding"] == "uint32-big-endian"
    assert contract["keyDerivation"] == "sha256(domainSeparator || seedBytes)"
    assert contract["keyLengthBytes"] == 32
    assert contract["rawPixelsNoised"] is False
    assert contract["preservePngDeBGChunk"] is True
    assert "Host rejects" in contract["invalidConfiguredValueBehavior"]
    assert "Host must reject" in contract["malformedCamouConfigBoundary"]

    escaped = contract["domainSeparatorAsciiEscaped"]
    assert escaped.endswith(r"\0")
    domain = escaped[:-2].encode("ascii") + b"\0"
    assert domain.hex() == contract["domainSeparatorHex"]

    keys: set[str] = set()
    for vector in contract["goldenVectors"]:
        seed = vector["seed"]
        assert type(seed) is int and 0 <= seed <= 0xFFFFFFFF
        key = hashlib.sha256(domain + seed.to_bytes(4, "big")).hexdigest()
        assert key == vector["keyHex"]
        keys.add(key)
    assert len(keys) == len(contract["goldenVectors"])
    assert contract["goldenVectors"][0]["seed"] == 0


def test_patch_is_narrow_additive_and_binding_scoped() -> None:
    patch = PATCH_PATH.read_text(encoding="utf-8")
    targets = re.findall(r"^diff --git a/(\S+) b/(\S+)$", patch, re.MULTILINE)
    assert targets == [
        (
            "toolkit/components/resistfingerprinting/nsRFPService.cpp",
            "toolkit/components/resistfingerprinting/nsRFPService.cpp",
        ),
        (
            "toolkit/components/resistfingerprinting/moz.build",
            "toolkit/components/resistfingerprinting/moz.build",
        ),
    ]

    deleted = [
        line
        for line in patch.splitlines()
        if line.startswith("-") and not line.startswith("---")
    ]
    assert deleted == [], "the downstream patch must preserve the stock fallback"

    added = "\n".join(
        line[1:]
        for line in patch.splitlines()
        if line.startswith("+") and not line.startswith("+++")
    )
    assert '#include "MaskConfig.hpp"' in added
    assert 'MaskConfig::HasKey("canvas:seed", maskConfig)' in added
    assert 'MaskConfig::GetUint32("canvas:seed")' in added
    assert '"verisilo-canvas-export-v1\\0"' in added
    assert "sizeof(kDomain) - 1" in added
    assert '#include "mozilla/crypto_hash_sha2.h"' in added
    assert "crypto_hash_sha256(input, sizeof(input), digest)" in added
    assert "aRandomizationKeyStr.Assign" in added
    assert "nsICryptoHash" not in added
    assert '"/camoucfg"' in added

    seed_lines = [
        "input[kDomainLength] = static_cast<uint8_t>(*canvasSeed >> 24);",
        "input[kDomainLength + 1] = static_cast<uint8_t>(*canvasSeed >> 16);",
        "input[kDomainLength + 2] = static_cast<uint8_t>(*canvasSeed >> 8);",
        "input[kDomainLength + 3] = static_cast<uint8_t>(*canvasSeed);",
    ]
    assert [added.index(line) for line in seed_lines] == sorted(
        added.index(line) for line in seed_lines
    )

    for forbidden in (
        "GetOrigin",
        "TopLevelSite",
        "GenerateUUID",
        "RandomGenerator",
        "ProcessId",
        "TimeStamp",
    ):
        assert forbidden not in added

    has_key_at = patch.index('MaskConfig::HasKey("canvas:seed", maskConfig)')
    get_seed_at = patch.index('MaskConfig::GetUint32("canvas:seed")')
    assert has_key_at < get_seed_at


def test_tracked_downstream_patch_digest() -> None:
    lock = _load_lock()
    downstream = lock["sourceInputs"]["downstreamPatches"]
    paths = [MIDL_COMPAT_PATCH_PATH, PATCH_PATH, CLOSE_BOUND_PATCH_PATH]
    assert [item["path"] for item in downstream] == [
        path.relative_to(REPO_ROOT).as_posix() for path in paths
    ]
    for item, path in zip(downstream, paths, strict=True):
        assert set(item) == {
            "applyAfterUpstream",
            "path",
            "sha256",
            "sizeBytes",
        }
        assert item["applyAfterUpstream"] is True
        assert item["sha256"] == _sha256(path)
        assert item["sizeBytes"] == path.stat().st_size


def test_downstream_patch_execution_order_is_closed() -> None:
    driver = STRICT_BUILD_PATH.read_text(encoding="utf-8")
    upstream = driver.index('label=f"upstream-patch-{index:02d}"')
    upstream_postimage = driver.index(
        'verify_patch_surface(\n            lock, source, patch_surface_paths, "postPatchSurface"',
        upstream,
    )
    canvas_preimage = driver.index(
        '_verify_seams(lock, source, "postUpstreamPatchSha256")',
        upstream_postimage,
    )
    midl_preimage = driver.index(
        "_verify_midl_compatibility_seams(", canvas_preimage
    )
    midl_preimage_hash = driver.index(
        '"postUpstreamPatchSha256"', midl_preimage
    )
    midl = driver.index(
        'label="verisilo-ff152-midl-cross-build-patch"', midl_preimage_hash
    )
    midl_postimage = driver.index('"postCompatibilityPatchSha256"', midl)
    canvas = driver.index('label="verisilo-canvas-patch"', midl_postimage)
    close_bound = driver.index(
        'label="verisilo-juggler-close-bound-patch"', canvas
    )
    canvas_postimage = driver.index('"postDownstreamPatchSha256"', close_bound)
    configure = driver.index(
        'label="configure-windows-x86_64-and-bootstrap-toolchains"',
        canvas_postimage,
    )
    assert (
        upstream
        < upstream_postimage
        < canvas_preimage
        < midl_preimage
        < midl_preimage_hash
        < midl
        < midl_postimage
        < canvas
        < close_bound
        < canvas_postimage
        < configure
    )

    order = _load_lock()["buildBinding"]["recipe"]["order"]
    assert order[order.index("apply-50-upstream-patches") : order.index(
        "configure-windows-x86_64-and-bootstrap-toolchains"
    )] == [
        "apply-50-upstream-patches",
        "verify-canvas-seam-preimages",
        "verify-ff152-midl-compatibility-seam-preimage",
        "apply-verisilo-ff152-midl-cross-build-patch",
        "verify-ff152-midl-compatibility-seam-postimage",
        "apply-verisilo-canvas-patch",
        "verify-canvas-seam-postimages",
        "apply-verisilo-juggler-close-bound-patch",
        "verify-juggler-close-bound-seam-postimage",
    ]


def test_build_inputs_record_ordered_downstream_patches() -> None:
    driver = STRICT_BUILD_PATH.read_text(encoding="utf-8")
    validation = driver.index(
        'raise BuildFailure("VeriSilo downstream patch order/contract mismatch")'
    )
    evidence = driver.index(
        '"downstreamPatches": [dict(item) for item in downstream_patches]',
        validation,
    )
    execute = driver.index("def execute(", evidence)
    assert validation < evidence < execute

    expected = _load_lock()["sourceInputs"]["downstreamPatches"]
    assert [item["path"] for item in expected] == [
        MIDL_COMPAT_PATCH_PATH.relative_to(REPO_ROOT).as_posix(),
        PATCH_PATH.relative_to(REPO_ROOT).as_posix(),
        CLOSE_BOUND_PATCH_PATH.relative_to(REPO_ROOT).as_posix(),
    ]


def test_pinned_oci_build_recipe_is_closed() -> None:
    lock = _load_lock()
    build = lock["buildBinding"]
    recipe = build["recipe"]
    expected_paths = [
        DOCKERFILE_PATH.relative_to(REPO_ROOT).as_posix(),
        STRICT_BUILD_PATH.relative_to(REPO_ROOT).as_posix(),
        BUILD_HOST_PATH.relative_to(REPO_ROOT).as_posix(),
    ]
    assert [item["path"] for item in recipe["files"]] == expected_paths
    for item in recipe["files"]:
        path = REPO_ROOT / item["path"]
        assert path.is_file()
        assert path.stat().st_size == item["sizeBytes"]
        assert _sha256(path) == item["sha256"]

    dockerfile = DOCKERFILE_PATH.read_text(encoding="utf-8")
    assert dockerfile.splitlines()[0] == (
        "FROM ubuntu:24.04@" + BASE_INDEX_DIGEST
    )
    assert "ubuntu:latest" not in dockerfile
    assert f'org.opencontainers.image.base.digest="{BASE_INDEX_DIGEST}"' in dockerfile
    assert (
        f'io.verisilo.base.linux-amd64-manifest="{BASE_AMD64_MANIFEST_DIGEST}"'
        in dockerfile
    )
    assert "COPY strict_build.py /usr/local/bin/verisilo-camoufox-strict-build" in dockerfile
    assert 'ENTRYPOINT ["python3", "/usr/local/bin/verisilo-camoufox-strict-build"]' in dockerfile

    host_runtime = build["runtimeContainer"]["hostRuntime"]
    assert host_runtime == EXPECTED_HOST_RUNTIME
    pinned_i386 = "libc6-i386=2.39-0ubuntu8.8"
    assert re.search(
        rf"(?m)^\s+{re.escape(pinned_i386)}\s+\\$", dockerfile
    )
    assert not re.search(r"(?m)^\s+libc6-i386\s+\\$", dockerfile)
    assert "dpkg --add-architecture" not in dockerfile
    assert "libc6:i386" not in dockerfile
    assert not re.search(r"\b[\w.+-]+:i386\b", dockerfile)
    for package in host_runtime["requiredDpkgPackages"]:
        query = (
            'test "$(dpkg-query -W -f=\'${Architecture}=${Version}\' '
            + package["name"]
            + ')" = "'
            + package["architecture"]
            + "="
            + package["version"]
            + '"'
        )
        assert query in dockerfile
    for path in host_runtime["requiredExecutablePaths"]:
        assert f"test -x {path}" in dockerfile
    assert 'test -z "$(dpkg --print-foreign-architectures)"' in dockerfile

    driver = STRICT_BUILD_PATH.read_text(encoding="utf-8")
    assert EXPECTED_DRIVER_MARKERS <= set(driver.splitlines())
    ordered_markers = [
        'label="setup-minimal"',
        'label=f"upstream-patch-{index:02d}"',
        'label="verisilo-canvas-patch"',
        'label="configure-windows-x86_64-and-bootstrap-toolchains"',
        'label="build-windows-x86_64"',
        'label="package-windows-x86_64"',
    ]
    assert [driver.index(marker) for marker in ordered_markers] == sorted(
        driver.index(marker) for marker in ordered_markers
    )
    for forbidden in ("make fetch", "ubuntu:latest", "page.addInitScript"):
        assert forbidden not in driver

    host = BUILD_HOST_PATH.read_text(encoding="utf-8")
    for marker in (
        'commands.add_parser("prepare-image")',
        'commands.add_parser("prepare-bound-image")',
        'commands.add_parser("build-engine")',
        '"--no-cache"',
        '"--pull=false"',
        '"--read-only"',
        'dst=/inputs,readonly',
        'tmpfs_spec = _runtime_tmpfs_spec(locked["lock"])',
        '"tmpfs": EXPECTED_TMPFS_CONTRACT',
    ):
        assert marker in host


def test_host_launcher_accepts_containerd_v3_quote_styles() -> None:
    host = _load_python_file("canvas_engine_build_host", BUILD_HOST_PATH)
    for quote in ('"', "'"):
        dump = (
            "version = 3\n"
            f"root = {quote}/mnt/camoufox-build/containerd-root{quote}\n"
            "state = '/run/containerd'\n"
        )
        with mock.patch.object(host, "_capture", return_value=dump):
            assert host._containerd_root() == Path(
                "/mnt/camoufox-build/containerd-root"
            )


def test_host_launcher_creates_user_owned_binary_output() -> None:
    host = _load_python_file("canvas_engine_build_host_binary", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        output = root / "image.tar"
        log = root / "save.log"
        returncode = host._run_binary_output(
            [
                sys.executable,
                "-c",
                (
                    "import sys; "
                    "sys.stdout.buffer.write(b'bound-image'); "
                    "sys.stderr.write('saved\\n')"
                ),
            ],
            cwd=root,
            output_path=output,
            log_path=log,
        )
        assert returncode == 0
        assert output.read_bytes() == b"bound-image"
        assert "saved" in log.read_text(encoding="utf-8")


def test_host_launcher_verifies_historical_recipe_source_blobs() -> None:
    host = _load_python_file("canvas_engine_build_host_history", BUILD_HOST_PATH)
    binding = _builder_binding_for_tests()
    verified = host._verify_historical_recipe_source(REPO_ROOT, binding)
    assert verified == {
        "commit": binding["recipeSourceCommit"],
        "tree": binding["recipeSourceTree"],
        "sourceLockSha256": binding["recipeSourceLockSha256"],
        "dockerfileSha256": binding["dockerfileSha256"],
    }

    corruptions = {
        "recipeSourceTree": "0" * 40,
        "recipeSourceLockSha256": "0" * 64,
        "dockerfileSha256": "0" * 64,
    }
    for field, value in corruptions.items():
        altered = dict(binding)
        altered[field] = value
        try:
            host._verify_historical_recipe_source(REPO_ROOT, altered)
        except host.HostBuildFailure:
            pass
        else:
            raise AssertionError(f"historical recipe verification accepted {field}")


def test_host_launcher_rechecks_prepared_image_evidence_and_tooling() -> None:
    host = _load_python_file("canvas_engine_build_host_evidence", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        provenance = Path(temporary)
        evidence = {
            "buildx.log": b"buildx-log\n",
            "buildx-metadata.json": b'{"metadata":"bound"}\n',
            "builder-image-inspect.json": b'{"inspect":"bound"}\n',
        }
        for name, payload in evidence.items():
            (provenance / name).write_bytes(payload)

        tooling = {
            "dockerRoot": "/mnt/camoufox-build/docker-data",
            "containerdRoot": "/mnt/camoufox-build/containerd-root",
            "dockerVersion": {"Server": {"Version": "29.1.3"}},
            "buildxVersion": "github.com/docker/buildx 0.30.1",
            "containerdVersion": "containerd 2.2.1",
        }
        binding = {
            "buildxLogSha256": _sha256(provenance / "buildx.log"),
            "buildxLogSizeBytes": (provenance / "buildx.log").stat().st_size,
            "buildxMetadataSha256": _sha256(
                provenance / "buildx-metadata.json"
            ),
            "imageInspectSha256": _sha256(
                provenance / "builder-image-inspect.json"
            ),
            "hostToolingSha256": hashlib.sha256(
                json.dumps(
                    tooling,
                    sort_keys=True,
                    separators=(",", ":"),
                    ensure_ascii=False,
                ).encode("utf-8")
            ).hexdigest(),
        }
        verified = host._verify_prepared_image_evidence(
            provenance, binding, tooling
        )
        assert verified["hostToolingSha256"] == binding["hostToolingSha256"]
        assert set(verified["files"]) == {
            "buildxLog",
            "buildxMetadata",
            "imageInspect",
        }

        for name, original in evidence.items():
            path = provenance / name
            path.write_bytes(original[:-2] + b"X\n")
            try:
                host._verify_prepared_image_evidence(provenance, binding, tooling)
            except host.HostBuildFailure:
                pass
            else:
                raise AssertionError(f"builder evidence tampering accepted: {name}")
            path.write_bytes(original)

        altered_tooling = dict(tooling)
        altered_tooling["buildxVersion"] = "changed"
        try:
            host._verify_prepared_image_evidence(
                provenance, binding, altered_tooling
            )
        except host.HostBuildFailure:
            pass
        else:
            raise AssertionError("host tooling drift was accepted")


def test_host_launcher_requires_exact_executable_tmpfs_contract() -> None:
    host = _load_python_file("canvas_engine_build_host_tmpfs", BUILD_HOST_PATH)
    lock = _load_lock()
    expected = "/tmp:rw,nosuid,nodev,exec,mode=1777,size=4g"
    assert host._runtime_tmpfs_spec(lock) == expected

    variants = []
    for options in (
        ["rw", "nosuid", "nodev", "mode=1777", "size=4g"],
        ["rw", "nosuid", "nodev", "noexec", "mode=1777", "size=4g"],
        [
            "rw",
            "nosuid",
            "nodev",
            "exec",
            "mode=1777",
            "size=4g",
            "unknown",
        ],
        ["rw", "nosuid", "exec", "nodev", "mode=1777", "size=4g"],
    ):
        altered = json.loads(json.dumps(lock))
        altered["buildBinding"]["runtimeContainer"]["tmpfs"]["options"] = options
        variants.append(altered)
    for altered in variants:
        try:
            host._runtime_tmpfs_spec(altered)
        except host.HostBuildFailure:
            pass
        else:
            raise AssertionError("non-exact executable /tmp contract was accepted")


def test_host_launcher_requires_exact_wine_prefix_contract_and_run_id() -> None:
    host = _load_python_file("canvas_engine_build_host_wine_prefix", BUILD_HOST_PATH)
    lock = _load_lock()
    run_id = "canvas-run-0001"
    expected = {
        "contract": EXPECTED_WINE_PREFIX,
        "resolvedPath": f"/work/{run_id}/.wine-prefix",
    }
    assert host._runtime_wine_prefix(lock, run_id) == expected

    corruptions = (
        ("policy", "open"),
        ("environmentVariable", "HOME"),
        ("pathTemplate", "/tmp/{runId}/wine"),
        ("defaultHomeFallbackAllowed", True),
        ("defaultHomeFallbackAllowed", 0),
    )
    for field, value in corruptions:
        altered = json.loads(json.dumps(lock))
        altered["buildBinding"]["runtimeContainer"]["winePrefix"][field] = value
        try:
            host._runtime_wine_prefix(altered, run_id)
        except host.HostBuildFailure as exc:
            assert str(exc) == "source lock Wine prefix contract is not exact"
        else:
            raise AssertionError(f"non-exact Wine prefix contract accepted: {field}")

    altered = json.loads(json.dumps(lock))
    altered["buildBinding"]["runtimeContainer"]["winePrefix"]["unexpected"] = True
    try:
        host._runtime_wine_prefix(altered, run_id)
    except host.HostBuildFailure as exc:
        assert str(exc) == "source lock Wine prefix contract is not exact"
    else:
        raise AssertionError("extended Wine prefix contract was accepted")

    for invalid_run_id in (
        "short",
        "Canvas-run-0001",
        "../escape-run-0001",
        12345678,
    ):
        try:
            host._runtime_wine_prefix(lock, invalid_run_id)
        except host.HostBuildFailure as exc:
            assert str(exc) == "Wine prefix run-id is not exact"
        else:
            raise AssertionError(f"invalid Wine prefix run-id accepted: {invalid_run_id}")


def test_host_launcher_injects_wine_prefix_before_image_and_records_it() -> None:
    host = _load_python_file("canvas_engine_build_host_wine_command", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        runs_root = Path(temporary).resolve() / "runs"
        run_id = "canvas-run-0001"
        run_root = runs_root / run_id
        inputs = run_root / "inputs"
        provenance_dir = run_root / "provenance"
        (inputs / "verisilo").mkdir(parents=True)
        (inputs / "upstream").mkdir()
        (inputs / host.FIREFOX_ARCHIVE_NAME).write_bytes(b"firefox")
        provenance_dir.mkdir()
        (provenance_dir / "builder-image-result.json").write_text(
            "{}\n", encoding="utf-8"
        )
        owner_token = "focused-owner-token"
        owner = {
            "recordType": "verisilo-camoufox-build-owner/v1",
            "runId": run_id,
            "token": owner_token,
        }
        (run_root / host.OWNER_NAME).write_text(
            json.dumps(owner), encoding="utf-8"
        )

        lock = json.loads(json.dumps(_load_lock()))
        binding = _builder_binding_for_tests()
        lock["buildBinding"]["builderImageBinding"] = binding
        source = {
            "branch": "test",
            "commit": "1" * 40,
            "tree": "2" * 40,
            "lockPath": host.LOCK_REL.as_posix(),
            "lockSha256": "3" * 64,
            "dockerfileSha256": binding["dockerfileSha256"],
        }
        locked = {"lock": lock, "firefox": {}}
        tooling = {
            "dockerRoot": "/mnt/camoufox-build/docker-data",
            "containerdRoot": "/mnt/camoufox-build/containerd-root",
        }
        captured: dict[str, object] = {}

        def run_logged(command, *, cwd, log_path, environment):
            captured["command"] = list(command)
            captured["cwd"] = cwd
            captured["environment"] = environment
            log_path.write_text("focused container failure\n", encoding="utf-8")
            return 17

        args = argparse.Namespace(
            run_id=run_id,
            run_root=str(run_root),
            owner_token=owner_token,
        )
        with (
            mock.patch.object(host, "RUNS_ROOT", runs_root),
            mock.patch.object(
                host,
                "_validate_data_mount",
                return_value={
                    "path": str(runs_root.parent),
                    "device": 1,
                    "runsRoot": str(runs_root),
                },
            ),
            mock.patch.object(
                host, "_validate_container_roots", return_value=tooling
            ),
            mock.patch.object(
                host, "_validate_verisilo", return_value=(source, locked)
            ),
            mock.patch.object(
                host,
                "_validate_other_inputs",
                return_value={"inputs": "verified"},
            ),
            mock.patch.object(
                host, "_verify_committed_builder_binding", return_value=binding
            ),
            mock.patch.object(
                host,
                "_verify_historical_recipe_source",
                return_value={"historical": "verified"},
            ),
            mock.patch.object(
                host,
                "_verify_prepared_image_evidence",
                return_value={"prepared": "verified"},
            ),
            mock.patch.object(
                host,
                "_verify_bound_image_archive",
                return_value={"archive": "verified"},
            ),
            mock.patch.object(
                host,
                "_verify_live_bound_image",
                return_value={"id": binding["imageId"]},
            ),
            mock.patch.object(host, "_run_logged", side_effect=run_logged),
        ):
            assert host.build_engine(args) == 17

        command = captured["command"]
        assert isinstance(command, list)
        expected_path = f"/work/{run_id}/.wine-prefix"
        wine_environment = f"WINEPREFIX={expected_path}"
        wine_index = command.index(wine_environment)
        image_index = command.index(binding["imageId"])
        assert command[wine_index - 1] == "--env"
        assert command.count(wine_environment) == 1
        assert wine_index < image_index
        assert not any(
            isinstance(item, str) and item.startswith("HOME=") for item in command
        )
        assert not any(
            isinstance(item, str) and item.startswith("WINEDEBUG=")
            for item in command
        )
        assert not {"--user", "--chown", "wineboot"}.intersection(command)

        provenance = json.loads(
            (provenance_dir / "host-provenance.json").read_text(encoding="utf-8")
        )
        assert provenance["status"] == "container-failed"
        assert provenance["container"]["exitCode"] == 17
        assert provenance["container"]["winePrefix"] == {
            "contract": EXPECTED_WINE_PREFIX,
            "resolvedPath": expected_path,
        }
        assert provenance["runtimeContainer"]["winePrefix"] == provenance[
            "container"
        ]["winePrefix"]


def test_host_launcher_records_wine_prefix_before_launcher_exception() -> None:
    host = _load_python_file("canvas_engine_build_host_wine_initial", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        run_id = "canvas-run-0002"
        run_root = Path(temporary).resolve() / run_id
        provenance_dir = run_root / "provenance"
        provenance_dir.mkdir(parents=True)
        layout = {}
        for name in ("build-home", "work", "out"):
            path = run_root / name
            path.mkdir()
            layout[name] = path

        lock = json.loads(json.dumps(_load_lock()))
        binding = _builder_binding_for_tests()
        lock["buildBinding"]["builderImageBinding"] = binding
        source = {
            "branch": "test",
            "commit": "1" * 40,
            "tree": "2" * 40,
            "lockPath": host.LOCK_REL.as_posix(),
            "lockSha256": "3" * 64,
            "dockerfileSha256": binding["dockerfileSha256"],
        }
        owner = {
            "recordType": "verisilo-camoufox-build-owner/v1",
            "runId": run_id,
            "token": "focused-owner-token",
        }
        command_seen: list[str] = []

        def raise_launcher(command, **_kwargs):
            command_seen.extend(command)
            raise OSError("focused launcher exception")

        args = argparse.Namespace(
            run_id=run_id,
            run_root=str(run_root),
            owner_token=owner["token"],
        )
        with (
            mock.patch.object(host, "_validate_data_mount", return_value={}),
            mock.patch.object(host, "_validate_container_roots", return_value={}),
            mock.patch.object(
                host,
                "_validate_input_layout",
                return_value={
                    "verisilo": run_root / "verisilo",
                    "upstream": run_root / "upstream",
                    "firefox": run_root / host.FIREFOX_ARCHIVE_NAME,
                },
            ),
            mock.patch.object(host, "_validate_engine_layout"),
            mock.patch.object(host, "_load_owner", return_value=owner),
            mock.patch.object(
                host,
                "_validate_verisilo",
                return_value=(source, {"lock": lock, "firefox": {}}),
            ),
            mock.patch.object(host, "_validate_other_inputs", return_value={}),
            mock.patch.object(host, "_strict_json", return_value={}),
            mock.patch.object(
                host, "_verify_committed_builder_binding", return_value=binding
            ),
            mock.patch.object(
                host, "_verify_historical_recipe_source", return_value={}
            ),
            mock.patch.object(
                host, "_verify_prepared_image_evidence", return_value={}
            ),
            mock.patch.object(host, "_verify_bound_image_archive", return_value={}),
            mock.patch.object(
                host,
                "_verify_live_bound_image",
                return_value={"id": binding["imageId"]},
            ),
            mock.patch.object(host, "_create_output_layout", return_value=layout),
            mock.patch.object(host, "_run_logged", side_effect=raise_launcher),
        ):
            try:
                host.build_engine(args)
            except OSError as exc:
                assert str(exc) == "focused launcher exception"
            else:
                raise AssertionError("launcher exception was not propagated")

        expected_path = f"/work/{run_id}/.wine-prefix"
        assert f"WINEPREFIX={expected_path}" in command_seen
        assert not {"--user", "--chown", "wineboot"}.intersection(command_seen)
        assert not any(item.startswith("WINEDEBUG=") for item in command_seen)
        provenance = json.loads(
            (provenance_dir / "host-provenance.json").read_text(encoding="utf-8")
        )
        assert provenance["status"] == "build-engine-started"
        assert "container" not in provenance
        assert provenance["runtimeContainer"]["winePrefix"] == {
            "contract": EXPECTED_WINE_PREFIX,
            "resolvedPath": expected_path,
        }


def test_host_launcher_prepared_record_types_are_fail_closed() -> None:
    host = _load_python_file("canvas_engine_build_host_records", BUILD_HOST_PATH)
    lock = _load_lock()
    binding = _builder_binding_for_tests()
    lock["buildBinding"]["builderImageBinding"] = binding
    source_run_id = "source-run-0001"
    fresh_run_id = "fresh-run-0001"
    source_owner = {
        "recordType": "verisilo-camoufox-build-owner/v1",
        "runId": source_run_id,
        "token": "source-token",
    }
    fresh_owner = {
        "recordType": "verisilo-camoufox-build-owner/v1",
        "runId": fresh_run_id,
        "token": "fresh-token",
    }
    original = {
        "recordType": host.ORIGINAL_PREPARED_RECORD,
        "runId": source_run_id,
        "owner": source_owner,
        "bindingProposal": dict(binding),
        "status": host.ORIGINAL_PREPARED_STATUS,
    }
    assert host._verify_committed_builder_binding(
        lock,
        original,
        expected_run_id=source_run_id,
        expected_owner=source_owner,
        allow_bound_prepared=False,
    ) == binding

    bound = {
        "recordType": host.BOUND_PREPARED_RECORD,
        "runId": fresh_run_id,
        "sourceRunId": source_run_id,
        "sourcePreparedRecord": {
            "recordType": host.ORIGINAL_PREPARED_RECORD,
            "status": host.ORIGINAL_PREPARED_STATUS,
            "runId": source_run_id,
            "sha256": "1" * 64,
            "sizeBytes": 1,
        },
        "owner": fresh_owner,
        "bindingProposal": dict(binding),
        "status": host.BOUND_PREPARED_STATUS,
    }
    assert host._verify_committed_builder_binding(
        lock,
        bound,
        expected_run_id=fresh_run_id,
        expected_owner=fresh_owner,
        allow_bound_prepared=True,
    ) == binding

    invalid = []
    wrong_run = dict(original)
    wrong_run["runId"] = fresh_run_id
    invalid.append((wrong_run, source_run_id, source_owner, False))
    wrong_status = dict(original)
    wrong_status["status"] = "prepared"
    invalid.append((wrong_status, source_run_id, source_owner, False))
    wrong_record = dict(original)
    wrong_record["recordType"] = host.BOUND_PREPARED_RECORD
    invalid.append((wrong_record, source_run_id, source_owner, False))
    chained = dict(bound)
    invalid.append((chained, fresh_run_id, fresh_owner, False))
    self_sourced = dict(bound)
    self_sourced["sourceRunId"] = fresh_run_id
    invalid.append((self_sourced, fresh_run_id, fresh_owner, True))
    extra_proposal = json.loads(json.dumps(original))
    extra_proposal["bindingProposal"]["unexpected"] = True
    invalid.append((extra_proposal, source_run_id, source_owner, False))
    wrong_owner = dict(original)
    wrong_owner["owner"] = fresh_owner
    invalid.append((wrong_owner, source_run_id, source_owner, False))
    missing_source_record = dict(bound)
    del missing_source_record["sourcePreparedRecord"]
    invalid.append((missing_source_record, fresh_run_id, fresh_owner, True))
    for prepared, expected_run_id, expected_owner, allow_bound in invalid:
        try:
            host._verify_committed_builder_binding(
                lock,
                prepared,
                expected_run_id=expected_run_id,
                expected_owner=expected_owner,
                allow_bound_prepared=allow_bound,
            )
        except host.HostBuildFailure:
            pass
        else:
            raise AssertionError("invalid prepared builder record was accepted")


def test_host_launcher_source_run_path_and_layout_are_fail_closed() -> None:
    host = _load_python_file("canvas_engine_build_host_source", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        runs_root = Path(temporary).resolve() / "runs"
        runs_root.mkdir()
        source = runs_root / "source-run-0001"
        destination = runs_root / "fresh-run-0001"
        source.mkdir()
        destination.mkdir()
        (source / "inputs").mkdir()
        (source / "provenance").mkdir()
        owner_record = {
            "recordType": "verisilo-camoufox-build-owner/v1",
            "runId": source.name,
            "token": "source-token",
        }
        (source / host.OWNER_NAME).write_text(
            json.dumps(owner_record),
            encoding="utf-8",
        )
        with mock.patch.object(host, "RUNS_ROOT", runs_root):
            actual = host._validate_source_run_root(source, destination)
            assert actual == {
                "runId": source.name,
                "provenance": source / "provenance",
                "owner": owner_record,
            }

            invalid = [source]
            nested = runs_root / "nested"
            nested.mkdir()
            nested_source = nested / "nested-run-0001"
            nested_source.mkdir()
            invalid.append(nested_source)
            for path in invalid:
                try:
                    host._validate_source_run_root(path, source)
                except host.HostBuildFailure:
                    pass
                else:
                    raise AssertionError("invalid source-run-root was accepted")

            owner = json.loads((source / host.OWNER_NAME).read_text(encoding="utf-8"))
            owner["runId"] = "wrong-run-0001"
            (source / host.OWNER_NAME).write_text(
                json.dumps(owner), encoding="utf-8"
            )
            try:
                host._validate_source_run_root(source, destination)
            except host.HostBuildFailure:
                pass
            else:
                raise AssertionError("source owner/run-id mismatch was accepted")


def test_host_launcher_prepares_fresh_run_from_frozen_bound_image_once() -> None:
    host = _load_python_file("canvas_engine_build_host_reuse", BUILD_HOST_PATH)
    with tempfile.TemporaryDirectory() as temporary:
        runs_root = Path(temporary).resolve() / "runs"
        runs_root.mkdir()

        def make_input_run(run_id: str) -> Path:
            run = runs_root / run_id
            inputs = run / "inputs"
            inputs.mkdir(parents=True)
            (inputs / "verisilo").mkdir()
            (inputs / "upstream").mkdir()
            (inputs / host.FIREFOX_ARCHIVE_NAME).write_bytes(b"firefox")
            return run

        source_run = runs_root / "source-run-0001"
        source_provenance = source_run / "provenance"
        (source_run / "inputs").mkdir(parents=True)
        source_provenance.mkdir()
        # A failed build-engine source remains immutable and may retain these.
        for name in ("build-home", "work", "out"):
            (source_run / name).mkdir()
        source_owner = {
            "recordType": "verisilo-camoufox-build-owner/v1",
            "runId": source_run.name,
            "token": "source-owner-token",
        }
        (source_run / host.OWNER_NAME).write_text(
            json.dumps(source_owner), encoding="utf-8"
        )

        evidence = {
            "buildx.log": b"bound build log\n",
            "buildx-metadata.json": b'{"bound":true}\n',
            "builder-image-inspect.json": b'{"Id":"bound"}\n',
            "builder-image.tar": b"bound-builder-image",
        }
        for name, payload in evidence.items():
            (source_provenance / name).write_bytes(payload)
        (source_provenance / "docker-save.log").write_text(
            "must not be copied\n", encoding="utf-8"
        )
        (source_provenance / "host-provenance.json").write_text(
            '{"mustNotBeCopied":true}\n', encoding="utf-8"
        )

        tooling = {
            "dockerRoot": "/mnt/camoufox-build/docker-data",
            "containerdRoot": "/mnt/camoufox-build/containerd-root",
            "dockerVersion": {"Server": {"Version": "29.1.3"}},
            "buildxVersion": "github.com/docker/buildx 0.30.1",
            "containerdVersion": "containerd 2.2.1",
        }
        lock = json.loads(json.dumps(_load_lock()))
        binding = _builder_binding_for_tests()
        binding.update(
            {
                "buildxLogSha256": _sha256(source_provenance / "buildx.log"),
                "buildxLogSizeBytes": (
                    source_provenance / "buildx.log"
                ).stat().st_size,
                "buildxMetadataSha256": _sha256(
                    source_provenance / "buildx-metadata.json"
                ),
                "imageInspectSha256": _sha256(
                    source_provenance / "builder-image-inspect.json"
                ),
                "savedArchiveSha256": _sha256(
                    source_provenance / "builder-image.tar"
                ),
                "savedArchiveSizeBytes": (
                    source_provenance / "builder-image.tar"
                ).stat().st_size,
                "hostToolingSha256": host._canonical_sha256(tooling),
            }
        )
        lock["buildBinding"]["builderImageBinding"] = binding
        source_prepared = {
            "recordType": host.ORIGINAL_PREPARED_RECORD,
            "runId": source_run.name,
            "owner": source_owner,
            "bindingProposal": dict(binding),
            "status": host.ORIGINAL_PREPARED_STATUS,
        }
        source_result_path = source_provenance / "builder-image-result.json"
        source_result_path.write_text(
            json.dumps(source_prepared, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

        fresh_run = make_input_run("fresh-run-0001")
        recipe_source = {
            "branch": "test",
            "commit": "1" * 40,
            "tree": "2" * 40,
            "lockPath": host.LOCK_REL.as_posix(),
            "lockSha256": "3" * 64,
            "dockerfileSha256": binding["dockerfileSha256"],
        }
        historical = {
            "commit": binding["recipeSourceCommit"],
            "tree": binding["recipeSourceTree"],
            "sourceLockSha256": binding["recipeSourceLockSha256"],
            "dockerfileSha256": binding["dockerfileSha256"],
        }
        image = {
            "id": binding["imageId"],
            "labels": {},
            "os": "linux",
            "architecture": "amd64",
        }
        args = argparse.Namespace(
            run_id=fresh_run.name,
            run_root=str(fresh_run),
            source_run_root=str(source_run),
        )
        with (
            mock.patch.object(host, "RUNS_ROOT", runs_root),
            mock.patch.object(host, "_validate_data_mount", return_value={
                "path": str(runs_root.parent),
                "device": 1,
                "runsRoot": str(runs_root),
            }),
            mock.patch.object(
                host, "_validate_container_roots", return_value=tooling
            ),
            mock.patch.object(
                host,
                "_validate_verisilo",
                return_value=(recipe_source, {"lock": lock, "firefox": {}}),
            ),
            mock.patch.object(
                host,
                "_validate_other_inputs",
                return_value={"inputs": "verified"},
            ),
            mock.patch.object(
                host,
                "_verify_historical_recipe_source",
                return_value=historical,
            ),
            mock.patch.object(
                host, "_verify_live_bound_image", return_value=image
            ),
            mock.patch.object(
                host,
                "_run_logged",
                side_effect=AssertionError("prepare-bound-image invoked buildx"),
            ),
            mock.patch.object(
                host,
                "_run_binary_output",
                side_effect=AssertionError("prepare-bound-image invoked docker save"),
            ),
        ):
            assert host.prepare_bound_image(args) == 0

            fresh_provenance = fresh_run / "provenance"
            assert {path.name for path in fresh_provenance.iterdir()} == {
                *host.FROZEN_BUILDER_EVIDENCE,
                "builder-image-result.json",
            }
            result = json.loads(
                (fresh_provenance / "builder-image-result.json").read_text(
                    encoding="utf-8"
                )
            )
            assert result["recordType"] == host.BOUND_PREPARED_RECORD
            assert result["status"] == host.BOUND_PREPARED_STATUS
            assert result["runId"] == fresh_run.name
            assert result["sourceRunId"] == source_run.name
            assert result["bindingProposal"] == binding
            assert result["sourcePreparedRecord"]["runId"] == source_run.name
            assert result["sourcePreparedRecord"]["sha256"] == _sha256(
                source_result_path
            )
            assert result["owner"]["token"] != source_owner["token"]
            assert (
                json.loads(
                    (fresh_run / host.OWNER_NAME).read_text(encoding="utf-8")
                )["token"]
                == result["owner"]["token"]
            )

            copied_tar = fresh_provenance / "builder-image.tar"
            copied_tar.write_bytes(b"tampered-builder-image")
            try:
                host._verify_bound_image_archive(fresh_provenance, binding)
            except host.HostBuildFailure:
                pass
            else:
                raise AssertionError("bound image archive tampering was accepted")

            tampered_run = make_input_run("tamper-run-0001")
            (source_provenance / "buildx.log").write_bytes(b"tampered log\n")
            tampered_args = argparse.Namespace(
                run_id=tampered_run.name,
                run_root=str(tampered_run),
                source_run_root=str(source_run),
            )
            try:
                host.prepare_bound_image(tampered_args)
            except host.HostBuildFailure:
                pass
            else:
                raise AssertionError("tampered frozen source evidence was accepted")
            assert not (tampered_run / host.OWNER_NAME).exists()


EXPECTED_DRIVER_MARKERS = {
    '        "--batch",',
    '        "--forward",',
    '        "--fuzz=0",',
    '    "--fuzz=2",',
    '    "--ignore-whitespace",',
    '            raise BuildFailure(f"{label} failed with exit code {returncode}")',
    '    if workspace.exists() or result_dir.exists():',
    '        raise BuildFailure("one-shot run workspace/output already exists")',
    '        _verify_midl_compatibility_seams(',
    '        _verify_seams(lock, source, "postUpstreamPatchSha256")',
    '        _verify_seams(lock, source, "postDownstreamPatchSha256")',
    '        verify_patch_surface(',
    "    mounts = validate_mounts()",
    '                BUILD_HOME, result_dir / "build-home-before-build.json"',
    '                BUILD_HOME, result_dir / "build-home-after-build.json"',
    '            raise BuildFailure("container dpkg closure changed during the build")',
    '        log.close()',
}


def test_build_recipe_covers_executed_upstream_inputs() -> None:
    inputs = _load_lock()["sourceInputs"]
    recipe_paths = {item["path"] for item in inputs["recipeFiles"]}
    assert {
        "Makefile",
        "multibuild.py",
        "scripts/_mixin.py",
        "scripts/copy-additions.sh",
        "scripts/package.py",
        "scripts/patch.py",
        "patches/librewolf/pack_vs.py",
        "assets/base.mozconfig",
        "assets/windows.mozconfig",
    } <= recipe_paths
    tree_paths = {item["path"] for item in inputs["inputTrees"]}
    assert {
        "additions",
        "settings",
        "assets",
        "bundle/fonts/macos",
        "bundle/fonts/linux",
    } == tree_paths


def test_upstream_patch_order_is_closed() -> None:
    patches = _load_lock()["sourceInputs"]["upstreamPatches"]
    assert len(patches) == 50
    paths = [item["path"] for item in patches]
    assert len(paths) == len(set(paths))
    assert paths == sorted(paths, key=lambda value: Path(value).name)
    for item in patches:
        assert re.fullmatch(r"[0-9a-f]{64}", item["sha256"])
        assert type(item["sizeBytes"]) is int and item["sizeBytes"] > 0


def test_upstream_patch_application_contract_and_commands() -> None:
    driver = _load_python_file("canvas_engine_strict_patch_policy", STRICT_BUILD_PATH)
    application = _load_lock()["sourceInputs"]["upstreamPatchApplication"]
    assert application == {
        "programVersion": "GNU patch 2.7.6",
        "command": [
            "patch",
            "-p1",
            "--batch",
            "--binary",
            "--forward",
            "--ignore-whitespace",
            "--fuzz=2",
            "--no-backup-if-mismatch",
            "-i",
            "{patch}",
        ],
        "headerPairCount": 281,
        "createdPathCount": 19,
        "surfacePathCount": 222,
        "pathListCanonicalization": (
            "ordinal-sorted safe relative POSIX paths, UTF-8, each LF-terminated"
        ),
        "surfacePathListSha256": (
            "6c4dd3fa1e6431773aaa4aa37173052761b43025df966a7438a4d34facde8ae6"
        ),
        "surfaceCanonicalization": (
            "same path order; compact sorted-key UTF-8 JSON array; file rows "
            "contain path,sha256,size,type=file; absent rows contain "
            "path,type=absent; mode and mtime excluded"
        ),
        "debrisBaselineCanonicalization": (
            "ordinal-sorted relative POSIX paths; orig path list is UTF-8 with "
            "each path LF-terminated; canonical orig state is a compact "
            "sorted-key UTF-8 JSON array with path,sha256,size; mode and mtime "
            "excluded"
        ),
        "prePatchDebrisBaseline": {
            "canonicalOrigSha256": (
                "ae23e8cf45f40bceed7292ab20965d399be12dc21323b94bd3a7c2c0f3b14da7"
            ),
            "origCount": 548,
            "origPathListSha256": (
                "5257c70c7d8d471eafbf33862e894324831bfd2a5c78d2f1ef08e7201280360f"
            ),
            "rejectCount": 0,
            "totalOrigBytes": 714909,
        },
        "prePatchSurface": {
            "absentCount": 19,
            "canonicalSurfaceSha256": (
                "1a6f9fb5fc08efab2814d8cfbaa306d1202b82b044a2b743de3af95068089408"
            ),
            "fileCount": 203,
            "surfacePathCount": 222,
            "totalFileBytes": 13879593,
        },
        "postPatchSurface": {
            "absentCount": 0,
            "canonicalSurfaceSha256": (
                "761da79324631c5cfccfb00e6621947c70fde9c392a83c7b9a0d8359cf70ca8d"
            ),
            "fileCount": 222,
            "surfacePathCount": 222,
            "totalFileBytes": 14068638,
        },
    }

    upstream = driver.upstream_patch_command(Path("locked-upstream.patch"))
    downstream = driver.downstream_patch_command(Path("verisilo.patch"))
    assert "--ignore-whitespace" in upstream
    assert "--fuzz=2" in upstream
    assert "--fuzz=0" not in upstream
    assert "--fuzz=0" in downstream
    assert "--ignore-whitespace" not in downstream


def test_patch_surface_parser_and_state_are_fail_closed() -> None:
    driver = _load_python_file("canvas_engine_patch_surface", STRICT_BUILD_PATH)

    def mini_lock(patch_name: str, patch_text: str, paths: list[str]) -> tuple[dict, Path]:
        patch_path = upstream / patch_name
        patch_path.write_text(patch_text, encoding="utf-8", newline="\n")
        encoded = "".join(f"{path}\n" for path in sorted(paths)).encode("utf-8")
        lock = {
            "sourceInputs": {
                "upstreamPatches": [{"path": patch_name}],
                "upstreamPatchApplication": {
                    "command": list(driver.EXPECTED_UPSTREAM_PATCH_COMMAND),
                    "createdPathCount": 1,
                    "debrisBaselineCanonicalization": (
                        "ordinal-sorted relative POSIX paths; orig path list is "
                        "UTF-8 with each path LF-terminated; canonical orig state "
                        "is a compact sorted-key UTF-8 JSON array with "
                        "path,sha256,size; mode and mtime excluded"
                    ),
                    "headerPairCount": 2,
                    "pathListCanonicalization": (
                        "ordinal-sorted safe relative POSIX paths, UTF-8, each "
                        "LF-terminated"
                    ),
                    "postPatchSurface": {},
                    "prePatchDebrisBaseline": {},
                    "prePatchSurface": {},
                    "programVersion": driver.EXPECTED_UPSTREAM_PATCH_PROGRAM,
                    "surfaceCanonicalization": (
                        "same path order; compact sorted-key UTF-8 JSON array; "
                        "file rows contain path,sha256,size,type=file; absent rows "
                        "contain path,type=absent; mode and mtime excluded"
                    ),
                    "surfacePathCount": len(paths),
                    "surfacePathListSha256": hashlib.sha256(encoded).hexdigest(),
                },
            }
        }
        return lock, patch_path

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        upstream = root / "upstream"
        source = root / "source"
        upstream.mkdir()
        source.mkdir()
        patch_text = """diff --git a/alpha.txt b/alpha.txt
--- a/alpha.txt
+++ b/alpha.txt
@@ -1 +1 @@
-old
+new
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+created
"""
        lock, _ = mini_lock("surface.patch", patch_text, ["alpha.txt", "new.txt"])
        paths, pairs, path_digest = driver.upstream_patch_surface(lock, upstream)
        assert paths == ["alpha.txt", "new.txt"]
        assert pairs == 2
        assert path_digest == lock["sourceInputs"]["upstreamPatchApplication"][
            "surfacePathListSha256"
        ]

        (source / "alpha.txt").write_text("old\n", encoding="utf-8", newline="\n")
        pre = driver.patch_surface_state(source, paths)
        assert pre["fileCount"] == 1
        assert pre["absentCount"] == 1
        assert pre["totalFileBytes"] == 4
        (source / "new.txt").write_text(
            "created\n", encoding="utf-8", newline="\n"
        )
        assert driver.patch_surface_state(source, paths) != pre

        (source / "alpha.txt").unlink()
        (source / "alpha.txt").mkdir()
        try:
            driver.patch_surface_state(source, paths)
        except driver.BuildFailure:
            pass
        else:
            raise AssertionError("a directory inside the patch surface was accepted")

        malformed = {
            "rename.patch": "--- a/alpha.txt\n+++ b/beta.txt\n",
            "delete.patch": "--- a/alpha.txt\n+++ /dev/null\n",
            "traversal.patch": "--- a/../escape.txt\n+++ b/../escape.txt\n",
            "unpaired.patch": "--- a/alpha.txt\nnot-a-new-header\n",
        }
        for name, text in malformed.items():
            bad_lock, _ = mini_lock(name, text, ["alpha.txt", "new.txt"])
            try:
                driver.upstream_patch_surface(bad_lock, upstream)
            except driver.BuildFailure:
                pass
            else:
                raise AssertionError(f"malformed upstream patch was accepted: {name}")


def test_patch_debris_baseline_is_exact_and_fail_closed() -> None:
    driver = _load_python_file("canvas_engine_patch_debris", STRICT_BUILD_PATH)

    def expect_failure(action, message: str) -> None:
        try:
            action()
        except driver.BuildFailure:
            pass
        else:
            raise AssertionError(message)

    with tempfile.TemporaryDirectory() as temporary:
        source = Path(temporary)
        legal_orig = source / "third_party" / "rust" / "crate" / "Cargo.toml.orig"
        legal_orig.parent.mkdir(parents=True)
        legal_orig.write_bytes(b"legal vendored manifest\n")
        expected = driver.patch_debris_summary(driver.patch_debris_state(source))
        lock = {
            "sourceInputs": {
                "upstreamPatchApplication": {
                    "prePatchDebrisBaseline": expected,
                }
            }
        }
        baseline = driver.capture_patch_debris_baseline(lock, source)
        driver.verify_patch_debris_unchanged(source, baseline)

        added_orig = source / "new-backup.orig"
        added_orig.write_bytes(b"unexpected backup\n")
        expect_failure(
            lambda: driver.verify_patch_debris_unchanged(source, baseline),
            "a new patch backup was accepted",
        )
        added_orig.unlink()

        legal_orig.write_bytes(b"modified baseline\n")
        expect_failure(
            lambda: driver.verify_patch_debris_unchanged(source, baseline),
            "a modified legal patch backup was accepted",
        )
        legal_orig.write_bytes(b"legal vendored manifest\n")

        legal_orig.unlink()
        expect_failure(
            lambda: driver.verify_patch_debris_unchanged(source, baseline),
            "a deleted legal patch backup was accepted",
        )
        legal_orig.write_bytes(b"legal vendored manifest\n")

        reject = source / "failed.cpp.rej"
        reject.write_bytes(b"rejected hunk\n")
        expect_failure(
            lambda: driver.verify_patch_debris_unchanged(source, baseline),
            "a new patch reject was accepted",
        )
        expect_failure(
            lambda: driver.capture_patch_debris_baseline(lock, source),
            "a pre-patch reject was accepted into the legal baseline",
        )


def test_patch_program_version_is_fail_closed() -> None:
    driver = _load_python_file("canvas_engine_patch_program", STRICT_BUILD_PATH)
    completed = subprocess.CompletedProcess(
        args=["patch", "--version"],
        returncode=0,
        stdout="GNU patch 9.9\n",
        stderr="",
    )
    with tempfile.TemporaryDirectory() as temporary:
        with mock.patch.object(driver.subprocess, "run", return_value=completed):
            try:
                driver.verify_patch_program(dict(), Path(temporary))
            except driver.BuildFailure:
                pass
            else:
                raise AssertionError("an unexpected GNU patch version was accepted")


def test_exact_upstream_patch_surface_postimage() -> None:
    if UPSTREAM_REPO is None or PATCHED_SOURCE_TREE is None:
        raise SkipTest("pass exact upstream and patched source trees")
    driver = _load_python_file("canvas_engine_exact_patch_surface", STRICT_BUILD_PATH)
    lock = _load_lock()
    paths, pairs, path_digest = driver.upstream_patch_surface(lock, UPSTREAM_REPO)
    application = lock["sourceInputs"]["upstreamPatchApplication"]
    assert pairs == application["headerPairCount"]
    assert path_digest == application["surfacePathListSha256"]
    assert driver.patch_surface_state(PATCHED_SOURCE_TREE, paths) == application[
        "postPatchSurface"
    ]


def test_exact_firefox_source_archive() -> None:
    global EXACT_FIREFOX_COMPATIBILITY_PREIMAGES
    if FIREFOX_SOURCE_ARCHIVE is None:
        raise SkipTest(
            "pass --firefox-source-archive for exact Firefox source verification"
        )
    source = _load_lock()["firefoxSource"]
    assert FIREFOX_SOURCE_ARCHIVE.is_file()
    assert FIREFOX_SOURCE_ARCHIVE.stat().st_size == source["sizeBytes"]
    assert _sha512(FIREFOX_SOURCE_ARCHIVE) == source["sha512"]

    driver = _load_python_file("canvas_engine_archive_debris", STRICT_BUILD_PATH)
    compatibility_seams = _load_lock()["midlCompatibilitySeamFiles"]
    compatibility_paths = {seam["path"] for seam in compatibility_seams}
    EXACT_FIREFOX_COMPATIBILITY_PREIMAGES = {}
    orig_files: list[dict] = []
    reject_paths: list[str] = []
    with tarfile.open(FIREFOX_SOURCE_ARCHIVE, "r:xz") as bundle:
        for member in bundle:
            prefix = "firefox-152.0.4/"
            if member.name.startswith(prefix) and member.name[len(prefix) :] in (
                compatibility_paths
            ):
                stream = bundle.extractfile(member)
                assert stream is not None
                EXACT_FIREFOX_COMPATIBILITY_PREIMAGES[
                    member.name[len(prefix) :]
                ] = stream.read()
            if not (member.name.endswith(".orig") or member.name.endswith(".rej")):
                continue
            assert member.isfile(), member.name
            prefix, separator, relative = member.name.partition("/")
            assert prefix == "firefox-152.0.4" and separator and relative
            if relative.endswith(".rej"):
                reject_paths.append(relative)
                continue
            stream = bundle.extractfile(member)
            assert stream is not None
            data = stream.read()
            orig_files.append(
                {
                    "path": relative,
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "size": len(data),
                }
            )
    orig_files.sort(key=lambda row: row["path"])
    reject_paths.sort()
    archive_debris = {"origFiles": orig_files, "rejectPaths": reject_paths}
    expected = _load_lock()["sourceInputs"]["upstreamPatchApplication"][
        "prePatchDebrisBaseline"
    ]
    assert driver.patch_debris_summary(archive_debris) == expected
    assert set(EXACT_FIREFOX_COMPATIBILITY_PREIMAGES) == compatibility_paths
    for seam in compatibility_seams:
        assert (
            hashlib.sha256(
                EXACT_FIREFOX_COMPATIBILITY_PREIMAGES[seam["path"]]
            ).hexdigest()
            == seam["postUpstreamPatchSha256"]
        )


def test_exact_windows_toolchain_selection_manifest() -> None:
    if PATCHED_SOURCE_TREE is None:
        raise SkipTest("pass --patched-source-tree for exact toolchain manifest")
    lock = _load_lock()
    expected = lock["buildBinding"]["windowsToolchain"]["selectionManifest"]
    path = PATCHED_SOURCE_TREE / expected["path"]
    assert path.is_file()
    assert path.stat().st_size == expected["size"]
    assert _sha256(path) == expected["sha256"]


def test_exact_upstream_checkout_inputs() -> None:
    if UPSTREAM_REPO is None:
        raise SkipTest("pass --upstream-repo for exact upstream input verification")

    lock = _load_lock()
    assert _git(UPSTREAM_REPO, "rev-parse", "HEAD") == lock["upstream"]["commit"]
    assert _git(UPSTREAM_REPO, "rev-parse", "HEAD^{tree}") == lock["upstream"]["tree"]
    assert (
        _git(
            UPSTREAM_REPO,
            "status",
            "--short",
            "--untracked-files=all",
            "--ignored=matching",
        )
        == ""
    )

    inputs = lock["sourceInputs"]
    actual_patch_paths = [
        path.relative_to(UPSTREAM_REPO).as_posix()
        for path in sorted(
            (UPSTREAM_REPO / "patches").rglob("*.patch"),
            key=lambda path: path.name,
        )
    ]
    assert actual_patch_paths == [item["path"] for item in inputs["upstreamPatches"]]
    for item in inputs["upstreamPatches"] + inputs["recipeFiles"]:
        path = UPSTREAM_REPO / item["path"]
        assert path.is_file(), item["path"]
        assert path.stat().st_size == item["sizeBytes"], item["path"]
        assert _sha256(path) == item["sha256"], item["path"]

    for expected in inputs["inputTrees"]:
        actual = _canonical_tree(UPSTREAM_REPO / expected["path"])
        assert actual == (
            expected["fileCount"],
            expected["totalBytes"],
            expected["canonicalTreeSha256"],
        )


def test_ff152_compatibility_patch_transforms_exact_archive_preimages_to_final_source() -> None:
    if FIREFOX_SOURCE_ARCHIVE is None or PATCHED_SOURCE_TREE is None:
        raise SkipTest("pass exact Firefox archive and final patched source tree")

    seams = _load_lock()["midlCompatibilitySeamFiles"]
    preimages = dict(EXACT_FIREFOX_COMPATIBILITY_PREIMAGES)
    if set(preimages) != {seam["path"] for seam in seams}:
        preimages = {}
        with tarfile.open(FIREFOX_SOURCE_ARCHIVE, "r:xz") as bundle:
            for seam in seams:
                stream = bundle.extractfile(f"firefox-152.0.4/{seam['path']}")
                assert stream is not None
                preimages[seam["path"]] = stream.read()

    for seam in seams:
        assert (
            hashlib.sha256(preimages[seam["path"]]).hexdigest()
            == seam["postUpstreamPatchSha256"]
        )
        final_source = PATCHED_SOURCE_TREE / seam["path"]
        assert final_source.is_file()
        assert _sha256(final_source) == seam["postCompatibilityPatchSha256"]

    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary)
        for seam in seams:
            target = root / seam["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(preimages[seam["path"]])
        check = subprocess.run(
            [
                "git",
                "-c",
                "core.autocrlf=false",
                "apply",
                "--check",
                "--unidiff-zero",
                "--whitespace=error-all",
                str(MIDL_COMPAT_PATCH_PATH),
            ],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert check.returncode == 0, check.stderr or check.stdout
        subprocess.run(
            [
                "git",
                "-c",
                "core.autocrlf=false",
                "apply",
                "--unidiff-zero",
                "--whitespace=error-all",
                str(MIDL_COMPAT_PATCH_PATH),
            ],
            cwd=root,
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        for seam in seams:
            target = root / seam["path"]
            final_source = PATCHED_SOURCE_TREE / seam["path"]
            assert _sha256(target) == seam["postCompatibilityPatchSha256"]
            assert target.read_bytes() == final_source.read_bytes()

        midl = (root / "build" / "midl.py").read_text(encoding="utf-8")
        assert "command = preprocessor + [relativize(input)]" in midl
        assert "command = preprocessor + [input]" not in midl
        rules = (root / "config" / "rules.mk").read_text(encoding="utf-8")
        assert (
            "$(CC) $(OUTOPTION)$@ -c $(COMPILE_CFLAGS) "
            "$($(notdir $<)_FLAGS) $(call relativize,$<)" in rules
        )
        assert (
            "$(CCC) $(OUTOPTION)$@ -c $(COMPILE_CXXFLAGS) "
            "$($(notdir $<)_FLAGS) $(call relativize,$<)" in rules
        )
        assert (
            "$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py "
            "$(DEFINES) $(INCLUDES) -o $@ $(call relativize,$<)" in rules
        )
        assert (
            "$(PYTHON3) $(MOZILLA_DIR)/config/create_res.py "
            "$(DEFINES) $(INCLUDES) -o $@ $<" not in rules
        )


def test_patch_applies_to_exact_seam_preimages() -> None:
    if PREIMAGE_SOURCE_TREE is None:
        raise SkipTest("pass --preimage-source-tree for exact patch application")

    lock = _load_lock()
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        for seam in lock["seamFiles"]:
            source = PREIMAGE_SOURCE_TREE / seam["path"]
            assert source.is_file(), seam["path"]
            assert _sha256(source) == seam["postUpstreamPatchSha256"]
            target = root / seam["path"]
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)

        check = subprocess.run(
            [
                "git",
                "-c",
                "core.autocrlf=false",
                "apply",
                "--check",
                "--whitespace=error-all",
                str(PATCH_PATH),
            ],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert check.returncode == 0, check.stderr or check.stdout
        for patch_path in (PATCH_PATH, CLOSE_BOUND_PATCH_PATH):
            subprocess.run(
                [
                    "git",
                    "-c",
                    "core.autocrlf=false",
                    "apply",
                    "--whitespace=error-all",
                    str(patch_path),
                ],
                cwd=root,
                check=True,
                capture_output=True,
                text=True,
                timeout=30,
            )
        for seam in lock["seamFiles"]:
            assert _sha256(root / seam["path"]) == seam["postDownstreamPatchSha256"]
        implementation = (
            root
            / "toolkit"
            / "components"
            / "resistfingerprinting"
            / "nsRFPService.cpp"
        ).read_text(encoding="utf-8")
        assert implementation.index('MaskConfig::HasKey("canvas:seed", maskConfig)') < (
            implementation.index("NS_ENSURE_TRUE_VOID(aCookieJarSettings)")
        )
        close_bound = (root / "juggler" / "protocol" / "BrowserHandler.js").read_text(
            encoding="utf-8"
        )
        _assert_juggler_close_bound_invariants(close_bound)


def _assert_juggler_close_bound_invariants(source: str) -> None:
    """Structural contract of the bounded Juggler Browser.close patch."""

    close_at = source.index("async ['Browser.close']()")
    method_end = source.index("async ['Browser.grantPermissions']", close_at)
    method = source[close_at:method_end]
    assert "Date.now() + 3000" in method
    assert method.count("verisiloBoundedStage(Promise.race([") == 1
    assert method.count("verisiloBoundedStage(this._startCompletePromise") == 1
    assert method.count("verisiloBoundedStage(Promise.all([") == 1
    assert "Promise.race([settled, deadlineFallback])" in method
    assert "clearTimeout(timer);" in method
    assert method.count("Services.startup.quit(Ci.nsIAppStartup.eForceQuit);") == 1
    assert method.index("verisiloBoundedStage(Promise.race([") < method.index(
        "verisiloBoundedStage(this._startCompletePromise"
    )
    assert method.index("verisiloBoundedStage(this._startCompletePromise") < method.index(
        "verisiloBoundedStage(Promise.all(["
    )
    assert method.index("'xpiStartupPromises')") < method.index(
        "Services.startup.quit(Ci.nsIAppStartup.eForceQuit);"
    )
    for unbounded in (
        "    await this._startCompletePromise;\n",
        "    await Promise.all([\n",
        "      ]);\n",
    ):
        assert unbounded not in method, unbounded
    assert (
        "const {setTimeout, clearTimeout} = "
        "ChromeUtils.importESModule('resource://gre/modules/Timer.sys.mjs');"
    ) in source


def test_juggler_close_bound_patch_contract_is_closed() -> None:
    """Offline closure for the tracked close-bound patch and its seam hashes."""

    lock = _load_lock()
    downstream = lock["sourceInputs"]["downstreamPatches"]
    close_entry = next(
        item
        for item in downstream
        if item["path"] == CLOSE_BOUND_PATCH_PATH.relative_to(REPO_ROOT).as_posix()
    )
    assert close_entry["applyAfterUpstream"] is True
    assert close_entry["sha256"] == _sha256(CLOSE_BOUND_PATCH_PATH)
    assert close_entry["sizeBytes"] == CLOSE_BOUND_PATCH_PATH.stat().st_size

    seam = next(
        item
        for item in lock["seamFiles"]
        if item["path"] == JUGGLER_CLOSE_SEAM_PATH
    )
    patch = CLOSE_BOUND_PATCH_PATH.read_text(encoding="utf-8")
    assert patch.count(f"diff --git a/{JUGGLER_CLOSE_SEAM_PATH}") == 1
    added = [
        line[1:]
        for line in patch.splitlines()
        if line.startswith("+") and not line.startswith("+++")
    ]
    removed = [
        line[1:]
        for line in patch.splitlines()
        if line.startswith("-") and not line.startswith("---")
    ]
    assert (
        "    await this._startCompletePromise;"
        in [line for line in removed if "startCompletePromise" in line]
    )
    assert "    await verisiloBoundedStage(Promise.all([" in added
    assert "    await verisiloBoundedStage(this._startCompletePromise, 'startComplete');" in added
    assert any("'idleTasksOrWindowClosed'" in line for line in added)
    assert any("'startComplete'" in line for line in added)
    assert any("'xpiStartupPromises'" in line for line in added)
    assert any("Date.now() + 3000" in line for line in added)
    assert any(
        "ChromeUtils.importESModule('resource://gre/modules/Timer.sys.mjs');"
        in line
        for line in added
    )
    # The preimage is the upstream additions file, byte-identical to the
    # camoufox v152.0.4-beta.28 checkout; the postimage is the patched tree.
    assert seam["postUpstreamPatchSha256"] == (
        "7eadb3dd570cd98688d4c3120f7ab36fe1cdc808f89ff51b61ce19db2661afc0"
    )
    assert seam["postDownstreamPatchSha256"] == (
        "3b5d24b610e85370e3c55f2c2d619b0c0bdf388061c3fa4290ab20f33db3b38e"
    )


def test_patched_source_seam_and_caller_graph() -> None:
    if PATCHED_SOURCE_TREE is None:
        raise SkipTest("pass --patched-source-tree for exact seam/caller verification")
    if shutil.which("rg") is None:
        raise SkipTest("rg is required for the exact caller-graph check")

    lock = _load_lock()
    for seam in lock["seamFiles"]:
        assert _sha256(PATCHED_SOURCE_TREE / seam["path"]) == seam[
            "postDownstreamPatchSha256"
        ]

    result = subprocess.run(
        [
            "rg",
            "-l",
            "GetFingerprintingRandomizationKeyAsString",
            str(PATCHED_SOURCE_TREE),
            "-g",
            "*.cpp",
            "-g",
            "*.h",
        ],
        check=True,
        capture_output=True,
        text=True,
        # A complete FF152 source-tree scan takes about 138 seconds on the
        # managed Windows NTFS worktree. Keep the whole-tree caller check,
        # but bound it above that measured runtime.
        timeout=180,
    )
    callers = {
        Path(line).relative_to(PATCHED_SOURCE_TREE).as_posix()
        for line in result.stdout.splitlines()
        if line.strip()
    }
    assert callers == {
        "dom/canvas/CanvasRenderingContextHelper.cpp",
        "dom/html/HTMLCanvasElement.cpp",
        "toolkit/components/resistfingerprinting/nsRFPService.cpp",
        "toolkit/components/resistfingerprinting/nsRFPService.h",
    }


def main() -> int:
    global UPSTREAM_REPO, FIREFOX_SOURCE_ARCHIVE, PREIMAGE_SOURCE_TREE
    global PATCHED_SOURCE_TREE

    parser = argparse.ArgumentParser()
    parser.add_argument("--tracked-only", action="store_true")
    parser.add_argument("--upstream-repo", type=Path)
    parser.add_argument("--firefox-source-archive", type=Path)
    parser.add_argument("--preimage-source-tree", type=Path)
    parser.add_argument("--patched-source-tree", type=Path)
    args = parser.parse_args()
    exact_paths = (
        args.upstream_repo,
        args.firefox_source_archive,
        args.preimage_source_tree,
        args.patched_source_tree,
    )
    if args.tracked_only:
        if any(path is not None for path in exact_paths):
            parser.error("--tracked-only cannot be combined with exact source paths")
    elif any(path is None for path in exact_paths):
        parser.error(
            "exact verification requires --upstream-repo, "
            "--firefox-source-archive, --preimage-source-tree, and "
            "--patched-source-tree; use --tracked-only only for reduced "
            "tracked-text self-consistency"
        )
    UPSTREAM_REPO = args.upstream_repo
    FIREFOX_SOURCE_ARCHIVE = args.firefox_source_archive
    PREIMAGE_SOURCE_TREE = args.preimage_source_tree
    PATCHED_SOURCE_TREE = args.patched_source_tree

    tests = [
        (name, fn)
        for name, fn in sorted(globals().items())
        if name.startswith("test_") and callable(fn)
    ]
    failed = 0
    skipped = 0
    skipped_names: set[str] = set()
    for name, fn in tests:
        try:
            fn()
            print(f"PASS {name}")
        except SkipTest as exc:
            skipped += 1
            skipped_names.add(name)
            print(f"SKIP {name}: {exc}")
        except Exception as exc:  # noqa: BLE001
            failed += 1
            print(f"FAIL {name}: {exc}")
    if failed:
        print(f"{failed}/{len(tests)} tests failed ({skipped} skipped)")
        return 1
    expected_tracked_only_skips = {
        "test_exact_firefox_source_archive",
        "test_exact_upstream_patch_surface_postimage",
        "test_exact_upstream_checkout_inputs",
        "test_exact_windows_toolchain_selection_manifest",
        "test_ff152_compatibility_patch_transforms_exact_archive_preimages_to_final_source",
        "test_patch_applies_to_exact_seam_preimages",
        "test_patched_source_seam_and_caller_graph",
    }
    if args.tracked_only and skipped_names != expected_tracked_only_skips:
        print(
            "tracked-only skipped an unexpected test set: "
            f"{sorted(skipped_names)!r}"
        )
        return 1
    if not args.tracked_only and skipped_names:
        print(f"exact verification cannot skip tests: {sorted(skipped_names)!r}")
        return 1
    print(f"all {len(tests) - skipped} executed tests passed ({skipped} skipped)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
