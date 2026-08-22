#!/usr/bin/env python3
"""Author VeriSilo R1-diag downstream patches 0003 / 0004 / 9000.

Pinned derivation inputs:
- Firefox 152.0.4 release sources (hg tag FIREFOX_152_0_4_RELEASE)
- camoufox upstream stack @ 0583c3ec94f5a9df5cb2d09553fbfe80589b6e2d
  (tree 1435d544d9b61dee7fcf74cf92462952ca43d38e)

Pre-images are reconstructed from pristine FF152 files by applying the
extracted per-file upstream sections committed next to this script
(upstream-sections/), using GNU patch with the exact flags of the frozen
candidate recipe (--batch --binary --forward --ignore-whitespace --fuzz=2).
Reconstruction outcomes match provenance/container.log hunk offsets of
canvas-close-engine-20260816t144711z-e571f6c (verified 2026-08-22):
registry hunks land at -2/-4/-4/-4, SpeechSynthesis #1 at -2.

Outputs three .patch files into the repo r1-diag patch directory plus a
SHA-256 summary (patch blobs and every seam pre/post image) for the future
source-lock authoring.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import shutil
import subprocess
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
HOST_DIR = SCRIPT_DIR.parents[1]
DEFAULT_SECTIONS = SCRIPT_DIR / "upstream-sections"
DEFAULT_OUT = HOST_DIR / "patches" / "camoufox" / "v152.0.4-beta.28-r1-diag"


def sha256(data: str) -> str:
    return hashlib.sha256(data.encode("utf-8")).hexdigest()


def replace_once(text: str, old: str, new: str, label: str) -> str:
    n = text.count(old)
    if n != 1:
        raise AssertionError(f"[{label}] expected exactly 1 occurrence, found {n}")
    return text.replace(old, new, 1)


def extract_section(patch_text: str, repo_path: str) -> str:
    marker = f"diff --git a/{repo_path} b/{repo_path}"
    i = patch_text.index(marker)
    j = patch_text.find("\ndiff --git ", i + 10)
    if j < 0:
        j = len(patch_text)
    else:
        j += 1
    return patch_text[i:j]


def apply_section(workdir: Path, section: Path, patch_exe: str) -> None:
    cmd = [
        patch_exe,
        "-d",
        str(workdir),
        "-p1",
        "--batch",
        "--binary",
        "--forward",
        "--ignore-whitespace",
        "--fuzz=2",
        "--no-backup-if-mismatch",
        "-i",
        str(section),
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True)
    log = proc.stdout + proc.stderr
    if proc.returncode != 0 or "FAILED" in log:
        raise RuntimeError(f"patch failed on {workdir}:\n{log}")
    print(log.rstrip())


def udiff(pre: str, post: str, rel: str, new_file: bool = False) -> str:
    lines = list(
        difflib.unified_diff(
            [] if new_file else pre.splitlines(),
            post.splitlines(),
            fromfile="/dev/null" if new_file else f"a/{rel}",
            tofile=f"b/{rel}",
            lineterm="",
        )
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ff152-src", type=Path, required=True,
                    help="dir with pristine FF152 release files (flat names)")
    ap.add_argument("--upstream-patches", type=Path, required=True,
                    help="camoufox patches/ tree @ pinned commit")
    ap.add_argument("--work", type=Path, required=True)
    ap.add_argument("--sections", type=Path, default=DEFAULT_SECTIONS)
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--patch-exe", default="patch")
    args = ap.parse_args()

    ff = args.ff152_src
    sec = args.sections
    work = args.work
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    args.out.mkdir(parents=True, exist_ok=True)

    flat = {
        "worker": ff / "WorkerNavigator.cpp",
        "registry": ff / "nsSynthVoiceRegistry.cpp",
        "speech": ff / "SpeechSynthesis.cpp",
        "sapi": ff / "SapiService.cpp",
        "parent": ff / "SpeechSynthesisParent.cpp",
        "apprunner": ff / "nsAppRunner.cpp",
        "xre_mozbuild": ff / "xre-moz.build",
        "win_mozbuild": ff / "win-mozbuild",
    }
    for name, path in flat.items():
        if not path.is_file():
            raise SystemExit(f"missing pristine input {name}: {path}")

    def reconstruct(tag: str, base_key: str, rel: str, sections: list[str]) -> tuple[str, str]:
        """Return (rel, pre_image) applying upstream sections onto pristine."""
        d = work / tag / rel
        d.parent.mkdir(parents=True, exist_ok=True)
        d.write_bytes(flat[base_key].read_bytes())
        for s in sections:
            apply_section(work / tag, sec / s, args.patch_exe)
        pre = d.read_text(encoding="utf-8")
        if "\r" in pre:
            raise AssertionError(f"CRLF found in {rel} pre-image")
        return rel, pre

    # ---------------- reconstruction ----------------
    rel_worker, worker_pre = reconstruct(
        "worker", "worker", "dom/workers/WorkerNavigator.cpp",
        ["fpin-worker.patch", "navspoof-worker.patch"],
    )
    rel_registry, registry_pre = reconstruct(
        "registry", "registry", "dom/media/webspeech/synth/nsSynthVoiceRegistry.cpp",
        ["voice-registry.patch"],
    )
    rel_speech, speech_pre = reconstruct(
        "speech", "speech", "dom/media/webspeech/synth/SpeechSynthesis.cpp",
        ["sv-synth.patch"],
    )

    # sanity anchors against known upstream-patch outcomes
    for needle, label in [
        ('if (auto value = MaskConfig::GetBool("navigator.globalPrivacyControl");',
         "worker GPC override"),
        ("// Load voices from MaskConfig", "registry injection"),
        ("voices:blockIfNotDefined", "registry AddVoice guard"),
        ("speechVoiceCtxId", "speech patched GetVoices"),
        ("#include \"SpeechVoicesManager.h\"", "speech manager include"),
        ('#include "MaskConfig.hpp"', "registry mask include"),
    ]:
        if needle not in {"worker": worker_pre, "registry": registry_pre, "speech": speech_pre}[label.split()[0]]:
            raise AssertionError(f"sanity anchor missing [{label}]: {needle!r}")

    sapi_pre = (flat["sapi"]).read_text(encoding="utf-8")
    parent_pre = (flat["parent"]).read_text(encoding="utf-8")
    apprunner_pre = (flat["apprunner"]).read_text(encoding="utf-8")
    xre_mb_pre = (flat["xre_mozbuild"]).read_text(encoding="utf-8")
    win_mb_pre = (flat["win_mozbuild"]).read_text(encoding="utf-8")

    # ---------------- new headers ----------------
    GPC_PROJECTION_H = """/* VeriSilo GPC policy-state projection (implementation contract model B).
 *
 * Canonical policy: artifact.policy.navigator.gpcPolicy is one of
 *   {native, managed-opt-out}.
 * Engine-facing ABI: MaskConfig key "navigator.globalPrivacyControl":
 *   managed-opt-out <=> exactly true ; native <=> key absent.
 * Explicit false never occurs post-validator; defensively any non-true value
 * is treated as native (zero managed writes).
 *
 * Projection runs in the parent process exactly once, before any window or
 * network channel exists, and writes ONLY the two canonical prefs named in
 * the code below (enabled + functionality_enabled), both to true.
 * PBM prefs are never written. Window/Worker/Sec-GPC all consume these prefs
 * through Firefox native machinery afterwards (single canonical state).
 */
