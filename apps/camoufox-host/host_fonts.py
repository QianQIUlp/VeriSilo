#!/usr/bin/env python3
"""Host font helpers for the fixed-universe font evidence.

The probe measures a FIXED font universe (identical input across artifacts).
Separately, host font negative controls pick families installed on the HOST
but neither injected by the artifact, nor shipped in the browser's bundled
font set, nor part of the fixed probe universe; the page must report them as
unavailable (masked), otherwise the host font set leaks into the identity.

Negative controls are judged by the RENDERING layer (canvas text metrics),
not by ``document.fonts.check``: the CSS Font Loading API cannot observe
system-font availability in Firefox and reports every well-formed family as
available, so it is unusable as a masking oracle.
"""

from __future__ import annotations

import ctypes
import subprocess
from pathlib import Path

from host_platform import IS_WINDOWS

# Fixed probe universe: identical measurement input for every artifact.
# Kept in sync with probe.html's fallback constant (unit-tested).
FONT_UNIVERSE = [
    "Arial", "Arial Black", "Arimo", "Calibri", "Cambria", "Comic Sans MS",
    "Consolas", "Courier New", "Cousine", "DejaVu Sans", "DejaVu Serif",
    "Georgia", "Helvetica", "Impact", "Liberation Mono", "Liberation Sans",
    "Liberation Serif", "Lucida Console", "Noto Sans", "Noto Serif",
    "Segoe UI", "Tahoma", "Times New Roman", "Trebuchet MS", "Verdana",
]

# A managed launch must prove masking against a non-trivial control set.
# An empty or tiny control set cannot distinguish real masking from a broken
# probe, so managed mode fails closed when fewer controls are testable.
MANAGED_MIN_FONT_CONTROLS = 8

# How many host negative controls one launch probes. The candidate pool on a
# stock Windows host is far larger; the bound keeps the probe page cheap
# while still covering the alphabetical head of the leak surface.
MANAGED_FONT_CONTROLS_LIMIT = 32


def _name_conflicts_with(a: str, b: str) -> bool:
    """True when one family name is a word-prefix variant of the other.

    "Bahnschrift SemiBold" resolves to the declared "Bahnschrift" family in
    the browser, so name variants of an always-available family must never
    become negative controls.
    """
    a_fold = " ".join(a.casefold().split())
    b_fold = " ".join(b.casefold().split())
    return a_fold.startswith(b_fold) or b_fold.startswith(a_fold)


def fc_list_families() -> list[str]:
    try:
        proc = subprocess.run(
            ["fc-list", ": family"],
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired):
        return []
    families = set()
    for line in proc.stdout.splitlines():
        if ":" not in line:
            continue
        family_part = line.split(":", 1)[1]
        family_part = family_part.split(":style=")[0]
        for family in family_part.split(","):
            family = family.strip()
            if family:
                families.add(family)
    return sorted(families)


def gdi_font_families() -> list[str]:
    """Families visible to the Windows font system (system-wide + per-user).

    Enumerates through GDI via EnumFontFamiliesExW, which sees every font
    registered under HKLM and the per-user font keys without elevation and
    without mutating any font state.
    """
    LF_FACESIZE = 32
    DEFAULT_CHARSET = 1

    class LOGFONTW(ctypes.Structure):
        _fields_ = [
            ("lfHeight", ctypes.c_long),
            ("lfWidth", ctypes.c_long),
            ("lfEscapement", ctypes.c_long),
            ("lfOrientation", ctypes.c_long),
            ("lfWeight", ctypes.c_long),
            ("lfItalic", ctypes.c_byte),
            ("lfUnderline", ctypes.c_byte),
            ("lfStrikeOut", ctypes.c_byte),
            ("lfCharSet", ctypes.c_byte),
            ("lfOutPrecision", ctypes.c_byte),
            ("lfClipPrecision", ctypes.c_byte),
            ("lfQuality", ctypes.c_byte),
            ("lfPitchAndFamily", ctypes.c_byte),
            ("lfFaceName", ctypes.c_wchar * LF_FACESIZE),
        ]

    class ENUMLOGFONTEXW(ctypes.Structure):
        _fields_ = [
            ("elfLogFont", LOGFONTW),
            ("elfFullName", ctypes.c_wchar * LF_FACESIZE),
            ("elfStyle", ctypes.c_wchar * LF_FACESIZE),
            ("elfScript", ctypes.c_wchar * LF_FACESIZE),
        ]

    ENUMFONTEXPROCW = ctypes.WINFUNCTYPE(
        ctypes.c_int,
        ctypes.POINTER(ENUMLOGFONTEXW),
        ctypes.c_void_p,
        ctypes.c_uint,
        ctypes.c_void_p,
    )

    gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
    user32 = ctypes.WinDLL("user32", use_last_error=True)

    families: set[str] = set()

    def _callback(logfont_ptr, _textmetric, _font_type, _lparam) -> int:
        family = logfont_ptr.contents.elfLogFont.lfFaceName
        if family:
            families.add(family)
        return 1

    cb = ENUMFONTEXPROCW(_callback)
    hdc = user32.GetDC(None)
    if not hdc:
        return []
    try:
        logfont = LOGFONTW()
        logfont.lfCharSet = DEFAULT_CHARSET
        gdi32.EnumFontFamiliesExW(hdc, ctypes.byref(logfont), cb, 0, 0)
    finally:
        user32.ReleaseDC(None, hdc)
    return sorted(families)


