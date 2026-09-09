#!/usr/bin/env python3
"""Build the bounded VeriSilo Camoufox Host schema-v3 package.

This is a package builder, not a release workflow.  It consumes the frozen
dual-boot Formal-v3 browser output, binds it directly through the pinned
source lock, build record, and browser tree, stages a fixed tree, and
optionally invokes the Windows SignedCms helper with a PFX path and password
environment name.  The password is never accepted as an argument.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

HOST_DIR = Path(__file__).resolve().parents[1] / "apps" / "camoufox-host"
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

from browser_tree import load_tree_manifest, verify_tree
from package_contract import (
    BROWSER_TREE_NAME,
    BROWSER_DIRECTORY,
    FORMAL_V3_ARCHIVE_SHA256,
    FORMAL_V3_ARCHIVE_SIZE,
    FORMAL_V3_BUILD_ID,
    FORMAL_V3_BUILD_RESULT_SHA256,
    FORMAL_V3_BROWSER_RELEASE,
    FORMAL_V3_CHANNEL,
    FORMAL_V3_ENGINE_VERSION,
    FORMAL_V3_ENGINE_REVISION,
    FORMAL_V3_EXECUTABLE_SHA256,
    FORMAL_V3_HOST_VERSION,
    FORMAL_V3_PLATFORM,
    FORMAL_V3_PROPERTIES_SHA256,
    FORMAL_V3_RUNTIME_TREE_SHA256,
    FORMAL_V3_RUNTIME_TREE_SIZE,
    FORMAL_V3_SOURCE_COMMIT,
    FORMAL_V3_SOURCE_LOCK_SHA256,
    FORMAL_V3_SOURCE_STAMP,
    FORMAL_V3_SOURCE_TREE,
    HOST_NAME,
    PACKAGE_MANIFEST_NAME,
    PACKAGE_TREE_NAME,
    PackageContractError,
    PackageLayout,
    build_package_tree,
    compact_json_bytes,
    manifest_signing_payload,
    read_json,
    recheck_formal_package,
    sha256_bytes,
    sha256_file,
    strict_json_loads,
    validate_v3_manifest,
)

PYINSTALLER_VERSION = "6.22.2"
REPO_ROOT = Path(__file__).resolve().parents[1]
SOURCE_LOCK_DEFAULT = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json"
BUILD_RESULT_DEFAULT = HOST_DIR / "lock" / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json"
SIGNER_HELPER = Path(__file__).with_name("sign-camoufox-host-manifest.ps1")
# The dual-boot clean build is bound directly through the pinned frozen
# inputs (source lock, build record, browser tree, browser executable).
# The historical FP3 intermediate runtime-asset lock is not a build input.
FORMAL_V3_PATCH_ORDER = [
    "0000",
    "0001",
    "0002",
    "0003",
    "0003a",
    "0004",
    "0005",
    "0006",
    "0007",
]
FORMAL_V3_BUILD_RECORD_TYPE = "verisilo-camoufox-r1-formal-build-run/v1"
FORMAL_V3_BUILD_RUN_ID = "r1formal-v3-win11dual-20260902"
FORMAL_V3_BUILD_TARGET = "x86_64-pc-windows-msvc"
FORMAL_V3_ARCHIVE_NAME = "camoufox-152.0.4-beta.28-win.x86_64.zip"
FORMAL_V3_EXTRACTION_TREE_NAME = "windows-extraction-tree.json"
FORMAL_V3_EXTRACTION_TREE_SHA256 = "ec342bfa1fec6101c88d1170b9ca0749e9080e4e1908d0ee61508b962526468d"
FORMAL_V3_EXTRACTION_TREE_SIZE = 95902
FORMAL_V3_UPSTREAM_PATCH_COUNT = 50
FORMAL_V3_BUILD_RESULT_RELPATH = (
    "apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json"
)
FORMAL_V3_CLAIMS = {
    "browserLaunches": 0,
    "compiled": True,
    "formalR1Passed": False,
    "formalSource": True,
    "runtimeVerified": False,
    "windowsRuntimeObserved": False,
}
CAPABILITIES = [
    "identity_template",
    "ua_ua_ch",
    "language_timezone",
    "screen",
    "canvas",
    "webgl",
    "fonts",
    "media_devices",
    "request_headers",
    "window",
    "iframe",
    "dedicated_worker",
]


def _fail(message: str) -> None:
    raise PackageContractError(message)


def _read_formal_json(path: Path) -> dict[str, Any]:
    value = read_json(path, max_bytes=2 * 1024 * 1024)
    if type(value) is not dict:
        _fail(f"{path.name} must contain one JSON object")
    return value


def validate_source_lock_content(source: dict[str, Any]) -> None:
    """Structural checks on the frozen Formal-v3 source lock content."""
    if source.get("engineRevision") != FORMAL_V3_ENGINE_REVISION:
        _fail("Formal-v3 source lock is not the frozen candidate")
    if source.get("completeAppliedPatchOrder") != FORMAL_V3_PATCH_ORDER:
        _fail("Formal-v3 source lock patch order is not the frozen candidate")
    build_result_binding = source.get("buildResultBinding")
    if (
        type(build_result_binding) is not dict
        or set(build_result_binding) != {"path", "status", "sourceLockImmutableAfterCheckpoint"}
        or build_result_binding.get("path") != FORMAL_V3_BUILD_RESULT_RELPATH
        or build_result_binding.get("status") != "absent-until-clean-build"
        or build_result_binding.get("sourceLockImmutableAfterCheckpoint") is not True
    ):
        _fail("Formal-v3 source lock build-result binding is not exact")
    build_binding = source.get("buildBinding")
    if type(build_binding) is not dict or build_binding.get("target") != FORMAL_V3_BUILD_TARGET:
        _fail("Formal-v3 source lock build binding is not exact")


def validate_formal_build_record(build: dict[str, Any]) -> None:
    """Structural checks on the frozen dual-boot Formal-v3 build record."""
    if type(build) is not dict or build.get("recordType") != FORMAL_V3_BUILD_RECORD_TYPE:
        _fail("Formal-v3 build record is not a formal build run record")
    if build.get("engineRevision") != FORMAL_V3_ENGINE_REVISION:
        _fail("Formal-v3 build record engine revision is not the frozen candidate")
    if (
        build.get("buildMode") != "formal"
        or build.get("formalSource") is not True
        or build.get("diagnosticOnly") is not False
    ):
        _fail("Formal-v3 build record classification is not exact")
    if build.get("runId") != FORMAL_V3_BUILD_RUN_ID:
        _fail("Formal-v3 build record run id is not the frozen dual-boot build")
    if build.get("claims") != FORMAL_V3_CLAIMS:
        _fail("Formal-v3 build record claims are not the frozen dual-boot candidate")
    if build.get("completeAppliedPatchOrder") != FORMAL_V3_PATCH_ORDER:
        _fail("Formal-v3 build record patch order is not the frozen candidate")
    if type(build.get("startedAtUtc")) is not str or type(build.get("completedAtUtc")) is not str:
        _fail("Formal-v3 build record has no build window")
    archive = build.get("archive")
    if (
        type(archive) is not dict
        or archive.get("name") != FORMAL_V3_ARCHIVE_NAME
        or archive.get("sha256") != FORMAL_V3_ARCHIVE_SHA256
        or archive.get("sizeBytes") != FORMAL_V3_ARCHIVE_SIZE
        or archive.get("camoufoxExeSha256") != FORMAL_V3_EXECUTABLE_SHA256
        or archive.get("buildId") != FORMAL_V3_BUILD_ID
        or archive.get("sourceStamp") != FORMAL_V3_SOURCE_STAMP
    ):
        _fail("Formal-v3 build record archive binding is not exact")
    extraction_tree = archive.get("treeManifest")
    if (
        type(extraction_tree) is not dict
        or extraction_tree.get("name") != FORMAL_V3_EXTRACTION_TREE_NAME
        or extraction_tree.get("sha256") != FORMAL_V3_EXTRACTION_TREE_SHA256
        or extraction_tree.get("sizeBytes") != FORMAL_V3_EXTRACTION_TREE_SIZE
        or type(extraction_tree.get("entryCount")) is not int
    ):
        _fail("Formal-v3 build record extraction-tree binding is not exact")
    inputs = build.get("inputs")
    if (
        type(inputs) is not dict
        or inputs.get("sourceLockSha256") != FORMAL_V3_SOURCE_LOCK_SHA256
        or inputs.get("completeAppliedPatchOrder") != FORMAL_V3_PATCH_ORDER
        or inputs.get("upstreamPatchCount") != FORMAL_V3_UPSTREAM_PATCH_COUNT
        or type(inputs.get("verisiloCommit")) is not str
        or type(inputs.get("verisiloTree")) is not str
        or type(inputs.get("upstreamCommit")) is not str
        or type(inputs.get("upstreamTree")) is not str
    ):
        _fail("Formal-v3 build record source binding is not exact")


def validate_formal_v3_inputs(
    source_lock_path: Path,
    build_result_path: Path,
    browser_root: Path,
    frozen_browser_tree_path: Path,
) -> dict[str, Any]:
    """Validate only the frozen Formal-v3 inputs needed by this package.

    The dual-boot clean build is bound directly through the pinned source
    lock, the pinned build record, and the pinned browser tree; the
    historical FP3 intermediate runtime-asset lock is not a build input.
    """
    for path in (
        source_lock_path,
        build_result_path,
        frozen_browser_tree_path,
    ):
        if path.is_symlink() or not path.is_file():
            _fail(f"Formal-v3 input is not a regular file: {path}")
    if browser_root.is_symlink() or not browser_root.is_dir():
        _fail(f"Formal-v3 browser root is not a real directory: {browser_root}")
    source_raw = source_lock_path.read_bytes()
    if sha256_bytes(source_raw) != FORMAL_V3_SOURCE_LOCK_SHA256:
        _fail("Formal-v3 source lock SHA-256 is not the frozen input")
    source = strict_json_loads(source_raw, source_lock_path.name)
    if type(source) is not dict:
        _fail("Formal-v3 source lock must contain one JSON object")
    validate_source_lock_content(source)

    build_raw = build_result_path.read_bytes()
    if sha256_bytes(build_raw) != FORMAL_V3_BUILD_RESULT_SHA256:
        _fail("Formal-v3 build record SHA-256 is not the frozen input")
    try:
        build_result_relpath = build_result_path.relative_to(REPO_ROOT).as_posix()
    except ValueError:
        build_result_relpath = None
    if build_result_relpath != source["buildResultBinding"]["path"]:
        _fail("Formal-v3 build record is not the source-lock-declared build result")
    build = strict_json_loads(build_raw, build_result_path.name)
    if type(build) is not dict:
        _fail("Formal-v3 build record must contain one JSON object")
    validate_formal_build_record(build)

    frozen_tree_raw = frozen_browser_tree_path.read_bytes()
    if (
        sha256_bytes(frozen_tree_raw) != FORMAL_V3_RUNTIME_TREE_SHA256
        or len(frozen_tree_raw) != FORMAL_V3_RUNTIME_TREE_SIZE
    ):
        _fail("Formal-v3 browser tree SHA-256 is not the frozen input")
    frozen_tree = load_tree_manifest(frozen_browser_tree_path)
    if frozen_tree.get("treeRootLabel") != browser_root.name:
        _fail("frozen browser tree root label does not match browser root")
    verify_tree(browser_root, frozen_tree)
    executable = browser_root / "camoufox.exe"
    if sha256_file(executable) != FORMAL_V3_EXECUTABLE_SHA256:
        _fail("final browser executable does not match Formal-v3")
    properties = browser_root / "properties.json"
    if properties.is_symlink() or not properties.is_file():
        _fail("Formal-v3 browser properties.json is missing")
    if sha256_file(properties) != FORMAL_V3_PROPERTIES_SHA256:
        _fail("Formal-v3 browser properties.json does not match the frozen input")
    return {
        "source": source,
        "build": build,
        "browserTree": frozen_tree,
        "browserTreeSha256": sha256_bytes(frozen_tree_raw),
        "browserExecutableSha256": sha256_file(executable),
    }


def _copy_regular(source: Path, destination: Path) -> None:
    if not source.is_file() or source.is_symlink():
        _fail(f"input is not a regular file: {source}")
    if destination.exists() or destination.is_symlink():
        _fail(f"package member collision: {destination}")
    if any(parent.is_symlink() for parent in destination.parent.parents) or destination.parent.is_symlink():
        _fail(f"package member parent is a symlink: {destination.parent}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def _build_pyinstaller(source: Path, python: str, work_root: Path) -> Path:
    version = subprocess.run(
        [python, "-m", "PyInstaller", "--version"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if version != PYINSTALLER_VERSION:
        _fail(f"PyInstaller {PYINSTALLER_VERSION} is required (got {version})")
    dist = work_root / "dist"
    work = work_root / "work"
    spec = work_root / "spec"
    subprocess.run(
        [
            python,
            "-m",
            "PyInstaller",
            "--noconfirm",
            "--clean",
            "--onedir",
            "--console",
            "--name",
            "camoufox-host",
            "--distpath",
            str(dist),
            "--workpath",
            str(work),
            "--specpath",
            str(spec),
            "--collect-data",
            "camoufox",
            "--collect-data",
            "playwright",
            "--collect-data",
            "browserforge",
            "--collect-data",
            "apify_fingerprint_datapoints",
            "--collect-data",
            "language_tags",
            "--collect-data",
            "tzdata",
            "--copy-metadata",
            "camoufox",
            "--copy-metadata",
            "playwright",
            "--copy-metadata",
            "browserforge",
            "--copy-metadata",
            "apify-fingerprint-datapoints",
            "--copy-metadata",
            "language-tags",
            "--copy-metadata",
            "tzdata",
            str(source),
        ],
        check=True,
    )
    executable = dist / "camoufox-host" / HOST_NAME
    if not executable.is_file():
        _fail("PyInstaller did not produce the expected one-folder Host executable")
    return executable


def _package_asset_lock(formal: dict[str, Any], browser_tree_sha256: str) -> dict[str, Any]:
    archive = formal["build"]["archive"]
    return {
        "schema": "verisilo-camoufox-package-asset/v1",
        "assetKind": "self-built",
        "verified": False,
        "evidenceClass": "compiled-not-runtime-verified",
        "package": "camoufox",
        "release": FORMAL_V3_BROWSER_RELEASE,
        "platform": "windows-x86_64",
        "pythonPackage": "camoufox==0.5.4",
        "engineRevision": FORMAL_V3_ENGINE_REVISION,
        "sha256": archive["sha256"],
        "browserExecutableSha256": archive["camoufoxExeSha256"],
        "sizeBytes": archive["sizeBytes"],
        "executableRelativePath": "camoufox.exe",
        "buildId": archive["buildId"],
        "sourceStamp": archive["sourceStamp"],
        "propertiesJsonSha256": FORMAL_V3_PROPERTIES_SHA256,
        "sourceBinding": {
            "commit": FORMAL_V3_SOURCE_COMMIT,
            "tree": FORMAL_V3_SOURCE_TREE,
            "sourceLockSha256": FORMAL_V3_SOURCE_LOCK_SHA256,
            "completeAppliedPatchOrder": list(FORMAL_V3_PATCH_ORDER),
        },
        "buildResultSha256": FORMAL_V3_BUILD_RESULT_SHA256,
        "browserTreeManifestSha256": browser_tree_sha256,
    }


def _write_json(path: Path, value: object) -> bytes:
    raw = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    path.write_bytes(raw)
    return raw


def _smoke_packaged_host(layout: PackageLayout, temporary: Path) -> None:
    roots = temporary / "host-smoke"
    artifact_root = roots / "identity"
    profile_root = roots / "profiles"
    state_root = roots / "engine-state"
    for root in (artifact_root, profile_root, state_root):
        root.mkdir(parents=True)
    arguments = [
        str(layout.host),
        "--package-root",
        str(layout.root),
        "--artifact-root",
        str(artifact_root),
        "--profile-root",
        str(profile_root),
        "--state-root",
        str(state_root),
    ]
    hello = subprocess.run(
        arguments,
        input=b'{"id":"package-smoke","command":"hello"}\n',
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    if hello.returncode != 0:
        _fail("packaged Host hello failed")
    response = strict_json_loads(hello.stdout, "packaged Host hello")
    result = response.get("result") if type(response) is dict else None
    if (
        type(response) is not dict
        or response.get("ok") is not True
        or type(result) is not dict
        or result.get("protocol") != "verisilo-camoufox-host/v1"
        or result.get("browserRelease") != FORMAL_V3_BROWSER_RELEASE
        or result.get("assetSha256") != FORMAL_V3_ARCHIVE_SHA256
        or result.get("state") != "idle"
        or result.get("verified") is not False
    ):
        _fail("packaged Host hello binding is not exact")

    seed = bytes(range(32))
    request = compact_json_bytes(
        {
            "seed": base64.b64encode(seed).decode("ascii"),
            "preset": "balanced-en-us",
        }
    )
    provision = subprocess.run(
        [*arguments, "--provision-artifact"],
        input=len(request).to_bytes(4, "big") + request,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=120,
        check=False,
    )
    if provision.returncode != 0 or len(provision.stdout) < 5:
        detail = provision.stderr.decode("utf-8", errors="replace").strip()[-2000:]
        if len(provision.stdout) >= 5:
            size = int.from_bytes(provision.stdout[:4], "big")
            if size == len(provision.stdout) - 4:
                rejected = strict_json_loads(
                    provision.stdout[4:], "rejected packaged Host provisioning"
                )
                error = rejected.get("error") if type(rejected) is dict else None
                if type(error) is dict:
                    detail = f"{error.get('code')}: {error.get('message')}"
        _fail(
            f"packaged Host Artifact provisioning failed (exit {provision.returncode}): {detail}"
        )
    response_size = int.from_bytes(provision.stdout[:4], "big")
    if response_size != len(provision.stdout) - 4 or response_size > 8 * 1024:
        _fail("packaged Host Artifact provisioning response is malformed")
    response = strict_json_loads(provision.stdout[4:], "packaged Host provisioning")
    result = response.get("result") if type(response) is dict else None
    if type(response) is not dict or response.get("ok") is not True or type(result) is not dict:
        _fail("packaged Host Artifact provisioning was rejected")
    artifact_id = result.get("artifactId")
    artifact_sha256 = result.get("artifactFileSha256")
    artifact_suffix = artifact_id.removeprefix("identity-") if type(artifact_id) is str else ""
    if (
        type(artifact_id) is not str
        or not artifact_id.startswith("identity-")
        or not artifact_suffix
        or len(artifact_suffix) > 64
        or not artifact_suffix[0].isalnum()
        or not all(character.isascii() and (character.islower() or character.isdigit() or character == "-") for character in artifact_suffix)
        or result.get("schema") != "verisilo-camoufox-resolved-identity/v5"
        or type(artifact_sha256) is not str
        or len(artifact_sha256) != 64
        or not all(character in "0123456789abcdef" for character in artifact_sha256)
    ):
        _fail("packaged Host Artifact binding is invalid")
    artifact = artifact_root / f"{artifact_id}.json"
    sidecar = artifact.with_name(f"{artifact.name}.sha256")
    if (
        not artifact.is_file()
        or sha256_file(artifact) != artifact_sha256
        or not sidecar.is_file()
        or sidecar.read_bytes() != f"{artifact_sha256}  {artifact.name}\n".encode("ascii")
    ):
        _fail("packaged Host Artifact bytes or sidecar do not match")


def _stage(
    out: Path,
    *,
    browser_root: Path,
    frozen_tree: Path,
    supervisor: Path,
    probe: Path,
    host_executable: Path | None,
    host_directory: Path | None,
    host_source: Path,
    source_lock: Path,
    build_result: Path,
    python: str,
    sign: bool,
    pfx_path: Path | None,
    password_env: str,
) -> dict[str, Any]:
    if host_executable is not None:
        _fail("--host-executable single-file bypass is not supported; use --host-directory")
    formal = validate_formal_v3_inputs(
        source_lock,
        build_result,
        browser_root,
        frozen_tree,
    )
    if out.exists():
        _fail(f"output already exists: {out} (remove it explicitly before rebuilding)")
    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="verisilo-camoufox-package-") as temporary:
        staging = Path(temporary) / "package"
        layout = PackageLayout.from_root(staging)
        shutil.copytree(browser_root, layout.browser_root, symlinks=True)
        if host_directory is not None:
            if not host_directory.is_dir() or host_directory.is_symlink():
                _fail("Host one-folder input must be a real directory")
            shutil.copytree(host_directory, layout.host.parent, symlinks=True)
        else:
            host_executable = _build_pyinstaller(host_source, python, Path(temporary) / "pyinstaller")
            # PyInstaller one-folder output has DLLs beside the executable.
            shutil.copytree(host_executable.parent, layout.host.parent, symlinks=True)
        if layout.host.is_symlink() or not layout.host.is_file():
            _fail("staged Host one-folder output is missing camoufox-host.exe")
        _copy_regular(supervisor, layout.supervisor)
        _copy_regular(probe, layout.probe)
        # Preserve the accepted Formal-v3 tree bytes and their raw digest;
        # reserializing the same entries would sever that exact binding.
        browser_tree_raw = frozen_tree.read_bytes()
        if sha256_bytes(browser_tree_raw) != FORMAL_V3_RUNTIME_TREE_SHA256:
            _fail("Formal-v3 browser tree changed after input validation")
        layout.browser_tree.write_bytes(browser_tree_raw)
        package_asset = _package_asset_lock(formal, sha256_bytes(browser_tree_raw))
        _write_json(layout.asset_lock, package_asset)
        package_tree = build_package_tree(staging)
        package_tree_raw = _write_json(layout.package_tree, package_tree)
        host_sha = sha256_file(layout.host)
        manifest = {
            "schemaVersion": 3,
            "engineId": "camoufox",
            "engineVersion": FORMAL_V3_ENGINE_VERSION,
            "channel": FORMAL_V3_CHANNEL,
            "platform": FORMAL_V3_PLATFORM,
            "artifactSha256": host_sha,
            "signature": {
                "algorithm": "cms-detached-sha256",
                "keyId": "0" * 64,
                "value": "",
            },
            "capabilities": CAPABILITIES,
            "entrypoint": {
                "kind": "camoufox-host-v1",
                "relativePath": "host/camoufox-host.exe",
                "protocol": "verisilo-camoufox-host/v1",
                "sha256": host_sha,
            },
            "treeManifest": {
                "relativePath": PACKAGE_TREE_NAME,
                "sha256": sha256_bytes(package_tree_raw),
            },
            "browserTreeManifest": {
                "relativePath": BROWSER_TREE_NAME,
                "sha256": sha256_bytes(browser_tree_raw),
            },
            "hostVersion": FORMAL_V3_HOST_VERSION,
            "browserRelease": FORMAL_V3_BROWSER_RELEASE,
            "browserAssetSha256": FORMAL_V3_ARCHIVE_SHA256,
        }
        validate_v3_manifest(manifest, allow_unsigned=True)
        payload = manifest_signing_payload(manifest)
        unsigned_manifest_path = Path(temporary) / "engine-package.unsigned.json"
        payload_path = Path(temporary) / "engine-package.payload.bin"
        _write_json(unsigned_manifest_path, manifest)
        payload_path.write_bytes(payload)
        _write_json(layout.root / PACKAGE_MANIFEST_NAME, manifest)
        if sign:
            if os.name != "nt":
                _fail("CMS signing is available only on Windows")
            if pfx_path is None or not pfx_path.is_file():
                _fail("--pfx-path must name an external PFX file when --sign is used")
            if not password_env or password_env not in os.environ:
                _fail("the PFX password must be supplied through the named environment variable")
            subprocess.run(
                [
                    "pwsh",
                    "-NoProfile",
                    "-File",
                    str(SIGNER_HELPER),
                    "-ManifestPath",
                    str(layout.root / PACKAGE_MANIFEST_NAME),
                    "-PayloadPath",
                    str(payload_path),
                    "-PfxPath",
                    str(pfx_path),
                    "-PasswordEnv",
                    password_env,
                ],
                check=True,
                env=os.environ.copy(),
            )
        final_manifest = _read_formal_json(layout.root / PACKAGE_MANIFEST_NAME)
        validate_v3_manifest(final_manifest, allow_unsigned=not sign)
        recheck_formal_package(layout.root, final_manifest)
        _smoke_packaged_host(layout, Path(temporary))
        shutil.copytree(staging, out)
        # Keep unsigned canonical inputs beside (not inside) the package:
        # putting either in package-tree.json would create a hash cycle.  The
        # signer assigns keyId before producing its payload, so derive both
        # sidecars from the final manifest after signing.
        unsigned_manifest = dict(final_manifest)
        unsigned_signature = dict(final_manifest["signature"])
        unsigned_signature["value"] = ""
        unsigned_manifest["signature"] = unsigned_signature
        unsigned_payload = manifest_signing_payload(final_manifest)
        if payload_path.read_bytes() != unsigned_payload:
            _fail("CMS signer payload does not match the final keyId binding")
        _write_json(out.parent / f"{out.name}.unsigned.json", unsigned_manifest)
        (out.parent / f"{out.name}.unsigned.payload.bin").write_bytes(unsigned_payload)
    return {
        "packageRoot": str(out),
        "manifestSha256": sha256_file(out / PACKAGE_MANIFEST_NAME),
        "packageTreeSha256": sha256_file(out / PACKAGE_TREE_NAME),
        "browserTreeSha256": sha256_file(out / BROWSER_TREE_NAME),
        "signed": sign,
    }


def _self_test() -> int:
    with tempfile.TemporaryDirectory(prefix="verisilo-camoufox-package-self-test-") as temporary:
        root = Path(temporary)
        host = root / HOST_NAME
        host.write_bytes(b"host")
        tree = root / BROWSER_TREE_NAME
        tree.write_bytes(b"tree")
        manifest = {
            "schemaVersion": 3,
            "engineId": "camoufox",
            "engineVersion": "152.0.4-beta.28",
            "channel": "experimental",
            "platform": "windows-x64",
            "artifactSha256": sha256_file(host),
            "signature": {"algorithm": "cms-detached-sha256", "keyId": "0" * 64, "value": ""},
            "capabilities": CAPABILITIES,
            "entrypoint": {"kind": "camoufox-host-v1", "relativePath": "host/camoufox-host.exe", "protocol": "verisilo-camoufox-host/v1", "sha256": sha256_file(host)},
            "treeManifest": {"relativePath": "package-tree.json", "sha256": "0" * 64},
            "browserTreeManifest": {"relativePath": BROWSER_TREE_NAME, "sha256": sha256_file(tree)},
            "hostVersion": "0.1.0",
            "browserRelease": "v152.0.4-beta.28",
            "browserAssetSha256": FORMAL_V3_ARCHIVE_SHA256,
        }
        assert manifest_signing_payload(manifest).startswith(b"VeriSilo engine package manifest v3\0")
        assert manifest_signing_payload(manifest) == manifest_signing_payload(manifest)
    print("Camoufox Host package builder self-test passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--browser-root", type=Path)
    parser.add_argument("--browser-tree-manifest", type=Path)
    parser.add_argument("--supervisor", type=Path)
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--host-executable", type=Path)
    parser.add_argument("--host-directory", type=Path, help="Existing PyInstaller one-folder output")
    parser.add_argument("--source-lock", type=Path, default=SOURCE_LOCK_DEFAULT)
    parser.add_argument("--build-result", type=Path, default=BUILD_RESULT_DEFAULT)
    parser.add_argument("--host-source", type=Path, default=HOST_DIR / "host_v1.py")
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--sign", action="store_true")
    parser.add_argument("--pfx-path", type=Path)
    parser.add_argument("--password-env", default="VERISILO_CAMOUFOX_PFX_PASSWORD")
    parser.add_argument("--check", type=Path, metavar="PACKAGE_ROOT")
    parser.add_argument(
        "--require-signed",
        action="store_true",
        help="Require a non-empty CMS signature when checking a package.",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            return _self_test()
        if args.check is not None:
            root = args.check.absolute()
            manifest = _read_formal_json(root / PACKAGE_MANIFEST_NAME)
            validate_v3_manifest(manifest, allow_unsigned=not args.require_signed)
            result = recheck_formal_package(root, manifest)
            result["signed"] = bool(manifest["signature"]["value"])
            print(json.dumps(result, sort_keys=True))
            return 0
        required = {
            "--out": args.out,
            "--browser-root": args.browser_root,
            "--browser-tree-manifest": args.browser_tree_manifest,
            "--supervisor": args.supervisor,
            "--probe": args.probe,
        }
        missing = [name for name, value in required.items() if value is None]
        if missing:
            parser.error("missing required options: " + ", ".join(missing))
        result = _stage(
            args.out.absolute(),
            browser_root=args.browser_root.absolute(),
            frozen_tree=args.browser_tree_manifest.absolute(),
            supervisor=args.supervisor.absolute(),
            probe=args.probe.absolute(),
            host_executable=args.host_executable.absolute() if args.host_executable else None,
            host_directory=args.host_directory.absolute() if args.host_directory else None,
            host_source=args.host_source.absolute(),
            source_lock=args.source_lock.absolute(),
            build_result=args.build_result.absolute(),
            python=args.python,
            sign=args.sign,
            pfx_path=args.pfx_path.absolute() if args.pfx_path else None,
            password_env=args.password_env,
        )
        print(json.dumps(result, sort_keys=True))
        return 0
    except (PackageContractError, OSError, subprocess.CalledProcessError) as exc:
        print(f"package build failed: {type(exc).__name__}: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
