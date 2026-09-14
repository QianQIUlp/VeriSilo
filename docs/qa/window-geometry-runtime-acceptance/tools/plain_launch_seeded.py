"""One-shot diagnostic: plain-launch the 2026-09-09 rc2 cache-seeded browser copy."""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from plain_launch_control import (  # noqa: E402
    process_tree_snapshot,
    visible_windows_for_pid_tree,
)

EXE = Path(
    r"C:\Users\qiu\src\VeriSilo\artifacts\rc2-candidate-acceptance"
    r"\release-smoke-work\engine-state\camoufox-cache\camoufox\camoufox\Cache"
    r"\browsers\verisilo\152.0.4-beta.28-8a3ef192\camoufox.exe"
)
PROFILE = Path(r"C:\Users\qiu\AppData\Local\Temp\camoufox-plain-control\profile-seeded")
OUT = Path(
    r"C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\qa-accept-real-managed-e0f551"
    r"\docs\qa\window-geometry-runtime-acceptance\diagnostics\plain-launch-seeded.json"
)


def main() -> int:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    PROFILE.mkdir(parents=True, exist_ok=True)
    record: dict = {
        "exe": str(EXE), "exeExists": EXE.exists(),
        "startedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "samples": [],
    }
    stderr_path = OUT.parent / "plain-launch-seeded.stderr.log"
    process = subprocess.Popen(
        [str(EXE), "-no-remote", "-profile", str(PROFILE), "about:blank"],
        cwd=str(EXE.parent), stdout=subprocess.DEVNULL,
        stderr=stderr_path.open("wb"),
    )
    record["rootPid"] = process.pid
    try:
        for _ in range(7):
            time.sleep(8)
            sample = {
                "tree": process_tree_snapshot(process.pid),
                "windows": visible_windows_for_pid_tree(process.pid),
            }
            record["samples"].append(sample)
            print(json.dumps(sample, ensure_ascii=False), flush=True)
            if sample["windows"]:
                break
    finally:
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       capture_output=True)
        record["killedAt"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    record["stderrTail"] = stderr_path.read_text(
        encoding="utf-8", errors="replace").replace("\x00", "")[-1500:]
    OUT.write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n",
                   encoding="utf-8")
    print("WROTE", OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
