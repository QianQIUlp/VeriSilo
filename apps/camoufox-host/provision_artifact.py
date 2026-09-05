#!/usr/bin/env python3
"""Provision one strict identity Artifact from a short-lived request.

The input seed is process-local entropy for BrowserForge.  It is deliberately
never returned, persisted, placed in an exception, or written to a log.
"""

from __future__ import annotations

import base64
import copy
import hashlib
import ipaddress
import json
import math
import os
import random
import tempfile
from datetime import datetime, timezone
from importlib.metadata import version as dist_version
from pathlib import Path
from typing import Any, Callable

from host_runtime import (
    DownloadGuard,
    configure_camoufox_cache,
    ensure_browser_asset,
    firefox_user_prefs_for_config,
    install_download_guard,
    load_asset_lock,
    normalize_camou_config_env,
    seed_camoufox_cache,
)
from identity_policy import (
    ARTIFACT_SCHEMA_V5,
    ARTIFACT_SCHEMA_V6,
    GPC_POLICY_MANAGED_OPT_OUT,
    GPC_POLICY_NATIVE,
    GPC_CONFIG_KEY,
    ArtifactIntegrityError,
    apply_voices_policy,
    assert_artifact_clean,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    identity_policy,
    read_bundle_metadata,
    verify_artifact_raw,
    validate_artifact_strict,
)

PROVISION_FRAME_MAX_BYTES = 4096
PROVISION_REQUEST_KEYS = {
    "seed",
    "preset",
    "proxyServer",
    "window",
    "hardwareConcurrency",
    "followNetwork",
    "gpuPreset",
    "timezone",
}
GPU_PRESETS: dict[str, tuple[str, str]] = {
    "nvidia-rtx-3060": ("NVIDIA Corporation", "NVIDIA GeForce RTX 3060, or similar"),
    "nvidia-rtx-3070": ("NVIDIA Corporation", "NVIDIA GeForce RTX 3070, or similar"),
    "nvidia-rtx-4060": ("NVIDIA Corporation", "NVIDIA GeForce RTX 4060, or similar"),
    "nvidia-rtx-4070": ("NVIDIA Corporation", "NVIDIA GeForce RTX 4070, or similar"),
    "nvidia-rtx-4080": ("NVIDIA Corporation", "NVIDIA GeForce RTX 4080, or similar"),
    "nvidia-gtx-1660": ("NVIDIA Corporation", "NVIDIA GeForce GTX 1660, or similar"),
    "amd-rx-6600": ("ATI Technologies Inc.", "AMD Radeon RX 6600, or similar"),
    "amd-rx-7600": ("ATI Technologies Inc.", "AMD Radeon RX 7600, or similar"),
    "amd-rx-7800xt": ("ATI Technologies Inc.", "AMD Radeon RX 7800 XT, or similar"),
    "intel-uhd-770": ("Intel", "Intel(R) UHD Graphics 770, or similar"),
}
SUPPORTED_TIMEZONES = {
    "Asia/Shanghai",
    "Asia/Hong_Kong",
    "Asia/Tokyo",
    "Asia/Singapore",
    "Europe/London",
    "Europe/Berlin",
    "Europe/Paris",
    "America/New_York",
    "America/Chicago",
    "America/Los_Angeles",
    "UTC",
}
SUPPORTED_HARDWARE_CONCURRENCY = (2, 4, 6, 8, 12, 16)
PROVISION_PRESETS: dict[str, dict[str, Any]] = {
    "balanced-en-us": {
        "network": "direct",
        "locale": "en-US",
        "timezone": "America/New_York",
        "window": (1280, 800),
        "fontMode": "inherit",
        "voicesMode": "managed",
        "gpcPolicy": GPC_POLICY_MANAGED_OPT_OUT,
    },
    "balanced-zh-cn": {
        "network": "direct",
        "locale": "zh-CN",
        "timezone": "Asia/Shanghai",
        "window": (1280, 800),
        "fontMode": "inherit",
        "voicesMode": "managed",
        "gpcPolicy": GPC_POLICY_MANAGED_OPT_OUT,
    },
    "balanced-de-de": {
        "network": "direct",
        "locale": "de-DE",
        "timezone": "Europe/Berlin",
        "window": (1280, 800),
        "fontMode": "inherit",
        "voicesMode": "managed",
        "gpcPolicy": GPC_POLICY_MANAGED_OPT_OUT,
    },
    "match-fixed-proxy": {
        "network": "proxy",
        "locale": None,
        "timezone": None,
        "window": (1280, 800),
        "fontMode": "inherit",
        "voicesMode": "managed",
        "gpcPolicy": GPC_POLICY_MANAGED_OPT_OUT,
    },
}

