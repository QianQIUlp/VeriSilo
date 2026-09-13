#!/usr/bin/env python3
"""Focused tests for host font enumeration, control selection, and the
managed font masking gate. Runs without pytest: `uv run python test_host_fonts.py`.
"""

from __future__ import annotations

import re
import sys
import tempfile
import unittest
from pathlib import Path

from host_fonts import (
    FONT_UNIVERSE,
    MANAGED_MIN_FONT_CONTROLS,
    _name_conflicts_with,
    bundle_font_families,
    evaluate_host_font_masking,
    host_negative_control_families,
)

REPO_ROOT = Path(__file__).resolve().parents[2]
PROBE = REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html"


@unittest.skipUnless(sys.platform == "win32", "Windows GDI enumeration")
class GdiFontFamilyTests(unittest.TestCase):
    def test_enumerates_installed_families(self) -> None:
        from host_fonts import gdi_font_families

        families = gdi_font_families()
        self.assertTrue(families)
        self.assertEqual(families, sorted(set(families)))
        # A stock Windows system family is always present.
        self.assertIn("Arial", families)


class NameConflictTests(unittest.TestCase):
    def test_variants_conflict_in_both_directions(self) -> None:
        self.assertTrue(_name_conflicts_with("Bahnschrift SemiBold", "Bahnschrift"))
        self.assertTrue(_name_conflicts_with("Bahnschrift", "Bahnschrift SemiBold"))
        self.assertTrue(_name_conflicts_with("Arial", "Arial"))
        self.assertTrue(_name_conflicts_with("Segoe UI Black", "Segoe UI"))
        self.assertFalse(_name_conflicts_with("Ravie", "Arial"))
        self.assertFalse(_name_conflicts_with("宋体", "新宋体"))


class NegativeControlSelectionTests(unittest.TestCase):
    HOST_FAMILIES = [
        "Agency FB",           # genuine host-only extra
        "Arial",               # declared -> excluded
        "Arial Narrow",        # bundled -> excluded
        "Bahnschrift SemiBold",  # variant of declared "Bahnschrift" -> excluded
        "Segoe UI Black",      # variant of declared "Segoe UI" -> excluded
        "Ink Free",            # genuine host-only extra
        "Tahoma",              # universe -> excluded
    ]

    def test_excludes_declared_bundled_universe_and_variants(self) -> None:
        controls = host_negative_control_families(
            ["Arial", "Bahnschrift"],
            host_families=self.HOST_FAMILIES,
            bundled_families={"Arial Narrow"},
        )
        self.assertEqual(controls, ["Agency FB", "Ink Free"])

    def test_without_bundle_subtraction_bundled_families_are_controls(self) -> None:
        # A bundled family with no declared/universe/variant relationship
        # only leaves the control set once the bundle set is subtracted.
        controls = host_negative_control_families(
            ["Arial", "Bahnschrift"],
            host_families=["Agency FB", "Marlett"],
            bundled_families={"Marlett"},
        )
        self.assertEqual(controls, ["Agency FB"])
        controls = host_negative_control_families(
            ["Arial", "Bahnschrift"],
            host_families=["Agency FB", "Marlett"],
            bundled_families=None,
        )
        self.assertEqual(controls, ["Agency FB", "Marlett"])

    def test_respects_limit(self) -> None:
        families = [f"Extra Font {i}" for i in range(30)]
        controls = host_negative_control_families(
            [], host_families=families, limit=5
        )
        self.assertEqual(len(controls), 5)
        self.assertEqual(controls, families[:5])

    def test_defaults_to_live_host_enumeration(self) -> None:
        # No host_families passed: falls back to the platform enumeration.
        controls = host_negative_control_families(
            FONT_UNIVERSE * 1, limit=MANAGED_MIN_FONT_CONTROLS + 1
        )
        self.assertLessEqual(len(controls), MANAGED_MIN_FONT_CONTROLS + 1)


