#!/usr/bin/env python3
"""Focused source-binding tests for the FP1 Canvas Engine Patch.

The default run requires every exact source input. ``--tracked-only`` is an
explicit reduced mode for text self-consistency checks; it is not source,
compile, binary, or runtime evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import tempfile
from pathlib import Path
from unittest import SkipTest


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

UPSTREAM_REPO: Path | None = None
FIREFOX_SOURCE_ARCHIVE: Path | None = None
PREIMAGE_SOURCE_TREE: Path | None = None
PATCHED_SOURCE_TREE: Path | None = None


def _load_lock() -> dict:
    return json.loads(LOCK_PATH.read_text(encoding="utf-8"))


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _sha512(path: Path) -> str:
    digest = hashlib.sha512()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


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


def test_source_lock_contract() -> None:
    lock = _load_lock()
    assert lock["schema"] == "verisilo-camoufox-source-binding/v1"
    assert lock["engineRevision"] == (
        "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1"
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
    }
    assert any(
        "SHA-512 verification" in requirement
        for requirement in build["requiredBeforeRuntime"]
    )
    assert any(
        "upstream-patches -> VeriSilo-patch -> build -> package" in requirement
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
    assert len(downstream) == 1
    item = downstream[0]
    assert item["applyAfterUpstream"] is True
    assert item["path"] == PATCH_PATH.relative_to(REPO_ROOT).as_posix()
    assert item["sha256"] == _sha256(PATCH_PATH)
    assert item["sizeBytes"] == PATCH_PATH.stat().st_size


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
        'commands.add_parser("build-engine")',
        '"--no-cache"',
        '"--pull=false"',
        '"--read-only"',
        'dst=/inputs,readonly',
    ):
        assert marker in host


EXPECTED_DRIVER_MARKERS = {
    '        "--batch",',
    '        "--forward",',
    '        "--fuzz=0",',
    '            raise BuildFailure(f"{label} failed with exit code {returncode}")',
    '    if workspace.exists() or result_dir.exists():',
    '        raise BuildFailure("one-shot run workspace/output already exists")',
    '        _verify_seams(lock, source, "postUpstreamPatchSha256")',
    '        _verify_seams(lock, source, "postDownstreamPatchSha256")',
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


def test_exact_firefox_source_archive() -> None:
    if FIREFOX_SOURCE_ARCHIVE is None:
        raise SkipTest(
            "pass --firefox-source-archive for exact Firefox source verification"
        )
    source = _load_lock()["firefoxSource"]
    assert FIREFOX_SOURCE_ARCHIVE.is_file()
    assert FIREFOX_SOURCE_ARCHIVE.stat().st_size == source["sizeBytes"]
    assert _sha512(FIREFOX_SOURCE_ARCHIVE) == source["sha512"]


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
        subprocess.run(
            [
                "git",
                "-c",
                "core.autocrlf=false",
                "apply",
                "--whitespace=error-all",
                str(PATCH_PATH),
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
        timeout=30,
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
        "test_exact_upstream_checkout_inputs",
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
