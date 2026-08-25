#!/usr/bin/env python3
"""Static no-browser regressions for the authored R1 patches.

Locks (contract docs/camoufox-fp2-r1-engine-remediation-implementation.md):
- T1 golden native restore (0004 pure removal, exact override lines)
- T2 single GPC projection writer across the series
- DIAGNOSTIC-ONLY marker format (machine-greppable first line) and its
  exclusion from any formal r1 series directory; build-driver rejection of
  marked files is additionally enforced at diagnostic-build authoring time
- behavioral-invariant bans for 9000 instrumentation
- SHA-256 pins for patch blobs, derivation sections, and every seam pre/post
  image produced by build/r1-diag-v1/author_patches.py
"""

from __future__ import annotations

import hashlib
import re
import unittest
from pathlib import Path

HOST_DIR = Path(__file__).resolve().parent
PATCH_DIR = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1-diag"
FORMAL_DIR = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1"
SECTIONS_DIR = HOST_DIR / "build" / "r1-diag-v1" / "upstream-sections"

P_0003 = "0003-verisilo-gpc-canonical-pref-projection.patch"
P_0003A = "0003a-verisilo-gpc-preferences-namespace-compile-repair.patch"
P_0004 = "0004-verisilo-remove-worker-gpc-mask-override.patch"
P_0005 = "0005-verisilo-voices-final.patch"
P_9000 = "9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch"

PINNED_PATCHES = {
    P_0003: "3a13cb7923d7cc4da4bbd0a2761d9a48e9fe5267aea98661e22c857629a8e83b",
    P_0003A: "c2f9a9f88ba8aeb610eb1cb29f2515f1d79fcf582393397a571bc3206889588c",
    P_0004: "5598a95e1fa9bd1792bdff91731779a6ec246b8db7c494c1685dbce29adb7185",
    P_9000: "1bc478373f56d774487e20d73d847ed2de82149728d696e83627fa91b9d7b8f8",
}
PINNED_FORMAL_PATCHES = {
    P_0005: "998094f061fc34e0e190c1cc48524a9514df398656a0d3bbcb1ec0cd38d54bec",
}
FORMAL_ORDER = ["0000", "0001", "0002", "0003", "0003a", "0004", "0005"]

PINNED_SECTIONS = {
    "fpin-worker.patch": "87af0307e7476758ff88588da5031edbe65a7737bdfd3ab5dc23dbe40faf98b5",
    "navspoof-worker.patch": "7e00efc1069c0f5b67cbe9d6cda6945c00016e5b93b8e341db3d380aff8f759d",
    "voice-registry.patch": "72f66bba096e60137d25d0076b4a2738868caf07ecf02648b3f7b2904ef7585a",
    "sv-synth.patch": "4ce167f9e973068c13759323fe06d3b16a9b5257e87032061ba396858d148930",
}

