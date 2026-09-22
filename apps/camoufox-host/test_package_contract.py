#!/usr/bin/env python3
"""Small dependency-free checks for the package/provision seams."""

from __future__ import annotations

import base64
import io
import json
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parent))

from package_contract import (  # noqa: E402
    FORMAL_V3_ARCHIVE_SHA256,
    FORMAL_V3_ARCHIVE_SIZE,
    FORMAL_V3_BUILD_ID,
    FORMAL_V3_BUILD_RESULT_SHA256,
    FORMAL_V3_EXECUTABLE_SHA256,
    FORMAL_V3_PROPERTIES_SHA256,
    FORMAL_V3_RUNTIME_TREE_SHA256,
    FORMAL_V3_SOURCE_COMMIT,
    FORMAL_V3_SOURCE_LOCK_SHA256,
    FORMAL_V3_SOURCE_STAMP,
    FORMAL_V3_SOURCE_TREE,
    PackageLayout,
    PackageContractError,
    build_package_tree,
    manifest_signing_payload,
    recheck_package,
    recheck_formal_package,
    sha256_file,
    safe_relative_path,
    sha256_bytes,
    validate_probe_rendering_layer,
    validate_v3_manifest,
)
from browser_tree import TreeIntegrityError, build_tree_manifest, verify_tree  # noqa: E402
from provision_artifact import (  # noqa: E402
    PROVISION_PRESETS,
    PROVISION_REQUEST_KEYS,
    SUPPORTED_TIMEZONES,
    _artifact_id,
    parse_gpu_preset,
    parse_hardware_concurrency,
    parse_timezone,
    parse_window,
    _artifact_id,
    _atomic_first_writer,
    _network_identity_from_ipwhois,
    decode_seed,
    apply_identity_overrides,
)
from host_v1 import read_provision_frame  # noqa: E402


def _manifest() -> dict:
    host_sha = "1" * 64
    return {
        "schemaVersion": 3,
        "engineId": "camoufox",
        "engineVersion": "152.0.4-beta.28",
        "channel": "experimental",
        "platform": "windows-x64",
        "artifactSha256": host_sha,
        "signature": {"algorithm": "cms-detached-sha256", "keyId": "0" * 64, "value": ""},
        "capabilities": [
            "identity_template", "ua_ua_ch", "language_timezone", "screen", "canvas",
            "webgl", "fonts", "media_devices", "request_headers", "window", "iframe",
            "dedicated_worker",
        ],
        "entrypoint": {
            "kind": "camoufox-host-v1",
            "relativePath": "host/camoufox-host.exe",
            "protocol": "verisilo-camoufox-host/v1",
            "sha256": host_sha,
        },
        "treeManifest": {"relativePath": "package-tree.json", "sha256": "2" * 64},
        "browserTreeManifest": {"relativePath": "browser-tree-manifest.json", "sha256": "3" * 64},
        "hostVersion": "0.1.0",
        "browserRelease": "v152.0.4-beta.28",
        "browserAssetSha256": FORMAL_V3_ARCHIVE_SHA256,
    }