#ifndef camoucfg_GpcProjection_h
#define camoucfg_GpcProjection_h

#include "MaskConfig.hpp"

#include "mozilla/Preferences.h"
#include "nsXULAppAPI.h"

#include <mutex>

namespace camoucfg {

inline void ProjectGpcPolicyFromMaskConfig() {
  static std::once_flag sGpcProjectionOnce;
  std::call_once(sGpcProjectionOnce, [] {
    if (!XRE_IsParentProcess()) {
      return;
    }
    auto value = MaskConfig::GetBool("navigator.globalPrivacyControl");
    if (!(value.has_value() && value.value())) {
      return;  // native policy state: zero managed pref writes
    }
    Preferences::SetBool("privacy.globalprivacycontrol.enabled", true);
    Preferences::SetBool(
        "privacy.globalprivacycontrol.functionality_enabled", true);
  });
}

}  // namespace camoucfg

#endif  // camoucfg_GpcProjection_h
"""

    VSI_DIAG_H = """// VERISILO-DIAGNOSTIC-MARKER: v1
// VERISILO-DIAGNOSTIC-ONLY - investigation instrumentation for the FP2-R1
// voices V1-V4 discrimination (implementation contract section 2). Pure
// observer: never changes voice ordering, initialization timing, native
// suppression, managed inventory, or cache semantics. Bounded: <=512 events
// per process, then a single OVERFLOW note. Every event is one stderr line
// "VSIDIAG {...}" <=240 chars; URIs appear only as the first 12 hex chars of
// their SHA-256. Formal R1 builds MUST NOT include instrumentation.
#ifndef camoucfg_VsiDiag_h
#define camoucfg_VsiDiag_h

