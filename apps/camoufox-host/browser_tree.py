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
from pathlib import Path

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


def build_tree_manifest(tree_root: Path) -> dict:
    entries = []
    for path in sorted(tree_root.rglob("*")):
        if not path.is_file():
            continue
        rel = path.relative_to(tree_root).as_posix()
        entries.append(
            {
                "path": rel,
                "size": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        )
    return {
        "schema": TREE_MANIFEST_SCHEMA,
        "treeRootLabel": tree_root.name,
        "fileCount": len(entries),
        "totalBytes": sum(entry["size"] for entry in entries),
        "entries": entries,
    }


def load_tree_manifest(path: Path | str) -> dict:
    manifest = json.loads(Path(path).read_text(encoding="utf-8"))
    if manifest.get("schema") != TREE_MANIFEST_SCHEMA:
        raise TreeIntegrityError(f"tree manifest schema mismatch: {manifest.get('schema')!r}")
    return manifest


def verify_tree(tree_root: Path, manifest: dict, error_cap: int = 20) -> dict:
    expected = {entry["path"]: entry for entry in manifest["entries"]}
    actual_files = {
        path.relative_to(tree_root).as_posix(): path
        for path in tree_root.rglob("*")
        if path.is_file()
    }
    errors: list[str] = []
    missing = sorted(set(expected) - set(actual_files))
    extra = sorted(set(actual_files) - set(expected))
    if missing:
        errors.append("missing files: " + ", ".join(missing[:error_cap]))
    if extra:
        errors.append("extra files: " + ", ".join(extra[:error_cap]))
    mismatched = 0
    for rel, entry in expected.items():
        path = actual_files.get(rel)
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
