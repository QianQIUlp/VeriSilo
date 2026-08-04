#!/usr/bin/env python3
"""VeriSilo M2-0 identity policy: canonical serialization, digest rules, and a
RECURSIVE strict schema.

Schema families:

- IdentityPolicyV2: website-visible signal classification, timezone mode, and
  the exact resolved-config key contract.
- ResolvedCamoufoxIdentityV2: one resolved identity artifact bound to the
  verified browser archive (archive SHA, BuildID, SourceStamp,
  properties.json SHA) and generator versions.
- StableSignalProjectionV2: per-cold-start evidence with ConfiguredIdentityDigest
  (config, may include seeds, no artifactId) and ObservedWebsiteDigest
  (website-observed values only: no artifactId, no internal seeds, no canvas,
  no artifact-supplied font input).

Hardening guarantees (M2-0):

- Artifact bytes are read exactly ONCE; parsing, raw file SHA, sidecar
  comparison, and expectedArtifactFileSha256 all use the same bytes (no
  TOCTOU between reads).
- Integer fields require `type(value) is int` (bool is rejected); bool fields
  require `type(value) is bool`.
- policy / browserBinding / generatorVersions / stableSignalsDeclared /
  exclusions / resolvedConfig (including nested objects and list members) are
  validated recursively; every closed object requires ALL declared fields
  (missing fields are rejected) and rejects unknown fields.

Font policy (M2.0.1):

- policy.fontMode is `inherit` or `managed`. In `inherit` mode
  fontUniverseWidths are host-bound and are excluded from
  ObservedWebsiteDigest; only `managed` mode (with all host negative controls
  unavailable) may include font widths in the digest.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any

POLICY_SCHEMA = "verisilo-camoufox-identity-policy/v2"
ARTIFACT_SCHEMA = "verisilo-camoufox-resolved-identity/v2"
PROJECTION_SCHEMA = "verisilo-camoufox-stable-signal-projection/v2"
CONFIG_DIGEST_SCHEMA = "verisilo-camoufox-configured-identity/v1"
OBSERVED_DIGEST_SCHEMA = "verisilo-camoufox-observed-website/v1"

# Website-observed signals that must be identical across cold starts of the
# same artifact. No artifactId, no internal seeds, no canvas, no
# artifact-supplied font input (font evidence uses a FIXED probe universe).
STABLE_WEBSITE_SIGNAL_KEYS = [
    "userAgent",
    "language",
    "languages",
    "platform",
    "oscpu",
    "doNotTrack",
    "globalPrivacyControl",
    "screen",
    "devicePixelRatio",
    "hardwareConcurrency",
    "historyLength",
    "mediaDevices",
    "timezone",
    "utcOffsetMinutes",
    "fontNegativeControls",
    "webglVendor",
    "webglRenderer",
    "webglSummary",
    "voices",
    "audioHash",
]

# Font-width evidence is only identity-bearing in `managed` font mode, and
# only when host negative controls all pass. In `inherit` mode font widths are
# host-bound (the host font set can leak through width measurement), so they
# never enter ObservedWebsiteDigest.
MANAGED_FONT_SIGNAL_KEYS = ["fontUniverseWidths"]

SESSION_VARIABLE_SIGNAL_KEYS = [
    "navigatorOnLine",
    "windowScreenX",
    "windowScreenY",
    "documentFontsStatus",
    "canvasExportHash",
]

UNAVAILABLE_SIGNALS = {
    "deviceMemory": "Firefox does not expose navigator.deviceMemory",
    "userAgentData": "Firefox does not expose navigator.userAgentData",
    "windowChrome": "Chrome-only object, absent in Firefox",
}

CANVAS_CLASSIFICATION = {
    "rawPixels": "stable per bundle, but seed noise was NOT observable through getImageData",
    "exportEncoding": "session-variable: toDataURL output changed across browser restarts",
    "identity": "not stable; excluded from ObservedWebsiteDigest",
}

CANONICAL_JSON_RULE = (
    "UTF-8, recursively sorted object keys, compact separators (,/:), "
    "ensure_ascii=false, allow_nan=false; artifact digest excludes only "
    "canonicalDigest; ObservedWebsiteDigest excludes artifactId, internal "
    "seeds, canvas, and artifact-supplied font input"
)

ARTIFACT_ID_RE = re.compile(r"^identity-[a-z0-9-]{1,63}$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")

# --------------------------------------------------------------------------
# Recursive schema primitives (exact type identity, no bool-as-int).
# --------------------------------------------------------------------------

INT = int
BOOL = bool
STR = str
FLOAT = float
NULL = type(None)


def _check_value(value: Any, spec: Any, path: str, errors: list[str]) -> None:
    if isinstance(spec, type):
        if type(value) is not spec:
            errors.append(f"{path}: expected {spec.__name__}, got {type(value).__name__}")
        return
    if isinstance(spec, dict):
        if "keys" in spec:  # object with a closed key set
            if type(value) is not dict:
                errors.append(f"{path}: expected object, got {type(value).__name__}")
                return
            for key in spec["keys"]:
                if key not in value:
                    errors.append(f"{path}: missing required field {key}")
            for key, item in value.items():
                if key not in spec["keys"]:
                    errors.append(f"{path}.{key}: unknown field")
                    continue
                _check_value(item, spec["keys"][key], f"{path}.{key}", errors)
            return
        if "items" in spec:  # list
            if type(value) is not list:
                errors.append(f"{path}: expected list, got {type(value).__name__}")
                return
            if "min" in spec and len(value) < spec["min"]:
                errors.append(f"{path}: expected >= {spec['min']} items, got {len(value)}")
            if "len" in spec and len(value) != spec["len"]:
                errors.append(f"{path}: expected {spec['len']} items, got {len(value)}")
            for index, item in enumerate(value):
                _check_value(item, spec["items"], f"{path}[{index}]", errors)
            return
        if "valueType" in spec:  # object with arbitrary keys, validated values
            if type(value) is not dict:
                errors.append(f"{path}: expected object, got {type(value).__name__}")
                return
            pattern = spec.get("keyPattern", ".*")
            for key, item in value.items():
                if not re.fullmatch(pattern, str(key)):
                    errors.append(f"{path}.{key}: key does not match {pattern}")
                    continue
                _check_value(item, spec["valueType"], f"{path}.{key}", errors)
            return
        if "anyOf" in spec:
            for variant in spec["anyOf"]:
                sub_errors: list[str] = []
                _check_value(value, variant, path, sub_errors)
                if not sub_errors:
                    return
            errors.append(f"{path}: value does not match any allowed variant")
            return
    errors.append(f"{path}: unsupported spec {spec!r}")


# --------------------------------------------------------------------------
# Schemas
# --------------------------------------------------------------------------

WEBGL_PARAM_VALUE = {
    "anyOf": [INT, BOOL, FLOAT, STR, NULL, {"items": {"anyOf": [INT, BOOL, FLOAT, STR, NULL]}}]
}
WEBGL_CONTEXT_ATTRIBUTES = {
    "keys": {
        "alpha": BOOL,
        "antialias": BOOL,
        "depth": BOOL,
        "failIfMajorPerformanceCaveat": BOOL,
        "powerPreference": STR,
        "premultipliedAlpha": BOOL,
        "preserveDrawingBuffer": BOOL,
        "stencil": BOOL,
    }
}
WEBGL_PRECISION = {"keys": {"rangeMin": INT, "rangeMax": INT, "precision": INT}}
WEBGL_PRECISION_MAP = {"valueType": WEBGL_PRECISION, "keyPattern": r"^\d+,\d+$"}
WEBGL_PARAMETERS = {"valueType": WEBGL_PARAM_VALUE, "keyPattern": r"^\d+$"}
SUPPORTED_EXTENSIONS = {"items": STR}

VOICE = {
    "keys": {
        "name": STR,
        "lang": STR,
        "voiceUri": STR,
        "isDefault": BOOL,
        "isLocalService": BOOL,
    }
}
FONTS = {"items": STR, "min": 1}
VOICES = {"items": VOICE, "min": 1}

REQUIRED_CONFIG_KEYS: dict[str, Any] = {
    "navigator.userAgent": STR,
    "navigator.platform": STR,
    "navigator.oscpu": STR,
    "navigator.hardwareConcurrency": INT,
    "navigator.doNotTrack": STR,
    "navigator.globalPrivacyControl": BOOL,
    "navigator.appCodeName": STR,
    "navigator.appName": STR,
    "navigator.appVersion": STR,
    "navigator.product": STR,
    "screen.width": INT,
    "screen.height": INT,
    "screen.availWidth": INT,
    "screen.availHeight": INT,
    "screen.availTop": INT,
    "screen.availLeft": INT,
    "screen.colorDepth": INT,
    "screen.pixelDepth": INT,
    "window.outerWidth": INT,
    "window.outerHeight": INT,
    "window.history.length": INT,
    "window.screenX": INT,
    "window.screenY": INT,
    "canvas:seed": INT,
    "audio:seed": INT,
    "fonts:spacing_seed": INT,
    "locale:language": STR,
    "locale:region": STR,
    "locale:script": STR,
    "timezone": STR,
    "mediaDevices:enabled": BOOL,
    "mediaDevices:micros": INT,
    "mediaDevices:webcams": INT,
    "mediaDevices:speakers": INT,
    "headers.Accept-Encoding": STR,
    "fonts": FONTS,
    "voices": VOICES,
    "webGl:vendor": STR,
    "webGl:renderer": STR,
    "webGl:contextAttributes": WEBGL_CONTEXT_ATTRIBUTES,
    "webGl:parameters": WEBGL_PARAMETERS,
    "webGl:shaderPrecisionFormats": WEBGL_PRECISION_MAP,
    "webGl:supportedExtensions": SUPPORTED_EXTENSIONS,
    "webGl2:contextAttributes": WEBGL_CONTEXT_ATTRIBUTES,
    "webGl2:parameters": WEBGL_PARAMETERS,
    "webGl2:shaderPrecisionFormats": WEBGL_PRECISION_MAP,
    "webGl2:supportedExtensions": SUPPORTED_EXTENSIONS,
}
ALLOWED_CONFIG_KEYS = set(REQUIRED_CONFIG_KEYS)

POLICY_SPEC = {
    "keys": {
        "schema": STR,
        "version": INT,
        "targetOs": STR,
        "fontMode": STR,
        "window": {"items": INT, "len": 2},
        "locale": STR,
        "ffVersion": INT,
        "timezoneMode": STR,
        "stableWebsiteFields": {"items": STR},
        "sessionVariableFields": {"items": STR},
        "unavailableFields": {"valueType": STR},
        "canvasClassification": {"valueType": STR},
        "requiredConfigKeys": {"items": STR},
        "canonicalJsonRule": STR,
    }
}

BINDING_SPEC = {
    "keys": {
        "archiveSha256": STR,
        "archiveSizeBytes": INT,
        "buildId": STR,
        "sourceStamp": STR,
        "propertiesJsonSha256": STR,
    }
}

GENERATOR_SPEC = {
    "keys": {
        "camoufox": STR,
        "playwright": STR,
        "browserforge": STR,
    }
}

EXCLUSIONS_SPEC = {
    "keys": {
        "profilePath": STR,
        "display": STR,
        "tokens": STR,
        "proxySecrets": STR,
        "environment": STR,
    }
}

DECLARED_SCREEN = {
    "keys": {
        "width": INT,
        "height": INT,
        "availWidth": INT,
        "availHeight": INT,
        "availTop": INT,
        "availLeft": INT,
        "colorDepth": INT,
        "pixelDepth": INT,
    }
}
DECLARED_SPEC = {
    "keys": {
        "userAgent": STR,
        "language": STR,
        "screen": DECLARED_SCREEN,
        "devicePixelRatio": INT,
        "hardwareConcurrency": INT,
        "canvasSeed": INT,
        "audioSeed": INT,
        "fontSpacingSeed": INT,
        "webglVendor": STR,
        "webglRenderer": STR,
        "fonts": FONTS,
        "voices": VOICES,
    }
}

REQUIRED_ARTIFACT_KEYS = {
    "schema",
    "artifactId",
    "policy",
    "browserRelease",
    "browserBinding",
    "generatedBy",
    "generatedAtUtc",
    "generatorVersions",
    "resolvedConfig",
    "stableSignalsDeclared",
    "exclusions",
    "configuredIdentityDigest",
    "canonicalDigest",
}
ALLOWED_ARTIFACT_KEYS = set(REQUIRED_ARTIFACT_KEYS)
REQUIRED_BINDING_KEYS = set(BINDING_SPEC["keys"])
REQUIRED_GENERATOR_VERSIONS = ("camoufox", "playwright", "browserforge")


class ArtifactIntegrityError(Exception):
    """Raised when an identity artifact fails pre-launch validation."""


# --------------------------------------------------------------------------
# Canonical JSON and digests
# --------------------------------------------------------------------------


def canonical_json_bytes(obj: Any) -> bytes:
    return json.dumps(
        obj,
        sort_keys=True,
        separators=(",", ":"),
        ensure_ascii=False,
        allow_nan=False,
    ).encode("utf-8")


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(obj: Any) -> str:
    return "sha256:" + sha256_hex(canonical_json_bytes(obj))


def artifact_canonical_payload(artifact: dict) -> dict:
    payload = dict(artifact)
    payload.pop("canonicalDigest", None)
    return payload


def compute_artifact_digest(artifact: dict) -> str:
    return canonical_digest(artifact_canonical_payload(artifact))


def configured_identity_digest(config: dict) -> str:
    return canonical_digest(
        {"schema": CONFIG_DIGEST_SCHEMA, "resolvedConfig": config}
    )


def observed_website_digest(signals: dict) -> str:
    return canonical_digest(
        {"schema": OBSERVED_DIGEST_SCHEMA, "signals": signals}
    )


def diff_configs(disk: dict, sent: dict) -> dict:
    added = sorted(set(sent) - set(disk))
    removed = sorted(set(disk) - set(sent))
    changed = sorted(
        key for key in set(disk) & set(sent) if disk[key] != sent[key]
    )
    return {"added": added, "removed": removed, "changed": changed}


def build_projection(
    artifact_id: str,
    run_id: str,
    cold_start_index: int,
    configured_digest: str,
    observed_signals: dict,
) -> dict:
    return {
        "schema": PROJECTION_SCHEMA,
        "runId": run_id,
        "coldStartIndex": cold_start_index,
        "artifactId": artifact_id,
        "configuredIdentityDigest": configured_digest,
        "observedWebsiteSignals": observed_signals,
        "observedWebsiteDigest": observed_website_digest(observed_signals),
    }


def identity_policy_v2(
    target_os: str = "linux",
    font_mode: str = "inherit",
    window: tuple[int, int] = (1280, 800),
    locale: str = "en-US",
    ff_version: int = 152,
    timezone_mode: str = "fixed",
) -> dict:
    return {
        "schema": POLICY_SCHEMA,
        "version": 2,
        "targetOs": target_os,
        "fontMode": font_mode,
        "window": list(window),
        "locale": locale,
        "ffVersion": ff_version,
        "timezoneMode": timezone_mode,
        "stableWebsiteFields": list(STABLE_WEBSITE_SIGNAL_KEYS),
        "sessionVariableFields": list(SESSION_VARIABLE_SIGNAL_KEYS),
        "unavailableFields": dict(UNAVAILABLE_SIGNALS),
        "canvasClassification": dict(CANVAS_CLASSIFICATION),
        "requiredConfigKeys": sorted(REQUIRED_CONFIG_KEYS),
        "canonicalJsonRule": CANONICAL_JSON_RULE,
    }


# --------------------------------------------------------------------------
# Strict validation
# --------------------------------------------------------------------------


def validate_artifact_strict(artifact: dict) -> None:
    errors: list[str] = []

    if artifact.get("schema") != ARTIFACT_SCHEMA:
        errors.append(f"schema must be {ARTIFACT_SCHEMA!r}")
    artifact_id = artifact.get("artifactId")
    if not isinstance(artifact_id, str) or not ARTIFACT_ID_RE.match(artifact_id):
        errors.append(f"artifactId {artifact_id!r} does not match {ARTIFACT_ID_RE.pattern}")

    unknown = set(artifact) - ALLOWED_ARTIFACT_KEYS
    if unknown:
        errors.append("unknown artifact keys: " + ", ".join(sorted(unknown)))
    missing = REQUIRED_ARTIFACT_KEYS - set(artifact)
    if missing:
        errors.append("missing artifact keys: " + ", ".join(sorted(missing)))

    _check_value(artifact.get("policy"), POLICY_SPEC, "policy", errors)
    _check_value(artifact.get("browserBinding"), BINDING_SPEC, "browserBinding", errors)
    _check_value(
        artifact.get("generatorVersions"), GENERATOR_SPEC, "generatorVersions", errors
    )
    _check_value(artifact.get("exclusions"), EXCLUSIONS_SPEC, "exclusions", errors)
    _check_value(
        artifact.get("stableSignalsDeclared"),
        DECLARED_SPEC,
        "stableSignalsDeclared",
        errors,
    )

    policy = artifact.get("policy")
    if isinstance(policy, dict):
        if policy.get("version") != 2:
            errors.append("policy.version must be 2")
        if policy.get("targetOs") not in ("linux", "macos", "windows"):
            errors.append("policy.targetOs unsupported")
        if policy.get("fontMode") not in ("inherit", "managed"):
            errors.append("policy.fontMode must be inherit or managed")
        if policy.get("timezoneMode") not in ("fixed", "network-bound"):
            errors.append("policy.timezoneMode must be fixed or network-bound")
        if policy.get("stableWebsiteFields") != STABLE_WEBSITE_SIGNAL_KEYS:
            errors.append("policy.stableWebsiteFields does not match the canonical list")
        if policy.get("sessionVariableFields") != SESSION_VARIABLE_SIGNAL_KEYS:
            errors.append("policy.sessionVariableFields does not match the canonical list")
        if policy.get("unavailableFields") != UNAVAILABLE_SIGNALS:
            errors.append("policy.unavailableFields does not match the canonical map")
        if policy.get("canvasClassification") != CANVAS_CLASSIFICATION:
            errors.append("policy.canvasClassification does not match the canonical map")
        if policy.get("requiredConfigKeys") != sorted(REQUIRED_CONFIG_KEYS):
            errors.append("policy.requiredConfigKeys does not match the canonical key set")

    config = artifact.get("resolvedConfig")
    if not isinstance(config, dict):
        errors.append("resolvedConfig must be an object")
    else:
        missing_keys = set(REQUIRED_CONFIG_KEYS) - set(config)
        if missing_keys:
            errors.append("missing config keys: " + ", ".join(sorted(missing_keys)))
        unknown_keys = set(config) - ALLOWED_CONFIG_KEYS
        if unknown_keys:
            errors.append("unknown config keys: " + ", ".join(sorted(unknown_keys)))
        for key, spec in REQUIRED_CONFIG_KEYS.items():
            if key in config:
                _check_value(config[key], spec, f"resolvedConfig.{key}", errors)
        if (
            isinstance(config.get("screen.availTop"), int)
            and isinstance(config.get("screen.availHeight"), int)
            and isinstance(config.get("screen.height"), int)
        ):
            if config["screen.availTop"] + config["screen.availHeight"] > config["screen.height"]:
                errors.append("screen avail geometry inconsistent")

    binding = artifact.get("browserBinding")
    if isinstance(binding, dict):
        if not HEX64_RE.fullmatch(str(binding.get("archiveSha256", ""))):
            errors.append("browserBinding.archiveSha256 must be a 64-hex sha256")
        if not HEX64_RE.fullmatch(str(binding.get("propertiesJsonSha256", ""))):
            errors.append("browserBinding.propertiesJsonSha256 must be a 64-hex sha256")
        if not isinstance(binding.get("buildId"), str) or not binding["buildId"]:
            errors.append("browserBinding.buildId must be a non-empty string")
        if not isinstance(binding.get("sourceStamp"), str) or not binding["sourceStamp"]:
            errors.append("browserBinding.sourceStamp must be a non-empty string")

    declared = artifact.get("stableSignalsDeclared")
    if isinstance(declared, dict) and isinstance(config, dict):
        expected_language = None
        if isinstance(config.get("locale:language"), str) and isinstance(
            config.get("locale:region"), str
        ):
            expected_language = f"{config['locale:language']}-{config['locale:region']}"
        consistency = {
            "userAgent": (declared.get("userAgent"), config.get("navigator.userAgent")),
            "language": (declared.get("language"), expected_language),
            "hardwareConcurrency": (
                declared.get("hardwareConcurrency"),
                config.get("navigator.hardwareConcurrency"),
            ),
            "canvasSeed": (declared.get("canvasSeed"), config.get("canvas:seed")),
            "audioSeed": (declared.get("audioSeed"), config.get("audio:seed")),
            "fontSpacingSeed": (
                declared.get("fontSpacingSeed"),
                config.get("fonts:spacing_seed"),
            ),
            "webglVendor": (declared.get("webglVendor"), config.get("webGl:vendor")),
            "webglRenderer": (
                declared.get("webglRenderer"),
                config.get("webGl:renderer"),
            ),
        }
        for field, (declared_value, config_value) in consistency.items():
            if declared_value != config_value:
                errors.append(
                    f"stableSignalsDeclared.{field} inconsistent with resolvedConfig"
                )
        if declared.get("screen") != {
            "width": config.get("screen.width"),
            "height": config.get("screen.height"),
            "availWidth": config.get("screen.availWidth"),
            "availHeight": config.get("screen.availHeight"),
            "availTop": config.get("screen.availTop"),
            "availLeft": config.get("screen.availLeft"),
            "colorDepth": config.get("screen.colorDepth"),
            "pixelDepth": config.get("screen.pixelDepth"),
        }:
            errors.append("stableSignalsDeclared.screen inconsistent with resolvedConfig")
        if declared.get("fonts") != config.get("fonts"):
            errors.append("stableSignalsDeclared.fonts inconsistent with resolvedConfig")
        if declared.get("voices") != config.get("voices"):
            errors.append("stableSignalsDeclared.voices inconsistent with resolvedConfig")
        if type(declared.get("devicePixelRatio")) is not int or declared.get("devicePixelRatio") != 1:
            errors.append("stableSignalsDeclared.devicePixelRatio must be 1")

    if isinstance(policy, dict) and isinstance(config, dict):
        if policy.get("window") != [
            config.get("window.outerWidth"),
            config.get("window.outerHeight"),
        ]:
            errors.append("policy.window inconsistent with config window dimensions")
        locale = policy.get("locale")
        if locale and isinstance(config.get("locale:language"), str) and isinstance(
            config.get("locale:region"), str
        ):
            if locale != f"{config['locale:language']}-{config['locale:region']}":
                errors.append("policy.locale inconsistent with config locale keys")
        ua = config.get("navigator.userAgent", "")
        ff_version = policy.get("ffVersion")
        if isinstance(ua, str) and isinstance(ff_version, int):
            if f"Firefox/{ff_version}.0" not in ua:
                errors.append("policy.ffVersion inconsistent with UA")
            target_os = policy.get("targetOs")
            if target_os == "linux" and not ("X11;" in ua and "Linux" in ua):
                errors.append("policy.targetOs inconsistent with UA")
            elif target_os == "macos" and "Macintosh" not in ua:
                errors.append("policy.targetOs inconsistent with UA")
            elif target_os == "windows" and "Windows" not in ua:
                errors.append("policy.targetOs inconsistent with UA")
        if policy.get("timezoneMode") == "fixed" and not isinstance(
            config.get("timezone"), str
        ):
            errors.append("fixed timezone policy requires config timezone string")

    configured = artifact.get("configuredIdentityDigest")
    if isinstance(config, dict):
        expected_configured = configured_identity_digest(config)
        if configured != expected_configured:
            errors.append(
                f"configuredIdentityDigest mismatch: expected {expected_configured}, got {configured!r}"
            )

    if errors:
        raise ArtifactIntegrityError("strict validation failed: " + "; ".join(errors))


def _scan_for_sensitive(text: str) -> list[str]:
    patterns = [
        "/home/",
        "/Users/",
        "C:\\Users",
        "DISPLAY=",
        "VERISILO_",
        "proxy",
        "password",
        "authorization",
        "bearer ",
        "api_key",
        "private_key",
        "client_secret",
    ]
    lowered = text.lower()
    return sorted({pattern for pattern in patterns if pattern.lower() in lowered})


def assert_artifact_clean(artifact: dict) -> None:
    parts = [
        artifact.get("resolvedConfig", {}),
        artifact.get("stableSignalsDeclared", {}),
        artifact.get("browserBinding", {}),
    ]
    hits: list[str] = []
    for part in parts:
        hits.extend(_scan_for_sensitive(canonical_json_bytes(part).decode("utf-8")))
    if hits:
        raise ArtifactIntegrityError(
            "artifact contains sensitive-looking content: " + ", ".join(sorted(set(hits)))
        )


def verify_artifact_raw(
    path: Path | str,
    expected_file_sha: str | None = None,
) -> tuple[dict, str]:
    """Single-byte-read pre-launch validation.

    The artifact file is read exactly once; JSON parsing, the raw file SHA,
    expectedArtifactFileSha256, and the .sha256 sidecar all use that one read.
    """
    path = Path(path)
    try:
        raw = path.read_bytes()
    except OSError as exc:
        raise ArtifactIntegrityError(f"artifact unreadable: {exc}") from exc
    file_sha = sha256_hex(raw)
    if expected_file_sha is not None and file_sha != expected_file_sha:
        raise ArtifactIntegrityError(
            f"artifact file sha256 mismatch with expected: expected {expected_file_sha}, got {file_sha}"
        )
    try:
        artifact = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise ArtifactIntegrityError(f"artifact unreadable: {exc}") from exc
    if artifact.get("schema") != ARTIFACT_SCHEMA:
        raise ArtifactIntegrityError(
            f"artifact schema mismatch: {artifact.get('schema')!r}"
        )
    validate_artifact_strict(artifact)
    expected = artifact.get("canonicalDigest")
    if not expected:
        raise ArtifactIntegrityError("artifact has no canonicalDigest")
    actual = compute_artifact_digest(artifact)
    if actual != expected:
        raise ArtifactIntegrityError(
            f"artifact digest mismatch: expected {expected}, got {actual}"
        )
    sidecar = path.with_suffix(path.suffix + ".sha256")
    try:
        sidecar_raw = sidecar.read_bytes()
    except OSError as exc:
        raise ArtifactIntegrityError(f"artifact sidecar missing or unreadable: {sidecar}") from exc
    sidecar_text = sidecar_raw.decode("utf-8", errors="strict").strip()
    sidecar_hex = sidecar_text.split()[0] if sidecar_text else ""
    if sidecar_hex != file_sha:
        raise ArtifactIntegrityError(
            f"artifact file sha256 mismatch: expected {sidecar_hex}, got {file_sha}"
        )
    assert_artifact_clean(artifact)
    return artifact, file_sha


def verify_artifact(
    path: Path | str,
    expected_file_sha: str | None = None,
) -> dict:
    artifact, _ = verify_artifact_raw(path, expected_file_sha=expected_file_sha)
    return artifact


# --------------------------------------------------------------------------
# Browser binding
# --------------------------------------------------------------------------


def read_bundle_metadata(executable: Path | str) -> dict:
    bundle = Path(executable).parent
    ini_text = (bundle / "application.ini").read_text(encoding="utf-8")
    build_id = re.search(r"^BuildID=(.+)$", ini_text, re.MULTILINE)
    source_stamp = re.search(r"^SourceStamp=(.+)$", ini_text, re.MULTILINE)
    if not build_id or not source_stamp:
        raise ArtifactIntegrityError("browser application.ini missing BuildID/SourceStamp")
    properties_path = bundle / "properties.json"
    if not properties_path.exists():
        raise ArtifactIntegrityError("browser properties.json missing")
    return {
        "buildId": build_id.group(1).strip(),
        "sourceStamp": source_stamp.group(1).strip(),
        "propertiesJsonSha256": sha256_hex(properties_path.read_bytes()),
    }


def verify_browser_binding(
    artifact: dict,
    lock: dict,
    executable: Path | str,
    installed_versions: dict,
) -> None:
    binding = artifact.get("browserBinding") or {}
    errors: list[str] = []
    if binding.get("archiveSha256") != lock.get("sha256"):
        errors.append("browserBinding.archiveSha256 does not match the M0 asset lock")
    if binding.get("archiveSizeBytes") != lock.get("sizeBytes"):
        errors.append("browserBinding.archiveSizeBytes does not match the M0 asset lock")
    if artifact.get("browserRelease") != lock.get("release"):
        errors.append("browserRelease does not match the M0 asset lock release")
    try:
        actual = read_bundle_metadata(executable)
    except ArtifactIntegrityError as exc:
        raise ArtifactIntegrityError(f"cannot read browser bundle: {exc}") from exc
    if actual["buildId"] != binding.get("buildId"):
        errors.append(
            f"browserBinding.buildId mismatch: artifact={binding.get('buildId')!r} "
            f"bundle={actual['buildId']!r}"
        )
    if actual["sourceStamp"] != binding.get("sourceStamp"):
        errors.append("browserBinding.sourceStamp does not match the extracted bundle")
    if actual["propertiesJsonSha256"] != binding.get("propertiesJsonSha256"):
        errors.append(
            "browserBinding.propertiesJsonSha256 does not match the extracted bundle"
        )
    generator_versions = artifact.get("generatorVersions") or {}
    for name in REQUIRED_GENERATOR_VERSIONS:
        if generator_versions.get(name) != installed_versions.get(name):
            errors.append(
                f"generatorVersions.{name} does not match installed "
                f"({generator_versions.get(name)!r} vs {installed_versions.get(name)!r})"
            )
    if errors:
        raise ArtifactIntegrityError("browser binding mismatch: " + "; ".join(errors))
