#!/usr/bin/env python3
"""Unit tests for the M1.1 identity artifact pipeline.

Runs without pytest: `uv run python test_identity_artifact.py`.
"""

from __future__ import annotations

import asyncio
import copy
import hashlib
import inspect
import json
import os
import random
import re
import subprocess
import tempfile
from pathlib import Path
from unittest import mock

import numpy as np

import browser_asset
import host_v1
import run_spike as run_spike_module
from identity_policy import (
    ARTIFACT_SCHEMA,
    ARTIFACT_SCHEMA_V4,
    ARTIFACT_SCHEMA_V3,
    CANVAS_CLASSIFICATION,
    DETERMINISTIC_CANVAS_BROWSER_BINDING,
    DETERMINISTIC_CANVAS_CLASSIFICATION,
    DETERMINISTIC_CANVAS_POLICY_VARIANT,
    DETERMINISTIC_SESSION_VARIABLE_SIGNAL_KEYS,
    FORMAL_R1_CANVAS_BROWSER_BINDING,
    FORMAL_R1_V2_CANVAS_BROWSER_BINDING,
    GPC_POLICY_MANAGED_OPT_OUT,
    GPC_POLICY_NATIVE,
    GPC_CONFIG_KEY,
    DNT_CONFIG_KEY,
    LEGACY_CANVAS_POLICY_VARIANT,
    OBSERVED_DIGEST_SCHEMA,
    SESSION_VARIABLE_SIGNAL_KEYS,
    STABLE_WEBSITE_SIGNAL_KEYS,
    V5_STABLE_WEBSITE_SIGNAL_KEYS,
    V5_NATIVE_STABLE_WEBSITE_SIGNAL_KEYS,
    VOICE_DERIVED_CONFIG,
    VOICES_MODE_MANAGED,
    VOICES_MODE_NATIVE,
    ArtifactIntegrityError,
    UnsupportedSchemaVersionError,
    canonical_digest,
    canonical_json_bytes,
    canvas_policy_variant_for_browser_binding,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    apply_voices_policy,
    identity_policy,
    observed_website_digest,
    read_bundle_metadata,
    validate_artifact_strict,
    verify_browser_binding,
    verify_artifact,
    verify_artifact_raw,
)
from browser_tree import (
    TreeIntegrityError,
    build_tree_manifest,
    verify_tree,
)
from run_identity_spike import (
    MEDIA_READINESS_REASONS,
    MediaDeviceReadinessError,
    MediaDeviceReadinessTimeout,
    extract_observed_website_signals,
    expected_media_device_counts,
    observed_media_device_counts,
    wait_for_configured_media_devices,
    write_report,
)
from run_spike import (
    CANDIDATE_EXTRA_IDENTITY_FIELDS,
    UnclassifiedCandidateIdentityFieldError,
    classify_candidate_extra_identity_fields,
    firefox_user_prefs_for_config,
    normalize_camou_config_env,
)
from generate_identity import (
    complete_resolved_config,
    declared_stable_signals,
    rebind_identity_artifact,
    write_artifact_with_sidecar,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"

LEGACY_FIXTURE_SHA256 = {
    "identity-a": "2ba26226903d82c0134daa61297defb48930c83f1249c3d4c36bcca16f8cac9e",
    "identity-b": "f142ab0e006a40682a92d2a5b04f88b6a418edf3331084437b672b74dd63b67e",
    "identity-c": "132b108c488e43b01b206412b954ae4f94260d949d22acc89187076e8d1c7640",
    "identity-win-a": "a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f",
    "identity-win-b": "ae7ca69321614e924662e7f162e2f294911fc9facf96db4f4e15d001b0af5db9",
    "identity-win-c": "47572ab26176833807da59889bc07a6ed07e186e9014364e4fde6e0d6d7c10f6",
}


def load_fixture(name: str) -> dict:
    return json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))


def _camou_config_from_env(env: dict) -> dict:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    assert chunks, "CAMOU_CONFIG chunks missing"
    return json.loads("".join(value for _, value in chunks))


def _camou_env(config: dict) -> dict[str, str]:
    return {
        "CAMOU_CONFIG_1": json.dumps(
            config, ensure_ascii=False, separators=(",", ":")
        )
    }


def _assert_config_per_key(actual: dict, expected: dict) -> None:
    assert len(actual) == len(expected)
    assert set(actual) == set(expected)
    for key in sorted(expected):
        assert type(actual[key]) is type(expected[key]), key
        assert actual[key] == expected[key], key


def test_windows_fixtures_pass_strict_validation() -> None:
    lock = json.loads(
        (
            Path(__file__).resolve().parent
            / "lock"
            / "camoufox-v152.0.4-beta.28-windows-x86_64.json"
        ).read_text(encoding="utf-8")
    )
    for name in ("identity-win-a", "identity-win-b", "identity-win-c"):
        path = FIXTURES / f"{name}.json"
        artifact = verify_artifact(path)
        assert artifact["schema"] == ARTIFACT_SCHEMA_V3
        assert artifact["policy"]["schema"] == "verisilo-camoufox-identity-policy/v3"
        assert artifact["policy"]["targetOs"] == "windows"
        assert artifact["policy"]["fontMode"] == "inherit"
        binding = artifact["browserBinding"]
        assert binding["archiveSha256"] == lock["sha256"]
        assert binding["archiveSizeBytes"] == lock["sizeBytes"]
        assert artifact["browserRelease"] == lock["release"]
        assert artifact["canonicalDigest"] == compute_artifact_digest(artifact)
        assert artifact["configuredIdentityDigest"] == configured_identity_digest(
            artifact["resolvedConfig"]
        )
        assert artifact["generatedAtUtc"].endswith("Z")
        assert artifact["exclusions"] == {
            "profilePath": "not recorded",
            "display": "not recorded",
            "tokens": "none supplied",
            "proxySecrets": "none supplied",
            "environment": "not recorded",
        }
        sidecar = path.with_suffix(".json.sha256").read_text(encoding="utf-8").split()[0]
        assert sidecar == hashlib.sha256(path.read_bytes()).hexdigest()


def test_self_built_asset_lock_is_compiled_pinned_and_nonofficial() -> None:
    path = (
        Path(__file__).resolve().parent
        / "lock"
        / "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-windows-x86_64.json"
    )
    raw = path.read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    assert digest == run_spike_module.EXPECTED_SELF_BUILT_ASSET_LOCK_SHA256
    lock = browser_asset.load_asset_lock(
        path,
        expected_release="v152.0.4-beta.28",
        expected_platform="windows-x86_64",
    )
    assert browser_asset.asset_kind(lock) == "self-built"
    assert lock["verified"] is False
    assert lock["evidenceClass"] == "compiled-not-runtime-verified"
    assert lock["sha256"] == (
        "148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94"
    )
    assert lock["sizeBytes"] == 493100709
    assert "digestAgreement" not in lock
    assert "githubAsset" not in lock
    assert lock["sourceBinding"]["commit"] == (
        "e571f6c0b2cea90955b929a4ff04ad54007778fa"
    )
    assert lock["treeManifest"] == {
        **lock["treeManifest"],
        "rawSha256": (
            "3a7b9ba83d93e1d40fc30cb4831750d9a125c76db0551459197c74f6b14c86f9"
        ),
        "canonicalSha256": (
            "42fcfb3f7f028f0a7b71c794236c9f867bae4077d2e2a3087916673968fb98d1"
        ),
        "fileCount": 503,
        "totalBytes": 981205753,
    }


def test_asset_lock_selection_and_root_injection_are_fail_closed() -> None:
    host_dir = Path(__file__).resolve().parent
    self_built_path = (
        host_dir
        / "lock"
        / "camoufox-v152.0.4-beta.28-verisilo-canvas-v1-windows-x86_64.json"
    )
    self_built = run_spike_module.load_asset_lock(self_built_path)
    try:
        run_spike_module.ensure_browser_asset(
            self_built, allow_download=False, browser_root=None
        )
    except SystemExit as exc:
        assert "requires an explicit browser root" in str(exc)
    else:
        raise AssertionError("self-built asset without a browser root was accepted")

    official_path = (
        host_dir / "lock" / "camoufox-v152.0.4-beta.28-windows-x86_64.json"
    )
    official = run_spike_module.load_asset_lock(official_path)
    with tempfile.TemporaryDirectory() as tmp:
        browser_root = Path(tmp) / "browser"
        browser_root.mkdir()
        try:
            run_spike_module.ensure_browser_asset(
                official,
                allow_download=False,
                browser_root=browser_root,
            )
        except SystemExit as exc:
            assert "requires the pinned self-built lock" in str(exc)
        else:
            raise AssertionError("official lock accepted an injected browser root")

        copied_lock = Path(tmp) / self_built_path.name
        copied_lock.write_bytes(self_built_path.read_bytes())
        try:
            run_spike_module.resolve_asset_lock_path(copied_lock)
        except SystemExit as exc:
            assert "allowed pinned repository lock" in str(exc)
        else:
            raise AssertionError("an arbitrary copied lock became a trust anchor")


def test_self_built_launch_rechecks_the_pinned_tree_before_profile_use() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        artifact_root = root / "artifacts"
        artifact_root.mkdir()
        (artifact_root / "identity.json").write_bytes(b"{}\n")
        browser_root = root / "browser"
        browser_root.mkdir()
        executable = browser_root / "camoufox.exe"
        executable.write_bytes(b"bound executable")
        tree_manifest = root / "tree.json"
        tree_manifest.write_bytes(b"{}\n")

        host = object.__new__(host_v1.CamoufoxHost)
        host.artifact_root = artifact_root
        host.profile_root = root / "profiles"
        host.state_root = root / "state"
        host.tree_manifest = tree_manifest
        host.browser_root_arg = browser_root
        host.lock = {"schema": browser_asset.SELF_BUILT_ASSET_SCHEMA}
        host.executable = executable
        host.session = None

        with (
            mock.patch.object(
                host_v1,
                "verify_artifact_raw",
                return_value=({}, "a" * 64),
            ),
            mock.patch.object(host_v1, "verify_browser_binding"),
            mock.patch.object(
                host_v1,
                "verify_self_built_browser_root",
                side_effect=browser_asset.BrowserAssetError(
                    "locked manifest changed"
                ),
            ) as recheck,
        ):
            try:
                asyncio.run(host.launch("identity", "profile-a", "a" * 64))
            except ArtifactIntegrityError as exc:
                assert "locked manifest changed" in str(exc)
            else:
                raise AssertionError("launch continued after the locked-tree recheck failed")

        recheck.assert_called_once_with(
            host.lock,
            browser_root,
            repo_root=host_v1.REPO_ROOT,
            tree_manifest_path=tree_manifest,
            verify_tree_contents=True,
        )
        assert not host.profile_root.exists()


