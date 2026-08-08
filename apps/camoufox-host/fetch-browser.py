#!/usr/bin/env python3
"""VeriSilo M0 spike: download the pinned Camoufox browser archive and record
or verify its SHA-256 independently.

The lock file keeps two digests distinct:

- local: computed by this script over the bytes it actually downloaded (never
  copied from a vendor page).
- official: the digest GitHub publishes for the release asset (asset.digest
  from the GitHub releases API), recorded at --record time.

digestAgreement is true only when the local digest and the official digest
(and both sizes) agree. The lock file is the only pinned source of truth for
the browser asset.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SPIKE_ROOT = Path(__file__).resolve().parent
LOCK_DIR = SPIKE_ROOT / "lock"
ARTIFACT_DIR = REPO_ROOT / "artifacts" / "camoufox-m0"

RELEASE = "v152.0.4-beta.28"
PLATFORM = "linux-x86_64"
ASSET_NAME = "camoufox-152.0.4-beta.28-lin.x86_64.zip"
URL = (
    "https://github.com/daijro/camoufox/releases/download/"
    f"{RELEASE}/{ASSET_NAME}"
)
GITHUB_RELEASES_API = (
    "https://api.github.com/repos/daijro/camoufox/releases/tags/" + RELEASE
)
LOCK_PATH = LOCK_DIR / f"camoufox-{RELEASE}-{PLATFORM}.json"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        while True:
            chunk = fh.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def download(url: str, dest: Path) -> tuple[str, int]:
    tmp = dest.with_suffix(".part")
    digest = hashlib.sha256()
    size = 0
    request = urllib.request.Request(url, headers={"User-Agent": "VeriSilo-M0-spike"})
    with urllib.request.urlopen(request, timeout=120) as resp, tmp.open("wb") as fh:
        while True:
            chunk = resp.read(1024 * 1024)
            if not chunk:
                break
            fh.write(chunk)
            digest.update(chunk)
            size += len(chunk)
    os.replace(tmp, dest)
    return digest.hexdigest(), size


def fetch_github_asset_metadata() -> dict:
    """Fetch the release asset metadata GitHub publishes for this pin."""
    request = urllib.request.Request(
        GITHUB_RELEASES_API,
        headers={
            "Accept": "application/vnd.github+json",
            "User-Agent": "VeriSilo-M0-spike",
        },
    )
    with urllib.request.urlopen(request, timeout=60) as resp:
        release = json.load(resp)
    for asset in release.get("assets", []):
        if asset.get("name") != ASSET_NAME:
            continue
        digest = asset.get("digest") or ""
        algorithm, _, hex_digest = digest.partition(":")
        if not hex_digest:
            raise RuntimeError(
                f"GitHub API returned no digest for asset {ASSET_NAME}"
            )
        return {
            "assetId": asset["id"],
            "name": asset["name"],
            "sizeBytes": asset["size"],
            "officialDigest": digest,
            "officialDigestAlgorithm": algorithm,
            "officialDigestHex": hex_digest,
            "url": asset.get("browser_download_url") or URL,
            "metadataSource": GITHUB_RELEASES_API,
        }
    raise RuntimeError(f"asset {ASSET_NAME} not found in release {RELEASE} metadata")


def build_record(
    local_sha256: str,
    local_size: int,
    metadata: dict,
    computed_at: str,
) -> dict:
    official_hex = metadata["officialDigestHex"]
    agreement = (
        local_sha256 == official_hex and local_size == metadata["sizeBytes"]
    )
    return {
        "schema": "verisilo-camoufox-browser-asset/v2",
        "package": "camoufox",
        "release": RELEASE,
        "platform": PLATFORM,
        "pythonPackage": "camoufox==0.5.4",
        "url": URL,
        # Aliases kept for consumers of the v1 schema: these are the LOCAL
        # computed values, never copied from the vendor.
        "sha256": local_sha256,
        "sizeBytes": local_size,
        "githubAsset": {
            "assetId": metadata["assetId"],
            "name": metadata["name"],
            "url": metadata["url"],
            "sizeBytes": metadata["sizeBytes"],
            "officialDigest": metadata["officialDigest"],
            "officialDigestAlgorithm": metadata["officialDigestAlgorithm"],
            "officialDigestHex": metadata["officialDigestHex"],
            "metadataSource": metadata["metadataSource"],
        },
        "local": {
            "sha256": local_sha256,
            "sizeBytes": local_size,
            "computedBy": "VeriSilo fetch-browser.py (independent SHA-256 over bytes received)",
            "computedAtUtc": computed_at,
        },
        "digestAgreement": agreement,
        "digestAgreementBasis": (
            "local SHA-256 matches GitHub official digest and local size matches "
            "GitHub asset size"
            if agreement
            else "local SHA-256 and/or size do NOT match GitHub official metadata"
        ),
        "recordedBy": (
            "VeriSilo fetch-browser.py (local digest + GitHub releases API metadata)"
        ),
        "recordedAtUtc": datetime.now(timezone.utc).isoformat(),
    }


def load_lock() -> dict:
    if not LOCK_PATH.exists():
        return {}
    return json.loads(LOCK_PATH.read_text())


def write_lock(record: dict) -> None:
    LOCK_DIR.mkdir(parents=True, exist_ok=True)
    tmp = LOCK_PATH.with_suffix(".tmp")
    tmp.write_text(json.dumps(record, indent=2) + "\n")
    os.replace(tmp, LOCK_PATH)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--record",
        action="store_true",
        help="Download, fetch official GitHub asset metadata, and record the pin.",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="With --record, overwrite an existing pin.",
    )
    args = parser.parse_args()

    ARTIFACT_DIR.mkdir(parents=True, exist_ok=True)
    archive = ARTIFACT_DIR / ASSET_NAME
    lock = load_lock()
    pinned = lock.get("sha256")

    if args.record and (not pinned or args.force):
        try:
            metadata = fetch_github_asset_metadata()
        except Exception as exc:  # noqa: BLE001 - report the real cause
            raise SystemExit(f"failed to fetch GitHub asset metadata: {exc}")
        print(f"downloading {ASSET_NAME} (pinned release {RELEASE}) ...")
        digest, size = download(URL, archive)
        computed_at = datetime.now(timezone.utc).isoformat()
        record = build_record(digest, size, metadata, computed_at)
        write_lock(record)
        lock = record
        print(f"local sha256={digest} size={size}")
        print(
            f"official sha256={metadata['officialDigest']} "
            f"size={metadata['sizeBytes']} assetId={metadata['assetId']}"
        )
        print(
            "digestAgreement=true"
            if record["digestAgreement"]
            else "digestAgreement=false (local and official metadata disagree)"
        )
        return 0 if record["digestAgreement"] else 1

    if not pinned:
        print(
            "no pin recorded yet; run `uv run python fetch-browser.py --record` "
            "to bootstrap the lock (digest is computed locally)",
            file=sys.stderr,
        )
        return 2
    if not archive.exists():
        print("archive missing; run `uv run python fetch-browser.py --record`", file=sys.stderr)
        return 2

    digest = sha256_file(archive)
    size = archive.stat().st_size
    if digest != pinned:
        print(
            f"SHA-256 mismatch: expected {pinned}, got {digest}",
            file=sys.stderr,
        )
        return 1
    if lock.get("sizeBytes") != size:
        print(
            f"size mismatch: expected {lock.get('sizeBytes')}, got {size}",
            file=sys.stderr,
        )
        return 1

    official = lock.get("githubAsset") or {}
    official_digest = official.get("officialDigest") or ""
    official_hex = (
        official.get("officialDigestHex")
        or official_digest.partition(":")[2]
        or ""
    )
    official_size = official.get("sizeBytes")
    if not official_hex or official_size is None:
        print(
            "lock is missing GitHub asset metadata; run "
            "`uv run python fetch-browser.py --record --force` to refresh",
            file=sys.stderr,
        )
        return 2
    agreement = digest == official_hex and size == official_size
    print(
        f"verified {ASSET_NAME}: local sha256={digest} size={size} "
        f"official sha256={official_digest} size={official_size} "
        f"digestAgreement={'true' if agreement else 'false'}"
    )
    return 0 if agreement else 1


if __name__ == "__main__":
    raise SystemExit(main())
