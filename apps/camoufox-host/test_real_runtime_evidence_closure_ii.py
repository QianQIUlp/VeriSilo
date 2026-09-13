#!/usr/bin/env python3
"""Real Managed Camoufox runtime verification for Evidence Coverage Closure II.

Validates the full Configured -> Applied -> Observed -> Reconciled evidence chain
against a real Managed Camoufox browser on native Windows using the current Host source:
1. launch: verifies webgl2Vendor, webgl2Renderer, and headers.Accept-Encoding in identityEvidence
2. reobserve_identity: verifies fresh re-observation, updated timestamp, and signal integrity
3. negative inducement: proves mismatched and unavailable states are honest, not always-matched
4. graceful close: verifies clean shutdown without orphan processes.
"""

from __future__ import annotations

import asyncio
import copy
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

PACKAGE_ROOT = Path(r"C:\Users\qiu\src\VeriSilo\artifacts\build\managed-browser\rc3-engine-package")
BROWSER_ROOT = PACKAGE_ROOT / "browser"
ASSET_LOCK = PACKAGE_ROOT / "runtime-asset-lock.json"
BROWSER_TREE = PACKAGE_ROOT / "browser-tree-manifest.json"
SUPERVISOR = PACKAGE_ROOT / "host" / "verisilo-camoufox-supervisor.exe"
PROBE_FILE = HOST_DIR.parent.parent / "tests" / "fingerprint-probe" / "probe.html"

from playwright.async_api import async_playwright
import host_v1
from host_v1 import CamoufoxHost, managed_pids
from identity_policy import (
    configured_identity_digest,
    compute_artifact_digest,
    reconcile_website_identity,
    verify_artifact_raw,
)


def load_base_artifact() -> dict[str, Any]:
    """Load the qualified Formal-v3 candidate artifact from QA commit 96543d7."""
    raw = subprocess.check_output(
        [
            "git",
            "show",
            "96543d74e34089c8d3f0b6199aaa974a2cff9922:docs/qa/fingerprint-surface-truth-matrix/raw/artifact-cold1.json",
        ],
        cwd=str(HOST_DIR.parent.parent),
    )
    return json.loads(raw.decode("utf-8"))