def test_self_built_cache_seed_uses_separate_verisilo_namespace() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        browser_root = root / "browser"
        browser_root.mkdir()
        executable = browser_root / "camoufox.exe"
        executable.write_bytes(b"synthetic executable")
        (browser_root / "ordinary.dat").write_bytes(b"ordinary")
        install = root / "cache" / "camoufox"
        install.mkdir(parents=True)
        lock = {
            "schema": browser_asset.SELF_BUILT_ASSET_SCHEMA,
            "sha256": "a" * 64,
        }
        assert run_spike_module.seed_camoufox_cache(
            lock, executable, install_dir=install
        ) is True
        config = json.loads((install / "config.json").read_text(encoding="utf-8"))
        active = config["active_version"]
        assert active.startswith("browsers/verisilo/")
        assert "/official/" not in active
        assert (install / active / "ordinary.dat").read_bytes() == b"ordinary"


def test_host_hello_wire_shape_remains_v1_under_asset_injection() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        tree = root / "tree.json"
        tree.write_bytes(b"{}\n")
        host = object.__new__(host_v1.CamoufoxHost)
        host.artifact_root = root / "artifacts"
        host.profile_root = root / "profiles"
        host.state_root = root / "state"
        host.tree_manifest = tree
        host.probe_port = 0
        host.lock = {
            "release": "v152.0.4-beta.28",
            "sha256": "a" * 64,
        }
        host.session = None
        hello = host.hello()
        assert set(hello) == {
            "protocol",
            "hostVersion",
            "pythonVersion",
            "artifactRoot",
            "profileRoot",
            "stateRoot",
            "maxFrameBytes",
            "probePortPolicy",
            "browserRelease",
            "assetSha256",
            "treeManifest",
            "treeManifestSha256",
            "platform",
            "state",
            "verified",
            "evidenceClass",
        }


def test_legacy_fixtures_and_policy_remain_byte_exact() -> None:
    for name, expected_sha in LEGACY_FIXTURE_SHA256.items():
        path = FIXTURES / f"{name}.json"
        raw = path.read_bytes()
        artifact = verify_artifact(path)
        assert hashlib.sha256(raw).hexdigest() == expected_sha
        assert artifact["policy"]["sessionVariableFields"] == (
            SESSION_VARIABLE_SIGNAL_KEYS
        )
        assert artifact["policy"]["canvasClassification"] == CANVAS_CLASSIFICATION
        assert (
            canvas_policy_variant_for_browser_binding(artifact["browserBinding"])
            == LEGACY_CANVAS_POLICY_VARIANT
        )


def test_deterministic_canvas_fixtures_are_exact_rebinds() -> None:
    historical_a = load_fixture("identity-win-a")
    historical_b = load_fixture("identity-win-b")
    expected_sources = {
        "identity-win-canvas-v1-a": (historical_a, None),
        "identity-win-canvas-v1-b": (historical_b, None),
        "identity-win-canvas-v1-seed-b": (historical_a, 3261637135),
    }

    for name, (source, canvas_seed) in expected_sources.items():
        path = FIXTURES / f"{name}.json"
        raw = path.read_bytes()
        artifact = verify_artifact(path)
        expected = rebind_identity_artifact(
            source,
            artifact_id=name,
            binding=DETERMINISTIC_CANVAS_BROWSER_BINDING,
            canvas_seed=canvas_seed,
        )
        assert artifact == expected
        assert artifact["schema"] == ARTIFACT_SCHEMA_V3
        assert len(artifact["resolvedConfig"]) == 47
        assert artifact["browserBinding"] == DETERMINISTIC_CANVAS_BROWSER_BINDING
        assert (
            canvas_policy_variant_for_browser_binding(artifact["browserBinding"])
            == DETERMINISTIC_CANVAS_POLICY_VARIANT
        )
        assert artifact["policy"]["stableWebsiteFields"] == (
            STABLE_WEBSITE_SIGNAL_KEYS
        )
        assert artifact["policy"]["sessionVariableFields"] == (
            DETERMINISTIC_SESSION_VARIABLE_SIGNAL_KEYS
        )
        assert artifact["policy"]["canvasClassification"] == (
            DETERMINISTIC_CANVAS_CLASSIFICATION
        )
        assert artifact["configuredIdentityDigest"] == configured_identity_digest(
            artifact["resolvedConfig"]
        )
        assert artifact["canonicalDigest"] == compute_artifact_digest(artifact)
        assert not raw.startswith(b"\xef\xbb\xbf")
        assert b"\r\n" not in raw
        assert raw.endswith(b"\n")
        expected_sidecar = (
            f"{hashlib.sha256(raw).hexdigest()}  {path.name}\n".encode("ascii")
        )
        assert path.with_suffix(".json.sha256").read_bytes() == expected_sidecar

    focused_a = load_fixture("identity-win-canvas-v1-a")
    focused_seed_b = load_fixture("identity-win-canvas-v1-seed-b")
    assert sorted(
        key
        for key in focused_a
        if focused_a[key] != focused_seed_b[key]
    ) == [
        "artifactId",
        "canonicalDigest",
        "configuredIdentityDigest",
        "resolvedConfig",
        "stableSignalsDeclared",
    ]
    assert diff_configs(
        focused_a["resolvedConfig"], focused_seed_b["resolvedConfig"]
    ) == {"added": [], "removed": [], "changed": ["canvas:seed"]}
    assert sorted(
        key
        for key in focused_a["stableSignalsDeclared"]
        if focused_a["stableSignalsDeclared"][key]
        != focused_seed_b["stableSignalsDeclared"][key]
    ) == ["canvasSeed"]
    assert focused_seed_b["resolvedConfig"]["canvas:seed"] == 3261637135
    assert focused_seed_b["stableSignalsDeclared"]["canvasSeed"] == 3261637135


def test_canvas_policy_binding_selection_fails_closed() -> None:
    deterministic = load_fixture("identity-win-canvas-v1-a")
    legacy = load_fixture("identity-win-a")

    assert (
        canvas_policy_variant_for_browser_binding(FORMAL_R1_CANVAS_BROWSER_BINDING)
        == DETERMINISTIC_CANVAS_POLICY_VARIANT
    )
    formal_drift = copy.deepcopy(FORMAL_R1_CANVAS_BROWSER_BINDING)
    formal_drift["archiveSizeBytes"] += 1
    try:
        canvas_policy_variant_for_browser_binding(formal_drift)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("Formal R1 binding drift accepted")

    assert (
        canvas_policy_variant_for_browser_binding(FORMAL_R1_V2_CANVAS_BROWSER_BINDING)
        == DETERMINISTIC_CANVAS_POLICY_VARIANT
    )
    formal_v2_drift = copy.deepcopy(FORMAL_R1_V2_CANVAS_BROWSER_BINDING)
    formal_v2_drift["archiveSha256"] = "0" * 64
    try:
        canvas_policy_variant_for_browser_binding(formal_v2_drift)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("Formal R1 v2 binding drift accepted")

    def assert_binding_rejected(binding: dict, label: str) -> None:
        artifact = copy.deepcopy(deterministic)
        artifact["browserBinding"] = binding
        _recompute_digests(artifact)
        try:
            validate_artifact_strict(artifact)
        except ArtifactIntegrityError:
            return
        raise AssertionError(f"strict validator accepted {label}")

    # A valid deterministic policy under an official binding, and a legacy
    # policy under the patched binding, are both cross-variant artifacts.
    cross_variant = copy.deepcopy(deterministic)
    cross_variant["browserBinding"] = copy.deepcopy(legacy["browserBinding"])
    _recompute_digests(cross_variant)
    try:
        validate_artifact_strict(cross_variant)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("deterministic policy with legacy binding must fail")

    cross_variant = copy.deepcopy(legacy)
    cross_variant["browserBinding"] = copy.deepcopy(
        DETERMINISTIC_CANVAS_BROWSER_BINDING
    )
    _recompute_digests(cross_variant)
    try:
        validate_artifact_strict(cross_variant)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("legacy policy with deterministic binding must fail")

    mismatches = {
        "archiveSha256": legacy["browserBinding"]["archiveSha256"],
        "archiveSizeBytes": legacy["browserBinding"]["archiveSizeBytes"],
        "buildId": legacy["browserBinding"]["buildId"],
        "sourceStamp": "0" * 40,
        "propertiesJsonSha256": "0" * 64,
    }
    for field, value in mismatches.items():
        binding = copy.deepcopy(DETERMINISTIC_CANVAS_BROWSER_BINDING)
        binding[field] = value
        try:
            canvas_policy_variant_for_browser_binding(binding)
        except ArtifactIntegrityError:
            pass
        else:
            raise AssertionError(f"partial binding mismatch accepted: {field}")
        assert_binding_rejected(binding, f"binding mismatch {field}")

    partial = copy.deepcopy(DETERMINISTIC_CANVAS_BROWSER_BINDING)
    partial.pop("sourceStamp")
    try:
        canvas_policy_variant_for_browser_binding(partial)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("partial binding accepted")
    assert_binding_rejected(partial, "partial binding")

    unknown = {
        "archiveSha256": "0" * 64,
        "archiveSizeBytes": 1,
        "buildId": "unknown",
        "sourceStamp": "0" * 40,
        "propertiesJsonSha256": "0" * 64,
    }
    try:
        canvas_policy_variant_for_browser_binding(unknown)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("unknown complete binding accepted")
    assert_binding_rejected(unknown, "unknown complete binding")


def test_observed_digest_v2_contract_does_not_absorb_canvas() -> None:
    assert OBSERVED_DIGEST_SCHEMA == "verisilo-camoufox-observed-website/v2"
    assert "canvasExportHash" not in STABLE_WEBSITE_SIGNAL_KEYS
    deterministic = load_fixture("identity-win-canvas-v1-a")
    assert deterministic["policy"]["stableWebsiteFields"] == (
        STABLE_WEBSITE_SIGNAL_KEYS
    )
    assert "canvasExportHash" not in deterministic["policy"]["stableWebsiteFields"]


