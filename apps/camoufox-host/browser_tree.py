#!/usr/bin/env python3
"""Browser extraction-tree manifest: build once, verify before every launch.

The extracted browser tree is not re-extracted per launch; instead a tracked
manifest (relative path -> size + SHA-256) is verified against the live tree.
Any missing/extra/modified file fails pre-launch validation.

Usage:
    python browser_tree.py manifest --root <extract-dir> --out <manifest.json>
    python browser_tree.py verify --root <extract-dir> --manifest <manifest.json>
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
from pathlib import Path

from host_platform import IS_WINDOWS, ensure_no_reparse_points

TREE_MANIFEST_SCHEMA = "verisilo-camoufox-browser-tree-manifest/v1"


class TreeIntegrityError(Exception):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        while True:
            chunk = fh.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def _walk_entries(tree_root: Path) -> list[tuple[str, str, Path]]:
    """Yield (relative_posix, kind, path) for file/symlink/other entries.
    Directory symlinks are reported as symlinks and never followed, so a
    symlinked path component can never smuggle a file past verification."""
    entries: list[tuple[str, str, Path]] = []
    try:
        ensure_no_reparse_points(tree_root)
    except OSError as exc:
        raise TreeIntegrityError(f"tree root rejected: {exc}") from exc
    stack = [tree_root]
    while stack:
        current = stack.pop()
        try:
            children = sorted(os.scandir(current), key=lambda entry: entry.name)
        except OSError:
            continue
        for entry in children:
            path = Path(entry.path)
            rel = path.relative_to(tree_root).as_posix()
            try:
                ensure_no_reparse_points(path)
                entry_stat = entry.stat(follow_symlinks=False)
            except OSError:
                entries.append((rel, "reparse-point", path))
                continue
            if entry.is_symlink() or (
                IS_WINDOWS
                and getattr(entry_stat, "st_file_attributes", 0)
                & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x0400)
            ):
                entries.append((rel, "symlink", path))
            elif entry.is_file(follow_symlinks=False):
                entries.append((rel, "file", path))
            elif entry.is_dir(follow_symlinks=False):
                stack.append(path)
            else:
                entries.append((rel, "other", path))
    return entries


def build_tree_manifest(tree_root: Path) -> dict:
    entries: list[dict] = []
    irregular: list[str] = []
    for rel, kind, path in _walk_entries(tree_root):
        if kind != "file":
            irregular.append(f"{rel} ({kind})")
            continue
        rel = path.relative_to(tree_root).as_posix()
        entries.append(
            {
                "path": rel,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    if irregular:
        raise TreeIntegrityError(
            "tree contains symlink/non-regular entries: " + ", ".join(irregular)
        )
    return {
        "schema": TREE_MANIFEST_SCHEMA,
        "treeRootLabel": tree_root.name,
        "fileCount": len(entries),
        "totalBytes": sum(entry["size"] for entry in entries),
        "entries": entries,
    }


def load_tree_manifest(path: Path | str) -> dict:
    raw = Path(path).read_bytes()

    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict:
        result: dict = {}
        for key, value in pairs:
            if key in result:
                raise TreeIntegrityError(f"duplicate manifest key: {key}")
            result[key] = value
        return result

    try:
        manifest = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=lambda token: (_ for _ in ()).throw(
                TreeIntegrityError(f"invalid manifest number: {token}")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise TreeIntegrityError(f"tree manifest is not strict JSON: {exc}") from exc
    if type(manifest) is not dict:
        raise TreeIntegrityError("tree manifest must be an object")
    if set(manifest) != {"schema", "treeRootLabel", "fileCount", "totalBytes", "entries"}:
        raise TreeIntegrityError("tree manifest has an unexpected key set")
    if manifest.get("schema") != TREE_MANIFEST_SCHEMA:
        raise TreeIntegrityError(f"tree manifest schema mismatch: {manifest.get('schema')!r}")
    if type(manifest.get("treeRootLabel")) is not str:
        raise TreeIntegrityError("treeRootLabel must be a string")
    if type(manifest.get("fileCount")) is not int or type(manifest.get("totalBytes")) is not int:
        raise TreeIntegrityError("fileCount and totalBytes must be integers")
    entries = manifest.get("entries")
    if type(entries) is not list:
        raise TreeIntegrityError("entries must be a list")
    seen: set[str] = set()
    total = 0
    for index, entry in enumerate(entries):
        if type(entry) is not dict or set(entry) != {"path", "size", "sha256"}:
            raise TreeIntegrityError(f"invalid tree entry at index {index}")
        rel = entry["path"]
        if type(rel) is not str or not rel or rel.startswith(("/", "\\")):
            raise TreeIntegrityError(f"invalid tree entry path at index {index}")
        rel = rel.replace("\\", "/")
        parts = rel.split("/")
        if any(part in ("", ".", "..") for part in parts) or ":" in parts[0]:
            raise TreeIntegrityError(f"tree entry escapes root: {entry['path']!r}")
        key = rel.casefold() if IS_WINDOWS else rel
        if key in seen:
            raise TreeIntegrityError(f"duplicate tree entry path: {rel}")
        seen.add(key)
        if type(entry["size"]) is not int or entry["size"] < 0:
            raise TreeIntegrityError(f"invalid tree entry size: {rel}")
        if type(entry["sha256"]) is not str or len(entry["sha256"]) != 64:
            raise TreeIntegrityError(f"invalid tree entry digest: {rel}")
        total += entry["size"]
    if manifest["fileCount"] != len(entries) or manifest["totalBytes"] != total:
        raise TreeIntegrityError("tree manifest summary does not match entries")
    return manifest


def verify_tree(tree_root: Path, manifest: dict, error_cap: int = 20) -> dict:
    expected = {
        (entry["path"].replace("\\", "/").casefold() if IS_WINDOWS else entry["path"].replace("\\", "/")): entry
        for entry in manifest["entries"]
    }
    actual_files: dict[str, Path] = {}
    irregular: list[str] = []
    for rel, kind, path in _walk_entries(tree_root):
        if kind == "file":
            normalized = rel.casefold() if IS_WINDOWS else rel
            if normalized in actual_files:
                irregular.append(f"case-colliding files: {rel}")
            actual_files[normalized] = path
        else:
            irregular.append(f"{rel} ({kind})")
    errors: list[str] = []
    if irregular:
        errors.append(
            "symlink/non-regular entries are rejected: "
            + ", ".join(irregular[:error_cap])
        )
    missing = sorted(set(expected) - set(actual_files))
    extra = sorted(set(actual_files) - set(expected))
    if missing:
        errors.append("missing files: " + ", ".join(missing[:error_cap]))
    if extra:
        errors.append("extra files: " + ", ".join(extra[:error_cap]))
    mismatched = 0
    for rel, entry in expected.items():
        path = actual_files.get(rel.casefold() if IS_WINDOWS else rel)
        if path is None:
            continue
        size = path.stat().st_size
        if size != entry["size"]:
            errors.append(f"size mismatch: {rel} (expected {entry['size']}, got {size})")
            mismatched += 1
            if mismatched >= error_cap:
                break
            continue
        digest = sha256_file(path)
        if digest != entry["sha256"]:
            errors.append(f"sha256 mismatch: {rel}")
            mismatched += 1
            if mismatched >= error_cap:
                break
    if errors:
        raise TreeIntegrityError("; ".join(errors))
    return {
        "verified": True,
        "fileCount": len(expected),
        "totalBytes": sum(entry["size"] for entry in expected.values()),
        "manifestSha256": hashlib.sha256(
            json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p_manifest = sub.add_parser("manifest")
    p_manifest.add_argument("--root", required=True, type=Path)
    p_manifest.add_argument("--out", required=True, type=Path)
    p_verify = sub.add_parser("verify")
    p_verify.add_argument("--root", required=True, type=Path)
    p_verify.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()

    if args.command == "manifest":
        manifest = build_tree_manifest(args.root)
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(json.dumps(manifest, indent=1) + "\n")
        print(
            f"tree manifest written: {manifest['fileCount']} files, "
            f"{manifest['totalBytes']} bytes"
        )
        return 0
    manifest = load_tree_manifest(args.manifest)
    result = verify_tree(args.root, manifest)
    print(
        f"tree verified: {result['fileCount']} files, {result['totalBytes']} bytes"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
