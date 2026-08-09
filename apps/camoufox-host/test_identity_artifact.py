#!/usr/bin/env python3
"""Unit tests for the M1.1 identity artifact pipeline.

Runs without pytest: `uv run python test_identity_artifact.py`.
"""

from __future__ import annotations

import copy
import hashlib
import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

from identity_policy import (
    ARTIFACT_SCHEMA,
    OBSERVED_DIGEST_SCHEMA,
    ArtifactIntegrityError,
    UnsupportedSchemaVersionError,
    canonical_digest,
    canonical_json_bytes,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    observed_website_digest,
    read_bundle_metadata,
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
    expected_media_device_counts,
    observed_media_device_counts,
)
from run_spike import firefox_user_prefs_for_config

REPO_ROOT = Path(__file__).resolve().parents[2]
FIXTURES = REPO_ROOT / "tests" / "fixtures" / "camoufox"


def load_fixture(name: str) -> dict:
    return json.loads((FIXTURES / f"{name}.json").read_text(encoding="utf-8"))


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
        assert artifact["schema"] == ARTIFACT_SCHEMA
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
        assert firefox_user_prefs_for_config(config)[
            "media.navigator.streams.fake"
        ] is True


def test_diff_configs() -> None:
    diff = diff_configs({"a": 1}, {"a": 2, "b": 3})
    assert diff == {"added": ["b"], "removed": [], "changed": ["a"]}


def test_fixtures_pass_strict_validation() -> None:
    for name in ("identity-a", "identity-b", "identity-c"):
        artifact = verify_artifact(FIXTURES / f"{name}.json")
        assert artifact["schema"] == ARTIFACT_SCHEMA
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