def test_canonical_json_deterministic() -> None:
    a = {"b": 1, "a": [3, 2, {"z": None, "y": True}]}
    b = {"a": [3, 2, {"y": True, "z": None}], "b": 1}
    assert canonical_json_bytes(a) == canonical_json_bytes(b)
    assert canonical_json_bytes(a) == canonical_json_bytes(copy.deepcopy(a))


def test_configured_digest_includes_seeds_not_artifactId() -> None:
    base = {"navigator.userAgent": "x", "canvas:seed": 1}
    other_seed = dict(base)
    other_seed["canvas:seed"] = 2
    assert configured_identity_digest(base) != configured_identity_digest(other_seed)
    # The digest payload has no artifactId anywhere.
    assert "artifactId" not in canonical_json_bytes(
        {"schema": "verisilo-camoufox-configured-identity/v1", "resolvedConfig": base}
    ).decode()


def test_observed_digest_payload_shape() -> None:
    signals = {"userAgent": "x", "screen": {"width": 1}}
    assert observed_website_digest(signals) == canonical_digest(
        {"schema": OBSERVED_DIGEST_SCHEMA, "signals": signals}
    )
    # No artifactId and no internal seeds are part of the digest input.
    payload = canonical_json_bytes(
        {"schema": OBSERVED_DIGEST_SCHEMA, "signals": signals}
    ).decode()
    assert "artifactId" not in payload
    assert "canvasSeed" not in payload


def test_windows_media_device_policy_is_deterministic() -> None:
    config = {
        "mediaDevices:enabled": True,
        "mediaDevices:micros": 1,
        "mediaDevices:webcams": 1,
        "mediaDevices:speakers": 0,
    }
    assert expected_media_device_counts(config) == {
        "audioinput": 1,
        "videoinput": 1,
        "audiooutput": 0,
    }
    assert observed_media_device_counts(
        [{"kind": "videoinput"}, {"kind": "audioinput"}]
    ) == expected_media_device_counts(config)
    if os.name == "nt":
        prefs = firefox_user_prefs_for_config(config)
        assert prefs["media.navigator.streams.fake"] is True
        assert prefs["media.navigator.permission.disabled"] is True


def test_candidate_extra_identity_policy_is_closed_and_fail_closed() -> None:
    assert set(CANDIDATE_EXTRA_IDENTITY_FIELDS) == {
        "navigator.maxTouchPoints",
        "navigator.doNotTrack",
        "navigator.globalPrivacyControl",
    }
    rule = CANDIDATE_EXTRA_IDENTITY_FIELDS["navigator.maxTouchPoints"]
    assert rule["status"] == "host-bound"
    assert rule["artifactControl"] == "unavailable"
    assert rule["finalSource"]

    disk = copy.deepcopy(load_fixture("identity-win-a")["resolvedConfig"])
    candidate = copy.deepcopy(disk)
    candidate["navigator.maxTouchPoints"] = 5
    original = copy.deepcopy(candidate)
    audit = classify_candidate_extra_identity_fields(candidate, disk)
    assert audit == {"navigator.maxTouchPoints": rule}
    normalized, diff, rewritten = normalize_camou_config_env(
        _camou_env(candidate), disk
    )
    assert candidate == original
    _assert_config_per_key(normalized, disk)
    _assert_config_per_key(_camou_config_from_env(rewritten), disk)
    assert diff == {"added": [], "removed": [], "changed": []}

    v5_disk = copy.deepcopy(disk)
    v5_disk.pop(DNT_CONFIG_KEY)
    v5_disk.pop(GPC_CONFIG_KEY)
    v5_candidate = copy.deepcopy(v5_disk)
    v5_candidate[DNT_CONFIG_KEY] = "1"
    v5_candidate[GPC_CONFIG_KEY] = True
    audit = classify_candidate_extra_identity_fields(v5_candidate, v5_disk)
    assert set(audit) == {DNT_CONFIG_KEY, GPC_CONFIG_KEY}
    normalized, diff, rewritten = normalize_camou_config_env(
        _camou_env(v5_candidate), v5_disk
    )
    _assert_config_per_key(normalized, v5_disk)
    _assert_config_per_key(_camou_config_from_env(rewritten), v5_disk)
    assert diff == {"added": [], "removed": [], "changed": []}

    for invalid in (0, True, "2"):
        candidate = copy.deepcopy(v5_disk)
        candidate[DNT_CONFIG_KEY] = invalid
        try:
            normalize_camou_config_env(_camou_env(candidate), v5_disk)
        except UnclassifiedCandidateIdentityFieldError:
            pass
        else:
            raise AssertionError("invalid candidate DNT must fail closed")
    for invalid in (0, 1, "true", None):
        candidate = copy.deepcopy(v5_disk)
        candidate[GPC_CONFIG_KEY] = invalid
        try:
            normalize_camou_config_env(_camou_env(candidate), v5_disk)
        except UnclassifiedCandidateIdentityFieldError:
            pass
        else:
            raise AssertionError("invalid candidate GPC must fail closed")

    unknown = copy.deepcopy(disk)
    unknown["window.innerWidth"] = 987654321
    try:
        normalize_camou_config_env(_camou_env(unknown), disk)
    except UnclassifiedCandidateIdentityFieldError as exc:
        assert "window.innerWidth" in str(exc)
        assert "987654321" not in str(exc)
    else:
        raise AssertionError("unknown candidate identity field must fail closed")

    for invalid in (True, "5", -1):
        bad = copy.deepcopy(disk)
        bad["navigator.maxTouchPoints"] = invalid
        try:
            normalize_camou_config_env(_camou_env(bad), disk)
        except UnclassifiedCandidateIdentityFieldError as exc:
            assert "navigator.maxTouchPoints" in str(exc)
            assert repr(invalid) not in str(exc)
        else:
            raise AssertionError("invalid maxTouchPoints candidate must fail closed")


def test_identity_generator_classifies_candidate_only_fields_before_removal() -> None:
    executable = Path("camoufox.exe")

    with mock.patch(
        "camoufox.utils.launch_options",
        return_value={
            "env": _camou_env({"navigator.maxTouchPoints": "SECRET_INVALID_TOUCH"})
        },
    ):
        try:
            complete_resolved_config(executable, "windows", (1280, 800), "en-US", 152)
        except UnclassifiedCandidateIdentityFieldError as exc:
            assert "navigator.maxTouchPoints" in str(exc)
            assert "SECRET_INVALID_TOUCH" not in str(exc)
        else:
            raise AssertionError("generator must reject an invalid candidate-only field")

    with mock.patch(
        "camoufox.utils.launch_options",
        side_effect=(
            {"env": _camou_env({"navigator.maxTouchPoints": 5})},
            {"env": _camou_env({"timezone": "UTC"})},
        ),
    ):
        resolved = complete_resolved_config(
            executable, "windows", (1280, 800), "en-US", 152
        )
    assert resolved == {
        "timezone": "UTC",
        "window.outerHeight": 800,
        "window.outerWidth": 1280,
    }


def test_projection_is_deterministic_under_rng_and_environment_perturbation() -> None:
    from camoufox import DefaultAddons
    from camoufox.utils import launch_options

    artifact_path = FIXTURES / "identity-win-a.json"
    raw_before = artifact_path.read_bytes()
    raw_sha = hashlib.sha256(raw_before).hexdigest()
    assert raw_sha == "a214c21ccf4a68c97040af6e5f81b05e40903a127dea33ace6dce7d8f133279f"
    artifact, verified_sha = verify_artifact_raw(
        artifact_path, expected_file_sha=raw_sha
    )
    assert verified_sha == raw_sha
    disk = copy.deepcopy(artifact["resolvedConfig"])

    def property_type(value: object) -> str:
        return {
            bool: "bool",
            int: "int",
            float: "double",
            str: "str",
            list: "array",
            dict: "dict",
        }[type(value)]

    seen_extras: set[str] = set()
    with tempfile.TemporaryDirectory(prefix="verisilo-fp1-projection-") as tmp:
        root = Path(tmp)
        executable = root / "camoufox.exe"
        executable.write_bytes(b"")
        properties = [
            {"property": key, "type": property_type(value)}
            for key, value in disk.items()
        ]
        properties.append(
            {"property": "navigator.maxTouchPoints", "type": "uint"}
        )
        (root / "properties.json").write_text(
            json.dumps(properties), encoding="utf-8"
        )

        for seed in range(100):
            random.seed(seed)
            np.random.seed(seed ^ 0x5A5A5A5A)
            options = launch_options(
                config=copy.deepcopy(disk),
                os=artifact["policy"]["targetOs"],
                window=tuple(artifact["policy"]["window"]),
                locale=artifact["policy"]["locale"],
                ff_version=artifact["policy"]["ffVersion"],
                headless=False,
                executable_path=str(executable),
                user_data_dir=str(root / f"profile-{seed}"),
                firefox_user_prefs=firefox_user_prefs_for_config(disk),
                exclude_addons=[DefaultAddons.UBO],
                i_know_what_im_doing=True,
                env={
                    "FP1_ALLOWED_ENV_ENTROPY": f"iteration-{seed}",
                    "LANG": "en_US.UTF-8" if seed % 2 else "de_DE.UTF-8",
                    "TZ": "UTC" if seed % 2 else "Etc/GMT+7",
                },
            )
            candidate = _camou_config_from_env(options["env"])
            extras = classify_candidate_extra_identity_fields(candidate, disk)
            seen_extras.update(extras)
            assert set(extras).issubset(CANDIDATE_EXTRA_IDENTITY_FIELDS)
            normalized, diff, rewritten = normalize_camou_config_env(
                options["env"], disk
            )
            _assert_config_per_key(normalized, disk)
            _assert_config_per_key(_camou_config_from_env(rewritten), disk)
            assert diff == {"added": [], "removed": [], "changed": []}
            assert configured_identity_digest(normalized) == artifact[
                "configuredIdentityDigest"
            ]

    assert seen_extras == {"navigator.maxTouchPoints"}
    assert artifact_path.read_bytes() == raw_before
    assert hashlib.sha256(artifact_path.read_bytes()).hexdigest() == raw_sha