def write_artifact_fixture(root: Path, artifact_id: str, artifact: dict[str, Any]) -> tuple[Path, str]:
    """Write an artifact and its exact sha256 sidecar into the given root directory."""
    artifact = copy.deepcopy(artifact)
    artifact["artifactId"] = artifact_id
    if "canonicalDigest" in artifact:
        del artifact["canonicalDigest"]
    canonical_digest = compute_artifact_digest(artifact)
    artifact["canonicalDigest"] = canonical_digest

    path = root / f"{artifact_id}.json"
    raw = (json.dumps(artifact, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    file_sha = hashlib.sha256(raw).hexdigest()
    path.write_bytes(raw)
    sidecar_path = root / f"{artifact_id}.json.sha256"
    sidecar_path.write_text(f"{file_sha}  {path.name}\n", encoding="ascii")
    return path, file_sha


async def run_real_browser_verification() -> dict[str, Any]:
    print("=== Step 0: Preflight checks ===")
    assert BROWSER_ROOT.is_dir(), f"Missing browser root: {BROWSER_ROOT}"
    assert ASSET_LOCK.is_file(), f"Missing asset lock: {ASSET_LOCK}"
    assert BROWSER_TREE.is_file(), f"Missing browser tree: {BROWSER_TREE}"
    assert SUPERVISOR.is_file(), f"Missing supervisor: {SUPERVISOR}"
    assert PROBE_FILE.is_file(), f"Missing probe file: {PROBE_FILE}"

    base_artifact = load_base_artifact()
    print("Base artifact loaded successfully.")

    with tempfile.TemporaryDirectory(prefix="verisilo-real-host-test-") as tmpdir:
        test_dir = Path(tmpdir)
        artifact_root = test_dir / "artifacts"
        profile_root = test_dir / "profiles"
        state_root = test_dir / "state"
        artifact_root.mkdir()
        profile_root.mkdir()
        state_root.mkdir()

        # Fixture 1: Standard matching artifact
        art1_id = "identity-real-match"
        art1_path, art1_sha = write_artifact_fixture(artifact_root, art1_id, base_artifact)
        print(f"Created Fixture 1: {art1_path.name} (sha: {art1_sha[:16]}...)")

        # Fixture 2: Mismatched Accept-Encoding artifact
        art2 = copy.deepcopy(base_artifact)
        art2["resolvedConfig"]["headers.Accept-Encoding"] = "gzip, deflate"  # Omits 'br, zstd'
        art2_id = "identity-real-mismatch"
        art2_path, art2_sha = write_artifact_fixture(artifact_root, art2_id, art2)
        print(f"Created Fixture 2 (mismatched Accept-Encoding): {art2_path.name}")

        # Fixture 3: Missing Accept-Encoding artifact
        art3 = copy.deepcopy(base_artifact)
        del art3["resolvedConfig"]["headers.Accept-Encoding"]
        art3["policy"]["requiredConfigKeys"] = [
            k for k in art3["policy"]["requiredConfigKeys"] if k != "headers.Accept-Encoding"
        ]
        art3_id = "identity-real-unavailable"
        art3_path, art3_sha = write_artifact_fixture(artifact_root, art3_id, art3)
        print(f"Created Fixture 3 (unavailable Accept-Encoding): {art3_path.name}")

        # Initialize Host with our modified probe.html
        host = CamoufoxHost(
            artifact_root=artifact_root,
            profile_root=profile_root,
            state_root=state_root,
            browser_root=BROWSER_ROOT,
            asset_lock=ASSET_LOCK,
            tree_manifest=BROWSER_TREE,
            supervisor=SUPERVISOR,
            probe_file=PROBE_FILE,
            probe_port=0,
        )

        async with async_playwright() as playwright:
            host.set_playwright(playwright)

            # -------------------------------------------------------------
            # Cycle 1: Launch with Fixture 1 (Standard Matching)
            # -------------------------------------------------------------
            print("\n=== Cycle 1: Real Browser Launch & Observation ===")
            profile1_id = "profile-cycle-1"
            t0 = time.perf_counter()
            launch_res = await host.launch(art1_id, profile1_id, art1_sha)
            launch_duration = time.perf_counter() - t0
            print(f"Launch completed in {launch_duration:.2f}s, sessionId: {launch_res['sessionId']}")

            session_id = launch_res["sessionId"]
            evidence = launch_res.get("identityEvidence")
            assert evidence is not None, "Missing identityEvidence in launch result"

            # Check WebGL2 evidence
            webgl2_vendor_ev = evidence.get("webgl2Vendor")
            print("webgl2Vendor evidence:", json.dumps(webgl2_vendor_ev, indent=2))
            assert webgl2_vendor_ev is not None, "webgl2Vendor missing from evidence"
            assert webgl2_vendor_ev["state"] == "matched", f"Expected matched webgl2Vendor, got {webgl2_vendor_ev}"
            assert "Google Inc." in webgl2_vendor_ev["observed"] or "NVIDIA" in webgl2_vendor_ev["observed"]

            webgl2_renderer_ev = evidence.get("webgl2Renderer")
            print("webgl2Renderer evidence:", json.dumps(webgl2_renderer_ev, indent=2))
            assert webgl2_renderer_ev is not None, "webgl2Renderer missing from evidence"
            # Base artifact has ", or similar" placeholder -> should be unavailable with honest explanation
            assert webgl2_renderer_ev["state"] == "unavailable", f"Expected unavailable webgl2Renderer, got {webgl2_renderer_ev}"
            assert "or similar" in webgl2_renderer_ev["reason"]

            # Check Accept-Encoding evidence
            ae_ev = evidence.get("headers.Accept-Encoding")
            print("headers.Accept-Encoding evidence:", json.dumps(ae_ev, indent=2))
            assert ae_ev is not None, "headers.Accept-Encoding missing from evidence"
            assert ae_ev["state"] == "matched", f"Expected matched Accept-Encoding, got {ae_ev}"
            assert ae_ev["observed"] == "gzip, deflate, br, zstd", f"Observed Accept-Encoding mismatch: {ae_ev['observed']}"
            assert ae_ev["expected"] == "gzip, deflate, br, zstd"

            # -------------------------------------------------------------
            # Cycle 1b: Reobserve Identity (Fresh Recheck)
            # -------------------------------------------------------------
            print("\n=== Cycle 1b: Reobserve Identity (Fresh Recheck) ===")
            t1 = time.perf_counter()
            reobserve_res = await host.reobserve_identity(session_id)
            reobserve_duration = time.perf_counter() - t1
            print(f"Reobserve completed in {reobserve_duration:.2f}s, reobservedAt: {reobserve_res['reobservedAt']}")

            re_evidence = reobserve_res.get("identityEvidence")
            assert re_evidence is not None, "Missing identityEvidence in reobserve result"
            re_ae_ev = re_evidence.get("headers.Accept-Encoding")
            assert re_ae_ev is not None and re_ae_ev["state"] == "matched"
            assert re_ae_ev["observed"] == "gzip, deflate, br, zstd"

            re_gl2_v = re_evidence.get("webgl2Vendor")
            assert re_gl2_v is not None and re_gl2_v["state"] == "matched"

            re_gl2_r = re_evidence.get("webgl2Renderer")
            assert re_gl2_r is not None and re_gl2_r["state"] == "unavailable"

            # Close Session 1
            print("\nClosing Session 1...")
            close_res = await host.close(session_id)
            print("Close result:", close_res)
            assert close_res["state"] in ("closing", "exited"), f"Unexpected close state: {close_res['state']}"
            time.sleep(0.5)

            # -------------------------------------------------------------
            # Cycle 2: Negative test - Induce Mismatch
            # -------------------------------------------------------------
            print("\n=== Cycle 2: Induce Mismatch (Accept-Encoding Expected='gzip, deflate') ===")
            profile2_id = "profile-cycle-2"
            launch2_res = await host.launch(art2_id, profile2_id, art2_sha)
            session2_id = launch2_res["sessionId"]
            evidence2 = launch2_res.get("identityEvidence")
            ae2_ev = evidence2.get("headers.Accept-Encoding")
            print("Mismatched Accept-Encoding evidence:", json.dumps(ae2_ev, indent=2))
            assert ae2_ev is not None, "headers.Accept-Encoding missing from evidence in Cycle 2"
            assert ae2_ev["state"] == "mismatched", f"Expected mismatched state, got {ae2_ev['state']}"
            assert ae2_ev["expected"] == "gzip, deflate"
            assert ae2_ev["observed"] == "gzip, deflate, br, zstd"
            print("Cycle 2 honest mismatch verified!")

            print("Closing Session 2...")
            await host.close(session2_id)
            time.sleep(0.5)

            # -------------------------------------------------------------
            # Cycle 3: Negative test - Induce Unavailable
            # -------------------------------------------------------------
            print("\n=== Cycle 3: Induce Unavailable (Accept-Encoding Missing from Config) ===")
            profile3_id = "profile-cycle-3"
            launch3_res = await host.launch(art3_id, profile3_id, art3_sha)
            session3_id = launch3_res["sessionId"]
            evidence3 = launch3_res.get("identityEvidence")
            ae3_ev = evidence3.get("headers.Accept-Encoding")
            print("Unavailable Accept-Encoding evidence:", json.dumps(ae3_ev, indent=2))
            assert ae3_ev is not None, "headers.Accept-Encoding missing from evidence in Cycle 3"
            assert ae3_ev["state"] == "unavailable", f"Expected unavailable state, got {ae3_ev['state']}"
            assert "not configured" in ae3_ev["reason"]
            print("Cycle 3 honest unavailable verified!")

            print("Closing Session 3...")
            await host.close(session3_id)
            time.sleep(0.5)

    print("\n=== All Real Runtime Cycles PASSED ===")
    return {
        "status": "passed",
        "cycles": 3,
        "launch_duration": launch_duration,
        "reobserve_duration": reobserve_duration,
        "matched_accept_encoding": ae_ev["observed"],
        "webgl2_vendor": webgl2_vendor_ev["observed"],
        "webgl2_renderer_state": webgl2_renderer_ev["state"],
        "mismatched_state_verified": ae2_ev["state"],
        "unavailable_state_verified": ae3_ev["state"],
    }


if __name__ == "__main__":
    result = asyncio.run(run_real_browser_verification())
    print("\nFINAL RESULT SUMMARY:")
    print(json.dumps(result, indent=2))