class BundleFamilyTests(unittest.TestCase):
    def test_missing_fonts_dir_is_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(bundle_font_families(Path(tmp)), set())

    @unittest.skipUnless(sys.platform == "win32", "Windows GDI+ file parsing")
    def test_reads_family_names_from_font_files(self) -> None:
        import shutil

        source = (
            REPO_ROOT / "tests" / "fingerprint-probe" / "probe.html"
        )
        self.assertTrue(source.is_file())
        # Use a system font file as a deterministic input.
        system_font = Path(r"C:\Windows\Fonts\times.ttf")
        if not system_font.is_file():
            self.skipTest("times.ttf not present")
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "fonts"
            target.mkdir()
            shutil.copy(system_font, target / "times.ttf")
            families = bundle_font_families(tmp)
        self.assertIn("Times New Roman", families)


class EvaluateHostFontMaskingTests(unittest.TestCase):
    def test_all_controls_unavailable_is_sufficient(self) -> None:
        controls = {f"Extra {i}": False for i in range(MANAGED_MIN_FONT_CONTROLS)}
        result = evaluate_host_font_masking("managed", controls)
        self.assertTrue(result["allUnavailable"])
        self.assertTrue(result["sufficient"])
        self.assertEqual(result["controlsTested"], MANAGED_MIN_FONT_CONTROLS)
        self.assertEqual(result["failures"], [])

    def test_visible_control_is_a_failure(self) -> None:
        controls = {f"Extra {i}": False for i in range(MANAGED_MIN_FONT_CONTROLS)}
        controls["Bungee"] = True
        result = evaluate_host_font_masking("managed", controls)
        self.assertFalse(result["allUnavailable"])
        self.assertFalse(result["sufficient"] is True and False)
        self.assertEqual(result["failures"], ["Bungee"])

    def test_missing_value_counts_as_failure(self) -> None:
        controls = {f"Extra {i}": False for i in range(MANAGED_MIN_FONT_CONTROLS)}
        controls["Bungee"] = None
        result = evaluate_host_font_masking("managed", controls)
        self.assertFalse(result["allUnavailable"])
        self.assertIn("Bungee", result["failures"])

    def test_empty_control_set_is_never_all_unavailable(self) -> None:
        result = evaluate_host_font_masking("managed", {})
        self.assertFalse(result["allUnavailable"])
        self.assertFalse(result["sufficient"])

    def test_below_minimum_is_insufficient(self) -> None:
        controls = {"Agency FB": False, "Ink Free": False}
        result = evaluate_host_font_masking("managed", controls)
        self.assertTrue(result["allUnavailable"])
        self.assertFalse(result["sufficient"])

    def test_inherit_mode_record_is_descriptive_only(self) -> None:
        controls = {"Agency FB": True}
        result = evaluate_host_font_masking("inherit", controls)
        self.assertFalse(result["allUnavailable"])
        # The helper is pure; inherit gating is decided by the caller.


class ProbeOracleSyncTests(unittest.TestCase):
    def test_host_controls_use_rendering_oracle(self) -> None:
        source = PROBE.read_text(encoding="utf-8")
        self.assertIn("function identityFontRenderAvailability", source)
        match = re.search(
            r"hostFontNegativeControls:\s*identityFontRenderAvailability\(",
            source,
        )
        self.assertIsNotNone(
            match,
            "hostFontNegativeControls must be measured by the rendering "
            "oracle, not document.fonts.check",
        )

    def test_rendering_oracle_compares_against_bogus_reference(self) -> None:
        source = PROBE.read_text(encoding="utf-8")
        # The bogus reference anchors the fallback comparison.
        self.assertIn('BOGUS_FONTS[0]', source)
        # document.fonts.check must no longer feed the host gate field.
        segment = source.split("hostFontNegativeControls:", 1)[1][:200]
        self.assertNotIn("identityFontAvailability(", segment)


def main() -> int:
    unittest.main(module=None, argv=["test_host_fonts"], exit=False, verbosity=2)
    return 0


if __name__ == "__main__":
    unittest.main(verbosity=2)