def test_fp1_probe_fields_preserve_observed_digest_v2_voice_shape() -> None:
    probe = (REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html").read_text(
        encoding="utf-8"
    )
    for marker in (
        "appCodeName: navigator.appCodeName",
        "appName: navigator.appName",
        "appVersion: navigator.appVersion",
        "product: navigator.product",
        "maxTouchPoints: navigator.maxTouchPoints",
        "windowGeometry:",
        'identityWebGL("webgl2")',
        "isDefault: voice.default",
        "rawRgbaHash: rawHash",
        "decodedPngPixelsHash",
        "pngBytesHash",
        "dataUrlHash",
    ):
        assert marker in probe, marker

    observed = {
        "userAgent": "ua",
        "language": "en-US",
        "languages": ["en-US"],
        "platform": "Win32",
        "oscpu": "Windows NT 10.0; Win64; x64",
        "doNotTrack": "1",
        "globalPrivacyControl": False,
        "screen": {"width": 1},
        "devicePixelRatio": 1,
        "hardwareConcurrency": 8,
        "historyLength": 3,
        "mediaDevices": [],
        "session": {"timezone": "UTC", "utcOffsetMinutes": 0},
        "fontNegativeControls": {},
        "webglVendor": "vendor",
        "webglRenderer": "renderer",
        "webglSummary": {},
        "voices": [
            {
                "name": "voice",
                "lang": "en-US",
                "localService": True,
                "voiceURI": "urn:test",
                "isDefault": True,
            }
        ],
        "audioHash": "sha256:" + "0" * 64,
    }
    signals = extract_observed_website_signals(observed)
    assert signals["voices"] == [
        {
            "name": "voice",
            "lang": "en-US",
            "localService": True,
            "voiceURI": "urn:test",
        }
    ]


class _FakeMediaPage:
    def __init__(self, *responses: object) -> None:
        self.responses = list(responses)
        self.calls = 0
        self.arguments: list[dict] = []

    async def evaluate(self, _script: str, _argument: dict) -> object:
        self.calls += 1
        self.arguments.append(_argument)
        if not self.responses:
            raise AssertionError("unexpected media evaluate call")
        response = self.responses.pop(0)
        if isinstance(response, BaseException):
            raise response
        value = response() if callable(response) else response
        if inspect.isawaitable(value):
            return await value
        return value


def _media_config() -> dict:
    return {
        "mediaDevices:enabled": True,
        "mediaDevices:micros": 1,
        "mediaDevices:webcams": 1,
        "mediaDevices:speakers": 1,
    }


def _media_rpc_result(reason: str, *attempts: tuple[str, ...]) -> dict:
    return {
        "reason": reason,
        "attempts": [
            [{"kind": kind} for kind in attempt]
            for attempt in attempts
        ],
    }


def test_media_readiness_matching_enumeration_succeeds() -> None:
    page = _FakeMediaPage(
        _media_rpc_result(
            "success", ("audioinput", "videoinput", "audiooutput")
        ),
        {"continued": True},
    )

    async def run_case() -> tuple[dict, object]:
        result = await wait_for_configured_media_devices(
            page, _media_config(), clock=lambda: 0.0
        )
        return result, await page.evaluate("continued", {})

    result, continued = asyncio.run(run_case())
    assert page.calls == 2
    assert page.arguments[0] == {"timeoutMs": 7_500}
    assert continued == {"continued": True}
    assert result["reason"] == "success"
    assert result["matched"] is True
    assert result["attempts"][-1] == {
        "counts": {"audioinput": 1, "videoinput": 1, "audiooutput": 1},
        "matched": True,
    }


def test_media_readiness_enumerate_await_is_bounded() -> None:
    async def run_case() -> None:
        cancelled: list[bool] = []

        async def never_returns() -> None:
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        page = _FakeMediaPage(never_returns)
        try:
            await wait_for_configured_media_devices(
                page, _media_config(), timeout_seconds=0.01, poll_interval_ms=1
            )
        except MediaDeviceReadinessTimeout as exc:
            assert exc.reason == "enumerate_timeout"
        else:
            raise AssertionError("unbounded enumerate RPC did not time out")
        assert page.calls == 1
        assert cancelled == [True]

    asyncio.run(run_case())


def test_media_readiness_cancel_settle_is_itself_bounded() -> None:
    async def run_case() -> None:
        release = asyncio.Event()
        finished = asyncio.Event()

        async def cancellation_resistant() -> None:
            try:
                await asyncio.Event().wait()
            except asyncio.CancelledError:
                await release.wait()
            finally:
                finished.set()

        page = _FakeMediaPage(cancellation_resistant)
        started = asyncio.get_running_loop().time()
        try:
            await wait_for_configured_media_devices(
                page, _media_config(), timeout_seconds=0.01, poll_interval_ms=1
            )
        except MediaDeviceReadinessTimeout as exc:
            assert exc.reason == "enumerate_timeout"
        else:
            raise AssertionError("cancellation-resistant RPC did not time out")
        assert asyncio.get_running_loop().time() - started < 0.5
        release.set()
        await asyncio.wait_for(finished.wait(), timeout=0.1)

    asyncio.run(run_case())


def test_media_readiness_poll_await_is_bounded() -> None:
    async def run_case() -> None:
        cancelled: list[bool] = []

        async def never_returns(_seconds: float) -> None:
            try:
                await asyncio.Event().wait()
            finally:
                cancelled.append(True)

        page = _FakeMediaPage(
            _media_rpc_result("success", ("audioinput",)),
        )
        try:
            await wait_for_configured_media_devices(
                page,
                _media_config(),
                timeout_seconds=0.02,
                poll_interval_ms=1,
                readiness_wait=never_returns,
            )
        except MediaDeviceReadinessTimeout as exc:
            assert exc.reason == "readiness_timeout"
        else:
            raise AssertionError("unbounded readiness RPC did not time out")
        assert page.calls == 1
        assert cancelled == [True]

    asyncio.run(run_case())


def test_media_readiness_permanent_count_mismatch_is_typed() -> None:
    class FakeClock:
        value = 0.0

        def __call__(self) -> float:
            return self.value

        async def advance(self, seconds: float) -> None:
            self.value = round(self.value + seconds, 6)

    clock = FakeClock()
    page = _FakeMediaPage(
        _media_rpc_result("success", ("audioinput",)),
        _media_rpc_result("success", ("audioinput",)),
        _media_rpc_result("success", ("audioinput",)),
        {"continued": True},
    )

    async def run_case() -> tuple[dict, object]:
        result = await wait_for_configured_media_devices(
            page,
            _media_config(),
            timeout_seconds=0.005,
            poll_interval_ms=1,
            clock=clock,
            readiness_wait=clock.advance,
        )
        return result, await page.evaluate("continued", {})

    result, continued = asyncio.run(run_case())
    assert page.calls == 4
    assert continued == {"continued": True}
    assert result["reason"] == "count_mismatch"
    assert result["matched"] is False
    assert len(result["attempts"]) == 3


def test_media_readiness_playwright_exception_is_secret_free() -> None:
    secret = (
        "https://127.0.0.1/probe Cookie=secret artifact-seed token=lease "
        r"proxy=credential C:\Users\qiu\profile"
    )
    page = _FakeMediaPage(RuntimeError(secret))
    try:
        asyncio.run(
            wait_for_configured_media_devices(
                page, _media_config(), timeout_seconds=0.02, poll_interval_ms=1
            )
        )
    except MediaDeviceReadinessError as exc:
        assert exc.reason == "playwright_exception"
        assert exc.exception_class == "RuntimeError"
        encoded = str(exc)
    else:
        raise AssertionError("Playwright exception did not fail closed")
    for sentinel in (
        "127.0.0.1",
        "Cookie",
        "artifact-seed",
        "lease",
        "credential",
        r"C:\Users",
    ):
        assert sentinel not in encoded


def test_media_readiness_optional_evidence_is_not_a_launch_barrier() -> None:
    async def run_case(reason: str) -> None:
        page = _FakeMediaPage(
            _media_rpc_result(reason),
            {"continued": True},
        )
        result = await wait_for_configured_media_devices(
            page, _media_config(), timeout_seconds=0.02, poll_interval_ms=1
        )
        assert result["reason"] == reason
        assert result["matched"] is False
        assert await page.evaluate("continued", {}) == {"continued": True}

    for reason in ("enumerate_timeout", "unavailable"):
        asyncio.run(run_case(reason))


