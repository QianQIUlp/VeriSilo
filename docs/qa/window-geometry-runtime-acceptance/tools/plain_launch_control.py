#!/usr/bin/env python3
"""Diagnostic: plain-launch camoufox.exe without Host/supervisor/Playwright.

Records the child process tree and visible windows for ~60s, then kills the
tree.  Evidence-only diagnostic; touches no user data (fresh temp profile).
"""

from __future__ import annotations

import ctypes
import json
import os
import subprocess
import sys
import time
from pathlib import Path

EXE = Path(r"C:\Users\qiu\src\VeriSilo\artifacts\release\managed-browser\v0.1.0-rc2\engine-package\browser\camoufox.exe")
PROFILE = Path(r"C:\Users\qiu\AppData\Local\Temp\camoufox-plain-control\profile")
OUT = Path(r"C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\qa-accept-real-managed-e0f551\docs\qa\window-geometry-runtime-acceptance\diagnostics\plain-launch-control.json")

PARENT_FIELDS = ("exe_path",)


def visible_windows_for_pid_tree(root_pid: int) -> list[dict]:
    user32 = ctypes.windll.user32

    class RECT(ctypes.Structure):
        _fields_ = [("left", ctypes.c_long), ("top", ctypes.c_long),
                    ("right", ctypes.c_long), ("bottom", ctypes.c_long)]

    # Build the descendant pid set from a toolhelp snapshot.
    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", ctypes.c_uint32), ("cntUsage", ctypes.c_uint32),
            ("th32ProcessID", ctypes.c_uint32),
            ("th32DefaultHeapID", ctypes.POINTER(ctypes.c_ulong)),
            ("th32ModuleID", ctypes.c_uint32), ("cntThreads", ctypes.c_uint32),
            ("th32ParentProcessID", ctypes.c_uint32), ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", ctypes.c_uint32), ("szExeFile", ctypes.c_wchar * 260),
        ]

    kernel32 = ctypes.windll.kernel32
    TH32CS_SNAPPROCESS = 0x2
    snap = kernel32.CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)

    entries = []
    entry = PROCESSENTRY32W()
    entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
    if kernel32.Process32FirstW(snap, ctypes.byref(entry)):
        while True:
            entries.append((entry.th32ProcessID, entry.th32ParentProcessID,
                            entry.szExeFile))
            if not kernel32.Process32NextW(snap, ctypes.byref(entry)):
                break
    kernel32.CloseHandle(snap)

    def tree_pids(root: int) -> list[tuple[int, str]]:
        by_parent: dict[int, list[tuple[int, str]]] = {}
        for pid, parent, name in entries:
            by_parent.setdefault(parent, []).append((pid, name))
        result = []
        stack = [root]
        while stack:
            current = stack.pop()
            for pid, name in by_parent.get(current, []):
                result.append((pid, name))
                stack.append(pid)
        return result

    pids = {pid for pid, _ in tree_pids(root_pid)}

    windows = []
    @ctypes.WINFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)
    def visit(hwnd, _lparam):
        if not user32.IsWindowVisible(hwnd):
            return 1
        pid = ctypes.c_uint32()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value in pids:
            rect = RECT()
            user32.GetWindowRect(hwnd, ctypes.byref(rect))
            buf = ctypes.create_unicode_buffer(256)
            user32.GetWindowTextW(hwnd, buf, 256)
            windows.append({
                "pid": pid.value, "title": buf.value,
                "rect": [rect.left, rect.top, rect.right - rect.left,
                         rect.bottom - rect.top],
            })
        return 1

    user32.EnumWindows(visit, None)
    return windows


def process_tree_snapshot(root_pid: int) -> list[dict]:
    class PROCESSENTRY32W(ctypes.Structure):
        _fields_ = [
            ("dwSize", ctypes.c_uint32), ("cntUsage", ctypes.c_uint32),
            ("th32ProcessID", ctypes.c_uint32),
            ("th32DefaultHeapID", ctypes.POINTER(ctypes.c_ulong)),
            ("th32ModuleID", ctypes.c_uint32), ("cntThreads", ctypes.c_uint32),
            ("th32ParentProcessID", ctypes.c_uint32), ("pcPriClassBase", ctypes.c_long),
            ("dwFlags", ctypes.c_uint32), ("szExeFile", ctypes.c_wchar * 260),
        ]

    kernel32 = ctypes.windll.kernel32
    snap = kernel32.CreateToolhelp32Snapshot(0x2, 0)
    entries = []
    entry = PROCESSENTRY32W()
    entry.dwSize = ctypes.sizeof(PROCESSENTRY32W)
    if kernel32.Process32FirstW(snap, ctypes.byref(entry)):
        while True:
            entries.append((entry.th32ProcessID, entry.th32ParentProcessID,
                            entry.szExeFile))
            if not kernel32.Process32NextW(snap, ctypes.byref(entry)):
                break
    kernel32.CloseHandle(snap)

    def tree_pids(root: int) -> list[tuple[int, str]]:
        by_parent: dict[int, list[tuple[int, str]]] = {}
        for pid, parent, name in entries:
            by_parent.setdefault(parent, []).append((pid, name))
        result = []
        stack = [root]
        while stack:
            current = stack.pop()
            for pid, name in by_parent.get(current, []):
                result.append((pid, name))
                stack.append(pid)
        return result

    return [{"pid": pid, "name": name} for pid, name in tree_pids(root_pid)]


def main() -> int:
    OUT.parent.mkdir(parents=True, exist_ok=True)
    PROFILE.mkdir(parents=True, exist_ok=True)
    record: dict = {
        "exe": str(EXE), "exeExists": EXE.exists(),
        "ancestry": os.environ.get("VERISILO_CONTROL_ANCESTRY", "agent-sandbox"),
        "startedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "samples": [],
    }
    if not EXE.exists():
        record["error"] = "exe missing"
        OUT.write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
        print(json.dumps(record, indent=2))
        return 2
    stderr_path = OUT.parent / "plain-launch-control.stderr.log"
    process = subprocess.Popen(
        [str(EXE), "-no-remote", "-profile", str(PROFILE), "about:blank"],
        cwd=str(EXE.parent), stdout=subprocess.DEVNULL,
        stderr=stderr_path.open("wb"),
    )
    record["rootPid"] = process.pid
    try:
        for index in range(7):
            time.sleep(8 if index else 5)
            sample = {
                "at": round(time.monotonic() - 0, 1),
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
    tail = stderr_path.read_text(encoding="utf-8", errors="replace")[-2000:]
    record["stderrTail"] = tail
    OUT.write_text(json.dumps(record, indent=2, ensure_ascii=False) + "\n",
                   encoding="utf-8")
    print("WROTE", OUT)
    return 0


if __name__ == "__main__":
    sys.exit(main())