PINNED_SEAMS = {
    "toolkit-xre-nsAppRunner-pre": "f9c0dfb11cf20ab1864f3d5c791f88ec26e24b154474e93c0d8898f712099d11",
    "toolkit-xre-nsAppRunner-post": "7847e88093beeff74aa8a7e89f5e5f1e3ea0d6b1f9dece21f97387940fbe8b94",
    "xre-mozbuild-pre": "03442255ef528f22927c17f5769b089e37a79c79c9a9bec0004b2139dba4a3ba",
    "xre-mozbuild-post": "831f388c8b16c162f21d5ab034329dd837f5e542b85fed1b8c277cbce3233131",
    "GpcProjection-compile-repair-pre": "ab0b4c26e628a74d0ef4bac66d35bc6b0e9aee45cd67ad6bd5e5da91b609cf3f",
    "GpcProjection-compile-repair-post": "364655669418c106f80f030a7a48797dbdbca1030c0d29e4e91c841129999bda",
    "WorkerNavigator-pre": "b927fb42169159a6e001e442c7b4c0916f46d6b3b88c4d1baf5ef5d979be7f09",
    "WorkerNavigator-post": "693aee50dfa3ba44505656a0cdc5899753690df53ca76031b2b69c46ae0aa1d1",
    "nsSynthVoiceRegistry-pre": "9eac4b53804588d57f4309ed0e6f8d9c971ca876473efd698d30d90a4bf18a3f",
    "nsSynthVoiceRegistry-post": "19ccd59ce3f0601ebaf0fdf5b05e4a4c6192a03540d9cd962a8baf2999f91864",
    "SpeechSynthesis-pre": "54611fee854db93922b3c36ad6ff014d07eaa3ea68abb49bc1bbd9392a7ce75f",
    "SpeechSynthesis-post": "b2e908b78c2f442aaed5fafd7397855968d2503385d6bedd5db865406d6946b4",
    "SapiService-pre": "d316d9f48c6123aed43612b0d21e8d64a2ea388c6bd40cad45b2848f42066274",
    "SapiService-post": "42db11a25b6b089258e670c62adde8d2ecbe305c0a8a8429b14383f5987eb9e9",
    "SpeechSynthesisParent-pre": "c6171e3689fab1789c459b924c7420786d2efed0caf2741747b910e0a3dcd61f",
    "SpeechSynthesisParent-post": "9b978c5a833ab8250610ad8167a2961e8ae23146a5b68f7075d5c6a0d008ec84",
    "windows-mozbuild-pre": "db7ad453391005b2e93da3816bef2453a283e42d09572faa918ff5def52a9e6f",
    "windows-mozbuild-post": "84fb47618f1b1de47132ae6c8fcd1b7e0c2b39d400668193e6831c1b18b5ee83",
}

EVENT_IDS = [
    "E1_mvoices_parsed",
    "E3a_managed_batch_begin",
    "E3b_managed_batch_end",
    "E2a_sapi_init_begin",
    "E2b_sapi_init_end",
    "E4_sendinit_snapshot",
    "E5_send_voice_added",
    "E6_recv_initial_voices",
    "E6_recv_add_voice",
    "E7_getvoices",
]


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def plus_lines(text: str) -> list[str]:
    return [l for l in text.splitlines() if l.startswith("+") and not l.startswith("+++")]


def minus_lines(text: str) -> list[str]:
    return [l for l in text.splitlines() if l.startswith("-") and not l.startswith("---")]


def ctx_lines(text: str) -> list[str]:
    return [l for l in text.splitlines() if l.startswith(" ") and not l.startswith(" " * 4 + "@@")]


class EngineRemediationPatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.patches = {name: (PATCH_DIR / name).read_text(encoding="utf-8")
                       for name in PINNED_PATCHES}
        cls.formal_patches = {
            name: (FORMAL_DIR / name).read_text(encoding="utf-8")
            for name in PINNED_FORMAL_PATCHES
        }

    def test_patch_blobs_pinned(self) -> None:
        for name, digest in PINNED_PATCHES.items():
            self.assertTrue((PATCH_DIR / name).is_file(), name)
            self.assertEqual(sha256_text(self.patches[name]), digest, name)

    def test_upstream_sections_pinned(self) -> None:
        for name, digest in PINNED_SECTIONS.items():
            path = SECTIONS_DIR / name
            self.assertTrue(path.is_file(), name)
            self.assertEqual(sha256_text(path.read_text(encoding="utf-8")), digest, name)

    def test_formal_patch_blobs_pinned(self) -> None:
        for name, digest in PINNED_FORMAL_PATCHES.items():
            self.assertTrue((FORMAL_DIR / name).is_file(), name)
            self.assertEqual(sha256_text(self.formal_patches[name]), digest, name)

    # ---------------- 0003 ----------------

    def test_0003_single_projection_writer(self) -> None:
        plus = plus_lines(self.patches[P_0003])
        setbool = [l for l in plus if "SetBool(" in l and "globalprivacycontrol" in l]
        self.assertGreaterEqual(len(setbool), 1)
        self.assertEqual(
            sum(1 for l in plus if '"privacy.globalprivacycontrol.enabled"' in l), 1)
        self.assertEqual(
            sum(1 for l in plus
                if '"privacy.globalprivacycontrol.functionality_enabled"' in l), 1)
        self.assertFalse(any("pbmode" in l for l in plus))
        joined = "\n".join(plus)
        self.assertEqual(joined.count("ProjectGpcPolicyFromMaskConfig"), 2)
        self.assertEqual(
            sum(1 for l in plus
                if 'MaskConfig::GetBool("navigator.globalPrivacyControl")' in l), 1)
        self.assertTrue(any("std::call_once" in l for l in plus))
        self.assertTrue(any("XRE_IsParentProcess()" in l for l in plus))

    def test_0003a_is_namespace_only_compile_repair(self) -> None:
        text = self.patches[P_0003A]
        self.assertEqual(
            minus_lines(text),
            [
                '-    Preferences::SetBool("privacy.globalprivacycontrol.enabled", true);',
                "-    Preferences::SetBool(",
            ],
        )
        self.assertEqual(
            plus_lines(text),
            [
                '+    mozilla::Preferences::SetBool("privacy.globalprivacycontrol.enabled", true);',
                "+    mozilla::Preferences::SetBool(",
            ],
        )
        self.assertEqual(text.count("@@"), 2)
        self.assertNotIn("pbmode", text)
        self.assertNotIn("MaskConfig", "\n".join(plus_lines(text)))

    # ---------------- 0004 ----------------

    def test_0004_golden_removal_only(self) -> None:
        text = self.patches[P_0004]
        plus = plus_lines(text)
        self.assertEqual(plus, [], "0004 must be a pure removal")
        minus = minus_lines(text)
        self.assertEqual(minus, [
            '-  if (auto value = MaskConfig::GetBool("navigator.globalPrivacyControl");',
            "-      value.has_value())",
            "-    return value.value();",
        ])
        ctx = ctx_lines(text)
        self.assertIn(
            "   bool gpcStatus = StaticPrefs::privacy_globalprivacycontrol_enabled();",
            ctx)
        self.assertIn("     JSObject* jso = GetWrapper();", ctx)
        self.assertEqual(text.count("@@"), 2)

    # ---------------- 0005 ----------------

    def test_0005_is_the_frozen_single_file_constructor_guard(self) -> None:
        text = self.formal_patches[P_0005]
        target = "dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp"
        self.assertEqual(
            re.findall(r"^(?:---|\+\+\+) ([^\n]+)$", text, re.MULTILINE),
            [f"a/{target}", f"b/{target}"],
        )
        plus = plus_lines(text)
        self.assertEqual(
            plus,
            [
                '+#include "MaskConfig.hpp"',
                "+  if (MaskConfig::MVoices() &&",
                '+      MaskConfig::GetBool("voices:blockIfNotDefined").value_or(false) == true) {',
                "+    nsSynthVoiceRegistry::GetInstance();",
                "+  }",
            ],
        )
        for untouched in ("SendInit", "Recv", "SpeakImpl", "SapiService", "cache"):
            self.assertNotIn(untouched, text)
        self.assertEqual(text.count("@@"), 4)

    def test_formal_series_is_exact_and_rejects_9000(self) -> None:
        base_dir = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28"
        ids = [path.name.split("-", 1)[0] for path in sorted(base_dir.glob("*.patch"))]
        ids += [
            path.name.split("-", 1)[0]
            for path in sorted(PATCH_DIR.glob("000[34]*.patch"))
        ]
        ids += [
            path.name.split("-", 1)[0]
            for path in sorted(FORMAL_DIR.glob("*.patch"))
        ]
        self.assertEqual(ids, FORMAL_ORDER)
        self.assertEqual(
            sorted(path.name for path in FORMAL_DIR.glob("*.patch")), [P_0005]
        )
        self.assertNotIn("9000", ids)
        self.assertNotIn(
            "VERISILO-DIAGNOSTIC-MARKER", self.formal_patches[P_0005]
        )

    # ---------------- 9000 ----------------

    def test_9000_marker_and_event_completeness(self) -> None:
        text = self.patches[P_9000]
        lines = text.splitlines()
        self.assertEqual(lines[0], "# VERISILO-DIAGNOSTIC-MARKER: v1")
        self.assertIn("VERISILO-DIAGNOSTIC-ONLY", lines[1])
        for event in EVENT_IDS:
            with self.subTest(event=event):
                self.assertEqual(text.count(f'"{event}"'), 1)
        self.assertEqual(text.count("VsiDiagGetVoices"), 2)  # def + call site

    def test_9000_behavioral_invariant_bans(self) -> None:
        text = self.patches[P_9000]
        for banned in ["PR_Sleep(", "Sleep(", "WaitForSingleObject(",
                       "getVoices(", "blockIfNotDefined"]:
            self.assertNotIn(banned, text, banned)
        for line in plus_lines(text):
            self.assertLessEqual(len(line), 200, line)
        self.assertIn("512u", text)
        self.assertIn("compare_exchange_strong", text)
        self.assertIn('"e":"OVERFLOW"', text.replace('\\"', '"'))

    def test_9000_uri_hashing_only(self) -> None:
        text = self.patches[P_9000]
        direct_emit_uri = [
            l for l in plus_lines(text)
            if re.search(r"Emit\([^\n]*voiceURI\(\)", l)
        ]
        self.assertEqual(direct_emit_uri, [])
        self.assertEqual(text.count("VsiDiagEventUri"), 3)  # def + E5 + E6b
        self.assertFalse(any(".name()" in l for l in plus_lines(text)))

    def test_formal_series_excludes_diagnostics(self) -> None:
        first_0003 = self.patches[P_0003].splitlines()[0]
        first_0003a = self.patches[P_0003A].splitlines()[0]
        first_0004 = self.patches[P_0004].splitlines()[0]
        self.assertNotIn("VERISILO-DIAGNOSTIC-MARKER", first_0003)
        self.assertNotIn("VERISILO-DIAGNOSTIC-MARKER", first_0003a)
        self.assertNotIn("VERISILO-DIAGNOSTIC-MARKER", first_0004)
        if FORMAL_DIR.is_dir():
            for path in FORMAL_DIR.glob("*.patch"):
                self.assertNotIn(
                    "VERISILO-DIAGNOSTIC-MARKER",
                    path.read_text(encoding="utf-8"),
                    str(path),
                )

    def test_seam_digest_pins(self) -> None:
        # Pins recorded by build/r1-diag-v1/author_patches.py; recomputed there.
        self.assertEqual(PINNED_SEAMS["WorkerNavigator-post"],
                         "693aee50dfa3ba44505656a0cdc5899753690df53ca76031b2b69c46ae0aa1d1")
        self.assertEqual(PINNED_SEAMS["nsSynthVoiceRegistry-post"],
                         "19ccd59ce3f0601ebaf0fdf5b05e4a4c6192a03540d9cd962a8baf2999f91864")
        self.assertEqual(PINNED_SEAMS["toolkit-xre-nsAppRunner-post"],
                         "7847e88093beeff74aa8a7e89f5e5f1e3ea0d6b1f9dece21f97387940fbe8b94")
        self.assertEqual(PINNED_SEAMS["GpcProjection-compile-repair-pre"],
                         "ab0b4c26e628a74d0ef4bac66d35bc6b0e9aee45cd67ad6bd5e5da91b609cf3f")
        self.assertEqual(PINNED_SEAMS["GpcProjection-compile-repair-post"],
                         "364655669418c106f80f030a7a48797dbdbca1030c0d29e4e91c841129999bda")


if __name__ == "__main__":
    unittest.main(verbosity=1)