def test_fp1_launch_stage_diagnostics_are_bounded_and_secret_free() -> None:
    import host_v1

    lines: list[str] = []
    recorder = host_v1._LaunchStageRecorder()
    with mock.patch.object(host_v1, "_log", side_effect=lines.append):
        for stage in host_v1.FP1_LAUNCH_STAGES:
            with recorder.stage(stage):
                pass
            # A repeated stage cannot add another start or terminal.
            with recorder.stage(stage):
                pass
    assert recorder.event_count == len(host_v1.FP1_LAUNCH_STAGES) * 2
    assert recorder.event_count == host_v1.FP1_STAGE_MAX_EVENTS
    assert recorder.byte_count <= host_v1.FP1_STAGE_MAX_BYTES
    decoded = [json.loads(line.split(" ", 1)[1]) for line in lines]
    for stage in host_v1.FP1_LAUNCH_STAGES:
        events = [item["event"] for item in decoded if item["stage"] == stage]
        assert events == ["start", "success"], stage

    def emit_media_terminal(recorder: object, reason: str) -> None:
        try:
            with recorder.stage("observed.media") as stage:
                stage.set_terminal_reason(reason)
                if reason == "readiness_timeout":
                    raise MediaDeviceReadinessTimeout(reason)
                if reason == "playwright_exception":
                    raise MediaDeviceReadinessError("RuntimeError")
        except (MediaDeviceReadinessTimeout, MediaDeviceReadinessError):
            pass

    expected_terminal_events = {
        "success": "success",
        "enumerate_timeout": "success",
        "readiness_timeout": "timeout",
        "count_mismatch": "success",
        "playwright_exception": "error",
        "unavailable": "success",
    }
    for reason in sorted(MEDIA_READINESS_REASONS):
        reason_lines: list[str] = []
        with mock.patch.object(host_v1, "_log", side_effect=reason_lines.append):
            emit_media_terminal(host_v1._LaunchStageRecorder(), reason)
        reason_events = [
            json.loads(line.split(" ", 1)[1]) for line in reason_lines
        ]
        assert [item["event"] for item in reason_events] == [
            "start",
            expected_terminal_events[reason],
        ]
        assert reason_events[-1]["reason"] == reason

    async def timeout_case() -> None:
        timeout_recorder = host_v1._LaunchStageRecorder()
        timeout_lines: list[str] = []
        with mock.patch.object(host_v1, "_log", side_effect=timeout_lines.append):
            try:
                with timeout_recorder.stage("new_page"):
                    await asyncio.wait_for(asyncio.Event().wait(), timeout=0.001)
            except asyncio.TimeoutError:
                pass
        timeout_events = [
            json.loads(line.split(" ", 1)[1])["event"] for line in timeout_lines
        ]
        assert timeout_events == ["start", "timeout"]

    asyncio.run(timeout_case())

    async def cancelled_case() -> None:
        cancelled_recorder = host_v1._LaunchStageRecorder()
        cancelled_lines: list[str] = []

        async def wait_forever() -> None:
            with cancelled_recorder.stage("observed.media"):
                await asyncio.Event().wait()

        with mock.patch.object(host_v1, "_log", side_effect=cancelled_lines.append):
            task = asyncio.create_task(wait_forever())
            await asyncio.sleep(0)
            task.cancel()
            try:
                await task
            except asyncio.CancelledError:
                pass
        cancelled_events = [
            json.loads(line.split(" ", 1)[1])["event"]
            for line in cancelled_lines
        ]
        assert cancelled_events == ["start", "cancelled"]

    asyncio.run(cancelled_case())

    secret = (
        "https://127.0.0.1/probe Cookie=secret token=lease-seed "
        r"C:\Users\qiu\profile"
    )
    error_lines: list[str] = []
    with mock.patch.object(host_v1, "_log", side_effect=error_lines.append):
        try:
            with host_v1._LaunchStageRecorder().stage("observed.identity"):
                raise RuntimeError(secret)
        except RuntimeError:
            pass
    encoded_errors = "\n".join(error_lines)
    assert "RuntimeError" in encoded_errors
    for sentinel in ("127.0.0.1", "Cookie", "lease-seed", r"C:\Users"):
        assert sentinel not in encoded_errors

    response_lines: list[str] = []
    read_fd, write_fd = os.pipe()
    response_recorder = host_v1._LaunchStageRecorder()
    token = host_v1._ACTIVE_LAUNCH_DIAGNOSTICS.set(response_recorder)
    original_protocol_fd = host_v1._PROTOCOL_FD
    try:
        host_v1._PROTOCOL_FD = write_fd
        with mock.patch.object(host_v1, "_log", side_effect=response_lines.append):
            host_v1._send({"id": "fp1", "ok": True, "result": {"state": "running"}})
    finally:
        host_v1._PROTOCOL_FD = original_protocol_fd
        host_v1._ACTIVE_LAUNCH_DIAGNOSTICS.reset(token)
        os.close(write_fd)
    protocol_bytes = os.read(read_fd, 4096)
    os.close(read_fd)
    protocol_lines = protocol_bytes.decode("utf-8").splitlines()
    assert len(protocol_lines) == 1
    assert json.loads(protocol_lines[0]) == {
        "id": "fp1",
        "ok": True,
        "result": {"state": "running"},
    }
    assert "stage-diagnostic" not in protocol_lines[0]
    response_events = [
        json.loads(line.split(" ", 1)[1])["event"] for line in response_lines
    ]
    assert response_events == ["start", "success"]

    class FlushSink:
        def __init__(self) -> None:
            self.writes = 0
            self.flushes = 0

        def write(self, _value: bytes) -> None:
            self.writes += 1

        def flush(self) -> None:
            self.flushes += 1

    sink = FlushSink()
    stderr_read, stderr_write = os.pipe()
    with mock.patch.object(host_v1, "_STDERR_FD", stderr_write), mock.patch.object(
        host_v1, "_LOG_FILE", sink
    ):
        for reason in sorted(MEDIA_READINESS_REASONS):
            emit_media_terminal(host_v1._LaunchStageRecorder(), reason)
    os.close(stderr_write)
    stderr_lines = os.read(stderr_read, 4096).decode("utf-8").splitlines()
    os.close(stderr_read)
    expected_lines = len(MEDIA_READINESS_REASONS) * 2
    assert len(stderr_lines) == expected_lines
    assert sink.writes == expected_lines
    assert sink.flushes == expected_lines


def test_fp1_fake_stage_timeout_crosses_protocol_and_fail_closed_cleanup() -> None:
    import host_v1

    secret = (
        "https://127.0.0.1/probe Cookie=secret artifact-seed token=lease "
        r"proxy=credential C:\Users\qiu\profile"
    )

    async def run_case(
        root: Path, case: str
    ) -> tuple[bytes, list[str], dict, dict]:
        artifact_root = root / "artifacts"
        profile_root = root / "profiles"
        state_root = root / "state"
        artifact_root.mkdir()
        artifact_path = artifact_root / "identity-win-a.json"
        artifact_path.write_bytes((FIXTURES / "identity-win-a.json").read_bytes())
        artifact = load_fixture("identity-win-a")
        expected_sha = hashlib.sha256(artifact_path.read_bytes()).hexdigest()

        host = object.__new__(host_v1.CamoufoxHost)
        host.artifact_root = artifact_root
        host.profile_root = profile_root
        host.state_root = state_root
        host.tree_manifest = root / "tree.json"
        host.display_arg = None
        host.probe_port = 0
        host.playwright = object()
        host.lock = {"schema": browser_asset.OFFICIAL_ASSET_SCHEMA}
        host.executable = root / "browser" / "camoufox.exe"
        host.session = None

        class NeverMediaPage:
            async def evaluate(self, _script: str, _argument: dict) -> None:
                await asyncio.Event().wait()

        async def never_ready(_seconds: float) -> None:
            await asyncio.Event().wait()

        helper_kwargs: dict = {"timeout_seconds": 0.02, "poll_interval_ms": 1}
        if case == "enumerate_timeout":
            page: object = NeverMediaPage()
            helper_kwargs["timeout_seconds"] = 0.01
        elif case == "readiness_timeout":
            page = _FakeMediaPage(
                _media_rpc_result("success", ("audioinput",))
            )
            helper_kwargs["readiness_wait"] = never_ready
        elif case == "playwright_exception":
            page = _FakeMediaPage(RuntimeError(secret))
        else:
            raise AssertionError(f"unsupported hard media case: {case}")

        async def fake_launch(session: dict, launch_artifact: dict) -> None:
            session["expectedJobName"] = f"Local\\VeriSiloCamoufox-fp1-{case}"
            session["launchAttempted"] = True
            with host_v1._active_launch_stage("observed.media") as media_stage:
                try:
                    await wait_for_configured_media_devices(
                        page,
                        _media_config(),
                        **helper_kwargs,
                    )
                except (MediaDeviceReadinessTimeout, MediaDeviceReadinessError) as exc:
                    media_stage.set_terminal_reason(exc.reason)
                    raise

        host._launch_browser = fake_launch
        cleanup_observed: dict = {}

        def fake_terminate(session: dict, timeout: float) -> dict:
            cleanup_observed.update(
                {
                    "timeout": timeout,
                    "launchAttempted": session.get("launchAttempted"),
                    "expectedJobName": session.get("expectedJobName"),
                    "profileLockHeld": session.get("profileLock") is not None,
                }
            )
            return {
                "exited": True,
                "managedIdentities": [],
                "remaining": [],
                "job": {
                    "available": True,
                    "name": session["expectedJobName"],
                    "activeProcessCount": 0,
                    "terminateJobObject": True,
                },
                "sigterm": False,
                "sigkill": True,
            }

        read_fd, write_fd = os.pipe()
        original_protocol_fd = host_v1._PROTOCOL_FD
        log_lines: list[str] = []
        profile_id = f"fp1-{case.replace('_', '-')}"
        request = {
            "id": profile_id,
            "command": "launch",
            "params": {
                "artifactId": "identity-win-a",
                "profileId": profile_id,
                "expectedArtifactFileSha256": expected_sha,
            },
        }
        try:
            host_v1._PROTOCOL_FD = write_fd
            with mock.patch.object(
                host_v1,
                "verify_artifact_raw",
                return_value=(artifact, expected_sha),
            ), mock.patch.object(
                host_v1, "verify_browser_binding"
            ), mock.patch.object(
                host_v1, "installed_versions", return_value={}
            ), mock.patch.object(
                host_v1, "load_tree_manifest", return_value={}
            ), mock.patch.object(
                host_v1, "verify_tree"
            ), mock.patch.object(
                host_v1, "terminate_managed_tree", side_effect=fake_terminate
            ), mock.patch.object(
                host_v1, "_log", side_effect=log_lines.append
            ):
                should_stop = await host_v1.handle_frame(
                    host,
                    json.dumps(request, separators=(",", ":")).encode("utf-8"),
                )
                assert should_stop is False
        finally:
            host_v1._PROTOCOL_FD = original_protocol_fd
            os.close(write_fd)
        protocol = os.read(read_fd, 4096)
        os.close(read_fd)
        return protocol, log_lines, cleanup_observed, host.session

    expected = {
        "enumerate_timeout": ("timeout", "MediaDeviceReadinessTimeout"),
        "readiness_timeout": ("timeout", "MediaDeviceReadinessTimeout"),
        "playwright_exception": ("error", "MediaDeviceReadinessError"),
    }
    for case, (terminal_event, exception_class) in expected.items():
        with tempfile.TemporaryDirectory(
            prefix=f"verisilo-fp1-stage-{case}-"
        ) as tmp:
            root = Path(tmp)
            protocol, log_lines, cleanup, session = asyncio.run(
                run_case(root, case)
            )
            frames = protocol.decode("utf-8").splitlines()
            assert len(frames) == 1
            response = json.loads(frames[0])
            profile_id = f"fp1-{case.replace('_', '-')}"
            assert response["id"] == profile_id, response
            assert response["ok"] is False
            assert response["error"]["code"] == "launch_failed"
            assert session["state"] == "failed"
            assert session["processTreeExit"]["exited"] is True
            assert cleanup == {
                "timeout": 6,
                "launchAttempted": True,
                "expectedJobName": f"Local\\VeriSiloCamoufox-fp1-{case}",
                "profileLockHeld": True,
            }

            stage_events = [
                json.loads(line.split(" ", 1)[1])
                for line in log_lines
                if line.startswith("stage-diagnostic ")
            ]
            media_events = [
                event
                for event in stage_events
                if event["stage"] == "observed.media"
            ]
            response_events = [
                event["event"]
                for event in stage_events
                if event["stage"] == "response_write"
            ]
            assert [event["event"] for event in media_events] == [
                "start",
                terminal_event,
            ]
            assert media_events[-1]["reason"] == case
            assert media_events[-1]["exceptionClass"] == exception_class
            assert response_events == ["start", "success"]

            lock_path = root / "profiles" / f"{profile_id}.lock"
            reacquired = host_v1.ProfileLock.acquire(lock_path)
            reacquired.release()
            if os.name == "nt":
                assert host_v1.probe_supervisor_lock(lock_path) is True

            combined = (
                protocol.decode("utf-8", errors="replace")
                + "\n".join(log_lines)
                + str(session.get("failure"))
            )
            for sentinel in (
                "127.0.0.1",
                "Cookie",
                "artifact-seed",
                "token=lease",
                "credential",
                r"C:\Users",
            ):
                assert sentinel not in combined


