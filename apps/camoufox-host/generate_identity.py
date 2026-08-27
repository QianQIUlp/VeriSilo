#!/usr/bin/env python3
"""VeriSilo M1.1: generate one resolved Camoufox identity artifact.

Generation uses the SAME offline discipline as M0 replay:
- the M0-verified archive (ensure_browser_asset with allow_download=False);
- the controlled spike XDG_CACHE_HOME cache, seeded from the verified archive;
- an explicit executable_path;
- the webdl DownloadGuard installed before any launch_options() call, so an
  empty cache can never trigger a network fetch.

The resolved config is completed to a fixpoint: launch_options() is replayed
until the sent CAMOU_CONFIG adds no new keys (the first capture can miss keys
that only some random BrowserForge fingerprints carry, e.g.
navigator.globalPrivacyControl / screen.availTop / screen.availLeft). The
artifact then binds the config to the browser archive (archive SHA, BuildID,
SourceStamp, properties.json SHA) and records configuredIdentityDigest.

No profile path, display, token, or proxy secret is stored. The optional
--rng-seed is a fixture-only input and is NOT the Silo seed; it never enters
the browser process.
"""

from __future__ import annotations

import argparse
import copy
import ipaddress
import json
import math
import os
import random
from datetime import datetime, timezone
from importlib.metadata import version as dist_version
from pathlib import Path

import numpy as np

from identity_policy import (
    ARTIFACT_SCHEMA,
    ARTIFACT_SCHEMA_V5,
    ARTIFACT_SCHEMA_V6,
    ArtifactIntegrityError,
    GPC_POLICY_KEY,
    GPC_POLICY_MANAGED_OPT_OUT,
    GPC_POLICY_NATIVE,
    VOICES_MODE_MANAGED,
    VOICES_MODE_NATIVE,
    apply_voices_policy,
    assert_artifact_clean,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    identity_policy,
    _canvas_policy_fields_for_browser_binding,
    read_bundle_metadata,
    sha256_hex,
    validate_artifact_strict,
    verify_artifact,
)
from run_spike import (
    CANDIDATE_EXTRA_IDENTITY_FIELDS,
    classify_candidate_identity_fields,
    configure_camoufox_cache,
    DownloadGuard,
    RELEASE,
    XDG_CACHE_DIR,
    ensure_browser_asset,
    install_download_guard,
    load_asset_lock,
    seed_camoufox_cache,
)

TIMEZONE_MODE = "fixed"
TIMEZONE_VALUE = "UTC"


def reassemble_camou_config(env: dict) -> dict:
    chunks = sorted(
        (int(key.rsplit("_", 1)[1]), value)
        for key, value in env.items()
        if key.startswith("CAMOU_CONFIG_")
    )
    if not chunks:
        raise RuntimeError("launch_options returned no CAMOU_CONFIG env chunks")
    return json.loads("".join(value for _, value in chunks))


def artifact_json_bytes(artifact: dict) -> bytes:
    """Serialize tracked Artifacts as deterministic UTF-8/LF/no-BOM bytes."""

    return (json.dumps(artifact, indent=2, ensure_ascii=False) + "\n").encode(
        "utf-8"
    )


def write_artifact_with_sidecar(path: Path, artifact: dict) -> str:
    """Write one exact byte payload and hash those same bytes for its sidecar."""

    raw = artifact_json_bytes(artifact)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(raw)
    digest = sha256_hex(raw)
    path.with_suffix(path.suffix + ".sha256").write_bytes(
        f"{digest}  {path.name}\n".encode("ascii")
    )
    return digest


