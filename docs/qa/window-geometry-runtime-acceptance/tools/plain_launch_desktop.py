#!/usr/bin/env python3
"""Diagnostic: plain-launch camoufox.exe on the interactive desktop.

Tracks ALL camoufox.exe processes (the exe is a launcher stub: the tracked
child re-parents after the stub exits), their descendant trees, and every
visible window owned by the target executable path.  Kills only processes
running the exact target exe path.  Evidence-only diagnostic; fresh temp
profile, no user data.
"""

from __future__ import annotations

import argparse
import ctypes
import json
import subprocess
import sys
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(REPO_ROOT / "apps" / "camoufox-host"))
from host_platform import visible_windows_for_executable  # noqa: E402


class _PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [
        ("dwSize", ctypes.c_uint32), ("cntUsage", ctypes.c_uint32),
        ("th32ProcessID", ctypes.c_uint32),
        ("th32DefaultHeapID", ctypes.POINTER(ctypes.c_ulong)),
        ("th32ModuleID", ctypes.c_uint32), ("cntThreads", ctypes.c_uint32),
        ("th32ParentProcessID", ctypes.c_uint32), ("pcPriClassBase", ctypes.c_long),
        ("dwFlags", ctypes.c_uint32), ("szExeFile", ctypes.c_wchar * 260),
    ]


def snapshot() -> list[tuple[int, int, str]]:
    kernel32 = ctypes.windll.kernel32
    snap = kernel32.CreateToolhelp32Snapshot(0x2, 0)
    entry = _PROCESSENTRY32W()
    entry.dwSize = ctypes.sizeof(_PROCESSENTRY32W)
    rows: list[tuple[int, int, str]] = []
    if kernel32.Process32FirstW(snap, ctypes.byref(entry)):
        while True:
            rows.append((entry.th32ProcessID, entry.th32ParentProcessID,
                         entry.szExeFile))
            if not kernel32.Process32NextW(snap, ctypes.byref(entry)):
                break
    kernel32.CloseHandle(snap)
    return rows


def camoufox_pids(exe_name: str = "camoufox.exe") -> list[int]:
    return [pid for pid, _ppid, name in snapshot() if name.lower() == exe_name]


def descendant_tree(root: int) -> list[dict]:
    rows = snapshot()
    by_parent: dict[int, list[tuple[int, str]]] = {}
    for pid, ppid, name in rows:
        by_parent.setdefault(ppid, []).append((pid, name))
    result: list[dict] = []
    seen = {root}
    stack = [root]
    while stack:
        current = stack.pop()
        for pid, name in by_parent.get(current, []):
            if pid in seen:
                continue
            seen.add(pid)
            result.append({"pid": pid, "name": name})
            stack.append(pid)
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--profile", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=8)
    parser.add_argument("--interval", type=float, default=8.0)
    args = parser.parse_args()

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.profile.mkdir(parents=True, exist_ok=True)
    record: dict = {
        "exe": str(args.exe),
        "exeExists": args.exe.exists(),
        "startedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "samples": [],
    }
    if not args.exe.exists():
        record["error"] = "exe missing"
        args.out.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(record, indent=2))
        return 2

    stderr_path = args.out.parent / (args.out.stem + ".stderr.log")
    process = subprocess.Popen(
        [str(args.exe), "-no-remote", "-profile", str(args.profile), "about:blank"],
        cwd=str(args.exe.parent), stdout=subprocess.DEVNULL,
        stderr=stderr_path.open("wb"),
    )
    record["stubPid"] = process.pid
    try:
        for index in range(args.samples):
            time.sleep(args.interval if index else 4.0)
            pids = camoufox_pids()
            sample = {
                "camoufoxPids": pids,
                "windows": visible_windows_for_executable(args.exe),
                "trees": {str(pid): descendant_tree(pid) for pid in pids},
            }
            record["samples"].append(sample)
            print(json.dumps(sample, ensure_ascii=False), flush=True)
            if sample["windows"]:
                break
    finally:
        time.sleep(1.0)
        for pid in camoufox_pids():
            subprocess.run(
                ["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True)
        record["killedAt"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    record["stderrTail"] = stderr_path.read_text(
        encoding="utf-8", errors="replace").replace("\x00", "")[-1500:]
    args.out.write_text(
        json.dumps(record, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print("WROTE", args.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