def test_windows_launch_attempt_uses_expected_job_for_fail_closed_cleanup() -> None:
    if os.name != "nt":
        return
    import host_platform

    never_started = {
        "jobHandle": None,
        "supervisorMeta": None,
        "expectedJobName": None,
        "launchAttempted": False,
        "ctx": None,
        "pid": None,
    }
    no_spawn = host_platform.terminate_windows_job(never_started, timeout=0.01)
    assert no_spawn["exited"] is True
    assert no_spawn["job"]["reason"] == "no process was spawned"

    attempted = {
        **never_started,
        "expectedJobName": "Local\\VeriSiloCamoufox-fp1-test",
        "launchAttempted": True,
    }
    with mock.patch.object(
        host_platform.JobHandle, "open", side_effect=OSError("job unavailable")
    ):
        missing = host_platform.terminate_windows_job(attempted, timeout=0.01)
    assert missing["exited"] is False
    assert missing["job"]["available"] is False
    assert missing["job"]["reason"] != "no process was spawned"

    class FakeJob:
        name = "Local\\VeriSiloCamoufox-fp1-test"

        def __init__(self) -> None:
            self.waits = [(False, 1), (True, 0)]
            self.terminated = False
            self.closed = False

        def wait_empty(self, _timeout: float) -> tuple[bool, int]:
            return self.waits.pop(0)

        def terminate(self, _exit_code: int) -> None:
            self.terminated = True

        def close(self) -> None:
            self.closed = True

    fake_job = FakeJob()
    with mock.patch.object(host_platform.JobHandle, "open", return_value=fake_job):
        cleaned = host_platform.terminate_windows_job(attempted, timeout=0.01)
    assert cleaned["exited"] is True
    assert cleaned["job"]["activeProcessCount"] == 0
    assert cleaned["job"]["terminateJobObject"] is True
    assert fake_job.terminated is True
    assert fake_job.closed is True


def test_identity_report_sidecar_hashes_exact_disk_bytes() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        run_dir = Path(tmp)
        write_report(run_dir, {"message": "Windows receipt — exact bytes"})
        report_path = run_dir / "report.json"
        sidecar_digest, sidecar_name = (run_dir / "report.sha256").read_text(
            encoding="utf-8"
        ).split()
        assert sidecar_name == "report.json"
        assert sidecar_digest == hashlib.sha256(report_path.read_bytes()).hexdigest()


def test_artifact_writer_uses_exact_utf8_lf_bytes() -> None:
    artifact = load_fixture("identity-win-a")
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "identity-win-a.json"
        digest = write_artifact_with_sidecar(path, artifact)
        raw = path.read_bytes()
        sidecar_digest, sidecar_name = path.with_suffix(".json.sha256").read_text(
            encoding="ascii"
        ).split()
        assert raw.startswith(b"{") and not raw.startswith(b"\xef\xbb\xbf")
        assert raw.endswith(b"\n") and b"\r\n" not in raw
        assert digest == hashlib.sha256(raw).hexdigest() == sidecar_digest
        assert sidecar_name == path.name
        assert verify_artifact(path)["canonicalDigest"] == artifact["canonicalDigest"]


def test_diff_configs() -> None:
    diff = diff_configs({"a": 1}, {"a": 2, "b": 3})
    assert diff == {"added": ["b"], "removed": [], "changed": ["a"]}


def test_fixtures_pass_strict_validation() -> None:
    for name in ("identity-a", "identity-b", "identity-c"):
        artifact = verify_artifact(FIXTURES / f"{name}.json")
        assert artifact["schema"] == ARTIFACT_SCHEMA_V3
        assert artifact["configuredIdentityDigest"] == configured_identity_digest(
            artifact["resolvedConfig"]
        )
        assert "screen.availTop" in artifact["resolvedConfig"]
        assert "screen.availLeft" in artifact["resolvedConfig"]
        assert "navigator.globalPrivacyControl" in artifact["resolvedConfig"]
        assert artifact["policy"]["timezoneMode"] == "fixed"
        assert isinstance(artifact["resolvedConfig"]["timezone"], str)


def _expect_rejected(artifact: dict, label: str) -> None:
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / f"{label}.json"
        path.write_text(json.dumps(artifact, indent=2) + "\n")
        (path.with_suffix(".json.sha256")).write_text(
            f"{__import__('hashlib').sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        )
        try:
            verify_artifact(path)
        except ArtifactIntegrityError:
            return
        raise AssertionError(f"expected rejection for {label}")


def _recompute_digests(artifact: dict) -> None:
    """Recompute both self-digests so rejection must come from strict schema
    validation, not from a stale digest."""
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)


def _v4_voice_artifact(voices_mode: str) -> dict:
    artifact = copy.deepcopy(load_fixture("identity-win-a"))
    source_policy = artifact["policy"]
    artifact["schema"] = ARTIFACT_SCHEMA_V4
    artifact["policy"] = identity_policy(
        target_os=source_policy["targetOs"],
        font_mode=source_policy["fontMode"],
        window=tuple(source_policy["window"]),
        locale=source_policy["locale"],
        ff_version=source_policy["ffVersion"],
        timezone_mode=source_policy["timezoneMode"],
        browser_binding=artifact["browserBinding"],
        voices_mode=voices_mode,
        schema_version=4,
    )
    apply_voices_policy(artifact["resolvedConfig"], voices_mode)
    artifact["stableSignalsDeclared"] = declared_stable_signals(
        artifact["resolvedConfig"], source_policy["locale"]
    )
    _recompute_digests(artifact)
    return artifact


def _v5_artifact(voices_mode: str, gpc_policy: str) -> dict:
    artifact = copy.deepcopy(load_fixture("identity-win-a"))
    source_policy = artifact["policy"]
    artifact["schema"] = ARTIFACT_SCHEMA
    artifact["policy"] = identity_policy(
        target_os=source_policy["targetOs"],
        font_mode=source_policy["fontMode"],
        window=tuple(source_policy["window"]),
        locale=source_policy["locale"],
        ff_version=source_policy["ffVersion"],
        timezone_mode=source_policy["timezoneMode"],
        browser_binding=artifact["browserBinding"],
        voices_mode=voices_mode,
        gpc_policy=gpc_policy,
    )
    apply_voices_policy(artifact["resolvedConfig"], voices_mode)
    artifact["resolvedConfig"].pop(DNT_CONFIG_KEY, None)
    if gpc_policy == GPC_POLICY_MANAGED_OPT_OUT:
        artifact["resolvedConfig"][GPC_CONFIG_KEY] = True
    else:
        artifact["resolvedConfig"].pop(GPC_CONFIG_KEY, None)
    artifact["stableSignalsDeclared"] = declared_stable_signals(
        artifact["resolvedConfig"], source_policy["locale"], include_device_pixel_ratio=False
    )
    _recompute_digests(artifact)
    return artifact


def test_v5_gpc_dnt_dpr_policy_is_closed_and_ff_bound() -> None:
    for voices_mode in (VOICES_MODE_MANAGED, VOICES_MODE_NATIVE):
        for gpc_policy in (GPC_POLICY_MANAGED_OPT_OUT, GPC_POLICY_NATIVE):
            artifact = _v5_artifact(voices_mode, gpc_policy)
            validate_artifact_strict(artifact)
            assert artifact["policy"]["version"] == 5
            assert "doNotTrack" not in artifact["policy"]["stableWebsiteFields"]
            assert "devicePixelRatio" not in artifact["policy"]["stableWebsiteFields"]
            assert DNT_CONFIG_KEY not in artifact["resolvedConfig"]
            assert DNT_CONFIG_KEY not in artifact["policy"]["requiredConfigKeys"]
            assert "devicePixelRatio" not in artifact["stableSignalsDeclared"]
            if gpc_policy == GPC_POLICY_MANAGED_OPT_OUT:
                assert artifact["resolvedConfig"][GPC_CONFIG_KEY] is True
                assert "globalPrivacyControl" in artifact["policy"]["stableWebsiteFields"]
            else:
                assert GPC_CONFIG_KEY not in artifact["resolvedConfig"]
                assert "globalPrivacyControl" not in artifact["policy"]["stableWebsiteFields"]

    for label, mutate in (
        ("managed-false", lambda a: a["resolvedConfig"].__setitem__(GPC_CONFIG_KEY, False)),
        ("managed-missing", lambda a: a["resolvedConfig"].pop(GPC_CONFIG_KEY)),
        ("native-true", lambda a: a["resolvedConfig"].__setitem__(GPC_CONFIG_KEY, True)),
        ("native-false", lambda a: a["resolvedConfig"].__setitem__(GPC_CONFIG_KEY, False)),
        ("missing-policy", lambda a: a["policy"].pop("navigator.gpcPolicy")),
        ("unknown-policy", lambda a: a["policy"].__setitem__("navigator.gpcPolicy", "bool")),
    ):
        artifact = _v5_artifact(
            VOICES_MODE_NATIVE if label.startswith("native") else VOICES_MODE_MANAGED,
            GPC_POLICY_NATIVE if label.startswith("native") else GPC_POLICY_MANAGED_OPT_OUT,
        )
        mutate(artifact)
        _assert_strict_rejected(artifact, f"v5-{label}")

    artifact = _v5_artifact(VOICES_MODE_MANAGED, GPC_POLICY_MANAGED_OPT_OUT)
    artifact["resolvedConfig"][DNT_CONFIG_KEY] = "1"
    _assert_strict_rejected(artifact, "v5-dnt-config")
    artifact = _v5_artifact(VOICES_MODE_MANAGED, GPC_POLICY_MANAGED_OPT_OUT)
    artifact["policy"]["stableWebsiteFields"].append("doNotTrack")
    _assert_strict_rejected(artifact, "v5-dnt-stable")
    artifact = _v5_artifact(VOICES_MODE_MANAGED, GPC_POLICY_MANAGED_OPT_OUT)
    artifact["stableSignalsDeclared"]["devicePixelRatio"] = 1
    _assert_strict_rejected(artifact, "v5-dpr-declared")

    artifact = _v5_artifact(VOICES_MODE_MANAGED, GPC_POLICY_MANAGED_OPT_OUT)
    artifact["policy"]["ffVersion"] = 134
    artifact["resolvedConfig"]["navigator.userAgent"] = artifact["resolvedConfig"][
        "navigator.userAgent"
    ].replace("Firefox/152.0", "Firefox/134.0")
    artifact["stableSignalsDeclared"]["userAgent"] = artifact["resolvedConfig"][
        "navigator.userAgent"
    ]
    _assert_strict_rejected(artifact, "v5-ff-134")
    try:
        identity_policy(ff_version=134)
    except ValueError:
        pass
    else:
        raise AssertionError("v5 writer must reject FF < 135")


