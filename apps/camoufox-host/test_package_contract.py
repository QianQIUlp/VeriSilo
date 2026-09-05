#!/usr/bin/env python3
"""Small dependency-free checks for the package/provision seams."""

from __future__ import annotations

import base64
import io
import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from package_contract import (  # noqa: E402
    FORMAL_V3_ARCHIVE_SHA256,
    PackageLayout,
    PackageContractError,
    build_package_tree,
    manifest_signing_payload,
    recheck_package,
    safe_relative_path,
    sha256_bytes,
    validate_v3_manifest,
)
from browser_tree import TreeIntegrityError, build_tree_manifest, verify_tree  # noqa: E402
from provision_artifact import (  # noqa: E402
    PROVISION_PRESETS,
    PROVISION_REQUEST_KEYS,
    _artifact_id,
    parse_gpu_preset,
    parse_hardware_concurrency,
    parse_timezone,
    parse_window,
    _artifact_id,
    _atomic_first_writer,
    _network_identity_from_ipwhois,
    decode_seed,
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
    assert len(PROVISION_PRESETS) == 4
    assert set(PROVISION_PRESETS) == {
        "balanced-en-us",
        "balanced-zh-cn",
        "balanced-de-de",
        "match-fixed-proxy",
    }
    layout = PackageLayout.from_root("package")
    assert layout.asset_lock.name == "runtime-asset-lock.json"
    assert layout.supervisor.as_posix().endswith("host/verisilo-camoufox-supervisor.exe")
    assert layout.probe.as_posix().endswith("host/probe/probe.html")
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
        assert verify_tree(root, manifest)["verified"] is True
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
        layout.browser_tree.write_bytes(b"tree")
        for index in range(1500):
            (layout.browser_root / f"payload-{index:04d}.bin").write_bytes(b"x" * 64)
        tree_raw = (
            json.dumps(build_package_tree(root), indent=2, ensure_ascii=False) + "\n"
        ).encode("utf-8")
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
        assert recheck_package(root, large_manifest)["memberCount"] > 1500
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
    print("Camoufox Host package contract self-test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