#include "mozilla/crypto_hash_sha2.h"

#include <atomic>
#include <cstdio>
#include <mutex>
#include <set>
#include <string>

#include "nsString.h"
#include "nsXULAppAPI.h"

namespace camoucfg {

inline void VsiDiagEmit(const char* aEvent, const std::string& aFields) {
  static std::atomic<uint32_t> sSeq{0};
  static std::atomic<bool> sOverflow{false};
  uint32_t seq = sSeq.fetch_add(1, std::memory_order_relaxed);
  if (seq >= 512u) {
    bool expected = false;
    if (sOverflow.compare_exchange_strong(expected, true)) {
      std::fprintf(stderr, "VSIDIAG {\\"e\\":\\"OVERFLOW\\"}\\n");
    }
    return;
  }
  std::string line = "{\\"e\\":\\"";
  line += aEvent;
  line += "\\",\\"proc\\":\\"";
  line += (XRE_IsParentProcess() ? 'P' : 'C');
  line += "\\",\\"seq\\":";
  line += std::to_string(seq);
  if (!aFields.empty()) {
    line += ',';
    line += aFields;
  }
  line += '}';
  if (line.size() > 232u) {
    return;  // bounded output
  }
  std::fprintf(stderr, "VSIDIAG %s\\n", line.c_str());
}

inline std::string VsiDiagUriHash(const nsAString& aUri) {
  NS_ConvertUTF16toUTF8 utf8(aUri);
  uint8_t digest[32];
  crypto_hash_sha256(reinterpret_cast<const uint8_t*>(utf8.get()),
                     static_cast<unsigned long long>(utf8.Length()), digest);
  static const char kHex[] = "0123456789abcdef";
  std::string out;
  out.reserve(12);
  for (int i = 0; i < 6; ++i) {
    out.push_back(kHex[(digest[i] >> 4) & 0xF]);
    out.push_back(kHex[digest[i] & 0xF]);
  }
  return out;
}

inline void VsiDiagEventUri(const char* aEvent, const nsAString& aUri) {
  VsiDiagEmit(aEvent, "\\"h\\":\\"" + VsiDiagUriHash(aUri) + "\\"");
}

inline void VsiDiagGetVoices(void* aObj, unsigned aCtxId, unsigned aCount,
                             unsigned aCacheSize) {
  static std::mutex sMutex;
  static std::set<void*> sSeen;
  bool first = false;
  {
    std::lock_guard<std::mutex> lock(sMutex);
    first = sSeen.insert(aObj).second;
  }
  char fields[96];
  std::snprintf(fields, sizeof(fields),
                "\\"ctx\\":%u,\\"n\\":%u,\\"cache\\":%u,\\"first\\":%d", aCtxId,
                aCount, aCacheSize, first ? 1 : 0);
  VsiDiagEmit("E7_getvoices", fields);
}

}  // namespace camoucfg

