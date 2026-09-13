"""Spawn the direct probe detached from the current Job (CREATE_BREAKAWAY_FROM_JOB)."""

from __future__ import annotations

import argparse
import subprocess
import sys

CREATE_BREAKAWAY_FROM_JOB = 0x01000000
CREATE_NEW_CONSOLE = 0x00000010
DETACHED_PROCESS = 0x00000008


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("command", nargs="+")
    args = parser.parse_args()
    try:
        process = subprocess.Popen(
            args.command,
            stdout=open(args.out, "wb"),
            stderr=subprocess.STDOUT,
            creationflags=CREATE_BREAKAWAY_FROM_JOB | CREATE_NEW_CONSOLE,
        )
    except OSError as exc:
        print(f"breakaway spawn failed: {exc}")
        return 2
    print(f"detached pid={process.pid}")
    return process.wait(timeout=600)


if __name__ == "__main__":
    raise SystemExit(main())