LOOPBACK_PROXY_PREFIX = "socks5://127.0.0.1:"
OBSERVATION_PROXY_PREFIX = "socks5h://127.0.0.1:"


class ProvisionError(ValueError):
    """A request cannot produce a strict Artifact."""


def decode_seed(value: object) -> bytes:
    """Accept exactly 32 bytes as base64 or lowercase/uppercase hex text."""

    if isinstance(value, bytes):
        seed = value
    elif type(value) is str:
        text = value.strip()
        seed = b""
        if len(text) == 64:
            try:
                seed = bytes.fromhex(text)
            except ValueError:
                seed = b""
        if len(seed) != 32:
            try:
                seed = base64.b64decode(text, validate=True)
            except (ValueError, base64.binascii.Error):
                seed = b""
    elif type(value) is list and len(value) == 32 and all(type(item) is int and 0 <= item <= 255 for item in value):
        seed = bytes(value)
    else:
        seed = b""
    if len(seed) != 32:
        raise ProvisionError("seed must encode exactly 32 bytes")
    return seed


def parse_window(value: object, default: tuple[int, int]) -> tuple[int, int]:
    if value is None:
        return default
    if type(value) is not list or len(value) != 2:
        raise ProvisionError("window must be two positive integers")
    width, height = value
    if type(width) is not int or type(height) is not int:
        raise ProvisionError("window must be two positive integers")
    if not 800 <= width <= 7680 or not 600 <= height <= 4320:
        raise ProvisionError("window is out of range")
    return (width, height)


def parse_hardware_concurrency(value: object) -> int | None:
    if value is None:
        return None
    if type(value) is not int or value not in SUPPORTED_HARDWARE_CONCURRENCY:
        raise ProvisionError("hardwareConcurrency is not a supported core count")
    return value


def parse_gpu_preset(value: object) -> tuple[str, str] | None:
    if value is None or value == "auto" or value == "":
        return None
    if type(value) is not str or value not in GPU_PRESETS:
        raise ProvisionError("gpuPreset is not one of the supported GPU profiles")
    return GPU_PRESETS[value]


def parse_timezone(value: object) -> str | None:
    if value is None or value == "":
        return None
    if type(value) is not str or value not in SUPPORTED_TIMEZONES:
        raise ProvisionError("timezone is not one of the supported IANA zones")
    return value


def apply_identity_overrides(
    config: dict[str, Any],
    *,
    window: tuple[int, int],
    hardware_concurrency: int | None,
    gpu: tuple[str, str] | None = None,
) -> None:
    width, height = window
    config["screen.width"] = width
    config["screen.height"] = height
    config["screen.availWidth"] = width
    config["screen.availHeight"] = height
    config["window.outerWidth"] = width
    config["window.outerHeight"] = height
    if hardware_concurrency is not None:
        config["navigator.hardwareConcurrency"] = hardware_concurrency
    if gpu is not None:
        config["webGl:vendor"] = gpu[0]
        config["webGl:renderer"] = gpu[1]