def host_font_families() -> list[str]:
    """Families installed on this host (system-wide and per-user)."""
    if IS_WINDOWS:
        return gdi_font_families()
    return fc_list_families()


_BUNDLE_FONT_DIRNAME = "fonts"


def bundle_font_families(browser_root: Path | str) -> set[str]:
    """Families shipped inside the browser tree's bundled font directory.

    Bundled fonts are frozen with the browser package and render identically
    on every host, so they are not host leakage, but they MUST stay out of
    the negative-control set: the page can reach them regardless of the host
    font system. On a tree without the bundled font directory the set is
    empty, which shrinks the control set and tightens the managed gate
    (fail closed).
    """
    fonts_dir = Path(browser_root) / _BUNDLE_FONT_DIRNAME
    if not fonts_dir.is_dir():
        return set()
    if IS_WINDOWS:
        return _families_via_gdiplus(fonts_dir)
    return _families_via_fc_scan(fonts_dir)


def _families_via_gdiplus(fonts_dir: Path) -> set[str]:
    """Read family names from font files with the GDI+ flat API.

    A private font collection reads family names straight from the font
    files; it installs nothing and touches no process-global font state.
    """
    gdiplus = ctypes.WinDLL("gdiplus", use_last_error=True)

    class StartupInput(ctypes.Structure):
        _fields_ = [
            ("GdiplusVersion", ctypes.c_uint32),
            ("DebugEventCallback", ctypes.c_void_p),
            ("SuppressBackgroundThread", ctypes.c_int32),
            ("SuppressExternalCodecs", ctypes.c_int32),
        ]

        def __init__(self):
            super().__init__()
            self.GdiplusVersion = 1

    token = ctypes.c_ulong()
    if gdiplus.GdiplusStartup(
        ctypes.byref(token), ctypes.byref(StartupInput()), None
    ) != 0:
        return set()
    try:
        collection = ctypes.c_void_p()
        if gdiplus.GdipNewPrivateFontCollection(
            ctypes.byref(collection)
        ) != 0:
            return set()
        for path in sorted(fonts_dir.iterdir()):
            if path.suffix.lower() not in (".ttf", ".ttc", ".otf"):
                continue
            gdiplus.GdipPrivateAddFontFile(collection, str(path))
        family_count = ctypes.c_int()
        if gdiplus.GdipGetFontCollectionFamilyCount(
            collection, ctypes.byref(family_count)
        ) != 0:
            return set()
        families: set[str] = set()
        family = ctypes.c_void_p()
        name_buf = ctypes.create_unicode_buffer(128)
        family_list = (ctypes.c_void_p * family_count.value)()
        if gdiplus.GdipGetFontCollectionFamilyList(
            collection,
            family_count,
            family_list,
            ctypes.byref(ctypes.c_int(0)),
        ) != 0:
            return set()
        for index in range(family_count.value):
            family.value = family_list[index]
            if gdiplus.GdipGetFamilyName(
                family, name_buf, 0x0409  # en-US: stable English family names
            ) == 0 and name_buf.value:
                families.add(name_buf.value)
        return families
    finally:
        gdiplus.GdiplusShutdown(token)


def _families_via_fc_scan(fonts_dir: Path) -> set[str]:
    try:
        proc = subprocess.run(
            ["fc-scan", "--format", "%{family}\\n", str(fonts_dir)],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired):
        return set()
    families = set()
    for line in proc.stdout.splitlines():
        for family in line.split(","):
            family = family.strip()
            if family:
                families.add(family)
    return families


def host_negative_control_families(
    artifact_fonts: list[str],
    limit: int = 12,
    reserved_universe: set[str] | None = None,
    host_families: list[str] | None = None,
    bundled_families: set[str] | None = None,
) -> list[str]:
    """Host-installed families the managed page must report unavailable.

    A family only becomes a control when it is installed on the host and is
    neither injected by the artifact, nor part of the fixed probe universe,
    nor shipped inside the browser bundle, nor a name-variant of any of
    those (variants can still resolve to the same family in the browser).
    """
    reserved = set(reserved_universe or FONT_UNIVERSE)
    excluded = set(artifact_fonts) | reserved | set(bundled_families or ())
    source = host_families if host_families is not None else host_font_families()
    controls: list[str] = []
    for family in source:
        # "@Family" entries are GDI vertical-writing variants; CSS can never
        # resolve them, so they carry no masking information.
        if family.startswith("@"):
            continue
        if any(
            _name_conflicts_with(family, excluded_family)
            for excluded_family in excluded
        ):
            continue
        controls.append(family)
        if len(controls) >= limit:
            break
    return controls


def evaluate_host_font_masking(
    font_mode: str,
    host_font_controls: dict,
    min_controls: int = MANAGED_MIN_FONT_CONTROLS,
) -> dict:
    """Judge the observed host-font controls for the session's font mode.

    Returns the masking evidence record. ``allUnavailable`` is only true when
    the control set is non-empty and every control measured as not rendering;
    ``sufficient`` reports whether the control set is large enough to count
    as masking proof for a managed launch.
    """
    failures = [
        family
        for family, available in host_font_controls.items()
        if available is not False
    ]
    controls_tested = len(host_font_controls)
    return {
        "controlsTested": controls_tested,
        "allUnavailable": controls_tested > 0 and not failures,
        "sufficient": controls_tested >= min_controls,
        "failures": failures,
    }