def _assert_strict_rejected(artifact: dict, label: str) -> None:
    _recompute_digests(artifact)
    try:
        validate_artifact_strict(artifact)
    except ArtifactIntegrityError:
        return
    raise AssertionError(f"strict validator accepted invalid artifact shape: {label}")


def test_v4_managed_voices_policy_is_deterministic_and_fail_closed() -> None:
    artifact = _v4_voice_artifact(VOICES_MODE_MANAGED)
    config = artifact["resolvedConfig"]
    assert artifact["policy"]["voicesMode"] == VOICES_MODE_MANAGED
    assert config["voices"]
    assert sum(voice["isDefault"] is True for voice in config["voices"]) == 1
    assert {key: config[key] for key in VOICE_DERIVED_CONFIG} == VOICE_DERIVED_CONFIG
    validate_artifact_strict(artifact)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "identity-v4-managed.json"
        write_artifact_with_sidecar(path, artifact)
        assert verify_artifact(path) == artifact

    replay = copy.deepcopy(config)
    assert apply_voices_policy(replay, VOICES_MODE_MANAGED) == config

    mutations = {
        "empty": lambda a: a["resolvedConfig"]["voices"].clear(),
        "malformed": lambda a: a["resolvedConfig"]["voices"][0].pop("lang"),
        "no-default": lambda a: [
            voice.__setitem__("isDefault", False)
            for voice in a["resolvedConfig"]["voices"]
        ],
        "two-defaults": lambda a: a["resolvedConfig"]["voices"][1].__setitem__(
            "isDefault", True
        ),
        "false-suppression": lambda a: a["resolvedConfig"].__setitem__(
            "voices:blockIfNotDefined", False
        ),
        "false-completion": lambda a: a["resolvedConfig"].__setitem__(
            "voices:fakeCompletion", False
        ),
        "wrong-rate": lambda a: a["resolvedConfig"].__setitem__(
            "voices:fakeCompletion:charsPerSecond", 10.0
        ),
    }
    for key in VOICE_DERIVED_CONFIG:
        mutations[f"missing-{key}"] = lambda a, key=key: a["resolvedConfig"].pop(key)
    for label, mutate in mutations.items():
        candidate = copy.deepcopy(artifact)
        mutate(candidate)
        _assert_strict_rejected(candidate, label)

    empty = {"voices": []}
    try:
        apply_voices_policy(empty, VOICES_MODE_MANAGED)
    except ArtifactIntegrityError:
        pass
    else:
        raise AssertionError("generator policy accepted empty managed voices")


def test_v4_native_voices_policy_omits_all_voice_config() -> None:
    artifact = _v4_voice_artifact(VOICES_MODE_NATIVE)
    forbidden = ("voices", *VOICE_DERIVED_CONFIG)
    assert artifact["policy"]["voicesMode"] == VOICES_MODE_NATIVE
    assert "voices" not in artifact["policy"]["stableWebsiteFields"]
    assert "voices" not in artifact["stableSignalsDeclared"]
    assert all(key not in artifact["resolvedConfig"] for key in forbidden)
    assert all(key not in artifact["policy"]["requiredConfigKeys"] for key in forbidden)
    validate_artifact_strict(artifact)

    managed = _v4_voice_artifact(VOICES_MODE_MANAGED)["resolvedConfig"]
    for key in forbidden:
        candidate = copy.deepcopy(artifact)
        candidate["resolvedConfig"][key] = copy.deepcopy(managed[key])
        _assert_strict_rejected(candidate, f"native-{key}")


def test_tamper_modes_rejected() -> None:
    # digest: mutate config without recomputing canonical digest
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["canvas:seed"] = int(
        artifact["resolvedConfig"]["canvas:seed"]
    ) + 1
    _expect_rejected(artifact, "digest")

    # missing required field
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"].pop("screen.availTop")
    _expect_rejected(artifact, "missing-field")

    # type error
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["navigator.hardwareConcurrency"] = "8"
    _expect_rejected(artifact, "type-error")

    # policy/config mismatch
    artifact = load_fixture("identity-a")
    artifact["policy"]["window"] = [999, 999]
    _expect_rejected(artifact, "policy-mismatch")

    # stableSignalsDeclared/config mismatch
    artifact = load_fixture("identity-a")
    artifact["stableSignalsDeclared"]["userAgent"] = "tampered"
    _expect_rejected(artifact, "declared-mismatch")

    # unknown field
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["navigator.sneaky"] = "x"
    _expect_rejected(artifact, "unknown-field")

    # bad artifactId
    artifact = load_fixture("identity-a")
    artifact["artifactId"] = "../escape"
    _expect_rejected(artifact, "bad-id")


def test_sidecar_required() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        artifact = load_fixture("identity-a")
        path = tmp_path / "no-sidecar.json"
        path.write_text(json.dumps(artifact, indent=2) + "\n")
        try:
            verify_artifact(path)
        except ArtifactIntegrityError as exc:
            assert "sidecar missing" in str(exc)
            return
        raise AssertionError("missing sidecar should be rejected")


def test_mutation_with_recomputed_digest_is_rejected_by_strict_validation() -> None:
    # Recomputing the self-digest is NOT enough: strict validation also checks
    # policy/config consistency, so an internal-seed change with a recomputed
    # digest fails via stableSignalsDeclared mismatch.
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["canvas:seed"] = int(
        artifact["resolvedConfig"]["canvas:seed"]
    ) + 1
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    _expect_rejected(artifact, "recomputed-digest")


def test_browser_binding_verification() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        bundle = Path(tmp) / "bundle"
        bundle.mkdir()
        (bundle / "application.ini").write_text(
            "[App]\nBuildID=111\nSourceStamp=222\n", encoding="utf-8"
        )
        (bundle / "properties.json").write_bytes(b"{}")
        executable = bundle / "camoufox-bin"
        metadata = read_bundle_metadata(executable)
        artifact = load_fixture("identity-a")
        artifact["browserBinding"] = {
            **artifact["browserBinding"],
            **metadata,
        }
        lock = {
            "sha256": artifact["browserBinding"]["archiveSha256"],
            "sizeBytes": artifact["browserBinding"]["archiveSizeBytes"],
            "release": artifact["browserRelease"],
        }
        installed = artifact["generatorVersions"]
        verify_browser_binding(artifact, lock, executable, installed)

        artifact["browserBinding"]["buildId"] = "999"
        try:
            verify_browser_binding(artifact, lock, executable, installed)
        except ArtifactIntegrityError:
            pass
        else:
            raise AssertionError("buildId mismatch should be rejected")


def test_bool_rejected_for_int_fields() -> None:
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["navigator.hardwareConcurrency"] = True
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    # Rejection must come from the strict type check, not the digest.
    _expect_rejected(artifact, "bool-as-int")


def test_canvas_seed_must_be_uint32() -> None:
    for invalid in (-1, 0x100000000):
        artifact = load_fixture("identity-a")
        artifact["resolvedConfig"]["canvas:seed"] = invalid
        artifact["stableSignalsDeclared"]["canvasSeed"] = invalid
        _recompute_digests(artifact)
        _expect_rejected(artifact, f"canvas-seed-{invalid}")

    for valid in (0, 0xFFFFFFFF):
        artifact = load_fixture("identity-a")
        artifact["resolvedConfig"]["canvas:seed"] = valid
        artifact["stableSignalsDeclared"]["canvasSeed"] = valid
        _recompute_digests(artifact)
        validate_artifact_strict(artifact)


def test_fonts_list_rejects_non_strings() -> None:
    artifact = load_fixture("identity-a")
    artifact["resolvedConfig"]["fonts"] = list(artifact["resolvedConfig"]["fonts"]) + [123]
    artifact["stableSignalsDeclared"]["fonts"] = artifact["resolvedConfig"]["fonts"]
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    _expect_rejected(artifact, "fonts-non-string")


def test_nested_unknown_fields_rejected() -> None:
    cases = [
        ("policy", lambda a: a["policy"].__setitem__("sneaky", 1)),
        (
            "webgl-context",
            lambda a: a["resolvedConfig"]["webGl:contextAttributes"].__setitem__(
                "sneaky", True
            ),
        ),
        (
            "voice",
            lambda a: a["resolvedConfig"]["voices"][0].__setitem__("sneaky", "x"),
        ),
        (
            "declared-screen",
            lambda a: a["stableSignalsDeclared"]["screen"].__setitem__("sneaky", 1),
        ),
        ("exclusions", lambda a: a["exclusions"].__setitem__("sneaky", "x")),
        ("binding", lambda a: a["browserBinding"].__setitem__("sneaky", "x")),
        (
            "generator",
            lambda a: a["generatorVersions"].__setitem__("sneaky", "0.0.1"),
        ),
    ]
    for label, mutate in cases:
        artifact = load_fixture("identity-a")
        mutate(artifact)
        artifact["configuredIdentityDigest"] = configured_identity_digest(
            artifact["resolvedConfig"]
        )
        artifact["canonicalDigest"] = compute_artifact_digest(artifact)
        _expect_rejected(artifact, f"nested-{label}")


