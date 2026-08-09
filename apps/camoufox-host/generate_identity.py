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
import json
import os
import random
from datetime import datetime, timezone
from importlib.metadata import version as dist_version
from pathlib import Path

import numpy as np

from identity_policy import (
    ARTIFACT_SCHEMA,
    assert_artifact_clean,
    compute_artifact_digest,
    configured_identity_digest,
    diff_configs,
    identity_policy,
    read_bundle_metadata,
    sha256_hex,
    verify_artifact,
)
from run_spike import (
    configure_camoufox_cache,
    DownloadGuard,
    RUNTIME_ONLY_CONFIG_KEYS,
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
        for key in RUNTIME_ONLY_CONFIG_KEYS:
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


def declared_stable_signals(config: dict, locale: str) -> dict:
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
    return {
        "userAgent": config["navigator.userAgent"],
        "language": language or locale,
        "screen": screen,
        "devicePixelRatio": 1,
        "hardwareConcurrency": config["navigator.hardwareConcurrency"],
        "canvasSeed": config["canvas:seed"],
        "audioSeed": config["audio:seed"],
        "fontSpacingSeed": config["fonts:spacing_seed"],
        "webglVendor": config["webGl:vendor"],
        "webglRenderer": config["webGl:renderer"],
        "fonts": list(config["fonts"]),
        "voices": list(config["voices"]),
    }


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

    policy = identity_policy(
        target_os=args.os,
        font_mode=args.font_mode,
        window=(width, height),
        locale=args.locale,
        ff_version=args.ff_version,
        timezone_mode=TIMEZONE_MODE,
    )
    artifact = {
        "schema": ARTIFACT_SCHEMA,
        "artifactId": args.id,
        "policy": policy,
        "browserRelease": RELEASE,
        "browserBinding": browser_binding(lock, executable),
        "generatedBy": "VeriSilo generate_identity.py (M2.0.2)",
        "generatedAtUtc": datetime.now(timezone.utc)
        .isoformat()
        .replace("+00:00", "Z"),
        "generatorVersions": {
            "camoufox": dist_version("camoufox"),
            "playwright": dist_version("playwright"),
            "browserforge": dist_version("browserforge"),
        },
        "resolvedConfig": config,
        "stableSignalsDeclared": declared_stable_signals(config, args.locale),
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

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(
        json.dumps(artifact, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    sidecar = args.out.with_suffix(args.out.suffix + ".sha256")
    sidecar.write_text(
        f"{sha256_hex(args.out.read_bytes())}  {args.out.name}\n",
        encoding="utf-8",
    )
    verify_artifact(args.out)  # strict validation must pass before handoff
    print(f"artifact written to {args.out}")
    print(f"canonicalDigest={artifact['canonicalDigest']}")
    print(f"configuredIdentityDigest={artifact['configuredIdentityDigest']}")
    print(f"config keys={len(config)} binding={artifact['browserBinding']['buildId']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
