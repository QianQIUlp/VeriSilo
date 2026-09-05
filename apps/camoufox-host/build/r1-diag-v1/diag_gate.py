#!/usr/bin/env python3
"""Build-mode gate for the R1 patch series (implementation contract section 10).

Two explicit modes bound by the r1-diag source lock; there is deliberately NO
process-env override of any kind:

  formal     : any DIAGNOSTIC-MARKED patch present  -> HARD FAIL
               accepted series = exactly every pinned non-diagnostic patch
               -> verdict.formalEligible = True

  diagnostic : accepted series = exactly every pinned patch with pinned SHAs
               and the v1 marker in 9000
               -> verdict.diagnosticOnly = True, formalEligible = False,
                  purpose = fp2-r1-voices-v1-v4-discrimination

Any other series composition or SHA drift is rejected in BOTH modes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

SCHEMA = "verisilo-r1-diag-build-gate/v1"
MARKER_LINE = "# VERISILO-DIAGNOSTIC-MARKER: v1"
MODE_FORMAL = "formal"
MODE_DIAGNOSTIC = "diagnostic"
DIAG_PURPOSE = "fp2-r1-voices-v1-v4-discrimination"


@dataclass
class GateResult:
    ok: bool
    mode: str
    reason: str = ""
    diagnosticOnly: bool = False
    formalEligible: bool = False
    details: dict = field(default_factory=dict)

    def to_json(self) -> str:
        return json.dumps(
            {
                "schema": SCHEMA,
                "ok": self.ok,
                "mode": self.mode,
                "reason": self.reason,
                "diagnosticOnly": self.diagnosticOnly,
                "formalEligible": self.formalEligible,
                **({"details": self.details} if self.details else {}),
            },
            sort_keys=True,
        )


def _sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_expected_series(record_path: Path) -> dict:
    record = json.loads(record_path.read_text(encoding="utf-8"))
    patches = record["patches"]
    out = {}
    for name, meta in patches.items():
        key = name.split("-")[0]
        out[key] = {
            "filename": name,
            "sha256": meta["sha256"],
            "diagnosticOnly": bool(meta.get("diagnosticOnly", False)),
        }
    return out


def expected_series_from_lock(lock: dict) -> dict:
    """Return the frozen incremental series from the v2 source lock."""
    entries = lock["r1IncrementalPatches"]
    expected: dict[str, dict] = {}
    for entry in entries:
        key = entry["id"]
        if key in expected:
            raise ValueError(f"duplicate incremental patch id: {key}")
        expected[key] = {
            "filename": Path(entry["path"]).name,
            "sha256": entry["sha256"],
            "diagnosticOnly": bool(entry.get("diagnosticOnly", False)),
        }
    return expected


def _failure(
    mode: str,
    reason: str,
    *,
    missing: list[str] | None = None,
    extra: list[str] | None = None,
    drift: list[str] | None = None,
    marker: str | None = None,
) -> GateResult:
    details = {
        "missing": sorted(missing or []),
        "extra": sorted(extra or []),
        "drift": sorted(drift or []),
    }
    if marker is not None:
        details["marker"] = marker
    return GateResult(False, mode, reason, details=details)


def evaluate(
    mode: str,
    series_dir: Path,
    expected_series: dict,
    patch_texts: dict | None = None,
) -> GateResult:
    """patch_texts: optional {key: text} override for tests; else read files."""
    if mode not in (MODE_FORMAL, MODE_DIAGNOSTIC):
        return GateResult(False, mode, f"unknown build mode: {mode}")

    name_to_key = {meta["filename"]: key for key, meta in expected_series.items()}
    extra_files = []
    for path in sorted(series_dir.glob("*.patch")):
        if path.name not in name_to_key:
            extra_files.append(path.name)
    if extra_files:
        return _failure(
            mode,
            "unrecognized patch file(s)",
            extra=extra_files,
        )

    found = {}
    texts = {}
    for key, meta in expected_series.items():
        path = series_dir / meta["filename"]
        if not path.is_file():
            continue
        found[key] = _sha256_file(path)
        if patch_texts is not None and key in patch_texts:
            texts[key] = patch_texts[key]
        elif key == "9000":
            try:
                texts[key] = path.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                return _failure(
                    mode,
                    "9000 diagnostic patch is not valid UTF-8",
                    marker="invalid-utf8",
                )

    if mode == MODE_FORMAL:
        for key, text in texts.items():
            if text.splitlines()[:1] == [MARKER_LINE]:
                return _failure(
                    MODE_FORMAL,
                    f"HARD FAIL: diagnostic-marked patch {key} present in formal series",
                )
        if "9000" in found:
            return _failure(
                MODE_FORMAL,
                "HARD FAIL: 9000 diagnostics are never formal-carry-forward",
            )
        wanted = {k: v["sha256"] for k, v in expected_series.items() if k != "9000"}
        missing = sorted(k for k in wanted if k not in found)
        drifted = sorted(k for k in found if k in wanted and found[k] != wanted[k])
        if missing or drifted or set(found) != set(wanted):
            return _failure(
                MODE_FORMAL,
                "formal series must contain every pinned non-diagnostic patch",
                missing=missing,
                drift=drifted,
            )
        return GateResult(True, MODE_FORMAL, "formal series accepted",
                          formalEligible=True,
                          details={"patches": found})

    # diagnostic mode
    wanted = {k: v["sha256"] for k, v in expected_series.items()}
    missing = sorted(k for k in wanted if k not in found)
    drifted = sorted(k for k in found if k in wanted and found[k] != wanted[k])
    if missing or drifted or set(found) != set(wanted):
        if missing:
            reason = "diagnostic series is missing required patch(es)"
        elif drifted:
            reason = "diagnostic series contains SHA drift"
        else:
            reason = "diagnostic series has an unexpected patch set"
        return _failure(
            MODE_DIAGNOSTIC,
            reason,
            missing=missing,
            drift=drifted,
        )
    marker_ok = texts.get("9000", "").splitlines()[:1] == [MARKER_LINE]
    if not marker_ok:
        return _failure(
            MODE_DIAGNOSTIC,
            "9000 must carry the v1 DIAGNOSTIC marker as its first line",
            marker="missing-or-drifted",
        )
    return GateResult(
        True, MODE_DIAGNOSTIC, "diagnostic series accepted",
        diagnosticOnly=True, formalEligible=False,
        details={"purpose": DIAG_PURPOSE, "patches": found},
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--mode", choices=[MODE_FORMAL, MODE_DIAGNOSTIC], required=True)
    ap.add_argument("--series-dir", type=Path, required=True)
    ap.add_argument("--authoring-record", type=Path, required=True)
    args = ap.parse_args()
    result = evaluate(args.mode, args.series_dir, load_expected_series(args.authoring_record))
    print(result.to_json())
    return 0 if result.ok else 2


if __name__ == "__main__":
    sys.exit(main())