def test_missing_nested_fields_rejected() -> None:
    cases = [
        ("policy-canonical-rule", lambda a: a["policy"].pop("canonicalJsonRule")),
        ("policy-font-mode", lambda a: a["policy"].pop("fontMode")),
        ("exclusions-tokens", lambda a: a["exclusions"].pop("tokens")),
        (
            "webgl-context-alpha",
            lambda a: a["resolvedConfig"]["webGl:contextAttributes"].pop("alpha"),
        ),
        (
            "voice-field",
            lambda a: a["resolvedConfig"]["voices"][0].pop("isDefault"),
        ),
        (
            "declared-screen-field",
            lambda a: a["stableSignalsDeclared"]["screen"].pop("availTop"),
        ),
        ("binding-field", lambda a: a["browserBinding"].pop("sourceStamp")),
        ("generator-field", lambda a: a["generatorVersions"].pop("playwright")),
        (
            "declared-voices",
            lambda a: a["stableSignalsDeclared"].pop("voices"),
        ),
    ]
    for label, mutate in cases:
        artifact = load_fixture("identity-a")
        mutate(artifact)
        _recompute_digests(artifact)
        _expect_rejected(artifact, f"missing-{label}")


def test_fixtures_font_mode_inherit() -> None:
    for name in ("identity-a", "identity-b", "identity-c"):
        artifact = verify_artifact(FIXTURES / f"{name}.json")
        assert artifact["policy"]["fontMode"] == "inherit"
        assert "fontUniverseWidths" not in artifact["policy"]["stableWebsiteFields"]


def test_old_v2_artifact_returns_unsupported_schema_version() -> None:
    artifact = load_fixture("identity-a")
    artifact["schema"] = "verisilo-camoufox-resolved-identity/v2"
    artifact["policy"]["schema"] = "verisilo-camoufox-identity-policy/v2"
    artifact["policy"]["version"] = 2
    artifact["policy"].pop("fontMode")
    _recompute_digests(artifact)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "identity-v2.json"
        path.write_text(json.dumps(artifact, indent=2) + "\n")
        (path.with_suffix(".json.sha256")).write_text(
            f"{__import__('hashlib').sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        )
        try:
            verify_artifact(path)
        except UnsupportedSchemaVersionError as exc:
            assert "v2" in str(exc)
            return
        raise AssertionError("old v2 artifact must raise UnsupportedSchemaVersionError")


def test_top_level_scalar_closure() -> None:
    bad_values = [
        ("generatedBy-int", lambda a: a.__setitem__("generatedBy", 123)),
        (
            "generatedAt-bool",
            lambda a: a.__setitem__("generatedAtUtc", False),
        ),
        (
            "generatedAt-not-rfc3339",
            lambda a: a.__setitem__("generatedAtUtc", "2026-08-04 06:00:00"),
        ),
        (
            "generatedAt-non-utc-offset",
            lambda a: a.__setitem__("generatedAtUtc", "2026-08-04T06:00:00+02:00"),
        ),
        (
            "generatedAt-not-normalized-z",
            lambda a: a.__setitem__("generatedAtUtc", "2026-08-04T06:00:00+00:00"),
        ),
        (
            "browserRelease-int",
            lambda a: a.__setitem__("browserRelease", 123),
        ),
        (
            "configured-digest-format",
            lambda a: a.__setitem__("configuredIdentityDigest", "not-a-digest"),
        ),
    ]
    for label, mutate in bad_values:
        artifact = load_fixture("identity-a")
        mutate(artifact)
        if label == "configured-digest-format":
            # _recompute_digests would overwrite the tampered digest; keep the
            # garbage value and only keep canonicalDigest self-consistent.
            artifact["canonicalDigest"] = compute_artifact_digest(artifact)
        else:
            _recompute_digests(artifact)
        _expect_rejected(artifact, f"scalar-{label}")

    # Valid canonical Z timestamp passes after digest recompute.
    artifact = load_fixture("identity-a")
    artifact["generatedAtUtc"] = "2026-08-04T06:00:00.123456Z"
    _recompute_digests(artifact)
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "valid-z.json"
        path.write_text(json.dumps(artifact, indent=2) + "\n")
        (path.with_suffix(".json.sha256")).write_text(
            f"{__import__('hashlib').sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
        )
        assert verify_artifact(path)["generatedAtUtc"].endswith("Z")


def test_rfc3339_strict_rejects_lenient_forms() -> None:
    lenient_values = [
        "2026-08-04 06:00:00Z",  # space separator
        "20260804T060000Z",  # basic form
        "2026-08-04T06:00Z",  # missing seconds
        "2026-08-04T06:00:00+00:00",  # not normalized to Z
        "2026-08-04T06:00:00.123Z+02:00",
    ]
    for value in lenient_values:
        artifact = load_fixture("identity-a")
        artifact["generatedAtUtc"] = value
        _recompute_digests(artifact)
        _expect_rejected(artifact, f"rfc3339-{value[:12]}")
    for value in ("2026-08-04T06:00:00Z", "2026-08-04T06:00:00.123456Z"):
        artifact = load_fixture("identity-a")
        artifact["generatedAtUtc"] = value
        _recompute_digests(artifact)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "valid-z.json"
            path.write_text(json.dumps(artifact, indent=2) + "\n")
            (path.with_suffix(".json.sha256")).write_text(
                f"{__import__('hashlib').sha256(path.read_bytes()).hexdigest()}  {path.name}\n"
            )
            assert verify_artifact(path)["generatedAtUtc"] == value


def test_strict_json_parser_rejects_ambiguity() -> None:
    cases = [
        ("duplicate-key", b'{"artifactId":"a","artifactId":"b"}'),
        ("nan", b'{"generatedAtUtc": NaN}'),
        ("infinity", b'{"generatedAtUtc": Infinity}'),
        ("non-object", b'[1,2,3]'),
        ("nested-duplicate", b'{"policy":{"schema":"x","schema":"y"}}'),
    ]
    for label, raw in cases:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / f"{label}.json"
            path.write_bytes(raw)
            (path.with_suffix(".json.sha256")).write_text(
                f"{__import__('hashlib').sha256(raw).hexdigest()}  {path.name}\n"
            )
            try:
                verify_artifact(path)
            except ArtifactIntegrityError as exc:
                assert "strict JSON" in str(exc) or "JSON object" in str(exc), exc
            else:
                raise AssertionError(f"{label} must be rejected by strict parser")


def test_tree_rejects_symlink_and_non_regular() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        tree = Path(tmp) / "tree"
        tree.mkdir()
        (tree / "file.txt").write_text("hello")
        manifest = build_tree_manifest(tree)
        assert verify_tree(tree, manifest)["verified"] is True

        if os.name == "nt":
            (tree / "file.txt").unlink()
            target_dir = Path(tmp) / "target-dir"
            target_dir.mkdir()
            junction = tree / "file.txt"
            result = subprocess.run(
                ["cmd.exe", "/c", "mklink", "/J", str(junction), str(target_dir)],
                capture_output=True,
                text=True,
                timeout=30,
            )
            assert result.returncode == 0, result.stderr or result.stdout
            try:
                try:
                    verify_tree(tree, manifest)
                except TreeIntegrityError as exc:
                    assert "symlink" in str(exc) or "reparse" in str(exc)
                else:
                    raise AssertionError("junction must be rejected")
            finally:
                junction.rmdir()
            return

        # Replacing a regular file with a symlink to identical content must be
        # rejected: file-type integrity is part of tree integrity.
        (tree / "file.txt").unlink()
        target = Path(tmp) / "target.txt"
        target.write_text("hello")
        os.symlink(target, tree / "file.txt")
        try:
            verify_tree(tree, manifest)
        except TreeIntegrityError as exc:
            assert "symlink" in str(exc)
        else:
            raise AssertionError("symlinked file must be rejected")

        # A FIFO / non-regular entry must also be rejected.
        os.unlink(tree / "file.txt")
        os.mkfifo(tree / "file.txt")
        try:
            verify_tree(tree, manifest)
        except TreeIntegrityError as exc:
            assert "non-regular" in str(exc) or "symlink" in str(exc)
        else:
            raise AssertionError("non-regular entry must be rejected")


def test_expected_file_sha_mismatch() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        src = FIXTURES / "identity-a.json"
        path = Path(tmp) / "identity-a.json"
        path.write_bytes(src.read_bytes())
        (path.with_suffix(".json.sha256")).write_text(
            (FIXTURES / "identity-a.json.sha256").read_text(encoding="utf-8")
        )
        try:
            verify_artifact_raw(path, expected_file_sha="0" * 64)
        except ArtifactIntegrityError as exc:
            assert "expected" in str(exc)
            return
        raise AssertionError("expected file sha mismatch must be rejected")


def test_font_universe_sync_with_probe() -> None:
    from host_fonts import FONT_UNIVERSE

    probe = (REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html").read_text(
        encoding="utf-8"
    )
    match = re.search(r"const FONT_UNIVERSE = \[(.*?)\];", probe, re.DOTALL)
    assert match is not None, "FONT_UNIVERSE not found in probe.html"
    names = re.findall(r'"([^"]+)"', match.group(1))
    assert names == FONT_UNIVERSE, "probe.html FONT_UNIVERSE drifted from host_fonts.py"


def main() -> int:
    tests = [
        (name, fn)
        for name, fn in sorted(globals().items())
        if name.startswith("test_") and callable(fn)
    ]
    failed = 0
    for name, fn in tests:
        try:
            fn()
            print(f"PASS {name}")
        except Exception as exc:  # noqa: BLE001
            failed += 1
            print(f"FAIL {name}: {exc}")
    if failed:
        print(f"{failed}/{len(tests)} tests failed")
        return 1
    print(f"all {len(tests)} tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
