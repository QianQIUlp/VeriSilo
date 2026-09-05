#!/usr/bin/env python3
"""Small, dependency-free contract helpers for the Camoufox Host package.

The desktop loader already owns the signed-manifest trust decision.  This
module only describes the bytes and paths that the Host package itself owns;
it never reads a private key or a password.
"""

from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

PACKAGE_ASSET_LOCK_SCHEMA = "verisilo-camoufox-package-asset/v1"
PACKAGE_TREE_SCHEMA = "verisilo-camoufox-host-package-tree/v1"
PACKAGE_ENTRYPOINT_KIND = "camoufox-host-v1"
PACKAGE_PROTOCOL = "verisilo-camoufox-host/v1"
PACKAGE_MANIFEST_NAME = "engine-package.json"
PACKAGE_TREE_NAME = "package-tree.json"
BROWSER_TREE_NAME = "browser-tree-manifest.json"
ASSET_LOCK_NAME = "runtime-asset-lock.json"
BROWSER_DIRECTORY = "browser"
HOST_DIRECTORY = "host"
SUPERVISOR_DIRECTORY = HOST_DIRECTORY
PROBE_DIRECTORY = f"{HOST_DIRECTORY}/probe"
SUPERVISOR_NAME = "verisilo-camoufox-supervisor.exe"
PROBE_NAME = "probe.html"
HOST_NAME = "camoufox-host.exe"
FORMAL_V3_ENGINE_VERSION = "152.0.4-beta.28"
FORMAL_V3_BROWSER_RELEASE = f"v{FORMAL_V3_ENGINE_VERSION}"
FORMAL_V3_ENGINE_REVISION = "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3"
FORMAL_V3_HOST_VERSION = "0.1.0"
FORMAL_V3_CHANNEL = "experimental"
FORMAL_V3_PLATFORM = "windows-x64"
FORMAL_V3_ARCHIVE_SHA256 = (
    "8a3ef192e02cfb955bd3f9bcf71b009bd89f78e758e522b7cf373c6a0d988cbb"
)
FORMAL_V3_ARCHIVE_SIZE = 493496137
FORMAL_V3_EXECUTABLE_SHA256 = (
    "c5535c7ca64c1ed5096238d4267f4445203fd8d57b6da7760f6717dc9804b49e"
)
FORMAL_V3_SOURCE_LOCK_SHA256 = (
    "a32cf21852909be6ed4a3a4b10dec9310533908996dd73e465535e262f61bc53"
)
FORMAL_V3_BUILD_RESULT_SHA256 = (
    "a699dacdeac1df8380684bb06127d478e2b00f7c140ff98216ae5efee5ab853e"
)
FORMAL_V3_RUNTIME_ASSET_LOCK_SHA256 = (
    "81e73a69347272d0b770bfa3c9b3eb07449bb165efb0c16948eece2e5a0678ce"
)
FORMAL_V3_RUNTIME_TREE_SHA256 = (
    "d77002d0f872a1ca57675d9b3bc2f9d88769e406d2d082cd94b07d78c9f075e6"
)
FORMAL_V3_RUNTIME_TREE_CANONICAL_SHA256 = (
    "75f8f01d81ea9c1c9ba74f282cf599ed54130f8c1e6c771f9905d8c59845ab59"
)
FORMAL_V3_RUNTIME_TREE_SIZE = 81611
FORMAL_V3_SOURCE_COMMIT = "42012eeec617169a9dc6729f8634f8150e5cf3fa"
FORMAL_V3_SOURCE_TREE = "edb72f820d3b854787ac6d997114e8aa377606c9"
FORMAL_V3_BUILD_ID = "20260811045234"
FORMAL_V3_SOURCE_STAMP = "e39c605adc0fc049a165d7fe4a3f6517b761edf7"
FORMAL_V3_PROPERTIES_SHA256 = (
    "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018"
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
SEMVER_RE = re.compile(
    r"^(?:[1-9][0-9]{2})\.(?:0|[1-9][0-9]*?)\.(?:0|[1-9][0-9]*?)"
    r"(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?"
    r"(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$"
)


class PackageContractError(ValueError):
    """The package is not a strict, self-contained Camoufox package."""


def sha256_bytes(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def strict_json_loads(raw: bytes, label: str = "JSON") -> Any:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise PackageContractError(f"duplicate JSON key in {label}: {key}")
            result[key] = value
        return result

    def reject_constant(token: str) -> None:
        raise PackageContractError(f"invalid JSON number in {label}: {token}")

    try:
        return json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=reject_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PackageContractError(f"invalid JSON in {label}: {exc}") from exc


def read_json(path: Path, *, max_bytes: int = 65536) -> Any:
    raw = path.read_bytes()
    if len(raw) > max_bytes:
        raise PackageContractError(f"{path.name} exceeds {max_bytes} bytes")
    return strict_json_loads(raw, path.name)


def safe_relative_path(value: object) -> str:
    if (
        type(value) is not str
        or not value
        or len(value) > 4096
        or value.startswith("/")
        or "\\" in value
        or any(unicodedata.category(character) == "Cc" for character in value)
        or any(part in {"", ".", ".."} for part in value.split("/"))
        or ":" in value.split("/", 1)[0]
    ):
        raise PackageContractError(f"unsafe package-relative path: {value!r}")
    return value


def _require_sha(value: object, label: str) -> str:
    if type(value) is not str or SHA256_RE.fullmatch(value) is None:
        raise PackageContractError(f"{label} must be a lowercase SHA-256")
    return value


def _require_exact_keys(value: object, expected: set[str], label: str) -> dict[str, Any]:
    if type(value) is not dict or set(value) != expected:
        actual = sorted(value) if type(value) is dict else type(value).__name__
        raise PackageContractError(
            f"{label} key set is not exact: expected {sorted(expected)}, got {actual}"
        )
    return value


REQUIRED_RC1_MEMBERS = (
    ASSET_LOCK_NAME,
    BROWSER_TREE_NAME,
    PACKAGE_TREE_NAME,
    f"{HOST_DIRECTORY}/{HOST_NAME}",
    f"{HOST_DIRECTORY}/{SUPERVISOR_NAME}",
    f"{PROBE_DIRECTORY}/{PROBE_NAME}",
)
REQUIRED_RC1_DIRECTORIES = (BROWSER_DIRECTORY,)


@dataclass(frozen=True)
class PackageLayout:
    root: Path
    asset_lock: Path
    browser_root: Path
    browser_tree: Path
    package_tree: Path
    host: Path
    supervisor: Path
    probe: Path

    @classmethod
    def from_root(cls, root: Path | str) -> "PackageLayout":
        root = Path(root).absolute()
        return cls(
            root=root,
            asset_lock=root / ASSET_LOCK_NAME,
            browser_root=root / BROWSER_DIRECTORY,
            browser_tree=root / BROWSER_TREE_NAME,
            package_tree=root / PACKAGE_TREE_NAME,
            host=root / HOST_DIRECTORY / HOST_NAME,
            supervisor=root / SUPERVISOR_DIRECTORY / SUPERVISOR_NAME,
            probe=root / PROBE_DIRECTORY / PROBE_NAME,
        )


def _validate_package_asset_lock(lock: dict[str, Any]) -> None:
    expected = {
        "schema",
        "assetKind",
        "verified",
        "evidenceClass",
        "package",
        "release",
        "platform",
        "pythonPackage",
        "engineRevision",
        "sha256",
        "browserExecutableSha256",
        "sizeBytes",
        "executableRelativePath",
        "buildId",
        "sourceStamp",
        "propertiesJsonSha256",
        "sourceBinding",
        "buildResultSha256",
        "browserTreeManifestSha256",
    }
    _require_exact_keys(lock, expected, "package asset lock")
    if (
        lock["schema"] != PACKAGE_ASSET_LOCK_SCHEMA
        or lock["assetKind"] != "self-built"
        or lock["verified"] is not False
        or lock["evidenceClass"] != "compiled-not-runtime-verified"
        or lock["package"] != "camoufox"
        or lock["release"] != FORMAL_V3_BROWSER_RELEASE
        or lock["platform"] != "windows-x86_64"
        or lock["pythonPackage"] != "camoufox==0.5.4"
        or lock["engineRevision"] != FORMAL_V3_ENGINE_REVISION
        or lock["sha256"] != FORMAL_V3_ARCHIVE_SHA256
        or lock["browserExecutableSha256"] != FORMAL_V3_EXECUTABLE_SHA256
        or lock["sizeBytes"] != FORMAL_V3_ARCHIVE_SIZE
        or lock["executableRelativePath"] != "camoufox.exe"
        or lock["buildId"] != FORMAL_V3_BUILD_ID
        or lock["sourceStamp"] != FORMAL_V3_SOURCE_STAMP
        or lock["propertiesJsonSha256"] != FORMAL_V3_PROPERTIES_SHA256
        or lock["buildResultSha256"] != FORMAL_V3_BUILD_RESULT_SHA256
        or lock["browserTreeManifestSha256"] != FORMAL_V3_RUNTIME_TREE_SHA256
    ):
        raise PackageContractError("package asset lock classification is not exact")
    _require_sha(lock["sha256"], "package asset lock sha256")
    _require_sha(lock["browserExecutableSha256"], "package asset lock browserExecutableSha256")
    _require_sha(lock["propertiesJsonSha256"], "package asset lock propertiesJsonSha256")
    _require_sha(lock["buildResultSha256"], "package asset lock buildResultSha256")
    _require_sha(
        lock["browserTreeManifestSha256"],
        "package asset lock browserTreeManifestSha256",
    )
    if type(lock["sizeBytes"]) is not int or lock["sizeBytes"] <= 0:
        raise PackageContractError("package asset lock sizeBytes must be positive")
    source = _require_exact_keys(
        lock["sourceBinding"],
        {"commit", "tree", "sourceLockSha256", "completeAppliedPatchOrder"},
        "package asset sourceBinding",
    )
    if (
        source["commit"] != FORMAL_V3_SOURCE_COMMIT
        or source["tree"] != FORMAL_V3_SOURCE_TREE
        or source["sourceLockSha256"] != FORMAL_V3_SOURCE_LOCK_SHA256
        or type(source["completeAppliedPatchOrder"]) is not list
        or source["completeAppliedPatchOrder"]
        != ["0000", "0001", "0002", "0003", "0003a", "0004", "0005", "0006", "0007"]
    ):
        raise PackageContractError("package asset source binding is not Formal-v3")
    _require_sha(source["sourceLockSha256"], "package asset sourceLockSha256")


def load_package_asset_lock(path: Path | str) -> dict[str, Any]:
    path = Path(path)
    value = read_json(path)
    if type(value) is not dict:
        raise PackageContractError("package asset lock must be an object")
    _validate_package_asset_lock(value)
    return value


def verify_package_browser_root(
    lock: dict[str, Any],
    browser_root: Path | str,
    tree_manifest_path: Path | str,
    *,
    verify_tree_contents: bool = True,
) -> tuple[Path, dict[str, Any]]:
    """Verify the final staged browser root without consulting a repository."""

    _validate_package_asset_lock(lock)
    browser_root = Path(browser_root).absolute()
    tree_manifest_path = Path(tree_manifest_path).absolute()
    if not browser_root.is_dir() or browser_root.is_symlink():
        raise PackageContractError("package browser root must be a real directory")
    if not tree_manifest_path.is_file() or tree_manifest_path.is_symlink():
        raise PackageContractError("package browser tree manifest is missing")
    try:
        from browser_tree import load_tree_manifest, verify_tree
    except ImportError as exc:  # pragma: no cover - only malformed package setup
        raise PackageContractError("browser tree verifier is unavailable") from exc
    manifest_raw = tree_manifest_path.read_bytes()
    manifest = load_tree_manifest(tree_manifest_path)
    if manifest["treeRootLabel"] != browser_root.name:
        raise PackageContractError("browser tree root label does not match package root")
    executable = browser_root / lock["executableRelativePath"]
    if not executable.is_file() or executable.is_symlink():
        raise PackageContractError("package browser executable is missing or irregular")
    application_ini = browser_root / "application.ini"
    properties = browser_root / "properties.json"
    if not application_ini.is_file() or not properties.is_file():
        raise PackageContractError("package browser metadata is incomplete")
    text = application_ini.read_text(encoding="utf-8")
    build_id = re.search(r"^BuildID=(.+)$", text, re.MULTILINE)
    source_stamp = re.search(r"^SourceStamp=(.+)$", text, re.MULTILINE)
    if (
        build_id is None
        or source_stamp is None
        or build_id.group(1).strip() != lock["buildId"]
        or source_stamp.group(1).strip() != lock["sourceStamp"]
    ):
        raise PackageContractError("package browser metadata does not match asset lock")
    if verify_tree_contents:
        if sha256_bytes(manifest_raw) != lock["browserTreeManifestSha256"]:
            raise PackageContractError("browser tree manifest SHA-256 does not match asset lock")
        if sha256_file(executable) != lock["browserExecutableSha256"]:
            raise PackageContractError("package browser executable SHA-256 mismatch")
        if sha256_file(properties) != lock["propertiesJsonSha256"]:
            raise PackageContractError("package browser metadata does not match asset lock")
    verification: dict[str, Any] = {
        "verified": verify_tree_contents,
        "assetKind": "self-built",
        "treeManifestSha256": lock["browserTreeManifestSha256"],
        "fileCount": manifest["fileCount"],
        "totalBytes": manifest["totalBytes"],
    }
    if verify_tree_contents:
        verification["tree"] = verify_tree(browser_root, manifest)
        verification["treeManifestSha256"] = sha256_bytes(manifest_raw)
    return executable, verification


def _iter_package_files(root: Path) -> Iterable[tuple[str, Path]]:
    if not root.is_dir() or root.is_symlink():
        raise PackageContractError("package root must be a real directory")
    for current, directories, files in os.walk(root, topdown=True, followlinks=False):
        directories.sort()
        files.sort()
        current_path = Path(current)
        for name in directories:
            directory = current_path / name
            if directory.is_symlink() or not directory.is_dir():
                raise PackageContractError(
                    f"package member is not a regular directory: {directory}"
                )
        for name in files:
            path = current_path / name
            if path.is_symlink() or not path.is_file():
                raise PackageContractError(f"package member is not a regular file: {path}")
            relative = path.relative_to(root).as_posix()
            safe_relative_path(relative)
            yield relative, path


def build_package_tree(root: Path | str) -> dict[str, Any]:
    root = Path(root).absolute()
    entries = []
    seen: set[str] = set()
    for relative, path in _iter_package_files(root):
        if relative in {PACKAGE_MANIFEST_NAME, PACKAGE_TREE_NAME}:
            continue
        key = relative.casefold() if os.name == "nt" else relative
        if key in seen:
            raise PackageContractError(f"package tree has a case-colliding member: {relative}")
        seen.add(key)
        entries.append({"path": relative, "sha256": sha256_file(path)})
    entries.sort(key=lambda entry: entry["path"])
    if not entries:
        raise PackageContractError("package tree cannot be empty")
    return {"schema": PACKAGE_TREE_SCHEMA, "entries": entries}


def compact_json_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")


def manifest_signing_payload(manifest: dict[str, Any]) -> bytes:
    """Return the exact bytes consumed by the Windows SignedCms signer."""

    unsigned = dict(manifest)
    signature = dict(unsigned.get("signature") or {})
    signature["value"] = ""
    unsigned["signature"] = signature
    return b"VeriSilo engine package manifest v3\0" + compact_json_bytes(unsigned)


def _check_capabilities(value: object) -> list[str]:
    if type(value) is not list or any(type(item) is not str for item in value):
        raise PackageContractError("capabilities must be a duplicate-free list")
    if len(value) != len(set(value)):
        raise PackageContractError("capabilities must be a duplicate-free list")
    allowed = {
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
    }
    if any(type(item) is not str or item not in allowed for item in value):
        raise PackageContractError("package capabilities contain an unsupported value")
    if "identity_template" not in value or "site_fallback" in value:
        raise PackageContractError("Camoufox Host capabilities are not exact")
    return value


def validate_v3_manifest(manifest: dict[str, Any], *, allow_unsigned: bool = False) -> None:
    expected = {
        "schemaVersion",
        "engineId",
        "engineVersion",
        "channel",
        "platform",
        "artifactSha256",
        "signature",
        "capabilities",
        "entrypoint",
        "treeManifest",
        "browserTreeManifest",
        "hostVersion",
        "browserRelease",
        "browserAssetSha256",
    }
    _require_exact_keys(manifest, expected, "Camoufox Host manifest")
    if (
        manifest["schemaVersion"] != 3
        or manifest["engineId"] != "camoufox"
        or manifest["engineVersion"] != FORMAL_V3_ENGINE_VERSION
        or manifest["channel"] != FORMAL_V3_CHANNEL
        or manifest["platform"] != FORMAL_V3_PLATFORM
        or manifest["browserRelease"] != FORMAL_V3_BROWSER_RELEASE
        or manifest["hostVersion"] != FORMAL_V3_HOST_VERSION
    ):
        raise PackageContractError("Camoufox Host manifest identity is not Formal-v3")
    if SEMVER_RE.fullmatch(manifest["engineVersion"]) is None:
        raise PackageContractError("manifest engineVersion is not semantic")
    for field in ("artifactSha256", "browserAssetSha256"):
        _require_sha(manifest[field], f"manifest {field}")
    signature = _require_exact_keys(
        manifest["signature"], {"algorithm", "keyId", "value"}, "manifest signature"
    )
    if signature["algorithm"] != "cms-detached-sha256":
        raise PackageContractError("manifest signature algorithm is not CMS SHA-256")
    _require_sha(signature["keyId"], "manifest signature keyId")
    if type(signature["value"]) is not str:
        raise PackageContractError("manifest signature value must be base64 text")
    if signature["value"]:
        if signature["keyId"] == "0" * 64:
            raise PackageContractError("signed manifest must identify its CMS certificate")
        try:
            decoded = base64.b64decode(signature["value"], validate=True)
        except (ValueError, base64.binascii.Error) as exc:
            raise PackageContractError("manifest signature value is not base64") from exc
        if base64.b64encode(decoded).decode("ascii") != signature["value"]:
            raise PackageContractError("manifest signature value is not canonical base64")
        if (
            len(decoded) == 0
            or len(decoded) > 48 * 1024
            or len(signature["value"]) < 256
            or len(signature["value"]) > 60000
            or len(signature["value"]) % 4 != 0
        ):
            raise PackageContractError("manifest signature value has an invalid size")
    elif not allow_unsigned:
        raise PackageContractError("manifest is unsigned")
    _check_capabilities(manifest["capabilities"])
    entrypoint = _require_exact_keys(
        manifest["entrypoint"], {"kind", "relativePath", "protocol", "sha256"}, "entrypoint"
    )
    tree = _require_exact_keys(
        manifest["treeManifest"], {"relativePath", "sha256"}, "treeManifest"
    )
    browser_tree = _require_exact_keys(
        manifest["browserTreeManifest"],
        {"relativePath", "sha256"},
        "browserTreeManifest",
    )
    if (
        entrypoint["kind"] != PACKAGE_ENTRYPOINT_KIND
        or entrypoint["relativePath"] != f"{HOST_DIRECTORY}/{HOST_NAME}"
        or entrypoint["protocol"] != PACKAGE_PROTOCOL
        or tree["relativePath"] != PACKAGE_TREE_NAME
        or browser_tree["relativePath"] != BROWSER_TREE_NAME
        or manifest["artifactSha256"] != entrypoint["sha256"]
        or manifest["browserAssetSha256"] != FORMAL_V3_ARCHIVE_SHA256
    ):
        raise PackageContractError("manifest path or binding is not exact")
    for value, label in (
        (entrypoint["sha256"], "entrypoint sha256"),
        (tree["sha256"], "treeManifest sha256"),
        (browser_tree["sha256"], "browserTreeManifest sha256"),
    ):
        _require_sha(value, label)


def recheck_package(root: Path | str, manifest: dict[str, Any]) -> dict[str, Any]:
    """Re-read all final package bytes and compare the signed tree bindings."""

    root = Path(root).absolute()
    validate_v3_manifest(manifest, allow_unsigned=True)
    for relative in REQUIRED_RC1_DIRECTORIES:
        directory = root / relative
        if directory.is_symlink() or not directory.is_dir():
            raise PackageContractError(f"required RC1 package directory is missing: {relative}")
    for relative in REQUIRED_RC1_MEMBERS:
        member = root / relative
        if member.is_symlink() or not member.is_file():
            raise PackageContractError(f"required RC1 package member is missing: {relative}")
    tree_path = root / manifest["treeManifest"]["relativePath"]
    browser_tree_path = root / manifest["browserTreeManifest"]["relativePath"]
    tree_raw = tree_path.read_bytes()
    browser_tree_raw = browser_tree_path.read_bytes()
    if sha256_bytes(tree_raw) != manifest["treeManifest"]["sha256"]:
        raise PackageContractError("package tree digest changed")
    if sha256_bytes(browser_tree_raw) != manifest["browserTreeManifest"]["sha256"]:
        raise PackageContractError("browser tree digest changed")
    declared_tree = strict_json_loads(tree_raw, "package tree")
    if type(declared_tree) is not dict or set(declared_tree) != {"schema", "entries"}:
        raise PackageContractError("package tree schema is not exact")
    if declared_tree["schema"] != PACKAGE_TREE_SCHEMA or type(declared_tree["entries"]) is not list:
        raise PackageContractError("package tree schema is not exact")
    declared_entries: dict[str, str] = {}
    for entry in declared_tree["entries"]:
        if type(entry) is not dict or set(entry) != {"path", "sha256"}:
            raise PackageContractError("package tree entry schema is not exact")
        relative = safe_relative_path(entry["path"])
        digest = _require_sha(entry["sha256"], "package tree entry sha256")
        key = relative.casefold() if os.name == "nt" else relative
        if key in declared_entries:
            raise PackageContractError("package tree has a duplicate member")
        declared_entries[key] = digest
    actual = build_package_tree(root)
    expected = {entry["path"]: entry["sha256"] for entry in actual["entries"]}
    normalized_expected = {
        (path.casefold() if os.name == "nt" else path): digest
        for path, digest in expected.items()
    }
    if actual["schema"] != PACKAGE_TREE_SCHEMA or normalized_expected != declared_entries:
        raise PackageContractError("package tree does not match final bytes")
    host_path = root / manifest["entrypoint"]["relativePath"]
    if sha256_file(host_path) != manifest["entrypoint"]["sha256"]:
        raise PackageContractError("Host entrypoint digest changed")
    return {
        "packageTreeSha256": sha256_bytes(tree_raw),
        "browserTreeSha256": sha256_bytes(browser_tree_raw),
        "memberCount": len(expected),
    }


def recheck_formal_package(root: Path | str, manifest: dict[str, Any]) -> dict[str, Any]:
    """Recheck the structural package plus its exact frozen Formal-v3 browser binding."""

    root = Path(root).absolute()
    result = recheck_package(root, manifest)
    asset_lock = load_package_asset_lock(root / ASSET_LOCK_NAME)
    browser_tree_path = root / manifest["browserTreeManifest"]["relativePath"]
    browser_tree_sha256 = sha256_file(browser_tree_path)
    if asset_lock["browserTreeManifestSha256"] != browser_tree_sha256:
        raise PackageContractError("package asset lock browser tree binding changed")
    _, browser_verification = verify_package_browser_root(
        asset_lock,
        root / BROWSER_DIRECTORY,
        browser_tree_path,
        verify_tree_contents=True,
    )
    result.update(
        engineRevision=asset_lock["engineRevision"],
        browserFileCount=browser_verification["fileCount"],
    )
    return result
