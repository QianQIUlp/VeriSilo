#!/usr/bin/env python3
"""Clean M3-WI Host entrypoint (native Windows, evidence-only)."""

from __future__ import annotations

import os
import sys
from pathlib import Path


HOST_DIR = Path(__file__).resolve().parent
if str(HOST_DIR) not in sys.path:
    sys.path.insert(0, str(HOST_DIR))

import host_v1
import run_fp3_1b_windows as fp3


BASE_HOST = host_v1.CamoufoxHost


def main() -> int:
    if os.name != "nt":
        raise fp3.FP3HostError("clean M3-WI requires native Windows")
    if "--child-host" in sys.argv:
        sys.argv.remove("--child-host")
    fp3.patch_host()
    host_v1.CamoufoxHost = BASE_HOST
    return host_v1.main()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except fp3.FP3HostError as exc:
        raise SystemExit(f"clean M3-WI Host adapter rejected input: {exc}") from exc
