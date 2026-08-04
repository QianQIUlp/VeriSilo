#!/usr/bin/env python3
"""Host font helpers for the fixed-universe font evidence.

The probe measures a FIXED font universe (identical input across artifacts).
Separately, host font negative controls pick families installed on the HOST
but NOT in the artifact's injected list; the page must report them as
unavailable (masked), otherwise the host font set leaks into the identity.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

# Fixed probe universe: identical measurement input for every artifact.
# Kept in sync with probe.html's fallback constant (unit-tested).
FONT_UNIVERSE = [
    "Arial", "Arial Black", "Arimo", "Calibri", "Cambria", "Comic Sans MS",
    "Consolas", "Courier New", "Cousine", "DejaVu Sans", "DejaVu Serif",
    "Georgia", "Helvetica", "Impact", "Liberation Mono", "Liberation Sans",
    "Liberation Serif", "Lucida Console", "Noto Sans", "Noto Serif",
    "Segoe UI", "Tahoma", "Times New Roman", "Trebuchet MS", "Verdana",
]


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


def host_negative_control_families(
    artifact_fonts: list[str],
    limit: int = 12,
    reserved_universe: set[str] | None = None,
) -> list[str]:
    """Host-installed families that are NOT injected by the artifact and not in
    the probe's fixed universe; the page must report them unavailable."""
    reserved = set(reserved_universe or FONT_UNIVERSE)
    excluded = set(artifact_fonts) | reserved
    return [
        family
        for family in fc_list_families()
        if family not in excluded
    ][:limit]