def complete_resolved_config(
    executable: Path,
    target_os: str,
    window: tuple[int, int],
    locale: str,
    ff_version: int,
) -> dict:
    """Capture one resolved config, then replay launch_options() to a fixpoint
    so the config contains every key Camoufox/BrowserForge could ever add.
    Raises if launch_options ever CHANGES an existing value."""
    from camoufox import DefaultAddons
    from camoufox.utils import launch_options

    width, height = window
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
    config = reassemble_camou_config(first["env"])
    initial_candidate_fields = sorted(
        set(config).intersection(CANDIDATE_EXTRA_IDENTITY_FIELDS)
    )
    initial_candidate_audit = classify_candidate_identity_fields(
        config, initial_candidate_fields
    )
    for key in initial_candidate_audit:
        config.pop(key)
    # Fixed identity policy: timezone is bound to the artifact, not the host.
    config["timezone"] = TIMEZONE_VALUE

    for _ in range(5):
        candidate = copy.deepcopy(config)
        opts = launch_options(
            config=candidate,
            os=target_os,
            window=window,
            locale=locale,
            ff_version=ff_version,
            headless=False,
            executable_path=str(executable),
            exclude_addons=[DefaultAddons.UBO],
            i_know_what_im_doing=True,
        )
        sent = reassemble_camou_config(opts["env"])
        # These candidate-only browser fields are classified and type-checked
        # before removal. They are not part of the v3 Artifact fixpoint.
        candidate_fields = sorted(
            (set(sent) - set(config)).intersection(
                CANDIDATE_EXTRA_IDENTITY_FIELDS
            )
        )
        candidate_audit = classify_candidate_identity_fields(
            sent, candidate_fields
        )
        for key in candidate_audit:
            sent.pop(key, None)
        diff = diff_configs(config, sent)
        if diff["added"]:
            for key in diff["added"]:
                config[key] = sent[key]
            continue
        if diff["removed"] or diff["changed"]:
            raise RuntimeError(
                "launch_options mutated existing config values: " + json.dumps(diff)
            )
        break
    else:
        raise RuntimeError("resolved config did not reach a fixpoint after 5 replays")

    # Keep geometry self-consistent: availTop+availHeight <= height and
    # availLeft+availWidth <= width (strict validation enforces this).
    if isinstance(config.get("screen.height"), int) and isinstance(
        config.get("screen.availHeight"), int
    ):
        max_top = max(0, config["screen.height"] - config["screen.availHeight"])
        config["screen.availTop"] = min(config.get("screen.availTop", 0), max_top)
    if isinstance(config.get("screen.width"), int) and isinstance(
        config.get("screen.availWidth"), int
    ):
        max_left = max(0, config["screen.width"] - config["screen.availWidth"])
        config["screen.availLeft"] = min(config.get("screen.availLeft", 0), max_left)
    # The outer window is bound to the policy window; BrowserForge may
    # otherwise emit different outer dimensions for some seeds, which strict
    # validation (policy.window == outer dims) would reject.
    config["window.outerWidth"] = width
    config["window.outerHeight"] = height
    return config


def browser_binding(lock: dict, executable: Path) -> dict:
    metadata = read_bundle_metadata(executable)
    return {
        "archiveSha256": lock["sha256"],
        "archiveSizeBytes": lock["sizeBytes"],
        "buildId": metadata["buildId"],
        "sourceStamp": metadata["sourceStamp"],
        "propertiesJsonSha256": metadata["propertiesJsonSha256"],
    }


def declared_stable_signals(
    config: dict, locale: str, *, include_device_pixel_ratio: bool = True
) -> dict:
    language = (
        config.get("navigator.language")
        or f"{config['locale:language']}-{config['locale:region']}"
    )
    screen = {
        "width": config["screen.width"],
        "height": config["screen.height"],
        "availWidth": config["screen.availWidth"],
        "availHeight": config["screen.availHeight"],
        "availTop": config["screen.availTop"],
        "availLeft": config["screen.availLeft"],
        "colorDepth": config["screen.colorDepth"],
        "pixelDepth": config["screen.pixelDepth"],
    }
    declared = {
        "userAgent": config["navigator.userAgent"],
        "language": language or locale,
        "screen": screen,
        "hardwareConcurrency": config["navigator.hardwareConcurrency"],
        "canvasSeed": config["canvas:seed"],
        "audioSeed": config["audio:seed"],
        "fontSpacingSeed": config["fonts:spacing_seed"],
        "webglVendor": config["webGl:vendor"],
        "webglRenderer": config["webGl:renderer"],
        "fonts": list(config["fonts"]),
    }
    if include_device_pixel_ratio:
        declared["devicePixelRatio"] = 1
    if "voices" in config:
        declared["voices"] = list(config["voices"])
    return declared


def rebind_identity_artifact(
    source: dict,
    *,
    artifact_id: str,
    binding: dict,
    canvas_seed: int | None = None,
) -> dict:
    """Create a deterministic fixture/candidate rebind without browser I/O.

    Provenance fields and the historical resolved identity are preserved. The
    browser binding selects the Canvas Policy v3 variant. ``canvas_seed`` is
    optional so a focused contrast can change only that config value, its
    declaration, and the two derived digests beyond the ordinary rebind.
    """

    if canvas_seed is not None and (
        type(canvas_seed) is not int or not 0 <= canvas_seed <= 0xFFFFFFFF
    ):
        raise ValueError("canvas_seed must be an unsigned 32-bit integer")

    artifact = copy.deepcopy(source)
    source_policy = source["policy"]
    artifact["artifactId"] = artifact_id
    artifact["browserBinding"] = copy.deepcopy(binding)
    artifact["policy"] = copy.deepcopy(source_policy)
    session_variable_fields, canvas_classification = (
        _canvas_policy_fields_for_browser_binding(binding)
    )
    artifact["policy"]["sessionVariableFields"] = session_variable_fields
    artifact["policy"]["canvasClassification"] = canvas_classification
    if canvas_seed is not None:
        artifact["resolvedConfig"]["canvas:seed"] = canvas_seed
        artifact["stableSignalsDeclared"]["canvasSeed"] = canvas_seed
    artifact["configuredIdentityDigest"] = configured_identity_digest(
        artifact["resolvedConfig"]
    )
    artifact.pop("canonicalDigest", None)
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    assert_artifact_clean(artifact)
    return artifact