#endif  // camoucfg_VsiDiag_h
"""

    # ---------------- 0003 ----------------
    apprunner_post = replace_once(
        apprunner_pre,
        '#include "mozilla/Preferences.h"\n',
        '#include "mozilla/Preferences.h"\n#include "GpcProjection.h"\n',
        "0003 include anchor",
    )
    apprunner_post = replace_once(
        apprunner_post,
        'nsresult XREMain::XRE_mainRun() {\n'
        '  nsresult rv = NS_OK;\n'
        '  NS_ASSERTION(mScopedXPCOM, "Scoped xpcom not initialized.");\n',
        'nsresult XREMain::XRE_mainRun() {\n'
        '  nsresult rv = NS_OK;\n'
        '  NS_ASSERTION(mScopedXPCOM, "Scoped xpcom not initialized.");\n'
        '\n'
        '  // VeriSilo GPC policy-state projection (model B): parent-only, once,\n'
        '  // managed-opt-out only; native writes nothing. Runs before any window\n'
        '  // or network channel exists.\n'
        '  camoucfg::ProjectGpcPolicyFromMaskConfig();\n',
        "0003 call site",
    )
    xre_mb_post = replace_once(
        xre_mb_pre,
        '    "../profile",\n',
        '    "../profile",\n    "/camoucfg",\n',
        "0003 xre LOCAL_INCLUDES",
    )

    patch_0003 = (
        udiff("", GPC_PROJECTION_H, "camoucfg/GpcProjection.h", new_file=True)
        + udiff(xre_mb_pre, xre_mb_post, "toolkit/xre/moz.build")
        + udiff(apprunner_pre, apprunner_post, "toolkit/xre/nsAppRunner.cpp")
    )

    # ---------------- 0004 ----------------
    worker_post = replace_once(
        worker_pre,
        'bool WorkerNavigator::GlobalPrivacyControl() const {\n'
        '  if (auto value = MaskConfig::GetBool("navigator.globalPrivacyControl");\n'
        '      value.has_value())\n'
        '    return value.value();\n'
        '  bool gpcStatus = StaticPrefs::privacy_globalprivacycontrol_enabled();\n',
        'bool WorkerNavigator::GlobalPrivacyControl() const {\n'
        '  bool gpcStatus = StaticPrefs::privacy_globalprivacycontrol_enabled();\n',
        "0004 GPC override removal",
    )
    patch_0004 = udiff(worker_pre, worker_post, rel_worker)

    # ---------------- 9000 ----------------
    registry_post = replace_once(
        registry_pre,
        '#include "MaskConfig.hpp"\n',
        '#include "MaskConfig.hpp"\n#include "VsiDiag.h"\n',
        "9000 registry include",
    )
    registry_post = replace_once(
        registry_post,
        '      // Load voices from MaskConfig\n'
        '      if (auto voices = MaskConfig::MVoices()) {\n'
        '        for (const auto& [lang, name, uri, isDefault, isLocal] :\n'
        '             voices.value()) {\n',
        '      // Load voices from MaskConfig\n'
        '      auto vsiVoices = MaskConfig::MVoices();\n'
        '      camoucfg::VsiDiagEmit(\n'
        '          "E1_mvoices_parsed",\n'
        '          vsiVoices ? ("\\"n\\":" + std::to_string(vsiVoices->size()))\n'
        '                    : std::string("\\"n\\":null"));\n'
        '      if (vsiVoices) {\n'
        '        camoucfg::VsiDiagEmit("E3a_managed_batch_begin",\n'
        '                              ("\\"n\\":" + std::to_string(vsiVoices->size()))\n'
        '                                  .c_str());\n'
        '        for (const auto& [lang, name, uri, isDefault, isLocal] :\n'
        '             vsiVoices.value()) {\n',
        "9000 E1/E3a hoist",
    )
    registry_post = replace_once(
        registry_post,
        '          if (isDefault) {\n'
        '            gSynthVoiceRegistry->SetDefaultVoice(NS_ConvertUTF8toUTF16(uri),\n'
        '                                                 true);\n'
        '          }\n'
        '        }\n'
        '      }\n',
        '          if (isDefault) {\n'
        '            gSynthVoiceRegistry->SetDefaultVoice(NS_ConvertUTF8toUTF16(uri),\n'
        '                                                 true);\n'
        '          }\n'
        '        }\n'
        '        camoucfg::VsiDiagEmit("E3b_managed_batch_end", "");\n'
        '      }\n',
        "9000 E3b batch end",
    )
    registry_post = replace_once(
        registry_post,
        '    for (uint32_t i = 0; i < ssplist.Length(); ++i) {\n'
        '      (void)ssplist[i]->SendVoiceAdded(ssvoice);\n',
        '    for (uint32_t i = 0; i < ssplist.Length(); ++i) {\n'
        '      camoucfg::VsiDiagEventUri("E5_send_voice_added", ssvoice.voiceURI());\n'
        '      (void)ssplist[i]->SendVoiceAdded(ssvoice);\n',
        "9000 E5 broadcast",
    )
    registry_post = replace_once(
        registry_post,
        '  MOZ_ASSERT(gSynthVoiceRegistry);\n\n'
        '  for (uint32_t i = 0; i < aVoices.Length(); ++i) {\n',
        '  MOZ_ASSERT(gSynthVoiceRegistry);\n'
        '  camoucfg::VsiDiagEmit("E6_recv_initial_voices",\n'
        '                        ("\\"n\\":" + std::to_string(aVoices.Length())).c_str());\n\n'
        '  for (uint32_t i = 0; i < aVoices.Length(); ++i) {\n',
        "9000 E6 snapshot recv",
    )
    registry_post = replace_once(
        registry_post,
        'void nsSynthVoiceRegistry::RecvAddVoice(const RemoteVoice& aVoice) {\n',
        'void nsSynthVoiceRegistry::RecvAddVoice(const RemoteVoice& aVoice) {\n'
        '  camoucfg::VsiDiagEventUri("E6_recv_add_voice", aVoice.voiceURI());\n',
        "9000 E6 incremental recv",
    )

    speech_post = replace_once(
        speech_pre,
        '#include "SpeechVoicesManager.h"\n',
        '#include "SpeechVoicesManager.h"\n#include "VsiDiag.h"\n',
        "9000 speech include",
    )
    speech_post = replace_once(
        speech_post,
        '  for (uint32_t i = 0; i < aResult.Length(); i++) {\n'
        '    SpeechSynthesisVoice* voice = aResult[i];\n'
        '    mVoiceCache.InsertOrUpdate(voice->mUri, RefPtr{voice});\n'
        '  }\n}\n',
        '  for (uint32_t i = 0; i < aResult.Length(); i++) {\n'
        '    SpeechSynthesisVoice* voice = aResult[i];\n'
        '    mVoiceCache.InsertOrUpdate(voice->mUri, RefPtr{voice});\n'
        '  }\n'
        '\n'
        '  camoucfg::VsiDiagGetVoices(this, speechVoiceCtxId,\n'
        '                             static_cast<unsigned>(aResult.Length()),\n'
        '                             static_cast<unsigned>(mVoiceCache.Count()));\n'
        '}\n',
        "9000 E7 getvoices tail",
    )

    sapi_post = sapi_pre
    first_inc = sapi_post.find('#include "')
    if first_inc < 0:
        raise AssertionError("9000 sapi include anchor missing")
    eol = sapi_post.index("\n", first_inc) + 1
    sapi_post = sapi_post[:eol] + '#include "VsiDiag.h"\n' + sapi_post[eol:]
    sapi_post = replace_once(
        sapi_post,
        'bool SapiService::Init() {\n'
        '  AUTO_PROFILER_LABEL("SapiService::Init", OTHER);\n',
        'bool SapiService::Init() {\n'
        '  AUTO_PROFILER_LABEL("SapiService::Init", OTHER);\n'
        '\n'
        '  struct VsiScopeEndLog {\n'
        '    ~VsiScopeEndLog() { camoucfg::VsiDiagEmit("E2b_sapi_init_end", ""); }\n'
        '  } vsiScopeEndLog;\n'
        '  camoucfg::VsiDiagEmit("E2a_sapi_init_begin", "");\n',
        "9000 E2 scope logs",
    )

    parent_post = replace_once(
        parent_pre,
        '#include "nsSynthVoiceRegistry.h"\n',
        '#include "nsSynthVoiceRegistry.h"\n#include "VsiDiag.h"\n',
        "9000 parent include",
    )
    parent_post = replace_once(
        parent_post,
        'bool SpeechSynthesisParent::SendInit() {\n'
        '  return nsSynthVoiceRegistry::GetInstance()->SendInitialVoicesAndState(this);\n'
        '}\n',
        'bool SpeechSynthesisParent::SendInit() {\n'
        '  uint32_t vsiN = 0;\n'
        '  if (NS_SUCCEEDED(\n'
        '          nsSynthVoiceRegistry::GetInstance()->GetVoiceCount(&vsiN))) {\n'
        '    camoucfg::VsiDiagEmit("E4_sendinit_snapshot",\n'
        '                          ("\\"n\\":" + std::to_string(vsiN)).c_str());\n'
        '  }\n'
        '  return nsSynthVoiceRegistry::GetInstance()->SendInitialVoicesAndState(this);\n'
        '}\n',
        "9000 E4 sendinit",
    )

    win_mb_post = replace_once(
        win_mb_pre,
        'include("/ipc/chromium/chromium-config.mozbuild")\n',
        'include("/ipc/chromium/chromium-config.mozbuild")\n'
        '\n'
        'LOCAL_INCLUDES += [\n'
        '    "/camoucfg",\n'
        ']\n',
        "9000 windows moz.build",
    )

    diag_marker = (
        "# VERISILO-DIAGNOSTIC-MARKER: v1\n"
        "# VERISILO-DIAGNOSTIC-ONLY - investigation instrumentation only.\n"
        "# Formal R1 build drivers MUST reject this file.\n"
    )
    patch_9000 = (
        diag_marker
        + udiff("", VSI_DIAG_H, "camoucfg/VsiDiag.h", new_file=True)
        + udiff(win_mb_pre, win_mb_post, "dom/media/webspeech/synth/windows/moz.build")
        + udiff(sapi_pre, sapi_post, "dom/media/webspeech/synth/windows/SapiService.cpp")
        + udiff(parent_pre, parent_post,
                "dom/media/webspeech/synth/ipc/SpeechSynthesisParent.cpp")
        + udiff(registry_pre, registry_post, rel_registry)
        + udiff(speech_pre, speech_post, rel_speech)
    )

    # ---------------- self-checks ----------------
    plus_lines = [l for l in patch_0004.splitlines() if l.startswith("+") and not l.startswith("+++")]
    plus_0003 = [l for l in patch_0003.splitlines() if l.startswith("+") and not l.startswith("+++")]
    checks = [
        ("0003 canonical owner calls", patch_0003.count("ProjectGpcPolicyFromMaskConfig") == 2),
        ("0003 no pbmode writes", "pbmode" not in patch_0003),
        ("0003 single pref pair",
         sum(1 for l in plus_0003 if '"privacy.globalprivacycontrol.enabled"' in l) == 1
         and sum(1 for l in plus_0003
                 if '"privacy.globalprivacycontrol.functionality_enabled"' in l) == 1),
        ("0004 override gone",
         not any("navigator.globalPrivacyControl" in l for l in plus_lines)),
        ("0004 native body kept",
         "StaticPrefs::privacy_globalprivacycontrol_enabled()" in worker_post
         and "GetWrapper()" in worker_post),
        ("9000 marker present", "VERISILO-DIAGNOSTIC-MARKER" in patch_9000),
        ("9000 events complete", all(e in patch_9000 for e in [
            "E1_mvoices_parsed", "E3a_managed_batch_begin", "E3b_managed_batch_end",
            "E2a_sapi_init_begin", "E2b_sapi_init_end", "E4_sendinit_snapshot",
            "E5_send_voice_added", "E6_recv_initial_voices", "E6_recv_add_voice",
            "E7_getvoices"])),
        ("9000 no sleeps", "PR_Sleep" not in patch_9000 and "Sleep(" not in patch_9000),
        ("9000 no proactive getVoices", "getVoices(" not in patch_9000),
        ("9000 no suppression change", "blockIfNotDefined" not in patch_9000),
    ]
    failed = [name for name, ok in checks if not ok]
    if failed:
        raise AssertionError(f"self-check failures: {failed}")

    # ---------------- outputs ----------------
    outputs = {
        "0003-verisilo-gpc-canonical-pref-projection.patch": patch_0003,
        "0004-verisilo-remove-worker-gpc-mask-override.patch": patch_0004,
        "9000-verisilo-voices-diagnostics-DIAGNOSTIC-ONLY.patch": patch_9000,
    }
    seams = {
        "seam/toolkit-xre-nsAppRunner-pre": sha256(apprunner_pre),
        "seam/toolkit-xre-nsAppRunner-post": sha256(apprunner_post),
        "seam/xre-mozbuild-pre": sha256(xre_mb_pre),
        "seam/xre-mozbuild-post": sha256(xre_mb_post),
        "seam/WorkerNavigator-pre": sha256(worker_pre),
        "seam/WorkerNavigator-post": sha256(worker_post),
        "seam/nsSynthVoiceRegistry-pre": sha256(registry_pre),
        "seam/nsSynthVoiceRegistry-post": sha256(registry_post),
        "seam/SpeechSynthesis-pre": sha256(speech_pre),
        "seam/SpeechSynthesis-post": sha256(speech_post),
        "seam/SapiService-pre": sha256(sapi_pre),
        "seam/SapiService-post": sha256(sapi_post),
        "seam/SpeechSynthesisParent-pre": sha256(parent_pre),
        "seam/SpeechSynthesisParent-post": sha256(parent_post),
        "seam/windows-mozbuild-pre": sha256(win_mb_pre),
        "seam/windows-mozbuild-post": sha256(win_mb_post),
    }

    print("== outputs ==")
    for name, text in outputs.items():
        (args.out / name).write_text(text, encoding="utf-8", newline="\n")
        print(f"{sha256(text)}  {name}  ({len(text.encode('utf-8'))} bytes)")
    print("== seam digests ==")
    for k, v in seams.items():
        print(f"{v}  {k}")
    print("OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
