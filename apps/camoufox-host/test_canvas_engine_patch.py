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
PREVIOUS_BUILDER_IMAGE_BINDING = {
    "baseIndexDigest": BASE_INDEX_DIGEST,
    "baseLinuxAmd64ManifestDigest": BASE_AMD64_MANIFEST_DIGEST,
    "buildxLogSha256": (
        "8dc2c50cb39fc01c55b201677c62220d1ce5bc57421758bfb499e0df8a73acda"
    ),
    "buildxLogSizeBytes": 77778,
    "buildxMetadataSha256": (
        "147ec6dcc7a94c6594083383ef93d697bc9e40f95746578ddda14bcfaa7a8395"
    ),
    "dockerfileSha256": (
        "4f37cec6a6bce33f44ba3e5caaf2ae4fd2c08394c1e433d1d565523a125d9f43"
    ),
    "hostToolingSha256": (
        "a6857ad0855755e1c3fa70fbbfd42c8ed84abf4697510846f76b3c5c8127ad5b"
    ),
    "imageId": (
        "sha256:bf93fbf90499f32ff31be18cabe89fee85a342c752f972a72f51384182489e10"
    ),
    "imageInspectSha256": (
        "8bb317d396f3990ca111a7c410c6bb63e85f8cfe53da27c560c8db4470b7f81b"
    ),
    "recipeSourceCommit": "72ba3d30b2cdbb8a11197c6b7d7ba7eeca96e623",
    "recipeSourceLockSha256": (
        "904ac16a47ec2800f05d9ac45736d4c58a38aaf4cdff4751187ae696951d0e95"
    ),
    "recipeSourceTree": "ce8a6f329b70f5e25152a2cacd4c9bdce064627e",
    "savedArchiveSha256": (
        "2548a057a4134e2d16bf77421c86bcb766f32923971a7b2a74f04c4936e19e24"
    ),
    "savedArchiveSizeBytes": 479035392,
}

UPSTREAM_REPO: Path | None = None
FIREFOX_SOURCE_ARCHIVE: Path | None = None
PREIMAGE_SOURCE_TREE: Path | None = None
PATCHED_SOURCE_TREE: Path | None = None


def _load_lock() -> dict:
    return json.loads(LOCK_PATH.read_text(encoding="utf-8"))


def _builder_binding_for_tests() -> dict:
    active = _load_lock()["buildBinding"]["builderImageBinding"]
    return dict(active or PREVIOUS_BUILDER_IMAGE_BINDING)


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
    if FIREFOX_SOURCE_ARCHIVE is None:
        raise SkipTest(
            "pass --firefox-source-archive for exact Firefox source verification"
        )
    source = _load_lock()["firefoxSource"]
    assert FIREFOX_SOURCE_ARCHIVE.is_file()
    assert FIREFOX_SOURCE_ARCHIVE.stat().st_size == source["sizeBytes"]
    assert _sha512(FIREFOX_SOURCE_ARCHIVE) == source["sha512"]

    driver = _load_python_file("canvas_engine_archive_debris", STRICT_BUILD_PATH)
    orig_files: list[dict] = []
    reject_paths: list[str] = []
    with tarfile.open(FIREFOX_SOURCE_ARCHIVE, "r:xz") as bundle:
        for member in bundle:
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