def _artifact_id(
    seed: bytes,
    preset: str,
    *,
    window: tuple[int, int],
    hardware_concurrency: int | None,
    follow_network: bool,
    gpu_preset: str | None = None,
    timezone: str | None = None,
    network: dict[str, Any] | None = None,
) -> str:
    binding: dict[str, Any] = {
        "preset": preset,
        "window": [window[0], window[1]],
        "hardwareConcurrency": hardware_concurrency,
        "followNetwork": follow_network,
        "gpuPreset": gpu_preset,
        "timezone": timezone,
    }
    if network is not None:
        binding["network"] = {
            key: network[key]
            for key in (
                "expectedPublicAddress",
                "countryCode",
                "timezone",
                "latitude",
                "longitude",
            )
        }
    material = json.dumps(binding, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode(
        "utf-8"
    )
    return "identity-" + hashlib.sha256(material + seed).hexdigest()[:24]


def _reassemble_camou_config(env: dict) -> dict:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    if not chunks:
        raise ProvisionError("browser configuration was not returned")
    try:
        value = json.loads("".join(chunk for _, chunk in chunks))
    except json.JSONDecodeError as exc:
        raise ProvisionError("browser configuration was not valid JSON") from exc
    if type(value) is not dict:
        raise ProvisionError("browser configuration must be an object")
    return value


def _canonical_locale_parts(locale: str) -> tuple[str, str, str]:
    from camoufox.locales import normalize_locale

    normalized = normalize_locale(locale)
    if normalized.language is None or normalized.region is None:
        raise ProvisionError("preset locale is not canonical")
    script = normalized.script
    if script is None:
        language = normalized.language.lower()
        region = normalized.region.upper()
        if language == "zh":
            script = "Hant" if region in {"TW", "HK", "MO"} else "Hans"
        elif language == "ja":
            script = "Jpan"
        elif language == "ko":
            script = "Kore"
        else:
            script = "Latn"
    return normalized.language, normalized.region, script


def complete_resolved_config(
    executable: Path,
    *,
    target_os: str,
    window: tuple[int, int],
    locale: str,
    ff_version: int,
    timezone: str = "UTC",
) -> dict:
    from camoufox import DefaultAddons
    from camoufox.utils import launch_options

    first = launch_options(
        os=target_os,
        window=window,
        locale=locale,
        ff_version=ff_version,
        headless=False,
        executable_path=str(executable),
        exclude_addons=[DefaultAddons.UBO],
        i_know_what_im_doing=True,
    )
    config = _reassemble_camou_config(first["env"])
    for key in (
        "navigator.maxTouchPoints",
        "navigator.doNotTrack",
        "navigator.globalPrivacyControl",
    ):
        config.pop(key, None)
    config.setdefault("screen.availTop", 0)
    config.setdefault("screen.availLeft", 0)
    config["timezone"] = timezone
    for _ in range(5):
        opts = launch_options(
            config=copy.deepcopy(config),
            os=target_os,
            window=window,
            locale=locale,
            ff_version=ff_version,
            headless=False,
            executable_path=str(executable),
            exclude_addons=[DefaultAddons.UBO],
            i_know_what_im_doing=True,
        )
        sent = _reassemble_camou_config(opts["env"])
        _normalized, diff, _ = normalize_camou_config_env(opts["env"], config)
        # Normalize only removes known BrowserForge extras; all actual config
        # changes remain a hard failure at this trust boundary.
        if diff["removed"] or diff["changed"]:
            raise ProvisionError("browser configuration changed an existing value")
        if diff["added"]:
            for key in diff["added"]:
                config[key] = sent[key]
            continue
        break
    else:
        raise ProvisionError("browser configuration did not reach a fixpoint")
    if isinstance(config.get("screen.height"), int) and isinstance(config.get("screen.availHeight"), int):
        config["screen.availTop"] = min(
            config.get("screen.availTop", 0),
            max(0, config["screen.height"] - config["screen.availHeight"]),
        )
    if isinstance(config.get("screen.width"), int) and isinstance(config.get("screen.availWidth"), int):
        config["screen.availLeft"] = min(
            config.get("screen.availLeft", 0),
            max(0, config["screen.width"] - config["screen.availWidth"]),
        )
    config["window.outerWidth"], config["window.outerHeight"] = window
    language, region, script = _canonical_locale_parts(locale)
    config["locale:language"] = language
    config["locale:region"] = region
    config["locale:script"] = script
    return config


def _declared_stable_signals(config: dict, *, include_dpr: bool = False) -> dict:
    declared = {
        "userAgent": config["navigator.userAgent"],
        "language": config.get("navigator.language") or f"{config['locale:language']}-{config['locale:region']}",
        "screen": {
            key: config[f"screen.{key}"]
            for key in (
                "width",
                "height",
                "availWidth",
                "availHeight",
                "availTop",
                "availLeft",
                "colorDepth",
                "pixelDepth",
            )
        },
        "hardwareConcurrency": config["navigator.hardwareConcurrency"],
        "canvasSeed": config["canvas:seed"],
        "audioSeed": config["audio:seed"],
        "fontSpacingSeed": config["fonts:spacing_seed"],
        "webglVendor": config["webGl:vendor"],
        "webglRenderer": config["webGl:renderer"],
        "fonts": list(config["fonts"]),
    }
    if include_dpr:
        declared["devicePixelRatio"] = 1
    if "voices" in config:
        declared["voices"] = list(config["voices"])
    return declared


def _network_identity_from_ipwhois(
    proxy_server: str,
    *,
    fetch: Callable[[str, str], dict[str, Any]] | None = None,
) -> dict[str, Any]:
    if type(proxy_server) is not str or not proxy_server.startswith(LOOPBACK_PROXY_PREFIX):
        raise ProvisionError("proxyServer must be a loopback SOCKS5 endpoint")
    port = proxy_server[len(LOOPBACK_PROXY_PREFIX) :]
    if not port.isdigit() or not 1 <= int(port) <= 65535:
        raise ProvisionError("proxyServer must be a loopback SOCKS5 endpoint")
    observation_proxy = OBSERVATION_PROXY_PREFIX + port
    if fetch is None:
        try:
            import requests
        except ImportError as exc:  # pragma: no cover - package dependency issue
            raise ProvisionError("network observation dependency is unavailable") from exc

        def fetch(url: str, proxy: str) -> dict[str, Any]:
            session = requests.Session()
            session.trust_env = False
            response = session.get(
                url,
                proxies={"http": proxy, "https": proxy},
                timeout=10,
            )
            response.raise_for_status()
            value = response.json()
            return value if type(value) is dict else {}

    try:
        raw = fetch("https://ipwho.is/", observation_proxy)
    except Exception as exc:  # noqa: BLE001 - do not expose endpoint/credentials
        raise ProvisionError("ipwho.is observation failed") from exc
    if raw.get("success") is False:
        raise ProvisionError("ipwho.is observation failed")
    try:
        address = ipaddress.ip_address(raw["ip"])
        country = raw["country_code"]
        timezone_name = raw["timezone"]["id"]
        latitude = float(raw["latitude"])
        longitude = float(raw["longitude"])
    except (KeyError, TypeError, ValueError) as exc:
        raise ProvisionError("ipwho.is observation was incomplete") from exc
    if not address.is_global or address.is_multicast:
        raise ProvisionError("ipwho.is observation was not a global address")
    if type(country) is not str or len(country) != 2 or not country.isascii() or not country.isupper():
        raise ProvisionError("ipwho.is country code was invalid")
    if not math.isfinite(latitude) or not -90 <= latitude <= 90 or not math.isfinite(longitude) or not -180 <= longitude <= 180:
        raise ProvisionError("ipwho.is coordinates were invalid")
    return {
        "expectedPublicAddress": str(address),
        "countryCode": country,
        "timezone": timezone_name,
        "latitude": latitude,
        "longitude": longitude,
    }


def _atomic_first_writer(path: Path, raw: bytes) -> bool:
    """Publish one complete file without replacing an existing winner."""

    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        try:
            os.link(temporary, path)
        except FileExistsError:
            return False
        return True
    finally:
        try:
            os.unlink(temporary)
        except FileNotFoundError:
            pass


def _ensure_artifact_sidecar(path: Path, raw: bytes | None = None) -> None:
    if path.is_symlink() or not path.is_file():
        raise ProvisionError("existing artifact is not a regular file")
    raw = path.read_bytes() if raw is None else raw
    expected = f"{hashlib.sha256(raw).hexdigest()}  {path.name}\n".encode("ascii")
    sidecar = path.with_suffix(path.suffix + ".sha256")
    if sidecar.is_symlink():
        raise ProvisionError("existing artifact sidecar is not a regular file")
    if sidecar.exists():
        if not sidecar.is_file() or sidecar.read_bytes() != expected:
            raise ProvisionError("existing artifact sidecar does not match artifact bytes")
        return
    _atomic_first_writer(sidecar, expected)
    if sidecar.is_symlink() or not sidecar.is_file() or sidecar.read_bytes() != expected:
        raise ProvisionError("artifact sidecar was not published atomically")


def _validated_existing_result(path: Path, artifact_id: str) -> dict[str, Any]:
    if path.is_symlink() or not path.is_file():
        raise ProvisionError("existing artifact is not a regular file")
    _ensure_artifact_sidecar(path)
    try:
        artifact, file_sha = verify_artifact_raw(path)
    except (ArtifactIntegrityError, OSError, ValueError) as exc:
        raise ProvisionError("existing artifact failed strict validation") from exc
    if artifact.get("artifactId") != artifact_id:
        raise ProvisionError("existing artifact binding does not match the request")
    return {
        "artifactId": artifact_id,
        "artifactFileSha256": file_sha,
        "schema": artifact["schema"],
        "configuredIdentityDigest": artifact["configuredIdentityDigest"],
    }


def _v6_rebind(
    source: dict,
    artifact_id: str,
    locale: str,
    network: dict[str, Any],
    *,
    timezone_mode: str = "network-bound",
) -> dict:
    language, region, script = _canonical_locale_parts(locale)
    from camoufox.locales import normalize_locale

    normalized_locale = normalize_locale(locale)
    config = copy.deepcopy(source["resolvedConfig"])
    for key in tuple(config):
        if key.startswith("geolocation:") or key.startswith("webrtc:ipv"):
            config.pop(key)
    config.update(
        {
            "timezone": network["timezone"],
            "locale:language": language,
            "locale:region": region,
            "locale:script": script,
            "geolocation:latitude": float(network["latitude"]),
            "geolocation:longitude": float(network["longitude"]),
            f"webrtc:ipv{ipaddress.ip_address(network['expectedPublicAddress']).version}": network["expectedPublicAddress"],
        }
    )
    artifact = copy.deepcopy(source)
    artifact.update(
        {
            "schema": ARTIFACT_SCHEMA_V6,
            "artifactId": artifact_id,
            "generatedBy": "VeriSilo Camoufox Host provision-artifact (Artifact v6)",
            "generatedAtUtc": datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z"),
            "networkIdentity": {
                "expectedPublicAddress": network["expectedPublicAddress"],
                "countryCode": network["countryCode"],
                "timezone": network["timezone"],
                "locale": normalized_locale.as_string,
                "latitude": float(network["latitude"]),
                "longitude": float(network["longitude"]),
            },
            "resolvedConfig": config,
        }
    )
    policy = artifact["policy"]
    artifact["policy"] = identity_policy(
        target_os=policy["targetOs"],
        font_mode=policy["fontMode"],
        window=tuple(policy["window"]),
        locale=normalized_locale.as_string,
        ff_version=policy["ffVersion"],
        timezone_mode=timezone_mode,
        browser_binding=artifact["browserBinding"],
        voices_mode=policy["voicesMode"],
        gpc_policy=policy["navigator.gpcPolicy"],
        schema_version=6,
        network_ip_version=ipaddress.ip_address(network["expectedPublicAddress"]).version,
    )
    artifact["stableSignalsDeclared"] = _declared_stable_signals(config)
    artifact["configuredIdentityDigest"] = configured_identity_digest(config)
    artifact.pop("canonicalDigest", None)
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    validate_artifact_strict(artifact)
    assert_artifact_clean(artifact)
    return artifact


def provision_artifact(
    request: dict[str, Any],
    *,
    package_root: Path,
    artifact_root: Path,
    cache_root: Path,
    ipwhois_fetch: Callable[[str, str], dict[str, Any]] | None = None,
) -> dict[str, Any]:
    if type(request) is not dict or set(request) - PROVISION_REQUEST_KEYS:
        raise ProvisionError("provision request fields are not exact")
    if "seed" not in request or "preset" not in request:
        raise ProvisionError("provision request fields are not exact")
    preset_name = request.get("preset")
    if type(preset_name) is not str or preset_name not in PROVISION_PRESETS:
        raise ProvisionError("preset is not one of the four fixed presets")
    seed = decode_seed(request.get("seed"))
    preset = PROVISION_PRESETS[preset_name]
    window = parse_window(request.get("window"), tuple(preset["window"]))
    hardware_concurrency = parse_hardware_concurrency(request.get("hardwareConcurrency"))
    gpu = parse_gpu_preset(request.get("gpuPreset"))
    gpu_preset_name = request.get("gpuPreset") if gpu is not None else None
    timezone_override = parse_timezone(request.get("timezone"))
    follow_network = request.get("followNetwork")
    if follow_network is None:
        follow_network = preset["network"] == "proxy"
    elif type(follow_network) is not bool:
        raise ProvisionError("followNetwork must be a boolean")
    has_proxy = "proxyServer" in request
    if follow_network and not has_proxy:
        raise ProvisionError("followNetwork requires a proxy endpoint")
    if has_proxy and not follow_network and preset_name == "match-fixed-proxy":
        raise ProvisionError("match-fixed-proxy requires followNetwork")
    if not has_proxy and preset_name == "match-fixed-proxy":
        raise ProvisionError("match-fixed-proxy provisioning requires a proxy endpoint")
    artifact_root = Path(artifact_root)
    if not has_proxy:
        artifact_id = _artifact_id(
            seed,
            preset_name,
            window=window,
            hardware_concurrency=hardware_concurrency,
            follow_network=False,
            gpu_preset=gpu_preset_name if type(gpu_preset_name) is str else None,
            timezone=timezone_override,
        )
        artifact_root.mkdir(parents=True, exist_ok=True)
        path = artifact_root / f"{artifact_id}.json"
        if path.exists() or path.is_symlink():
            return _validated_existing_result(path, artifact_id)
    # Keep the raw seed out of all objects after deterministic PRNG setup.
    random.seed(int.from_bytes(seed, "big"))
    try:
        import numpy as np

        np.random.seed(int.from_bytes(seed[:4], "big"))
    except ImportError as exc:  # pragma: no cover - runtime dependency is pinned
        raise ProvisionError("identity generation dependency is unavailable") from exc
    lock = load_asset_lock(package_root=package_root)
    executable = ensure_browser_asset(
        lock,
        allow_download=False,
        browser_root=package_root / "browser",
        tree_manifest=package_root / "browser-tree-manifest.json",
        verify_tree_contents=False,
    )
    install_dir = configure_camoufox_cache(cache_root)
    seed_camoufox_cache(lock, executable, install_dir=install_dir)
    install_download_guard()
    DownloadGuard.reset()
    locale = preset["locale"] or "en-US"
    timezone_name = timezone_override or preset["timezone"] or "UTC"
    config = complete_resolved_config(
        executable,
        target_os="windows",
        window=window,
        locale=locale,
        ff_version=152,
        timezone=timezone_name,
    )
    apply_identity_overrides(
        config,
        window=window,
        hardware_concurrency=hardware_concurrency,
        gpu=gpu,
    )
    apply_voices_policy(config, preset["voicesMode"])
    config.pop("navigator.doNotTrack", None)
    if preset["gpcPolicy"] == GPC_POLICY_MANAGED_OPT_OUT:
        config[GPC_CONFIG_KEY] = True
    else:
        config.pop(GPC_CONFIG_KEY, None)
    metadata = read_bundle_metadata(executable)
    binding = {
        "archiveSha256": lock["sha256"],
        "archiveSizeBytes": lock["sizeBytes"],
        "buildId": metadata["buildId"],
        "sourceStamp": metadata["sourceStamp"],
        "propertiesJsonSha256": metadata["propertiesJsonSha256"],
    }
    network: dict[str, Any] | None = None
    if has_proxy:
        proxy_server = request.get("proxyServer")
        network = _network_identity_from_ipwhois(proxy_server, fetch=ipwhois_fetch)
        if follow_network:
            try:
                from camoufox.locales import StatisticalLocaleSelector

                # NumPy was seeded from the Silo seed above, so the locale choice
                # is stable while still respecting the observed exit country.
                network["locale"] = StatisticalLocaleSelector().from_region(
                    network["countryCode"]
                ).as_string
            except Exception as exc:  # noqa: BLE001 - country/locale data boundary
                raise ProvisionError(
                    "no supported locale matched the observed proxy country"
                ) from exc
        else:
            network["locale"] = locale
        artifact_id = _artifact_id(
            seed,
            preset_name,
            window=window,
            hardware_concurrency=hardware_concurrency,
            follow_network=follow_network,
            gpu_preset=gpu_preset_name if type(gpu_preset_name) is str else None,
            timezone=None if follow_network else timezone_override,
            network=network,
        )
        artifact_root.mkdir(parents=True, exist_ok=True)
        path = artifact_root / f"{artifact_id}.json"
        if path.exists() or path.is_symlink():
            return _validated_existing_result(path, artifact_id)
    else:
        artifact_id = _artifact_id(
            seed,
            preset_name,
            window=window,
            hardware_concurrency=hardware_concurrency,
            follow_network=False,
            gpu_preset=gpu_preset_name if type(gpu_preset_name) is str else None,
            timezone=timezone_override,
        )
    artifact = {
        "schema": ARTIFACT_SCHEMA_V5,
        "artifactId": artifact_id,
        "policy": identity_policy(
            target_os="windows",
            font_mode=preset["fontMode"],
            window=window,
            locale=locale,
            ff_version=152,
            timezone_mode="fixed",
            browser_binding=binding,
            voices_mode=preset["voicesMode"],
            gpc_policy=preset["gpcPolicy"],
        ),
        "browserRelease": lock["release"],
        "browserBinding": binding,
        "generatedBy": "VeriSilo Camoufox Host provision-artifact (Artifact v5)",
        "generatedAtUtc": datetime.now(timezone.utc).isoformat(timespec="microseconds").replace("+00:00", "Z"),
        "generatorVersions": {
            "camoufox": dist_version("camoufox"),
            "playwright": dist_version("playwright"),
            "browserforge": dist_version("browserforge"),
        },
        "resolvedConfig": config,
        "stableSignalsDeclared": _declared_stable_signals(config),
        "configuredIdentityDigest": configured_identity_digest(config),
        "exclusions": {
            "profilePath": "not recorded",
            "display": "not recorded",
            "tokens": "none supplied",
            "proxySecrets": "none supplied",
            "environment": "not recorded",
        },
    }
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    if network is not None:
        if not follow_network:
            network = {**network, "timezone": timezone_name}
        artifact = _v6_rebind(artifact, artifact_id, network["locale"], network)
        apply_identity_overrides(
            artifact["resolvedConfig"],
            window=window,
            hardware_concurrency=hardware_concurrency,
            gpu=gpu,
        )
        artifact["stableSignalsDeclared"] = _declared_stable_signals(artifact["resolvedConfig"])
        artifact["configuredIdentityDigest"] = configured_identity_digest(
            artifact["resolvedConfig"]
        )
        artifact.pop("canonicalDigest", None)
        artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    if DownloadGuard.tripped:
        raise ProvisionError("unpinned browser download attempted")
    validate_artifact_strict(artifact)
    assert_artifact_clean(artifact)
    path = artifact_root / f"{artifact_id}.json"
    raw = (json.dumps(artifact, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    sidecar = f"{hashlib.sha256(raw).hexdigest()}  {path.name}\n".encode("ascii")
    sidecar_path = path.with_suffix(path.suffix + ".sha256")
    if sidecar_path.exists() or sidecar_path.is_symlink():
        if sidecar_path.is_symlink() or not sidecar_path.is_file() or sidecar_path.read_bytes() != sidecar:
            raise ProvisionError("existing artifact sidecar does not match generated bytes")
    if not _atomic_first_writer(path, raw):
        return _validated_existing_result(path, artifact_id)
    _ensure_artifact_sidecar(path, raw)
    return _validated_existing_result(path, artifact_id)