def main() -> int:
    # BrowserForge may place the available area below/right of the screen origin.
    # A full-screen preset must replace that origin along with its dimensions,
    # and clamp window coordinates to eliminate screenX > screen.width contradictions.
    config = {
        "screen.availTop": 4,
        "screen.availLeft": 8,
        "window.screenX": 2232,
        "window.screenY": 140,
    }
    apply_identity_overrides(config, window=(1280, 800), hardware_concurrency=None)
    assert 0 <= config["screen.availTop"] + config["screen.availHeight"] <= config["screen.height"]
    assert 0 <= config["screen.availLeft"] + config["screen.availWidth"] <= config["screen.width"]
    assert config["window.screenX"] == 0
    assert config["window.screenY"] == 0
    assert config["window.screenX"] <= config["screen.width"] - config["window.outerWidth"]
    assert config["window.screenY"] <= config["screen.height"] - config["window.outerHeight"]

    # For smaller window on an available area, clamp within available bounds
    sub_config = {
        "screen.availTop": 0,
        "screen.availLeft": 0,
        "screen.availWidth": 1280,
        "screen.availHeight": 800,
        "window.screenX": 2232,
        "window.screenY": 1400,
    }
    # Pretend apply_identity_overrides without resizing screen
    avail_w = sub_config["screen.availWidth"]
    avail_h = sub_config["screen.availHeight"]
    win_w, win_h = 1000, 600
    max_x = avail_w - win_w  # 280
    max_y = avail_h - win_h  # 200
    clamped_x = max(0, min(sub_config["window.screenX"], max_x))
    clamped_y = max(0, min(sub_config["window.screenY"], max_y))
    assert clamped_x == 280
    assert clamped_y == 200
    assert len(PROVISION_PRESETS) == 19
    # Compact binary frames address presets by index: the four historical
    # entries must keep their original table positions.
    assert tuple(PROVISION_PRESETS)[:4] == (
        "balanced-en-us",
        "balanced-zh-cn",
        "balanced-de-de",
        "match-fixed-proxy",
    )
    assert set(PROVISION_PRESETS) == {
        "balanced-en-us",
        "balanced-zh-cn",
        "balanced-de-de",
        "balanced-ja-jp",
        "balanced-ko-kr",
        "balanced-en-gb",
        "balanced-fr-fr",
        "balanced-es-es",
        "balanced-it-it",
        "balanced-ru-ru",
        "balanced-pt-br",
        "balanced-en-ca",
        "balanced-en-au",
        "balanced-en-in",
        "balanced-en-sg",
        "balanced-en-ph",
        "balanced-tr-tr",
        "balanced-ar-eg",
        "match-fixed-proxy",
    }
    direct_presets = tuple(
        name for name, preset in PROVISION_PRESETS.items() if preset["network"] == "direct"
    )
    assert len(direct_presets) == 18
    assert all(
        isinstance(preset["locale"], str) and isinstance(preset["timezone"], str)
        for preset in PROVISION_PRESETS.values()
        if preset["network"] == "direct"
    )
    assert all(preset["fontMode"] == "managed" for preset in PROVISION_PRESETS.values())
    # The desktop form always sends a concrete timezone; every preset default
    # must therefore be accepted by the host's own whitelist, or provisioning
    # fails for that country.
    assert all(
        preset["timezone"] in SUPPORTED_TIMEZONES
        for preset in PROVISION_PRESETS.values()
        if preset["network"] == "direct"
    )
    layout = PackageLayout.from_root("package")
    assert layout.asset_lock.name == "runtime-asset-lock.json"
    assert layout.supervisor.as_posix().endswith("host/verisilo-camoufox-supervisor.exe")
    assert layout.probe.as_posix().endswith("host/probe/probe.html")
    canonical_probe = (
        Path(__file__).resolve().parents[2] / "tests" / "fingerprint-probe" / "probe.html"
    )
    validate_probe_rendering_layer(canonical_probe.read_text(encoding="utf-8"), "canonical-probe.html")
    for stale_probe in (
        "",
        '<script>hostFontNegativeControls: identityFontAvailability(window.__probeHostFonts);</script>',
        "<script>function identityFontRenderAvailability(fonts){return {};}</script>",
    ):
        try:
            validate_probe_rendering_layer(stale_probe, "stale-probe.html")
        except PackageContractError:
            continue
        raise AssertionError("a probe without rendering-layer masking must be rejected")
    seed = bytes(range(32))
    assert decode_seed(base64.b64encode(seed).decode()) == seed
    assert decode_seed(seed.hex()) == seed
    assert decode_seed(list(seed)) == seed
    assert PROVISION_REQUEST_KEYS == {
        "seed",
        "preset",
        "proxyServer",
        "window",
        "hardwareConcurrency",
        "followNetwork",
        "gpuPreset",
        "timezone",
    }
    assert parse_window(None, (1280, 800)) == (1280, 800)
    assert parse_window([1920, 1080], (1280, 800)) == (1920, 1080)
    assert parse_hardware_concurrency(8) == 8
    assert parse_hardware_concurrency(None) is None
    assert parse_timezone("Asia/Tokyo") == "Asia/Tokyo"
    assert parse_timezone(None) is None
    assert parse_gpu_preset("nvidia-rtx-4070")[1].startswith("NVIDIA GeForce RTX 4070")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "browser"
        root.mkdir()
        (root / "keep.bin").write_bytes(b"keep")
        manifest = build_tree_manifest(root)
        (root / "version.json").write_text("{}", encoding="utf-8")
        verified = verify_tree(root, manifest)
        assert verified["verified"] is True
        with patch("browser_tree.sha256_file", side_effect=AssertionError("browser rehashed")):
            assert verify_tree(
                root,
                manifest,
                _verified_digests={"keep.bin": sha256_bytes(b"keep")},
            ) == verified
        try:
            verify_tree(root, manifest, _verified_digests={})
        except TreeIntegrityError:
            pass
        else:
            raise AssertionError("missing verified package digest was accepted")
        (root / "extra.bin").write_bytes(b"nope")
        try:
            verify_tree(root, manifest)
        except TreeIntegrityError:
            pass
        else:
            raise AssertionError("unknown extra files must still be rejected")
    direct_id = _artifact_id(
        seed,
        "balanced-zh-cn",
        window=(1280, 800),
        hardware_concurrency=None,
        follow_network=False,
    )
    tuned_id = _artifact_id(
        seed,
        "balanced-zh-cn",
        window=(1920, 1080),
        hardware_concurrency=8,
        follow_network=False,
    )
    assert direct_id != tuned_id
    assert direct_id.startswith("identity-")
    try:
        decode_seed(base64.b64encode(seed[:-1]).decode())
    except ValueError:
        pass
    else:
        raise AssertionError("short seed accepted")
    request = json.dumps({"preset": "balanced-en-us", "seed": seed.hex()}).encode()
    assert read_provision_frame(io.BytesIO(len(request).to_bytes(4, "big") + request))["preset"] == "balanced-en-us"
    assert read_provision_frame(io.BytesIO((33).to_bytes(4, "big") + bytes([3]) + seed))["preset"] == "match-fixed-proxy"

    seen: list[tuple[str, str]] = []
    def fake_fetch(url: str, proxy: str) -> dict:
        seen.append((url, proxy))
        return {
            "success": True,
            "ip": "1.1.1.1",
            "country_code": "SG",
            "timezone": {"id": "Asia/Singapore"},
            "latitude": 1.3521,
            "longitude": 103.8198,
        }
    observed = _network_identity_from_ipwhois("socks5://127.0.0.1:43127", fetch=fake_fetch)
    assert observed["countryCode"] == "SG"
    assert seen == [("https://ipwho.is/", "socks5h://127.0.0.1:43127")]

    network = {
        "expectedPublicAddress": "1.1.1.1",
        "countryCode": "SG",
        "timezone": "Asia/Singapore",
        "latitude": 1.3521,
        "longitude": 103.8198,
    }
    assert _artifact_id(
        seed,
        "match-fixed-proxy",
        window=(1280, 800),
        hardware_concurrency=None,
        follow_network=True,
        network=network,
    ) == _artifact_id(
        seed,
        "match-fixed-proxy",
        window=(1280, 800),
        hardware_concurrency=None,
        follow_network=True,
        network=dict(network),
    )
    changed_network = dict(network, expectedPublicAddress="8.8.8.8")
    assert _artifact_id(
        seed,
        "match-fixed-proxy",
        window=(1280, 800),
        hardware_concurrency=None,
        follow_network=True,
        network=network,
    ) != _artifact_id(
        seed,
        "match-fixed-proxy",
        window=(1280, 800),
        hardware_concurrency=None,
        follow_network=True,
        network=changed_network,
    )

    manifest = _manifest()
    validate_v3_manifest(manifest, allow_unsigned=True)
    payload = manifest_signing_payload(manifest)
    assert payload.startswith(b"VeriSilo engine package manifest v3\0")
    assert b'"value":""' in payload
    with tempfile.TemporaryDirectory(prefix="verisilo-package-test-") as temporary:
        root = Path(temporary)
        (root / "host").mkdir()
        (root / "host" / "camoufox-host.exe").write_bytes(b"host")
        (root / "browser-tree-manifest.json").write_bytes(b"tree")
        (root / "runtime-asset-lock.json").write_bytes(b"lock")
        tree = build_package_tree(root)
        assert [entry["path"] for entry in tree["entries"]] == [
            "browser-tree-manifest.json", "host/camoufox-host.exe", "runtime-asset-lock.json"
        ]

    with tempfile.TemporaryDirectory(prefix="verisilo-package-large-tree-") as temporary:
        root = Path(temporary)
        layout = PackageLayout.from_root(root)
        layout.browser_root.mkdir(parents=True)
        layout.host.parent.mkdir(parents=True)
        layout.host.write_bytes(b"host")
        layout.supervisor.write_bytes(b"supervisor")
        layout.probe.parent.mkdir(parents=True)
        layout.probe.write_bytes(b"probe")
        layout.asset_lock.write_bytes(b"lock")
        (layout.browser_root / "camoufox.exe").write_bytes(b"browser")
        (layout.browser_root / "application.ini").write_text(
            "BuildID=fixture\nSourceStamp=fixture\n", encoding="utf-8"
        )
        (layout.browser_root / "properties.json").write_bytes(b"{}")
        for index in range(1500):
            (layout.browser_root / f"payload-{index:04d}.bin").write_bytes(b"x" * 64)
        layout.browser_tree.write_bytes(
            (json.dumps(build_tree_manifest(layout.browser_root), indent=2) + "\n").encode("utf-8")
        )
        large_tree = build_package_tree(root)
        tree_raw = (json.dumps(large_tree, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
        layout.package_tree.write_bytes(tree_raw)
        assert len(tree_raw) > 65536
        large_manifest = _manifest()
        host_sha = sha256_bytes(layout.host.read_bytes())
        large_manifest["artifactSha256"] = host_sha
        large_manifest["entrypoint"]["sha256"] = host_sha
        large_manifest["treeManifest"]["sha256"] = sha256_bytes(tree_raw)
        large_manifest["browserTreeManifest"]["sha256"] = sha256_bytes(
            layout.browser_tree.read_bytes()
        )
        with patch("package_contract.sha256_file", wraps=sha256_file) as hashing:
            assert recheck_package(root, large_manifest)["memberCount"] > 1500
            assert hashing.call_count == len(large_tree["entries"])
        fixture_lock = {
            "browserTreeManifestSha256": sha256_bytes(layout.browser_tree.read_bytes()),
            "executableRelativePath": "camoufox.exe",
            "buildId": "fixture",
            "sourceStamp": "fixture",
            "browserExecutableSha256": sha256_bytes(b"browser"),
            "propertiesJsonSha256": sha256_bytes(b"{}"),
            "engineRevision": "fixture",
        }
        with (
            patch("package_contract.load_package_asset_lock", return_value=fixture_lock),
            patch("package_contract._validate_package_asset_lock"),
            patch("browser_tree.sha256_file", side_effect=AssertionError("browser rehashed")),
        ):
            formal = recheck_formal_package(root, large_manifest)
        assert formal["browserFileCount"] == 1503
        layout.probe.unlink()
        try:
            recheck_package(root, large_manifest)
        except PackageContractError:
            pass
        else:
            raise AssertionError("RC1 probe member missing was accepted")

    with tempfile.TemporaryDirectory(prefix="verisilo-package-atomic-") as temporary:
        path = Path(temporary) / "artifact.json"
        assert _atomic_first_writer(path, b"first")
        assert not _atomic_first_writer(path, b"second")
        assert path.read_bytes() == b"first"
    bad = json.loads(json.dumps(manifest))
    bad["entrypoint"]["relativePath"] = "../host/camoufox-host.exe"
    try:
        validate_v3_manifest(bad, allow_unsigned=True)
    except PackageContractError:
        pass
    else:
        raise AssertionError("path traversal accepted")
    assert safe_relative_path("browser/fonts/Academy Engraved LET Fonts.ttf")

    # Dual-boot Formal-v3 build-input contract regression: the production
    # package builder binds the frozen inputs directly and must reject every
    # stale/historical binding (including the old FP3 runtime-asset lock).
    import importlib.util

    repo_root = Path(__file__).resolve().parents[2]
    builder_path = repo_root / "scripts" / "build-camoufox-host-package.py"
    builder_spec = importlib.util.spec_from_file_location(
        "build_camoufox_host_package", builder_path
    )
    builder = importlib.util.module_from_spec(builder_spec)
    builder_spec.loader.exec_module(builder)

    # A reused PyInstaller directory must carry a source receipt matching the
    # current runtime import seam; an unknown/stale directory is rejected.
    host_source = repo_root / "apps" / "camoufox-host" / "host_v1.py"
    with tempfile.TemporaryDirectory(prefix="verisilo-host-provenance-") as temporary:
        host_directory = Path(temporary) / "host"
        host_directory.mkdir()
        builder._write_host_source_provenance(host_directory, host_source)
        builder._validate_host_source_provenance(host_directory, host_source)
        receipt_path = host_directory / builder.HOST_SOURCE_PROVENANCE_NAME
        receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
        receipt["sourceFiles"]["apps/camoufox-host/provision_artifact.py"] = "0" * 64
        receipt_path.write_text(json.dumps(receipt), encoding="utf-8")
        try:
            builder._validate_host_source_provenance(host_directory, host_source)
        except PackageContractError:
            pass
        else:
            raise AssertionError("stale Host source provenance was accepted")

    with tempfile.TemporaryDirectory(prefix="verisilo-host-provenance-missing-") as temporary:
        try:
            builder._validate_host_source_provenance(Path(temporary), host_source)
        except PackageContractError:
            pass
        else:
            raise AssertionError("Host directory without source provenance was accepted")

    builder._validate_smoke_artifact(
        "balanced-zh-cn", {"policy": {"fontMode": "managed"}}, "managed"
    )
    try:
        builder._validate_smoke_artifact(
            "balanced-zh-cn", {"policy": {"fontMode": "inherit"}}, "managed"
        )
    except PackageContractError:
        pass
    else:
        raise AssertionError("semantic Host smoke accepted the wrong font mode")

    def _dual_boot_record() -> dict:
        return {
            "recordType": "verisilo-camoufox-r1-formal-build-run/v1",
            "runId": "r1formal-v3-win11dual-20260902",
            "buildMode": "formal",
            "engineRevision": "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3",
            "formalSource": True,
            "diagnosticOnly": False,
            "claims": {
                "browserLaunches": 0,
                "compiled": True,
                "formalR1Passed": False,
                "formalSource": True,
                "runtimeVerified": False,
                "windowsRuntimeObserved": False,
            },
            "completeAppliedPatchOrder": [
                "0000", "0001", "0002", "0003", "0003a",
                "0004", "0005", "0006", "0007",
            ],
            "startedAtUtc": "2026-09-02T12:41:00.688104Z",
            "completedAtUtc": "2026-09-02T13:54:52.797012Z",
            "archive": {
                "name": "camoufox-152.0.4-beta.28-win.x86_64.zip",
                "sha256": FORMAL_V3_ARCHIVE_SHA256,
                "sizeBytes": FORMAL_V3_ARCHIVE_SIZE,
                "camoufoxExeSha256": FORMAL_V3_EXECUTABLE_SHA256,
                "buildId": FORMAL_V3_BUILD_ID,
                "sourceStamp": FORMAL_V3_SOURCE_STAMP,
                "treeManifest": {
                    "name": "windows-extraction-tree.json",
                    "sha256": builder.FORMAL_V3_EXTRACTION_TREE_SHA256,
                    "sizeBytes": builder.FORMAL_V3_EXTRACTION_TREE_SIZE,
                    "entryCount": 514,
                },
            },
            "inputs": {
                "sourceLockSha256": FORMAL_V3_SOURCE_LOCK_SHA256,
                "completeAppliedPatchOrder": [
                    "0000", "0001", "0002", "0003", "0003a",
                    "0004", "0005", "0006", "0007",
                ],
                "verisiloCommit": "6497828aa0643f94fed3ae708734eef6b85f8305",
                "verisiloTree": "3b796a461ad80e8cad82bad5cc6d73b8f8a1e58a",
                "upstreamCommit": "0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d",
                "upstreamTree": "1435d544d9b61dee7fcf74cf92462952ca43d38e",
                "upstreamPatchCount": 50,
            },
        }

    builder.validate_formal_build_record(_dual_boot_record())

    def _reject_record(mutate) -> None:
        candidate = _dual_boot_record()
        mutate(candidate)
        try:
            builder.validate_formal_build_record(candidate)
        except PackageContractError:
            pass
        else:
            raise AssertionError("invalid Formal-v3 build record accepted")

    # The pre-dual-boot kernel/inputs must not be accepted as the current build.
    _reject_record(lambda r: r["archive"].__setitem__(
        "sha256", "032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5"))
    _reject_record(lambda r: r["archive"].__setitem__(
        "camoufoxExeSha256", "b147602826db5bf852e5777f56cd56036dc04e8ea8868a8e55f8b08744f142a6"))
    _reject_record(lambda r: r["archive"].__setitem__(
        "sizeBytes", 493493005))
    # Historical run identity and any source-binding drift stay rejected.
    _reject_record(lambda r: r.__setitem__("runId", "r1formal-v3-engine-20260827t031900z"))
    _reject_record(lambda r: r["inputs"].__setitem__("sourceLockSha256", "0" * 64))
    _reject_record(lambda r: r["claims"].__setitem__("compiled", False))
    _reject_record(lambda r: r.__setitem__(
        "completeAppliedPatchOrder", ["0000", "0001", "0002"]))
    _reject_record(lambda r: r.__setitem__("recordType", "verisilo-r1-formal-build-result/v1"))

    # The committed build record must be exactly the pinned dual-boot record.
    build_result_path = (
        repo_root / "apps" / "camoufox-host" / "lock"
        / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-build-result.json"
    )
    build_result_raw = build_result_path.read_bytes()
    assert sha256_bytes(build_result_raw) == FORMAL_V3_BUILD_RESULT_SHA256
    builder.validate_formal_build_record(json.loads(build_result_raw))

    # The committed source lock must declare exactly this build-record path.
    source_lock_path = (
        repo_root / "apps" / "camoufox-host" / "lock"
        / "camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-source.json"
    )
    source_lock_raw = source_lock_path.read_bytes()
    assert sha256_bytes(source_lock_raw) == FORMAL_V3_SOURCE_LOCK_SHA256
    builder.validate_source_lock_content(json.loads(source_lock_raw))

    # The generated package asset lock binds exactly the dual-boot inputs.
    expected_lock = {
        "schema": "verisilo-camoufox-package-asset/v1",
        "assetKind": "self-built",
        "verified": False,
        "evidenceClass": "compiled-not-runtime-verified",
        "package": "camoufox",
        "release": "v152.0.4-beta.28",
        "platform": "windows-x86_64",
        "pythonPackage": "camoufox==0.5.4",
        "engineRevision": "verisilo-camoufox-152.0.4-beta.28-r1-formal-v3",
        "sha256": FORMAL_V3_ARCHIVE_SHA256,
        "browserExecutableSha256": FORMAL_V3_EXECUTABLE_SHA256,
        "sizeBytes": FORMAL_V3_ARCHIVE_SIZE,
        "executableRelativePath": "camoufox.exe",
        "buildId": FORMAL_V3_BUILD_ID,
        "sourceStamp": FORMAL_V3_SOURCE_STAMP,
        "propertiesJsonSha256": FORMAL_V3_PROPERTIES_SHA256,
        "sourceBinding": {
            "commit": FORMAL_V3_SOURCE_COMMIT,
            "tree": FORMAL_V3_SOURCE_TREE,
            "sourceLockSha256": FORMAL_V3_SOURCE_LOCK_SHA256,
            "completeAppliedPatchOrder": [
                "0000", "0001", "0002", "0003", "0003a",
                "0004", "0005", "0006", "0007",
            ],
        },
        "buildResultSha256": FORMAL_V3_BUILD_RESULT_SHA256,
        "browserTreeManifestSha256": FORMAL_V3_RUNTIME_TREE_SHA256,
    }
    assert builder._package_asset_lock(
        {"build": _dual_boot_record()}, FORMAL_V3_RUNTIME_TREE_SHA256
    ) == expected_lock

    print("Camoufox Host package contract self-test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