def rebind_network_identity_artifact(
    source: dict,
    *,
    artifact_id: str,
    network_identity: dict,
    generated_at_utc: str,
) -> dict:
    """Create one deterministic, configured-only network-bound Artifact v6."""

    validate_artifact_strict(source)
    if source.get("schema") != ARTIFACT_SCHEMA_V5:
        raise ValueError("network identity rebind requires an Artifact v5 source")
    if source.get("canonicalDigest") != compute_artifact_digest(source):
        raise ArtifactIntegrityError("source artifact canonicalDigest mismatch")
    required = {
        "expectedPublicAddress",
        "countryCode",
        "timezone",
        "locale",
        "latitude",
        "longitude",
    }
    if type(network_identity) is not dict or set(network_identity) != required:
        raise ValueError("network_identity must contain the exact v6 field set")

    address_input = network_identity["expectedPublicAddress"]
    if type(address_input) is not str:
        raise ValueError("expectedPublicAddress must be an IP address string")
    try:
        address = ipaddress.ip_address(address_input)
    except (TypeError, ValueError) as exc:
        raise ValueError("expectedPublicAddress must be an IP address") from exc
    if not address.is_global or address.is_multicast:
        raise ValueError("expectedPublicAddress must be global unicast")

    coordinates: dict[str, float] = {}
    for key, lower, upper in (
        ("latitude", -90, 90),
        ("longitude", -180, 180),
    ):
        value = network_identity[key]
        if (
            type(value) not in (int, float)
            or not math.isfinite(value)
            or not lower <= value <= upper
        ):
            raise ValueError(f"{key} must be finite and in [{lower}, {upper}]")
        coordinates[key] = float(value)

    from camoufox.locales import normalize_locale

    locale = normalize_locale(network_identity["locale"])
    if locale.region is None or locale.script is None:
        raise ValueError("locale must resolve to language, region, and script")
    normalized_locale = locale.as_string

    artifact = copy.deepcopy(source)
    source_policy = source["policy"]
    config = artifact["resolvedConfig"]
    for key in tuple(config):
        if key.startswith("geolocation:") or key.startswith("webrtc:ipv"):
            config.pop(key)
    config.update(
        {
            "timezone": network_identity["timezone"],
            "locale:language": locale.language,
            "locale:region": locale.region,
            "locale:script": locale.script,
            "geolocation:latitude": coordinates["latitude"],
            "geolocation:longitude": coordinates["longitude"],
            f"webrtc:ipv{address.version}": str(address),
        }
    )
    artifact.update(
        {
            "schema": ARTIFACT_SCHEMA_V6,
            "artifactId": artifact_id,
            "generatedBy": "VeriSilo generate_identity.py (Artifact v6 network-bound rebind)",
            "generatedAtUtc": generated_at_utc,
            "networkIdentity": {
                "expectedPublicAddress": str(address),
                "countryCode": network_identity["countryCode"],
                "timezone": network_identity["timezone"],
                "locale": normalized_locale,
                **coordinates,
            },
        }
    )
    artifact["policy"] = identity_policy(
        target_os=source_policy["targetOs"],
        font_mode=source_policy["fontMode"],
        window=tuple(source_policy["window"]),
        locale=normalized_locale,
        ff_version=source_policy["ffVersion"],
        timezone_mode="network-bound",
        browser_binding=artifact["browserBinding"],
        voices_mode=source_policy["voicesMode"],
        gpc_policy=source_policy[GPC_POLICY_KEY],
        schema_version=6,
        network_ip_version=address.version,
    )
    artifact["stableSignalsDeclared"] = declared_stable_signals(
        config, normalized_locale, include_device_pixel_ratio=False
    )
    artifact["configuredIdentityDigest"] = configured_identity_digest(config)
    artifact.pop("canonicalDigest", None)
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)
    validate_artifact_strict(artifact)
    assert_artifact_clean(artifact)
    return artifact


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path, help="Output artifact path (.json)")
    parser.add_argument("--id", required=True, help="Artifact id, e.g. identity-a")
    parser.add_argument("--rng-seed", type=int, default=None, help="Reproducible generation seed (test-only)")
    parser.add_argument("--os", default="linux", choices=("linux", "macos", "windows"))
    parser.add_argument(
        "--font-mode",
        default="inherit",
        choices=("inherit", "managed"),
        help=(
            "inherit: font widths are host-bound and excluded from "
            "ObservedWebsiteDigest; managed: font widths enter the digest and "
            "Host must prove host negative controls are all unavailable"
        ),
    )
    parser.add_argument(
        "--voices-mode",
        default=VOICES_MODE_MANAGED,
        choices=(VOICES_MODE_MANAGED, VOICES_MODE_NATIVE),
    )
    parser.add_argument(
        "--gpc-policy",
        default=GPC_POLICY_MANAGED_OPT_OUT,
        choices=(GPC_POLICY_NATIVE, GPC_POLICY_MANAGED_OPT_OUT),
    )
    parser.add_argument("--window", default="1280x800", help="Outer window size WxH")
    parser.add_argument("--locale", default="en-US")
    parser.add_argument("--ff-version", type=int, default=152)
    parser.add_argument(
        "--cache-dir",
        type=Path,
        default=None,
        help="XDG cache root to use (default: M0 controlled cache). Pass an "
        "empty dir to prove generation never goes online on an empty cache.",
    )
    args = parser.parse_args()

    lock = load_asset_lock()
    if lock.get("digestAgreement") is not True:
        raise SystemExit(
            "asset lock digestAgreement is not true; refresh with fetch-browser.py --record --force"
        )
    executable = ensure_browser_asset(lock, allow_download=False)
    cache_dir = (args.cache_dir or XDG_CACHE_DIR).resolve()
    install_dir = configure_camoufox_cache(cache_dir)
    seed_camoufox_cache(lock, executable, install_dir=install_dir)
    install_download_guard()
    DownloadGuard.reset()

    if args.rng_seed is not None:
        random.seed(args.rng_seed)
        np.random.seed(args.rng_seed)

    width, height = (int(part) for part in args.window.lower().split("x"))
    config = complete_resolved_config(
        executable=executable,
        target_os=args.os,
        window=(width, height),
        locale=args.locale,
        ff_version=args.ff_version,
    )
    if DownloadGuard.tripped:
        raise SystemExit("unpinned download attempted during generation; aborting")
    apply_voices_policy(config, args.voices_mode)
    config.pop("navigator.doNotTrack", None)
    if args.gpc_policy == GPC_POLICY_MANAGED_OPT_OUT:
        config["navigator.globalPrivacyControl"] = True
    else:
        config.pop("navigator.globalPrivacyControl", None)

    binding = browser_binding(lock, executable)
    policy = identity_policy(
        target_os=args.os,
        font_mode=args.font_mode,
        window=(width, height),
        locale=args.locale,
        ff_version=args.ff_version,
        timezone_mode=TIMEZONE_MODE,
        browser_binding=binding,
        voices_mode=args.voices_mode,
        gpc_policy=args.gpc_policy,
    )
    artifact = {
        "schema": ARTIFACT_SCHEMA,
        "artifactId": args.id,
        "policy": policy,
        "browserRelease": RELEASE,
        "browserBinding": binding,
        "generatedBy": "VeriSilo generate_identity.py (Artifact v5)",
        "generatedAtUtc": datetime.now(timezone.utc)
        .isoformat()
        .replace("+00:00", "Z"),
        "generatorVersions": {
            "camoufox": dist_version("camoufox"),
            "playwright": dist_version("playwright"),
            "browserforge": dist_version("browserforge"),
        },
        "resolvedConfig": config,
        "stableSignalsDeclared": declared_stable_signals(
            config, args.locale, include_device_pixel_ratio=False
        ),
        "configuredIdentityDigest": configured_identity_digest(config),
        "exclusions": {
            "profilePath": "not recorded",
            "display": "not recorded",
            "tokens": "none supplied",
            "proxySecrets": "none supplied",
            "environment": "not recorded",
        },
    }
    assert_artifact_clean(artifact)
    artifact["canonicalDigest"] = compute_artifact_digest(artifact)

    write_artifact_with_sidecar(args.out, artifact)
    verify_artifact(args.out)  # strict validation must pass before handoff
    print(f"artifact written to {args.out}")
    print(f"canonicalDigest={artifact['canonicalDigest']}")
    print(f"configuredIdentityDigest={artifact['configuredIdentityDigest']}")
    print(f"config keys={len(config)} binding={artifact['browserBinding']['buildId']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
