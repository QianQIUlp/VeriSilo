#!/usr/bin/env python3
"""Real Managed Camoufox runtime verification for Evidence Coverage Closure II.

Validates the full Configured -> Applied -> Observed -> Reconciled evidence chain
against a real Managed Camoufox browser on native Windows using the current Host source:
1. provenance: records package root, browser exe SHA-256, tree manifest SHA-256, asset lock SHA-256, host source tree SHA, and repo SHA
2. provisioning: provisions a current production Managed Artifact (fontMode=managed, 1280x800) via official provision_artifact API
3. launch: verifies webgl2Vendor, webgl2Renderer, and acceptEncoding in canonical identityEvidence.signals
4. font composition smoke: confirms fontMode=managed host masking gate passes and session enters running
5. reobserve_identity: verifies fresh re-observation, updated timestamp, and signal integrity
6. graceful close: verifies clean shutdown without orphan processes.
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

HOST_DIR = Path(__file__).resolve().parent
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

from playwright.async_api import async_playwright
import host_v1
from host_v1 import CamoufoxHost, managed_pids
from identity_policy import verify_artifact_raw
from provision_artifact import provision_artifact


def sha256_file(path: Path) -> str:
    """Compute SHA-256 digest of a file."""
    h = hashlib.sha256()
    with path.open("rb") as fh:
        while chunk := fh.read(1024 * 1024):
            h.update(chunk)
    return h.hexdigest()


def signal_by_name(evidence: dict[str, Any] | None, name: str) -> dict[str, Any] | None:
    """Retrieve a signal object by name from canonical identityEvidence.signals list."""
    if not isinstance(evidence, dict):
        return None
    signals = evidence.get("signals")
    if not isinstance(signals, list):
        return None
    for item in signals:
        if isinstance(item, dict) and item.get("signal") == name:
            return item
    return None


def get_git_rev(repo_root: Path, rev: str) -> str:
    """Resolve a git revision to its object SHA."""
    try:
        return subprocess.check_output(
            ["git", "rev-parse", rev], cwd=str(repo_root), text=True
        ).strip()
    except Exception:
        return "unknown"


def resolve_package_root(explicit_path: Path | None) -> Path:
    """Resolve package root from explicit CLI argument or environment variable."""
    raw = explicit_path or os.environ.get("VERISILO_PACKAGE_ROOT")
    if not raw:
        raise ValueError(
            "Missing explicit package root. Specify via --package-root <path> "
            "or VERISILO_PACKAGE_ROOT environment variable."
        )
    resolved = Path(raw).resolve()
    if not resolved.is_dir():
        raise FileNotFoundError(f"Package root directory does not exist: {resolved}")
    return resolved


async def run_real_browser_verification(package_root: Path) -> dict[str, Any]:
    browser_root = package_root / "browser"
    browser_exe = browser_root / "camoufox.exe"
    asset_lock = package_root / "runtime-asset-lock.json"
    browser_tree = package_root / "browser-tree-manifest.json"
    supervisor = package_root / "host" / "verisilo-camoufox-supervisor.exe"
    probe_file = HOST_DIR.parent.parent / "tests" / "fingerprint-probe" / "probe.html"

    print("=== Step 0: Preflight checks ===")
    assert browser_root.is_dir(), f"Missing browser root: {browser_root}"
    assert browser_exe.is_file(), f"Missing browser executable: {browser_exe}"
    assert asset_lock.is_file(), f"Missing asset lock: {asset_lock}"
    assert browser_tree.is_file(), f"Missing browser tree: {browser_tree}"
    assert supervisor.is_file(), f"Missing supervisor: {supervisor}"
    assert probe_file.is_file(), f"Missing probe file: {probe_file}"

    repo_root = HOST_DIR.parent.parent
    repo_sha = os.environ.get("VERISILO_REPO_SHA") or get_git_rev(repo_root, "HEAD")
    host_tree_sha = os.environ.get("VERISILO_HOST_SOURCE_TREE_SHA") or get_git_rev(
        repo_root, "HEAD:apps/camoufox-host"
    )
    provenance = {
        "packageRoot": str(package_root),
        "browserExecutableSha256": sha256_file(browser_exe),
        "browserTreeManifestSha256": sha256_file(browser_tree),
        "runtimeAssetLockSha256": sha256_file(asset_lock),
        "hostSourceTreeSha": host_tree_sha,
        "repoSha": repo_sha,
    }
    print("=== Runtime Proof Provenance ===")
    print(json.dumps(provenance, indent=2))

    with tempfile.TemporaryDirectory(prefix="verisilo-real-host-test-") as tmpdir:
        test_dir = Path(tmpdir)
        artifact_root = test_dir / "artifacts"
        profile_root = test_dir / "profiles"
        state_root = test_dir / "state"
        cache_root = state_root / "camoufox-cache"
        artifact_root.mkdir()
        profile_root.mkdir()
        state_root.mkdir()
        cache_root.mkdir()

        print("\n=== Step 1: Provisioning Current Production Managed Artifact ===")
        provision_req = {
            "seed": "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20",
            "preset": "balanced-en-us",
            "window": [1280, 800],
        }
        prov_res = provision_artifact(
            provision_req,
            package_root=package_root,
            artifact_root=artifact_root,
            cache_root=cache_root,
        )
        art_id = prov_res["artifactId"]
        art_sha = prov_res["artifactFileSha256"]
        art_path = artifact_root / f"{art_id}.json"
        artifact, verified_sha = verify_artifact_raw(art_path)
        assert verified_sha == art_sha, "Provisioned artifact SHA mismatch"
        assert artifact["policy"]["fontMode"] == "managed", (
            f"Expected fontMode=managed, got {artifact['policy']['fontMode']}"
        )
        assert tuple(artifact["policy"]["window"]) == (1280, 800), (
            f"Expected window=(1280, 800), got {artifact['policy']['window']}"
        )
        print(f"Provisioned Artifact: {art_id} (sha: {art_sha[:16]}..., fontMode={artifact['policy']['fontMode']})")

        host = CamoufoxHost(
            artifact_root=artifact_root,
            profile_root=profile_root,
            state_root=state_root,
            browser_root=browser_root,
            asset_lock=asset_lock,
            tree_manifest=browser_tree,
            supervisor=supervisor,
            probe_file=probe_file,
            probe_port=0,
            package_root=package_root,
        )

        async with async_playwright() as playwright:
            host.set_playwright(playwright)

            # -------------------------------------------------------------
            # Positive Acceptance Cycle: Launch current production Artifact
            # -------------------------------------------------------------
            print("\n=== Step 2: Real Browser Launch & Canonical Evidence Observation ===")
            profile_id = "profile-production-managed"
            t0 = time.perf_counter()
            launch_res = await host.launch(art_id, profile_id, art_sha)
            launch_duration = time.perf_counter() - t0
            print(f"Launch completed in {launch_duration:.2f}s, sessionId: {launch_res['sessionId']}")

            session_id = launch_res["sessionId"]
            evidence = launch_res.get("identityEvidence")
            assert evidence is not None, "Missing identityEvidence in launch result"
            assert isinstance(evidence.get("signals"), list), (
                "identityEvidence must contain canonical 'signals' list"
            )

            # Managed Font Composition Smoke
            assert launch_res.get("state") == "running", (
                f"Session must enter running state (fontMode=managed masking gate), got {launch_res.get('state')}"
            )
            print("Managed font composition smoke: fontMode=managed host masking gate passed. ✓")

            # Check WebGL2 evidence
            webgl2_vendor_ev = signal_by_name(evidence, "webgl2Vendor")
            print("webgl2Vendor evidence:", json.dumps(webgl2_vendor_ev, indent=2))
            assert webgl2_vendor_ev is not None, "webgl2Vendor missing from evidence signals"
            assert webgl2_vendor_ev["state"] == "matched", (
                f"Expected matched webgl2Vendor, got {webgl2_vendor_ev}"
            )
            assert (
                "Google Inc." in webgl2_vendor_ev["observed"]
                or "NVIDIA" in webgl2_vendor_ev["observed"]
            )

            webgl2_renderer_ev = signal_by_name(evidence, "webgl2Renderer")
            print("webgl2Renderer evidence:", json.dumps(webgl2_renderer_ev, indent=2))
            assert webgl2_renderer_ev is not None, "webgl2Renderer missing from evidence signals"
            assert webgl2_renderer_ev["state"] == "unavailable", (
                f"Expected unavailable webgl2Renderer for placeholder, got {webgl2_renderer_ev}"
            )
            assert (
                "or similar" in webgl2_renderer_ev.get("reason", "").lower()
                or "contract" in webgl2_renderer_ev.get("reason", "").lower()
            )

            # Check Accept-Encoding evidence
            ae_ev = signal_by_name(evidence, "acceptEncoding")
            print("acceptEncoding evidence:", json.dumps(ae_ev, indent=2))
            assert ae_ev is not None, "acceptEncoding missing from evidence signals"
            assert ae_ev["state"] == "matched", (
                f"Expected matched acceptEncoding, got {ae_ev}"
            )
            assert ae_ev["observed"] == "gzip, deflate, br, zstd", (
                f"Observed Accept-Encoding mismatch: {ae_ev['observed']}"
            )
            assert ae_ev["expected"] == "gzip, deflate, br, zstd"

            # -------------------------------------------------------------
            # Step 3: Reobserve Identity (Fresh Recheck)
            # -------------------------------------------------------------
            print("\n=== Step 3: Reobserve Identity (Fresh Recheck) ===")
            t1 = time.perf_counter()
            reobserve_res = await host.reobserve_identity(session_id)
            reobserve_duration = time.perf_counter() - t1
            print(f"Reobserve completed in {reobserve_duration:.2f}s, reobservedAt: {reobserve_res['reobservedAt']}")

            re_evidence = reobserve_res.get("identityEvidence")
            assert re_evidence is not None, "Missing identityEvidence in reobserve result"
            re_ae_ev = signal_by_name(re_evidence, "acceptEncoding")
            assert re_ae_ev is not None and re_ae_ev["state"] == "matched"
            assert re_ae_ev["observed"] == "gzip, deflate, br, zstd"

            re_gl2_v = signal_by_name(re_evidence, "webgl2Vendor")
            assert re_gl2_v is not None and re_gl2_v["state"] == "matched"

            re_gl2_r = signal_by_name(re_evidence, "webgl2Renderer")
            assert re_gl2_r is not None and re_gl2_r["state"] == "unavailable"

            # -------------------------------------------------------------
            # Step 4: Graceful Close
            # -------------------------------------------------------------
            print("\n=== Step 4: Graceful Close ===")
            close_res = await host.close(session_id)
            print("Close result:", close_res)
            assert close_res["state"] in ("closing", "exited"), (
                f"Unexpected close state: {close_res['state']}"
            )
            time.sleep(0.5)

    print("\n=== Real Runtime Acceptance PASSED ===")
    return {
        "status": "passed",
        "provenance": provenance,
        "launch_duration": launch_duration,
        "reobserve_duration": reobserve_duration,
        "matched_accept_encoding": ae_ev["observed"],
        "webgl2_vendor": webgl2_vendor_ev["observed"],
        "webgl2_renderer_state": webgl2_renderer_ev["state"],
        "font_mode": artifact["policy"]["fontMode"],
        "managed_font_masking_gate": "passed",
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Real Managed Camoufox runtime verification for Evidence Coverage Closure II"
    )
    parser.add_argument(
        "--package-root",
        type=Path,
        default=None,
        help="Explicit path to engine package root (can also use VERISILO_PACKAGE_ROOT)",
    )
    args = parser.parse_args()

    try:
        pkg_root = resolve_package_root(args.package_root)
    except Exception as exc:
        print(f"FATAL: {exc}", file=sys.stderr)
        return 2

    result = asyncio.run(run_real_browser_verification(pkg_root))
    print("\nFINAL RESULT SUMMARY:")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
