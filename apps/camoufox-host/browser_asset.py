#!/usr/bin/env python3
"""Strict Camoufox browser-asset lock validation.

Official release locks and VeriSilo self-built locks are deliberately distinct
formats.  In particular, a self-built lock can never satisfy the GitHub
``digestAgreement`` contract used by an official release asset.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path, PurePosixPath
from typing import Any

from browser_tree import load_tree_manifest, sha256_file, verify_tree


OFFICIAL_ASSET_SCHEMA = "verisilo-camoufox-browser-asset/v2"
SELF_BUILT_ASSET_SCHEMA = "verisilo-camoufox-browser-asset/v3"
SELF_BUILT_ASSET_KIND = "self-built"
SELF_BUILT_ENGINE_REVISION = (
    "verisilo-camoufox-152.0.4-beta.28-canvas-export-v1-close-bound-v1"
)
SELF_BUILT_TREE_MANIFEST_PATH = (
    "tests/fixtures/camoufox/"
    "browser-tree-manifest-verisilo-canvas-v1-windows.json"
)
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
SHA512_RE = re.compile(r"^[0-9a-f]{128}$")
GIT_OBJECT_RE = re.compile(r"^[0-9a-f]{40}$")


class BrowserAssetError(RuntimeError):
    """The selected browser asset is not exactly bound by its lock."""


def _strict_json_loads(raw: bytes, label: str) -> dict:
    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict:
        result: dict = {}
        for key, value in pairs:
            if key in result:
                raise BrowserAssetError(f"duplicate JSON key in {label}: {key}")
            result[key] = value
        return result

    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                BrowserAssetError(f"invalid JSON number in {label}: {token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise BrowserAssetError(f"invalid JSON in {label}: {exc}") from exc
    if type(value) is not dict:
        raise BrowserAssetError(f"{label} must contain one JSON object")
    return value


def load_asset_lock(
    path: Path | str,
    *,
    expected_release: str,
    expected_platform: str,
) -> dict:
    path = Path(path)
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise BrowserAssetError(f"asset lock is unreadable: {path}: {exc}") from exc
    lock = _strict_json_loads(raw, path.name)
    validate_asset_lock(
        lock,
        expected_release=expected_release,
        expected_platform=expected_platform,
    )
    return lock


def asset_kind(lock: dict) -> str:
    schema = lock.get("schema")
    if schema == OFFICIAL_ASSET_SCHEMA:
        return "official"
    if schema == SELF_BUILT_ASSET_SCHEMA:
        return SELF_BUILT_ASSET_KIND
    raise BrowserAssetError(f"unsupported browser asset lock schema: {schema!r}")


def _require_exact_keys(value: object, expected: set[str], label: str) -> dict:
    if type(value) is not dict or set(value) != expected:
        actual = sorted(value) if type(value) is dict else type(value).__name__
        raise BrowserAssetError(
            f"{label} key set is not exact: expected {sorted(expected)}, got {actual}"
        )
    return value


def _require_sha(value: object, label: str, *, bits: int = 256) -> str:
    pattern = SHA256_RE if bits == 256 else SHA512_RE
    if type(value) is not str or not pattern.fullmatch(value):
        raise BrowserAssetError(f"{label} is not a lowercase SHA-{bits}")
    return value


def _require_positive_int(value: object, label: str) -> int:
    if type(value) is not int or value <= 0:
        raise BrowserAssetError(f"{label} must be a positive integer")
    return value


def _validate_common(lock: dict, expected_release: str, expected_platform: str) -> None:
    if lock.get("package") != "camoufox":
        raise BrowserAssetError("asset lock package must be camoufox")
    if lock.get("release") != expected_release:
        raise BrowserAssetError("asset lock release does not match the selected release")
    if lock.get("platform") != expected_platform:
        raise BrowserAssetError("asset lock platform does not match this host")
    if lock.get("pythonPackage") != "camoufox==0.5.4":
        raise BrowserAssetError("asset lock Python package pin is not exact")
    _require_sha(lock.get("sha256"), "asset lock sha256")
    _require_positive_int(lock.get("sizeBytes"), "asset lock sizeBytes")


def _validate_official_lock(lock: dict) -> None:
    _require_exact_keys(
        lock,
        {
            "schema",
            "package",
            "release",
            "platform",
            "pythonPackage",
            "url",
            "sha256",
            "sizeBytes",
            "githubAsset",
            "local",
            "digestAgreement",
            "digestAgreementBasis",
            "recordedBy",
            "recordedAtUtc",
        },
        "official asset lock",
    )
    github = _require_exact_keys(
        lock["githubAsset"],
        {
            "assetId",
            "name",
            "url",
            "sizeBytes",
            "officialDigest",
            "officialDigestAlgorithm",
            "officialDigestHex",
            "metadataSource",
        },
        "official asset lock githubAsset",
    )
    local = _require_exact_keys(
        lock["local"],
        {"sha256", "sizeBytes", "computedBy", "computedAtUtc"},
        "official asset lock local",
    )
    digest = lock["sha256"]
    size = lock["sizeBytes"]
    if (
        lock["digestAgreement"] is not True
        or github["officialDigestAlgorithm"] != "sha256"
        or github["officialDigest"] != f"sha256:{digest}"
        or github["officialDigestHex"] != digest
        or github["sizeBytes"] != size
        or github["url"] != lock["url"]
        or local["sha256"] != digest
        or local["sizeBytes"] != size
    ):
        raise BrowserAssetError("official asset digestAgreement evidence is inconsistent")
    if type(github["assetId"]) is not int or github["assetId"] <= 0:
        raise BrowserAssetError("official assetId must be a positive integer")


def _validate_digest_map(value: object, expected_names: set[str], label: str) -> None:
    mapping = _require_exact_keys(value, expected_names, label)
    for name, digest in mapping.items():
        _require_sha(digest, f"{label}.{name}")


def _validate_self_built_lock(lock: dict) -> None:
    _require_exact_keys(
        lock,
        {
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
            "sizeBytes",
            "executableRelativePath",
            "sourceBinding",
            "build",
            "archive",
            "treeManifest",
        },
        "self-built asset lock",
    )
    if (
        lock["assetKind"] != SELF_BUILT_ASSET_KIND
        or lock["verified"] is not False
        or lock["evidenceClass"] != "compiled-not-runtime-verified"
        or lock["engineRevision"] != SELF_BUILT_ENGINE_REVISION
        or lock["platform"] != "windows-x86_64"
        or lock["executableRelativePath"] != "camoufox.exe"
    ):
        raise BrowserAssetError("self-built asset classification is not exact")
    if any(key in lock for key in ("githubAsset", "digestAgreement", "url", "local")):
        raise BrowserAssetError("self-built lock must not contain official GitHub evidence")

    source = _require_exact_keys(
        lock["sourceBinding"],
        {"commit", "tree", "sourceLock", "upstream", "firefoxSource", "downstreamPatches"},
        "self-built sourceBinding",
    )
    if not GIT_OBJECT_RE.fullmatch(source["commit"]) or not GIT_OBJECT_RE.fullmatch(
        source["tree"]
    ):
        raise BrowserAssetError("self-built source commit/tree is malformed")
    source_lock = _require_exact_keys(
        source["sourceLock"], {"path", "sha256", "sizeBytes"}, "source lock binding"
    )
    if source_lock["path"] != (
        "apps/camoufox-host/lock/"
        "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-source.json"
    ):
        raise BrowserAssetError("self-built source lock path is not exact")
    _require_sha(source_lock["sha256"], "source lock sha256")
    _require_positive_int(source_lock["sizeBytes"], "source lock sizeBytes")
    upstream = _require_exact_keys(
        source["upstream"], {"repository", "tag", "commit", "tree"}, "upstream binding"
    )
    if not GIT_OBJECT_RE.fullmatch(upstream["commit"]) or not GIT_OBJECT_RE.fullmatch(
        upstream["tree"]
    ):
        raise BrowserAssetError("upstream commit/tree is malformed")
    firefox = _require_exact_keys(
        source["firefoxSource"],
        {"version", "sizeBytes", "sha512"},
        "Firefox source binding",
    )
    _require_positive_int(firefox["sizeBytes"], "Firefox source sizeBytes")
    _require_sha(firefox["sha512"], "Firefox source sha512", bits=512)
    patches = source["downstreamPatches"]
    if type(patches) is not list or not patches:
        raise BrowserAssetError("downstream patch binding must be a non-empty list")
    for index, patch in enumerate(patches):
        row = _require_exact_keys(
            patch,
            {"path", "applyAfterUpstream", "sha256", "sizeBytes"},
            f"downstream patch {index}",
        )
        if row["applyAfterUpstream"] is not True:
            raise BrowserAssetError("downstream patch order flag is not true")
        _require_sha(row["sha256"], f"downstream patch {index} sha256")
        _require_positive_int(row["sizeBytes"], f"downstream patch {index} sizeBytes")

    build = _require_exact_keys(
        lock["build"],
        {
            "runId",
            "target",
            "mozBuildDate",
            "startedAtUtc",
            "completedAtUtc",
            "buildResult",
            "hostProvenance",
            "builderImage",
            "toolchain",
        },
        "self-built build binding",
    )
    if (
        build["target"] != "x86_64-pc-windows-msvc"
        or type(build["runId"]) is not str
        or not re.fullmatch(r"canvas-close-engine-[a-z0-9-]{8,63}", build["runId"])
        or type(build["mozBuildDate"]) is not str
        or not re.fullmatch(r"[0-9]{14}", build["mozBuildDate"])
    ):
        raise BrowserAssetError("self-built target/runId is not exact")
    for name in ("buildResult", "hostProvenance"):
        record = _require_exact_keys(
            build[name],
            {"name", "recordType", "sha256", "sizeBytes", "status"},
            name,
        )
        _require_sha(record["sha256"], f"{name} sha256")
        _require_positive_int(record["sizeBytes"], f"{name} sizeBytes")
    if build["buildResult"] != {
        **build["buildResult"],
        "name": "build-result.json",
        "recordType": "verisilo-camoufox-build-run/v1",
        "status": "compiled-not-runtime-verified",
    }:
        raise BrowserAssetError("build-result binding classification is not exact")
    if build["hostProvenance"] != {
        **build["hostProvenance"],
        "name": "host-provenance.json",
        "recordType": "verisilo-camoufox-build-host-provenance/v1",
        "status": "container-passed",
    }:
        raise BrowserAssetError("host-provenance binding classification is not exact")
    builder = _require_exact_keys(
        build["builderImage"],
        {
            "imageId",
            "savedArchiveSha256",
            "savedArchiveSizeBytes",
            "baseIndexDigest",
            "baseLinuxAmd64ManifestDigest",
            "recipeSourceCommit",
            "recipeSourceTree",
            "recipeSourceLockSha256",
            "dockerfileSha256",
            "hostToolingSha256",
        },
        "builder image binding",
    )
    if type(builder["imageId"]) is not str or not builder["imageId"].startswith("sha256:"):
        raise BrowserAssetError("builder image ID is malformed")
    for key in (
        "savedArchiveSha256",
        "recipeSourceLockSha256",
        "dockerfileSha256",
        "hostToolingSha256",
    ):
        _require_sha(builder[key], f"builderImage.{key}")
    _require_positive_int(builder["savedArchiveSizeBytes"], "builder image archive size")
    if not GIT_OBJECT_RE.fullmatch(builder["recipeSourceCommit"]) or not GIT_OBJECT_RE.fullmatch(
        builder["recipeSourceTree"]
    ):
        raise BrowserAssetError("builder recipe commit/tree is malformed")

    toolchain = _require_exact_keys(
        build["toolchain"],
        {"compilerVersion", "windowsSdkVersion", "selectionManifest", "packagedCrt"},
        "Windows toolchain binding",
    )
    selection = _require_exact_keys(
        toolchain["selectionManifest"], {"path", "sha256", "size"}, "toolchain manifest"
    )
    _require_sha(selection["sha256"], "toolchain selection manifest sha256")
    _require_positive_int(selection["size"], "toolchain selection manifest size")
    crt = _require_exact_keys(
        toolchain["packagedCrt"],
        {
            "redistVersion",
            "architecture",
            "family",
            "fileCount",
            "totalBytes",
            "canonicalTreeSha256",
        },
        "packaged CRT binding",
    )
    _require_sha(crt["canonicalTreeSha256"], "packaged CRT canonical tree sha256")
    _require_positive_int(crt["fileCount"], "packaged CRT fileCount")
    _require_positive_int(crt["totalBytes"], "packaged CRT totalBytes")

    archive = _require_exact_keys(
        lock["archive"],
        {
            "name",
            "sha256",
            "sizeBytes",
            "memberCount",
            "fileMemberCount",
            "totalUncompressedFileBytes",
            "buildId",
            "sourceStamp",
            "requiredMemberSha256",
            "packagedCrtMemberSha256",
            "crcVerified",
            "safePathsVerified",
            "caseCollisionFree",
            "linksRejected",
        },
        "self-built archive binding",
    )
    _require_sha(archive["sha256"], "archive sha256")
    for name in ("sizeBytes", "memberCount", "fileMemberCount", "totalUncompressedFileBytes"):
        _require_positive_int(archive[name], f"archive {name}")
    if archive["sha256"] != lock["sha256"] or archive["sizeBytes"] != lock["sizeBytes"]:
        raise BrowserAssetError("top-level archive binding is inconsistent")
    if (
        archive["buildId"] != build["mozBuildDate"]
        or type(archive["sourceStamp"]) is not str
        or not archive["sourceStamp"]
    ):
        raise BrowserAssetError("archive BuildID/SourceStamp binding is inconsistent")
    if any(
        archive[name] is not True
        for name in ("crcVerified", "safePathsVerified", "caseCollisionFree", "linksRejected")
    ):
        raise BrowserAssetError("archive verification flags are not all true")
    _validate_digest_map(
        archive["requiredMemberSha256"],
        {"application.ini", "camoufox.cfg", "camoufox.exe", "platform.ini", "properties.json"},
        "required member hashes",
    )
    _validate_digest_map(
        archive["packagedCrtMemberSha256"],
        {
            "concrt140.dll",
            "msvcp140.dll",
            "msvcp140_1.dll",
            "msvcp140_2.dll",
            "msvcp140_atomic_wait.dll",
            "msvcp140_codecvt_ids.dll",
            "vccorlib140.dll",
            "vcruntime140.dll",
            "vcruntime140_1.dll",
            "vcruntime140_threads.dll",
        },
        "packaged CRT member hashes",
    )

    tree = _require_exact_keys(
        lock["treeManifest"],
        {
            "path",
            "schema",
            "encoding",
            "newline",
            "rawSha256",
            "canonicalSha256",
            "canonicalization",
            "fileCount",
            "totalBytes",
        },
        "tree manifest binding",
    )
    if (
        tree["path"] != SELF_BUILT_TREE_MANIFEST_PATH
        or tree["schema"] != "verisilo-camoufox-browser-tree-manifest/v1"
        or tree["encoding"] != "utf-8"
        or tree["newline"] != "lf"
    ):
        raise BrowserAssetError("self-built tree manifest format/path is not exact")
    _require_sha(tree["rawSha256"], "tree manifest raw sha256")
    _require_sha(tree["canonicalSha256"], "tree manifest canonical sha256")
    _require_positive_int(tree["fileCount"], "tree manifest fileCount")
    _require_positive_int(tree["totalBytes"], "tree manifest totalBytes")


def validate_asset_lock(
    lock: dict,
    *,
    expected_release: str,
    expected_platform: str,
) -> str:
    kind = asset_kind(lock)
    _validate_common(lock, expected_release, expected_platform)
    if kind == "official":
        _validate_official_lock(lock)
    else:
        _validate_self_built_lock(lock)
    return kind


def canonical_json_sha256(value: object) -> str:
    payload = json.dumps(
        value, sort_keys=True, separators=(",", ":"), ensure_ascii=False
    ).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def _safe_repo_relative_path(repo_root: Path, value: str) -> Path:
    relative = PurePosixPath(value)
    if (
        not value
        or value.startswith(("/", "\\"))
        or "\\" in value
        or any(part in {"", ".", ".."} for part in relative.parts)
        or ":" in relative.parts[0]
        or relative.as_posix() != value
    ):
        raise BrowserAssetError("tree manifest path is not a safe repository-relative path")
    root = repo_root.resolve(strict=True)
    path = (root / Path(*relative.parts)).resolve(strict=True)
    if root not in path.parents:
        raise BrowserAssetError("tree manifest path escapes the repository")
    return path


def verify_self_built_browser_root(
    lock: dict,
    browser_root: Path | str,
    *,
    repo_root: Path | str,
    tree_manifest_path: Path | str | None = None,
    verify_tree_contents: bool = True,
) -> tuple[Path, dict]:
    if asset_kind(lock) != SELF_BUILT_ASSET_KIND:
        raise BrowserAssetError("explicit self-built browser root requires a self-built lock")
    browser_root = Path(browser_root)
    if not browser_root.is_dir() or browser_root.is_symlink():
        raise BrowserAssetError("self-built browser root must be a real directory")
    browser_root = browser_root.resolve(strict=True)
    repo_root = Path(repo_root)
    locked_manifest_path = _safe_repo_relative_path(
        repo_root, lock["treeManifest"]["path"]
    )
    if tree_manifest_path is not None:
        supplied = Path(tree_manifest_path).resolve(strict=True)
        if supplied != locked_manifest_path:
            raise BrowserAssetError("supplied tree manifest differs from the asset lock")
    raw = locked_manifest_path.read_bytes()
    if raw.startswith(b"\xef\xbb\xbf") or b"\r" in raw or not raw.endswith(b"\n"):
        raise BrowserAssetError("self-built tree manifest is not UTF-8 LF without BOM")
    raw_sha = hashlib.sha256(raw).hexdigest()
    if raw_sha != lock["treeManifest"]["rawSha256"]:
        raise BrowserAssetError("self-built tree manifest raw SHA-256 mismatch")
    manifest = load_tree_manifest(locked_manifest_path)
    if canonical_json_sha256(manifest) != lock["treeManifest"]["canonicalSha256"]:
        raise BrowserAssetError("self-built tree manifest canonical SHA-256 mismatch")
    if (
        manifest["fileCount"] != lock["treeManifest"]["fileCount"]
        or manifest["totalBytes"] != lock["treeManifest"]["totalBytes"]
    ):
        raise BrowserAssetError("self-built tree manifest summary differs from the lock")
    entries = {entry["path"]: entry for entry in manifest["entries"]}
    bound_members = {
        **lock["archive"]["requiredMemberSha256"],
        **lock["archive"]["packagedCrtMemberSha256"],
    }
    for name, digest in bound_members.items():
        entry = entries.get(name)
        if entry is None or entry["sha256"] != digest:
            raise BrowserAssetError(f"tree manifest member binding mismatch: {name}")
    executable = browser_root / lock["executableRelativePath"]
    if not executable.is_file() or executable.is_symlink():
        raise BrowserAssetError("self-built browser executable is missing or irregular")
    if sha256_file(executable) != bound_members[lock["executableRelativePath"]]:
        raise BrowserAssetError("self-built browser executable SHA-256 mismatch")
    try:
        application_ini = (browser_root / "application.ini").read_text(encoding="utf-8")
    except OSError as exc:
        raise BrowserAssetError(f"self-built application.ini is unreadable: {exc}") from exc
    build_id = re.search(r"^BuildID=(.+)$", application_ini, re.MULTILINE)
    source_stamp = re.search(r"^SourceStamp=(.+)$", application_ini, re.MULTILINE)
    if (
        build_id is None
        or source_stamp is None
        or build_id.group(1).strip() != lock["archive"]["buildId"]
        or source_stamp.group(1).strip() != lock["archive"]["sourceStamp"]
    ):
        raise BrowserAssetError("self-built browser BuildID/SourceStamp mismatch")
    verification = {
        "verified": False,
        "assetKind": SELF_BUILT_ASSET_KIND,
        "treeManifestRawSha256": raw_sha,
        "treeManifestCanonicalSha256": canonical_json_sha256(manifest),
        "fileCount": manifest["fileCount"],
        "totalBytes": manifest["totalBytes"],
    }
    if verify_tree_contents:
        verification["tree"] = verify_tree(browser_root, manifest)
    return executable, verification
