#!/usr/bin/env python3
"""Pre-generate QA legacy artifact and package provenance for Sandbox acceptance."""

import argparse
import base64
import hashlib
import json
import os
import subprocess
import sys
from pathlib import Path

HOST_DIR = Path(__file__).resolve().parents[1] / "apps" / "camoufox-host"
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

from identity_policy import (
    assert_artifact_clean,
    compute_artifact_digest,
    configured_identity_digest,
    validate_artifact_strict,
)
from provision_artifact import _declared_stable_signals
from generate_identity import write_artifact_with_sidecar


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--package-root", type=Path, required=True)
    parser.add_argument("--out-driver-dir", type=Path, required=True)
    args = parser.parse_args()

    pkg_root = args.package_root.resolve()
    host_exe = pkg_root / "host" / "camoufox-host.exe"
    if not host_exe.is_file():
        raise FileNotFoundError(f"Missing host executable: {host_exe}")

    driver_dir = args.out_driver_dir.resolve()
    legacy_out_dir = driver_dir / "legacy"
    legacy_out_dir.mkdir(parents=True, exist_ok=True)

    # 1. Gather package provenance
    git_head = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    git_host_tree = subprocess.check_output(["git", "rev-parse", "HEAD:apps/camoufox-host"], text=True).strip()

    package_provenance = {
        "repoHeadSha": git_head,
        "hostSourceTreeSha": git_host_tree,
        "packageRoot": str(pkg_root),
        "hostExecutableSha256": sha256_file(host_exe),
        "supervisorExecutableSha256": sha256_file(pkg_root / "host" / "verisilo-camoufox-supervisor.exe"),
        "probeHtmlSha256": sha256_file(pkg_root / "host" / "probe" / "probe.html"),
        "manifestSha256": sha256_file(pkg_root / "engine-package.json"),
        "packageTreeSha256": sha256_file(pkg_root / "package-tree.json"),
        "browserTreeSha256": sha256_file(pkg_root / "browser-tree-manifest.json"),
        "signed": False,
        "classification": "UNSIGNED_DEV_ENGINE_PACKAGE",
    }
    (driver_dir / "package-provenance.json").write_text(
        json.dumps(package_provenance, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Wrote package provenance to {driver_dir / 'package-provenance.json'}")

    # 2. Provision base artifact using packaged camoufox-host.exe
    temp_staging = driver_dir / "temp-staging"
    temp_art = temp_staging / "identity"
    temp_prof = temp_staging / "profiles"
    temp_state = temp_staging / "state"
    for d in (temp_art, temp_prof, temp_state):
        d.mkdir(parents=True, exist_ok=True)

    seed = bytes(range(32))
    req_json = json.dumps({
        "seed": base64.b64encode(seed).decode("ascii"),
        "preset": "balanced-en-us",
        "window": [1280, 800],
    }).encode("utf-8")
    framed_req = len(req_json).to_bytes(4, "big") + req_json

    proc = subprocess.run(
        [
            str(host_exe),
            "--package-root", str(pkg_root),
            "--artifact-root", str(temp_art),
            "--profile-root", str(temp_prof),
            "--state-root", str(temp_state),
            "--provision-artifact",
        ],
        input=framed_req,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=True,
    )

    res_len = int.from_bytes(proc.stdout[:4], "big")
    res = json.loads(proc.stdout[4:4+res_len].decode("utf-8"))
    if not res.get("ok"):
        raise RuntimeError(f"Base artifact provisioning failed: {res}")

    base_art_id = res["result"]["artifactId"]
    base_art_file = temp_art / f"{base_art_id}.json"
    base_data = json.loads(base_art_file.read_text(encoding="utf-8"))

    # 3. Derive legacy artifact with out-of-bounds coordinates
    legacy_data = json.loads(json.dumps(base_data))
    config = legacy_data["resolvedConfig"]
    injected_x = 2232
    injected_y = 140
    config["window.screenX"] = injected_x
    config["window.screenY"] = injected_y
    legacy_data["stableSignalsDeclared"] = _declared_stable_signals(config)
    legacy_data["configuredIdentityDigest"] = configured_identity_digest(config)
    legacy_data.pop("canonicalDigest", None)
    legacy_data["canonicalDigest"] = compute_artifact_digest(legacy_data)
    validate_artifact_strict(legacy_data)
    assert_artifact_clean(legacy_data)

    legacy_file = legacy_out_dir / f"{base_art_id}.json"
    digest = write_artifact_with_sidecar(legacy_file, legacy_data)
    legacy_sha = sha256_file(legacy_file)

    legacy_meta = {
        "artifactId": base_art_id,
        "legacyFile": legacy_file.name,
        "expectedSha256": legacy_sha,
        "canonicalDigest": digest,
        "injectedScreenX": injected_x,
        "injectedScreenY": injected_y,
        "window": [1280, 800],
        "fontMode": legacy_data.get("policy", {}).get("fontMode", "managed"),
    }
    (legacy_out_dir / "legacy-meta.json").write_text(
        json.dumps(legacy_meta, indent=2) + "\n", encoding="utf-8"
    )
    print(f"Wrote legacy artifact {legacy_file.name} (SHA256: {legacy_sha}) to {legacy_out_dir}")

    # Clean up temp staging
    import shutil
    shutil.rmtree(temp_staging, ignore_errors=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
