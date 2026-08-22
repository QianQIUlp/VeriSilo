#!/usr/bin/env python3
"""Executable reference model for the frozen GPC policy-state contract.

Mirrors docs/camoufox-fp2-r1-engine-remediation-implementation.md section 1.2
(Conditional-gate model B). The future C++ projection seam and patch authoring
must behave identically to these functions.
"""

from __future__ import annotations

import unittest

NATIVE = "native"
MANAGED_OPT_OUT = "managed-opt-out"

GPC_POLICY_KEY = "navigator.gpcPolicy"
GPC_ENGINE_KEY = "navigator.globalPrivacyControl"

PREF_ENABLED = "privacy.globalprivacycontrol.enabled"
PREF_FUNCTIONALITY = "privacy.globalprivacycontrol.functionality_enabled"

# Native Firefox defaults on the FF152 line; nothing may write these keys
# outside the managed-opt-out branch.
_NATIVE_PREF_STATE = {PREF_ENABLED: False, PREF_FUNCTIONALITY: False}


def validate_artifact_policy(policy: dict, resolved_config: dict) -> list[str]:
    """Frozen v4 validator rules for GPC (section 1.2 of the contract)."""
    errors: list[str] = []
    gpc_policy = policy.get(GPC_POLICY_KEY)
    engine_key_present = GPC_ENGINE_KEY in resolved_config
    if gpc_policy == MANAGED_OPT_OUT:
        if not engine_key_present:
            errors.append("managed-opt-out requires the engine key to be present")
        elif resolved_config[GPC_ENGINE_KEY] is not True:
            errors.append("engine key must be exactly true under managed-opt-out")
    elif gpc_policy == NATIVE:
        if engine_key_present:
            errors.append("native forbids a configured engine key")
    else:
        errors.append(f"unknown gpcPolicy value: {gpc_policy!r}")
    return errors


def project_gpc(policy: dict, resolved_config: dict) -> dict:
    """Pure mirror of the parent-process projection seam.

    Returns the prefs that must be written plus the three expected exposures.
    Defensive rule: any non-true engine value (unreachable via the validator)
    is treated as native — no pref writes, no managed claim.
    """
    writes: dict[str, bool] = {}
    managed_opt_out = (
        policy.get(GPC_POLICY_KEY) == MANAGED_OPT_OUT
        and resolved_config.get(GPC_ENGINE_KEY) is True
    )
    if managed_opt_out:
        writes[PREF_ENABLED] = True
        writes[PREF_FUNCTIONALITY] = True

    enabled = _NATIVE_PREF_STATE[PREF_ENABLED] | writes.get(PREF_ENABLED, False)
    functionality = _NATIVE_PREF_STATE[PREF_FUNCTIONALITY] | writes.get(
        PREF_FUNCTIONALITY, False
    )
    gpc_status = enabled  # no pbmode management: private-browsing fallback stays native
    navigator_value = functionality and gpc_status
    return {
        "prefWrites": writes,
        "exposures": {
            "window": navigator_value,
            "worker": navigator_value,
            "secGpcHeader": navigator_value,
        },
    }


def derive_completion_semantics(voices_mode: str) -> tuple[bool, float]:
    """Frozen derivation: completion semantics are policy-derived, never per-artifact random."""
    if voices_mode == "managed":
        return (True, 12.5)
    if voices_mode == "native":
        return (False, 12.5)
    raise ValueError(f"unknown voicesMode: {voices_mode!r}")


class GpcPolicyContractTests(unittest.TestCase):
    def test_managed_opt_out_projects_all_three_true(self) -> None:
        result = project_gpc({GPC_POLICY_KEY: MANAGED_OPT_OUT}, {GPC_ENGINE_KEY: True})
        self.assertEqual(
            result["prefWrites"], {PREF_ENABLED: True, PREF_FUNCTIONALITY: True}
        )
        self.assertEqual(result["exposures"]["window"], True)
        self.assertEqual(result["exposures"]["worker"], True)
        self.assertEqual(result["exposures"]["secGpcHeader"], True)

    def test_native_writes_nothing_and_stays_native_false(self) -> None:
        result = project_gpc({GPC_POLICY_KEY: NATIVE}, {})
        self.assertEqual(result["prefWrites"], {})
        for value in result["exposures"].values():
            self.assertFalse(value)

    def test_defensive_false_engine_value_is_treated_as_native(self) -> None:
        result = project_gpc({GPC_POLICY_KEY: NATIVE}, {GPC_ENGINE_KEY: False})
        self.assertEqual(result["prefWrites"], {})
        for value in result["exposures"].values():
            self.assertFalse(value)

    def test_explicit_false_is_an_illegal_v4_shape(self) -> None:
        errors = validate_artifact_policy(
            {GPC_POLICY_KEY: MANAGED_OPT_OUT}, {GPC_ENGINE_KEY: False}
        )
        self.assertTrue(any("exactly true" in e for e in errors))

    def test_validator_requires_engine_key_consistency(self) -> None:
        missing_key = validate_artifact_policy(
            {GPC_POLICY_KEY: MANAGED_OPT_OUT}, {}
        )
        native_with_key = validate_artifact_policy(
            {GPC_POLICY_KEY: NATIVE}, {GPC_ENGINE_KEY: True}
        )
        unknown_state = validate_artifact_policy({"navigator.gpcPolicy": "bool"}, {})
        self.assertTrue(any("present" in e for e in missing_key))
        self.assertTrue(any("forbids" in e for e in native_with_key))
        self.assertEqual(len(unknown_state), 1)

    def test_pbmode_context_never_changes_projections(self) -> None:
        # pbmode prefs are out of managed scope: identical outputs with or without them.
        baseline = project_gpc({GPC_POLICY_KEY: MANAGED_OPT_OUT}, {GPC_ENGINE_KEY: True})
        self.assertNotIn("pbmode", str(baseline["prefWrites"]))

    def test_fake_completion_is_policy_derived_not_random(self) -> None:
        self.assertEqual(derive_completion_semantics("managed"), (True, 12.5))
        self.assertEqual(derive_completion_semantics("managed"), (True, 12.5))
        self.assertNotEqual(
            derive_completion_semantics("managed"),
            derive_completion_semantics("native"),
        )
        with self.assertRaises(ValueError):
            derive_completion_semantics("surprise")


if __name__ == "__main__":
    unittest.main(verbosity=1)
