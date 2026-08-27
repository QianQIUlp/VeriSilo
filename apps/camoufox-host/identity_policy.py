#!/usr/bin/env python3
"""VeriSilo M2-0 identity policy: canonical serialization, digest rules, and a
RECURSIVE strict schema.

Schema families:

- IdentityPolicyV3: website-visible signal classification, timezone mode,
  fontMode (inherit/managed), and
  the exact resolved-config key contract.
- ResolvedCamoufoxIdentityV3: one resolved identity artifact bound to the
  verified browser archive (archive SHA, BuildID, SourceStamp,
  properties.json SHA) and generator versions.
- StableSignalProjectionV3: per-cold-start evidence with ConfiguredIdentityDigest
  (config, may include seeds, no artifactId) and ObservedWebsiteDigest
  (website-observed values only: no artifactId, no internal seeds, no canvas,
  no artifact-supplied font input).

Version contract (M2.0.2):

- Artifact/Policy v5 adds the FF>=135 DNT-native, DPR host-bound, and
  managed/native GPC contracts; v3/v4 remain readable for historical
  artifacts; ObservedWebsiteDigest is v2.
  Old v2 artifacts are
  rejected with UnsupportedSchemaVersionError (protocol code
  unsupported_schema_version), never as a plain missing-field error.
- generatedBy must be a non-empty string; generatedAtUtc must be strict
  RFC 3339 UTC normalized to 'Z'.

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
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

POLICY_SCHEMA_V3 = "verisilo-camoufox-identity-policy/v3"
ARTIFACT_SCHEMA_V3 = "verisilo-camoufox-resolved-identity/v3"
POLICY_SCHEMA_V4 = "verisilo-camoufox-identity-policy/v4"
ARTIFACT_SCHEMA_V4 = "verisilo-camoufox-resolved-identity/v4"
POLICY_SCHEMA_V5 = "verisilo-camoufox-identity-policy/v5"
ARTIFACT_SCHEMA_V5 = "verisilo-camoufox-resolved-identity/v5"
POLICY_SCHEMA = POLICY_SCHEMA_V5
ARTIFACT_SCHEMA = ARTIFACT_SCHEMA_V5
PROJECTION_SCHEMA = "verisilo-camoufox-stable-signal-projection/v3"
CONFIG_DIGEST_SCHEMA = "verisilo-camoufox-configured-identity/v1"
OBSERVED_DIGEST_SCHEMA = "verisilo-camoufox-observed-website/v2"

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

# Canvas Policy v3 has two deliberately closed variants.  The legacy variant
# preserves the byte-for-byte policy embedded in the accepted official
# Camoufox fixtures.  The deterministic variant is selected only by the exact
# VeriSilo-patched Windows browser binding; no individual binding field is a
# sufficient capability signal.
LEGACY_CANVAS_POLICY_VARIANT = "legacy-session-variable"
DETERMINISTIC_CANVAS_POLICY_VARIANT = "deterministic-artifact-v1"

LEGACY_BROWSER_BINDINGS = (
    {
        "archiveSha256": "924f3109ccd6d47cd6a0384d67a345fadf975d48b6319f8dbbd5954c588982bd",
        "archiveSizeBytes": 663387175,
        "buildId": "20260719045650",
        "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
        "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
    },
    {
        "archiveSha256": "386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d",
        "archiveSizeBytes": 492370020,
        "buildId": "20260719045835",
        "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
        "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
    },
)

DETERMINISTIC_CANVAS_BROWSER_BINDING = {
    "archiveSha256": "148d3a067cb94e830723745682e904c3a416cd2cf75282299ab7ce11c8050a94",
    "archiveSizeBytes": 493100709,
    "buildId": "20260811045234",
    "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
    "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
}

FORMAL_R1_CANVAS_BROWSER_BINDING = {
    "archiveSha256": "a81649c538a101dce106e42f13f11dbdb08cbc0e8a1c9af6b497719a392a6cdc",
    "archiveSizeBytes": 493497411,
    "buildId": "20260811045234",
    "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
    "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
}

FORMAL_R1_V2_CANVAS_BROWSER_BINDING = {
    "archiveSha256": "bea161d2e61a8cd4ac91f60b2247d419f48df0228919fac23d6d3fd94434ae00",
    "archiveSizeBytes": 493493008,
    "buildId": "20260811045234",
    "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
    "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
}

FORMAL_R1_V3_CANVAS_BROWSER_BINDING = {
    "archiveSha256": "032ca1a43f7e8082cf9e36668fd5b58cf4a27f4f41d0f7be833c3d2eb9c2abd5",
    "archiveSizeBytes": 493493005,
    "buildId": "20260811045234",
    "sourceStamp": "e39c605adc0fc049a165d7fe4a3f6517b761edf7",
    "propertiesJsonSha256": "c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018",
}

DETERMINISTIC_SESSION_VARIABLE_SIGNAL_KEYS = [
    key for key in SESSION_VARIABLE_SIGNAL_KEYS if key != "canvasExportHash"
]

DETERMINISTIC_CANVAS_CLASSIFICATION = {
    "rawPixels": "stable raw RGBA; canvas:seed does not add pixel noise",
    "exportEncoding": "artifact-silo deterministic PNG export from canvas:seed",
    "identity": "stable hard-observed Canvas surface; excluded from ObservedWebsiteDigest v2",
}

CANONICAL_JSON_RULE = (
    "UTF-8, recursively sorted object keys, compact separators (,/:), "
    "ensure_ascii=false, allow_nan=false; artifact digest excludes only "
    "canonicalDigest; ObservedWebsiteDigest excludes artifactId, internal "
    "seeds, canvas, and artifact-supplied font input"
)

ARTIFACT_ID_RE = re.compile(r"^identity-[a-z0-9-]{1,63}$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
RFC3339_UTC_Z_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$")

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

VOICES_MODE_MANAGED = "managed"
VOICES_MODE_NATIVE = "native"
GPC_POLICY_KEY = "navigator.gpcPolicy"
GPC_POLICY_NATIVE = "native"
GPC_POLICY_MANAGED_OPT_OUT = "managed-opt-out"
GPC_CONFIG_KEY = "navigator.globalPrivacyControl"
DNT_CONFIG_KEY = "navigator.doNotTrack"
VOICE_DERIVED_CONFIG = {
    "voices:blockIfNotDefined": True,
    "voices:fakeCompletion": True,
    "voices:fakeCompletion:charsPerSecond": 12.5,
}

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
V4_MANAGED_CONFIG_KEYS = {
    **REQUIRED_CONFIG_KEYS,
    "voices:blockIfNotDefined": BOOL,
    "voices:fakeCompletion": BOOL,
    "voices:fakeCompletion:charsPerSecond": FLOAT,
}
V4_NATIVE_CONFIG_KEYS = {
    key: spec for key, spec in REQUIRED_CONFIG_KEYS.items() if key != "voices"
}
V5_REQUIRED_CONFIG_KEYS = {
    key: spec
    for key, spec in REQUIRED_CONFIG_KEYS.items()
    if key not in (DNT_CONFIG_KEY, GPC_CONFIG_KEY)
}
V5_MANAGED_CONFIG_KEYS = {
    **V5_REQUIRED_CONFIG_KEYS,
    GPC_CONFIG_KEY: BOOL,
    "voices:blockIfNotDefined": BOOL,
    "voices:fakeCompletion": BOOL,
    "voices:fakeCompletion:charsPerSecond": FLOAT,
}
V5_NATIVE_CONFIG_KEYS = {
    key: spec for key, spec in V5_REQUIRED_CONFIG_KEYS.items() if key != "voices"
}
V5_MANAGED_VOICES_CONFIG_KEYS = {
    **V5_REQUIRED_CONFIG_KEYS,
    "voices:blockIfNotDefined": BOOL,
    "voices:fakeCompletion": BOOL,
    "voices:fakeCompletion:charsPerSecond": FLOAT,
}
V5_NATIVE_GPC_CONFIG_KEYS = {
    **V5_NATIVE_CONFIG_KEYS,
    GPC_CONFIG_KEY: BOOL,
}
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
V4_POLICY_SPEC = {
    "keys": {
        **POLICY_SPEC["keys"],
        "voicesMode": STR,
    }
}
V5_POLICY_SPEC = {
    "keys": {
        **V4_POLICY_SPEC["keys"],
        GPC_POLICY_KEY: STR,
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
V4_NATIVE_DECLARED_SPEC = {
    "keys": {
        key: spec for key, spec in DECLARED_SPEC["keys"].items() if key != "voices"
    }
}
V5_DECLARED_SPEC = {
    "keys": {
        key: spec
        for key, spec in DECLARED_SPEC["keys"].items()
        if key != "devicePixelRatio"
    }
}
V5_NATIVE_DECLARED_SPEC = {
    "keys": {
        key: spec for key, spec in V5_DECLARED_SPEC["keys"].items() if key != "voices"
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

# Unified closed schema for every top-level artifact field, including the
# dynamic-key resolvedConfig (its full key set is itself a closed schema).
TOP_LEVEL_SPEC = {
    "keys": {
        "schema": STR,
        "artifactId": STR,
        "policy": POLICY_SPEC,
        "browserRelease": STR,
        "browserBinding": BINDING_SPEC,
        "generatedBy": STR,
        "generatedAtUtc": STR,
        "generatorVersions": GENERATOR_SPEC,
        "resolvedConfig": {"keys": REQUIRED_CONFIG_KEYS},
        "stableSignalsDeclared": DECLARED_SPEC,
        "exclusions": EXCLUSIONS_SPEC,
        "configuredIdentityDigest": STR,
        "canonicalDigest": STR,
    }
}
V4_MANAGED_TOP_LEVEL_SPEC = {
    "keys": {
        **TOP_LEVEL_SPEC["keys"],
        "policy": V4_POLICY_SPEC,
        "resolvedConfig": {"keys": V4_MANAGED_CONFIG_KEYS},
    }
}
V4_NATIVE_TOP_LEVEL_SPEC = {
    "keys": {
        **V4_MANAGED_TOP_LEVEL_SPEC["keys"],
        "resolvedConfig": {"keys": V4_NATIVE_CONFIG_KEYS},
        "stableSignalsDeclared": V4_NATIVE_DECLARED_SPEC,
    }
}
V5_MANAGED_TOP_LEVEL_SPEC = {
    "keys": {
        **TOP_LEVEL_SPEC["keys"],
        "policy": V5_POLICY_SPEC,
        "resolvedConfig": {"keys": V5_MANAGED_CONFIG_KEYS},
        "stableSignalsDeclared": V5_DECLARED_SPEC,
    }
}
V5_NATIVE_TOP_LEVEL_SPEC = {
    "keys": {
        **V5_MANAGED_TOP_LEVEL_SPEC["keys"],
        "resolvedConfig": {"keys": V5_NATIVE_CONFIG_KEYS},
        "stableSignalsDeclared": V5_NATIVE_DECLARED_SPEC,
    }
}
V4_NATIVE_STABLE_WEBSITE_SIGNAL_KEYS = [
    key for key in STABLE_WEBSITE_SIGNAL_KEYS if key != "voices"
]
V5_STABLE_WEBSITE_SIGNAL_KEYS = [
    key
    for key in STABLE_WEBSITE_SIGNAL_KEYS
    if key not in ("doNotTrack", "devicePixelRatio")
]
V5_NATIVE_STABLE_WEBSITE_SIGNAL_KEYS = [
    key for key in V5_STABLE_WEBSITE_SIGNAL_KEYS if key != "globalPrivacyControl"
]


def _v5_config_keys(voices_mode: str, gpc_policy: str) -> dict[str, Any]:
    if voices_mode == VOICES_MODE_NATIVE:
        keys = V5_NATIVE_GPC_CONFIG_KEYS if gpc_policy == GPC_POLICY_MANAGED_OPT_OUT else V5_NATIVE_CONFIG_KEYS
    else:
        keys = V5_MANAGED_CONFIG_KEYS if gpc_policy == GPC_POLICY_MANAGED_OPT_OUT else V5_MANAGED_VOICES_CONFIG_KEYS
    return keys


def _v5_stable_website_fields(voices_mode: str, gpc_policy: str) -> list[str]:
    fields = list(V5_STABLE_WEBSITE_SIGNAL_KEYS)
    if voices_mode == VOICES_MODE_NATIVE:
        fields.remove("voices")
    if gpc_policy == GPC_POLICY_NATIVE:
        fields.remove("globalPrivacyControl")
    return fields


def _v5_artifact_spec(voices_mode: str, gpc_policy: str) -> dict[str, Any]:
    return {
        "keys": {
            **TOP_LEVEL_SPEC["keys"],
            "policy": V5_POLICY_SPEC,
            "resolvedConfig": {"keys": _v5_config_keys(voices_mode, gpc_policy)},
            "stableSignalsDeclared": (
                V5_NATIVE_DECLARED_SPEC
                if voices_mode == VOICES_MODE_NATIVE
                else V5_DECLARED_SPEC
            ),
        }
    }


class ArtifactIntegrityError(Exception):
    """Raised when an identity artifact fails pre-launch validation."""


class UnsupportedSchemaVersionError(ArtifactIntegrityError):
    """Raised when an artifact uses an older/newer resolved-identity schema
    version. This is a version contract error, NOT a missing-field error."""


def _browser_binding_matches_exact(actual: Any, expected: dict) -> bool:
    """Compare a binding as a closed, exact-type capability tuple.

    In particular, bool must not compare equal to an integer size and an
    otherwise-correct object with an extra field must not select a policy.
    """

    return (
        type(actual) is dict
        and set(actual) == set(expected)
        and all(
            type(actual[key]) is type(expected_value)
            and actual[key] == expected_value
            for key, expected_value in expected.items()
        )
    )


def canvas_policy_variant_for_browser_binding(binding: Any) -> str:
    """Select exactly one Canvas Policy v3 variant or fail closed."""

    matches: list[str] = []
    if any(
        _browser_binding_matches_exact(binding, expected)
        for expected in LEGACY_BROWSER_BINDINGS
    ):
        matches.append(LEGACY_CANVAS_POLICY_VARIANT)
    if any(
        _browser_binding_matches_exact(binding, expected)
        for expected in (
            DETERMINISTIC_CANVAS_BROWSER_BINDING,
            FORMAL_R1_CANVAS_BROWSER_BINDING,
            FORMAL_R1_V2_CANVAS_BROWSER_BINDING,
            FORMAL_R1_V3_CANVAS_BROWSER_BINDING,
        )
    ):
        matches.append(DETERMINISTIC_CANVAS_POLICY_VARIANT)
    if len(matches) != 1:
        raise ArtifactIntegrityError(
            "browserBinding does not select exactly one approved Canvas Policy v3 variant"
        )
    return matches[0]


def _canvas_policy_fields_for_browser_binding(binding: Any) -> tuple[list[str], dict]:
    variant = canvas_policy_variant_for_browser_binding(binding)
    if variant == DETERMINISTIC_CANVAS_POLICY_VARIANT:
        return (
            list(DETERMINISTIC_SESSION_VARIABLE_SIGNAL_KEYS),
            dict(DETERMINISTIC_CANVAS_CLASSIFICATION),
        )
    return list(SESSION_VARIABLE_SIGNAL_KEYS), dict(CANVAS_CLASSIFICATION)


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
        key
        for key in set(disk) & set(sent)
        if type(disk[key]) is not type(sent[key]) or disk[key] != sent[key]
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


def _managed_voice_errors(voices: Any) -> list[str]:
    errors: list[str] = []
    _check_value(voices, VOICES, "resolvedConfig.voices", errors)
    if type(voices) is list:
        defaults = sum(
            type(voice) is dict and voice.get("isDefault") is True
            for voice in voices
        )
        if defaults != 1:
            errors.append(
                f"resolvedConfig.voices must contain exactly one default, got {defaults}"
            )
    return errors


def apply_voices_policy(config: dict, voices_mode: str) -> dict:
    """Apply the closed Artifact voices policy to a resolved config."""

    if voices_mode == VOICES_MODE_MANAGED:
        errors = _managed_voice_errors(config.get("voices"))
        if errors:
            raise ArtifactIntegrityError("invalid managed voices: " + "; ".join(errors))
        config.update(VOICE_DERIVED_CONFIG)
    elif voices_mode == VOICES_MODE_NATIVE:
        for key in ("voices", *VOICE_DERIVED_CONFIG):
            config.pop(key, None)
    else:
        raise ValueError(f"unknown voices mode: {voices_mode!r}")
    return config


def identity_policy(
    target_os: str = "linux",
    font_mode: str = "inherit",
    window: tuple[int, int] = (1280, 800),
    locale: str = "en-US",
    ff_version: int = 152,
    timezone_mode: str = "fixed",
    browser_binding: dict | None = None,
    voices_mode: str | None = VOICES_MODE_MANAGED,
    gpc_policy: str = GPC_POLICY_MANAGED_OPT_OUT,
    schema_version: int | None = None,
) -> dict:
    if schema_version is None:
        schema_version = 3 if voices_mode is None else 5
    if schema_version not in (3, 4, 5):
        raise ValueError(f"unknown identity policy schema version: {schema_version!r}")
    if schema_version == 3 and voices_mode is not None:
        raise ValueError("v3 policy cannot declare voicesMode")
    if schema_version in (4, 5) and voices_mode not in (
        VOICES_MODE_MANAGED,
        VOICES_MODE_NATIVE,
    ):
        raise ValueError(f"{schema_version} policy requires a voices mode")
    if schema_version == 5:
        if type(ff_version) is not int or ff_version < 135:
            raise ValueError("Artifact/Policy v5 requires ffVersion >= 135")
        if gpc_policy not in (GPC_POLICY_NATIVE, GPC_POLICY_MANAGED_OPT_OUT):
            raise ValueError(f"unknown gpc policy: {gpc_policy!r}")
    elif gpc_policy != GPC_POLICY_MANAGED_OPT_OUT:
        raise ValueError("gpc_policy is only available for Artifact/Policy v5")
    if browser_binding is None:
        # Preserve the legacy default for existing callers and fixtures. New
        # artifact generation always supplies its resolved browser binding.
        session_variable_fields = list(SESSION_VARIABLE_SIGNAL_KEYS)
        canvas_classification = dict(CANVAS_CLASSIFICATION)
    else:
        session_variable_fields, canvas_classification = (
            _canvas_policy_fields_for_browser_binding(browser_binding)
        )
    if schema_version == 3:
        stable_website_fields = list(STABLE_WEBSITE_SIGNAL_KEYS)
        required_config_keys = sorted(REQUIRED_CONFIG_KEYS)
    elif schema_version == 4 and voices_mode == VOICES_MODE_NATIVE:
        stable_website_fields = list(V4_NATIVE_STABLE_WEBSITE_SIGNAL_KEYS)
        required_config_keys = sorted(V4_NATIVE_CONFIG_KEYS)
    elif schema_version == 4:
        stable_website_fields = list(STABLE_WEBSITE_SIGNAL_KEYS)
        required_config_keys = sorted(V4_MANAGED_CONFIG_KEYS)
    elif voices_mode == VOICES_MODE_NATIVE:
        stable_website_fields = _v5_stable_website_fields(voices_mode, gpc_policy)
        required_config_keys = sorted(_v5_config_keys(voices_mode, gpc_policy))
    else:
        stable_website_fields = _v5_stable_website_fields(voices_mode, gpc_policy)
        required_config_keys = sorted(_v5_config_keys(voices_mode, gpc_policy))
    policy = {
        "schema": {
            3: POLICY_SCHEMA_V3,
            4: POLICY_SCHEMA_V4,
            5: POLICY_SCHEMA_V5,
        }[schema_version],
        "version": schema_version,
        "targetOs": target_os,
        "fontMode": font_mode,
        "window": list(window),
        "locale": locale,
        "ffVersion": ff_version,
        "timezoneMode": timezone_mode,
        "stableWebsiteFields": stable_website_fields,
        "sessionVariableFields": session_variable_fields,
        "unavailableFields": dict(UNAVAILABLE_SIGNALS),
        "canvasClassification": canvas_classification,
        "requiredConfigKeys": required_config_keys,
        "canonicalJsonRule": CANONICAL_JSON_RULE,
    }
    if schema_version in (4, 5):
        policy["voicesMode"] = voices_mode
    if schema_version == 5:
        policy[GPC_POLICY_KEY] = gpc_policy
    return policy


def _is_rfc3339_utc_z(value: Any) -> bool:
    """Strict RFC 3339 UTC timestamp normalized to 'Z' (e.g.
    2026-08-04T06:05:24.485917Z). Offsets other than UTC and non-Z spellings
    are rejected; writers must normalize to Z. The explicit regex rejects
    space separators, basic form, missing seconds, and non-UTC offsets; the
    date parse then rejects impossible calendar values."""
    if type(value) is not str or not RFC3339_UTC_Z_RE.fullmatch(value):
        return False
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        return False
    return parsed.tzinfo is not None and parsed.utcoffset() == timedelta(0)


# --------------------------------------------------------------------------
# Strict validation
# --------------------------------------------------------------------------


def validate_artifact_strict(artifact: dict) -> None:
    errors: list[str] = []
    artifact_schema = artifact.get("schema")
    policy = artifact.get("policy")
    voices_mode = policy.get("voicesMode") if isinstance(policy, dict) else None

    gpc_policy = policy.get(GPC_POLICY_KEY) if isinstance(policy, dict) else None
    if artifact_schema == ARTIFACT_SCHEMA_V5:
        artifact_spec = _v5_artifact_spec(voices_mode, gpc_policy)
    elif artifact_schema == ARTIFACT_SCHEMA_V4 and voices_mode == VOICES_MODE_NATIVE:
        artifact_spec = V4_NATIVE_TOP_LEVEL_SPEC
    elif artifact_schema == ARTIFACT_SCHEMA_V4:
        artifact_spec = V4_MANAGED_TOP_LEVEL_SPEC
    else:
        artifact_spec = TOP_LEVEL_SPEC
    _check_value(artifact, artifact_spec, "artifact", errors)

    if artifact_schema not in (
        ARTIFACT_SCHEMA_V3,
        ARTIFACT_SCHEMA_V4,
        ARTIFACT_SCHEMA_V5,
    ):
        errors.append(
            f"schema must be one of {ARTIFACT_SCHEMA_V3!r}, "
            f"{ARTIFACT_SCHEMA_V4!r}, or {ARTIFACT_SCHEMA_V5!r}"
        )
    artifact_id = artifact.get("artifactId")
    if not isinstance(artifact_id, str) or not ARTIFACT_ID_RE.match(artifact_id):
        errors.append(f"artifactId {artifact_id!r} does not match {ARTIFACT_ID_RE.pattern}")
    generated_by = artifact.get("generatedBy")
    if type(generated_by) is not str or not generated_by.strip():
        errors.append("generatedBy must be a non-empty string")
    if not _is_rfc3339_utc_z(artifact.get("generatedAtUtc")):
        errors.append("generatedAtUtc must be strict RFC 3339 UTC normalized to Z")
    browser_release = artifact.get("browserRelease")
    if type(browser_release) is not str or not browser_release.strip():
        errors.append("browserRelease must be a non-empty string")
    for digest_key in ("configuredIdentityDigest", "canonicalDigest"):
        digest_value = artifact.get(digest_key)
        if (
            type(digest_value) is not str
            or not digest_value.startswith("sha256:")
            or not HEX64_RE.fullmatch(digest_value[len("sha256:"):])
        ):
            errors.append(f"{digest_key} must be sha256:<64 hex chars>")

    binding = artifact.get("browserBinding")
    expected_session_variable_fields: list[str] | None = None
    expected_canvas_classification: dict | None = None
    try:
        (
            expected_session_variable_fields,
            expected_canvas_classification,
        ) = _canvas_policy_fields_for_browser_binding(binding)
    except ArtifactIntegrityError as exc:
        errors.append(str(exc))

    if isinstance(policy, dict):
        expected_policy_schema = {
            ARTIFACT_SCHEMA_V3: POLICY_SCHEMA_V3,
            ARTIFACT_SCHEMA_V4: POLICY_SCHEMA_V4,
            ARTIFACT_SCHEMA_V5: POLICY_SCHEMA_V5,
        }.get(artifact_schema)
        expected_policy_version = {
            ARTIFACT_SCHEMA_V3: 3,
            ARTIFACT_SCHEMA_V4: 4,
            ARTIFACT_SCHEMA_V5: 5,
        }.get(artifact_schema)
        if policy.get("schema") != expected_policy_schema:
            errors.append(f"policy.schema must be {expected_policy_schema!r}")
        if policy.get("version") != expected_policy_version:
            errors.append(f"policy.version must be {expected_policy_version}")
        if artifact_schema in (ARTIFACT_SCHEMA_V4, ARTIFACT_SCHEMA_V5) and voices_mode not in (
            VOICES_MODE_MANAGED,
            VOICES_MODE_NATIVE,
        ):
            errors.append("policy.voicesMode must be managed or native")
        if artifact_schema == ARTIFACT_SCHEMA_V5:
            if policy.get(GPC_POLICY_KEY) not in (
                GPC_POLICY_NATIVE,
                GPC_POLICY_MANAGED_OPT_OUT,
            ):
                errors.append(
                    "policy.navigator.gpcPolicy must be native or managed-opt-out"
                )
            if isinstance(policy.get("ffVersion"), int) and policy["ffVersion"] < 135:
                errors.append("Artifact/Policy v5 requires ffVersion >= 135")
        if policy.get("targetOs") not in ("linux", "macos", "windows"):
            errors.append("policy.targetOs unsupported")
        if policy.get("fontMode") not in ("inherit", "managed"):
            errors.append("policy.fontMode must be inherit or managed")
        if policy.get("timezoneMode") not in ("fixed", "network-bound"):
            errors.append("policy.timezoneMode must be fixed or network-bound")
        if artifact_schema == ARTIFACT_SCHEMA_V5:
            expected_stable_fields = _v5_stable_website_fields(voices_mode, gpc_policy)
        elif artifact_schema == ARTIFACT_SCHEMA_V4:
            expected_stable_fields = (
                V4_NATIVE_STABLE_WEBSITE_SIGNAL_KEYS
                if voices_mode == VOICES_MODE_NATIVE
                else STABLE_WEBSITE_SIGNAL_KEYS
            )
        else:
            expected_stable_fields = STABLE_WEBSITE_SIGNAL_KEYS
        if policy.get("stableWebsiteFields") != expected_stable_fields:
            errors.append("policy.stableWebsiteFields does not match the canonical list")
        if (
            expected_session_variable_fields is not None
            and policy.get("sessionVariableFields")
            != expected_session_variable_fields
        ):
            errors.append("policy.sessionVariableFields does not match the canonical list")
        if policy.get("unavailableFields") != UNAVAILABLE_SIGNALS:
            errors.append("policy.unavailableFields does not match the canonical map")
        if (
            expected_canvas_classification is not None
            and policy.get("canvasClassification")
            != expected_canvas_classification
        ):
            errors.append("policy.canvasClassification does not match the canonical map")
        if artifact_schema == ARTIFACT_SCHEMA_V5:
            expected_config_keys = sorted(_v5_config_keys(voices_mode, gpc_policy))
        elif artifact_schema == ARTIFACT_SCHEMA_V4 and voices_mode == VOICES_MODE_NATIVE:
            expected_config_keys = sorted(V4_NATIVE_CONFIG_KEYS)
        elif artifact_schema == ARTIFACT_SCHEMA_V4:
            expected_config_keys = sorted(V4_MANAGED_CONFIG_KEYS)
        else:
            expected_config_keys = sorted(REQUIRED_CONFIG_KEYS)
        if policy.get("requiredConfigKeys") != expected_config_keys:
            errors.append("policy.requiredConfigKeys does not match the canonical key set")

    config = artifact.get("resolvedConfig")
    if isinstance(config, dict):
        if (
            artifact_schema in (ARTIFACT_SCHEMA_V4, ARTIFACT_SCHEMA_V5)
            and voices_mode == VOICES_MODE_MANAGED
        ):
            errors.extend(_managed_voice_errors(config.get("voices")))
            for key, expected in VOICE_DERIVED_CONFIG.items():
                if (
                    key not in config
                    or type(config[key]) is not type(expected)
                    or config[key] != expected
                ):
                    errors.append(f"resolvedConfig.{key} must be exactly {expected!r}")
        elif (
            artifact_schema in (ARTIFACT_SCHEMA_V4, ARTIFACT_SCHEMA_V5)
            and voices_mode == VOICES_MODE_NATIVE
        ):
            for key in ("voices", *VOICE_DERIVED_CONFIG):
                if key in config:
                    errors.append(f"native voices mode forbids resolvedConfig.{key}")
        if artifact_schema == ARTIFACT_SCHEMA_V5:
            if gpc_policy == GPC_POLICY_MANAGED_OPT_OUT:
                if config.get(GPC_CONFIG_KEY) is not True:
                    errors.append(
                        "managed-opt-out requires resolvedConfig.navigator.globalPrivacyControl "
                        "to be exactly true"
                    )
            elif gpc_policy == GPC_POLICY_NATIVE and GPC_CONFIG_KEY in config:
                errors.append(
                    "native gpcPolicy forbids resolvedConfig.navigator.globalPrivacyControl"
                )
        canvas_seed = config.get("canvas:seed")
        if type(canvas_seed) is int and not 0 <= canvas_seed <= 0xFFFFFFFF:
            errors.append(
                "resolvedConfig.canvas:seed must be an unsigned 32-bit integer"
            )
        if (
            isinstance(config.get("screen.availTop"), int)
            and isinstance(config.get("screen.availHeight"), int)
            and isinstance(config.get("screen.height"), int)
        ):
            if config["screen.availTop"] + config["screen.availHeight"] > config["screen.height"]:
                errors.append("screen avail geometry inconsistent")

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
        if artifact_schema != ARTIFACT_SCHEMA_V5 and (
            type(declared.get("devicePixelRatio")) is not int
            or declared.get("devicePixelRatio") != 1
        ):
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


def _strict_json_loads(raw: bytes) -> Any:
    """Strict JSON parse for the persistent identity format: recursively
    rejects duplicate object keys and NaN/Infinity constants, so every
    conforming parser (Python now, Rust later) sees the same object."""

    def reject_duplicates(pairs: list[tuple[str, Any]]) -> dict:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f"duplicate JSON key: {key}")
            result[key] = value
        return result

    def reject_constant(token: str) -> None:
        raise ValueError(f"invalid JSON constant: {token}")

    try:
        return json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=reject_duplicates,
            parse_constant=reject_constant,
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        raise ArtifactIntegrityError(f"artifact is not strict JSON: {exc}") from exc


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
    artifact = _strict_json_loads(raw)
    if type(artifact) is not dict:
        raise ArtifactIntegrityError(
            f"artifact must be a JSON object, got {type(artifact).__name__}"
        )
    schema = artifact.get("schema")
    if schema not in (
        ARTIFACT_SCHEMA_V3,
        ARTIFACT_SCHEMA_V4,
        ARTIFACT_SCHEMA_V5,
    ):
        if isinstance(schema, str) and schema.startswith(
            "verisilo-camoufox-resolved-identity/"
        ):
            raise UnsupportedSchemaVersionError(
                f"unsupported artifact schema version: {schema!r}; "
                f"expected one of {ARTIFACT_SCHEMA_V3!r}, "
                f"{ARTIFACT_SCHEMA_V4!r}, or {ARTIFACT_SCHEMA_V5!r}"
            )
        raise ArtifactIntegrityError(f"artifact schema mismatch: {schema!r}")
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
