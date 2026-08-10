#!/usr/bin/env python3
"""Small platform boundary for the standalone Camoufox Host.

The protocol and Artifact code are intentionally platform-neutral.  This
module contains the operating-system operations that cannot be expressed by
the Python standard library on both Linux and Windows: profile leases,
process identity, process containment, binary stdio, and durable replacement.
"""

from __future__ import annotations

import ctypes
import os
import time
from pathlib import Path
from typing import Any, Optional

IS_WINDOWS = os.name == "nt"


if IS_WINDOWS:
    import msvcrt

    _KERNEL32 = ctypes.WinDLL("kernel32", use_last_error=True)
    _INVALID_HANDLE_VALUE = ctypes.c_void_p(-1).value
    _FILE_ATTRIBUTE_REPARSE_POINT = 0x0400
    _FILE_ATTRIBUTE_NORMAL = 0x0080
    _FILE_FLAG_OPEN_REPARSE_POINT = 0x00200000
    _FILE_SHARE_READ = 0x00000001
    _FILE_SHARE_WRITE = 0x00000002
    _GENERIC_READ = 0x80000000
    _GENERIC_WRITE = 0x40000000
    _OPEN_ALWAYS = 4
    _LOCKFILE_EXCLUSIVE_LOCK = 0x00000002
    _LOCKFILE_FAIL_IMMEDIATELY = 0x00000001
    _MOVEFILE_REPLACE_EXISTING = 0x00000001
    _MOVEFILE_WRITE_THROUGH = 0x00000008
    _PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    _PROCESS_SYNCHRONIZE = 0x00100000
    _JOB_OBJECT_QUERY = 0x0004
    _JOB_OBJECT_TERMINATE = 0x0008
    _JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000
    _JOB_OBJECT_EXTENDED_LIMIT_INFORMATION = 9
    _JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION = 1
    _STILL_ACTIVE = 259

    class _FILETIME(ctypes.Structure):
        _fields_ = [("dwLowDateTime", ctypes.c_uint32), ("dwHighDateTime", ctypes.c_uint32)]

    class _OVERLAPPED(ctypes.Structure):
        _fields_ = [
            ("Internal", ctypes.c_void_p),
            ("InternalHigh", ctypes.c_void_p),
            ("Offset", ctypes.c_uint32),
            ("OffsetHigh", ctypes.c_uint32),
            ("hEvent", ctypes.c_void_p),
        ]

    class _JOBOBJECT_BASIC_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("PerProcessUserTimeLimit", ctypes.c_longlong),
            ("PerJobUserTimeLimit", ctypes.c_longlong),
            ("LimitFlags", ctypes.c_uint32),
            ("MinimumWorkingSetSize", ctypes.c_size_t),
            ("MaximumWorkingSetSize", ctypes.c_size_t),
            ("ActiveProcessLimit", ctypes.c_uint32),
            ("Affinity", ctypes.c_size_t),
            ("PriorityClass", ctypes.c_uint32),
            ("SchedulingClass", ctypes.c_uint32),
        ]

    class _IO_COUNTERS(ctypes.Structure):
        _fields_ = [
            ("ReadOperationCount", ctypes.c_ulonglong),
            ("WriteOperationCount", ctypes.c_ulonglong),
            ("OtherOperationCount", ctypes.c_ulonglong),
            ("ReadTransferCount", ctypes.c_ulonglong),
            ("WriteTransferCount", ctypes.c_ulonglong),
            ("OtherTransferCount", ctypes.c_ulonglong),
        ]

    class _JOBOBJECT_EXTENDED_LIMIT_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("BasicLimitInformation", _JOBOBJECT_BASIC_LIMIT_INFORMATION),
            ("IoInfo", _IO_COUNTERS),
            ("ProcessMemoryLimit", ctypes.c_size_t),
            ("JobMemoryLimit", ctypes.c_size_t),
            ("PeakProcessMemoryUsed", ctypes.c_size_t),
            ("PeakJobMemoryUsed", ctypes.c_size_t),
        ]

    class _JOBOBJECT_BASIC_ACCOUNTING_INFORMATION(ctypes.Structure):
        _fields_ = [
            ("TotalUserTime", ctypes.c_longlong),
            ("TotalKernelTime", ctypes.c_longlong),
            ("ThisPeriodTotalUserTime", ctypes.c_longlong),
            ("ThisPeriodTotalKernelTime", ctypes.c_longlong),
            ("TotalPageFaultCount", ctypes.c_uint32),
            ("TotalProcesses", ctypes.c_uint32),
            ("ActiveProcesses", ctypes.c_uint32),
            ("TotalTerminatedProcesses", ctypes.c_uint32),
        ]

    _KERNEL32.CreateFileW.argtypes = [
        ctypes.c_wchar_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_void_p,
    ]
    _KERNEL32.CreateFileW.restype = ctypes.c_void_p
    _KERNEL32.CloseHandle.argtypes = [ctypes.c_void_p]
    _KERNEL32.CloseHandle.restype = ctypes.c_int
    _KERNEL32.LockFileEx.argtypes = [
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.POINTER(_OVERLAPPED),
    ]
    _KERNEL32.LockFileEx.restype = ctypes.c_int
    _KERNEL32.UnlockFileEx.argtypes = [
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.c_uint32,
        ctypes.POINTER(_OVERLAPPED),
    ]
    _KERNEL32.UnlockFileEx.restype = ctypes.c_int
    _KERNEL32.GetFileAttributesW.argtypes = [ctypes.c_wchar_p]
    _KERNEL32.GetFileAttributesW.restype = ctypes.c_uint32
    _KERNEL32.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    _KERNEL32.OpenProcess.restype = ctypes.c_void_p
    _KERNEL32.GetProcessTimes.argtypes = [
        ctypes.c_void_p,
        ctypes.POINTER(_FILETIME),
        ctypes.POINTER(_FILETIME),
        ctypes.POINTER(_FILETIME),
        ctypes.POINTER(_FILETIME),
    ]
    _KERNEL32.GetProcessTimes.restype = ctypes.c_int
    _KERNEL32.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    _KERNEL32.GetExitCodeProcess.restype = ctypes.c_int
    _KERNEL32.OpenJobObjectW.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_wchar_p]
    _KERNEL32.OpenJobObjectW.restype = ctypes.c_void_p
    _KERNEL32.QueryInformationJobObject.argtypes = [
        ctypes.c_void_p,
        ctypes.c_int,
        ctypes.c_void_p,
        ctypes.c_uint32,
        ctypes.POINTER(ctypes.c_uint32),
    ]
    _KERNEL32.QueryInformationJobObject.restype = ctypes.c_int
    _KERNEL32.TerminateJobObject.argtypes = [ctypes.c_void_p, ctypes.c_uint32]
    _KERNEL32.TerminateJobObject.restype = ctypes.c_int
    _KERNEL32.MoveFileExW.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_uint32]
    _KERNEL32.MoveFileExW.restype = ctypes.c_int
    _KERNEL32.FlushFileBuffers.argtypes = [ctypes.c_void_p]
    _KERNEL32.FlushFileBuffers.restype = ctypes.c_int


def _win_error(message: str) -> OSError:
    return ctypes.WinError(ctypes.get_last_error(), message)


def set_binary_stdio() -> None:
    """Disable CRT text translation for the Host's protocol descriptors."""
    if not IS_WINDOWS:
        return
    for fd in (0, 1, 2):
        try:
            msvcrt.setmode(fd, os.O_BINARY)
        except OSError:
            pass


def _windows_path_attributes(path: Path) -> Optional[int]:
    if not IS_WINDOWS:
        return None
    attrs = _KERNEL32.GetFileAttributesW(str(path))
    if attrs == 0xFFFFFFFF:
        error = ctypes.get_last_error()
        if error == 2:  # ERROR_FILE_NOT_FOUND
            return None
        if error == 3:  # ERROR_PATH_NOT_FOUND
            return None
        raise _win_error(f"GetFileAttributesW failed for {path}")
    return int(attrs)


def ensure_no_reparse_points(path: Path, allow_missing: bool = False) -> None:
    """Reject reparse points in every existing component of ``path``."""
    if not IS_WINDOWS:
        return
    absolute = Path(path)
    if not absolute.is_absolute():
        absolute = Path.cwd() / absolute
    current = Path(absolute.anchor)
    for component in absolute.parts[1:]:
        current = current / component
        attrs = _windows_path_attributes(current)
        if attrs is None:
            if allow_missing:
                return
            raise FileNotFoundError(str(current))
        if attrs & _FILE_ATTRIBUTE_REPARSE_POINT:
            raise OSError(f"reparse point rejected: {current}")


class ProfileLock:
    """A cross-process profile lease held by a live OS file handle."""

    def __init__(self, path: Path, offset: int = 0) -> None:
        self.path = Path(path)
        self.offset = offset
        self._fd: Optional[int] = None
        self._handle: Optional[int] = None
        self._overlapped: Any = None

    @classmethod
    def acquire(cls, path: Path, offset: int = 0) -> "ProfileLock":
        lock = cls(path, offset)
        lock._acquire()
        return lock

    def _acquire(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        if IS_WINDOWS:
            ensure_no_reparse_points(self.path.parent, allow_missing=False)
            attrs = _windows_path_attributes(self.path)
            if attrs is not None and attrs & _FILE_ATTRIBUTE_REPARSE_POINT:
                raise OSError(f"reparse point rejected: {self.path}")
            handle = _KERNEL32.CreateFileW(
                str(self.path),
                _GENERIC_READ | _GENERIC_WRITE,
                _FILE_SHARE_READ | _FILE_SHARE_WRITE,
                None,
                _OPEN_ALWAYS,
                _FILE_ATTRIBUTE_NORMAL | _FILE_FLAG_OPEN_REPARSE_POINT,
                None,
            )
            if handle in (None, 0, _INVALID_HANDLE_VALUE):
                raise _win_error(f"cannot open profile lock {self.path}")
            self._handle = int(handle)
            overlapped = _OVERLAPPED()
            overlapped.Offset = self.offset & 0xFFFFFFFF
            overlapped.OffsetHigh = (self.offset >> 32) & 0xFFFFFFFF
            if not _KERNEL32.LockFileEx(
                ctypes.c_void_p(self._handle),
                _LOCKFILE_EXCLUSIVE_LOCK | _LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                ctypes.byref(overlapped),
            ):
                _KERNEL32.CloseHandle(ctypes.c_void_p(self._handle))
                self._handle = None
                raise _win_error(f"profile lock is already held: {self.path}")
            self._overlapped = overlapped
            return

        import fcntl

        self._fd = os.open(self.path, os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(self._fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            os.close(self._fd)
            self._fd = None
            raise

    def release(self) -> None:
        if IS_WINDOWS:
            if self._handle is None:
                return
            try:
                _KERNEL32.UnlockFileEx(
                    ctypes.c_void_p(self._handle),
                    0,
                    1,
                    0,
                    ctypes.byref(self._overlapped),
                )
            finally:
                _KERNEL32.CloseHandle(ctypes.c_void_p(self._handle))
                self._handle = None
                self._overlapped = None
            return

        if self._fd is not None:
            import fcntl

            try:
                fcntl.flock(self._fd, fcntl.LOCK_UN)
            finally:
                os.close(self._fd)
                self._fd = None

    @property
    def handle_value(self) -> Optional[int]:
        return self._handle if IS_WINDOWS else self._fd

    def __enter__(self) -> "ProfileLock":
        return self

    def __exit__(self, *_args: Any) -> None:
        self.release()


def probe_supervisor_lock(path: Path) -> bool:
    """Check the supervisor-owned byte without retaining the probe lock."""
    if not IS_WINDOWS:
        return True
    try:
        lock = ProfileLock.acquire(path, offset=1)
    except OSError:
        return False
    lock.release()
    return True


def _filetime_value(value: Any) -> int:
    return (int(value.dwHighDateTime) << 32) | int(value.dwLowDateTime)


def process_creation_time(pid: int) -> Optional[int]:
    if not IS_WINDOWS:
        return None
    if not isinstance(pid, int) or pid <= 0:
        return None
    handle = _KERNEL32.OpenProcess(
        _PROCESS_QUERY_LIMITED_INFORMATION | _PROCESS_SYNCHRONIZE, 0, pid
    )
    if handle in (None, 0):
        return None
    try:
        creation = _FILETIME()
        exit_time = _FILETIME()
        kernel_time = _FILETIME()
        user_time = _FILETIME()
        if not _KERNEL32.GetProcessTimes(
            handle,
            ctypes.byref(creation),
            ctypes.byref(exit_time),
            ctypes.byref(kernel_time),
            ctypes.byref(user_time),
        ):
            return None
        return _filetime_value(creation)
    finally:
        _KERNEL32.CloseHandle(handle)


def process_identity_alive(identity: dict) -> bool:
    if not IS_WINDOWS:
        return False
    pid = identity.get("pid")
    expected = identity.get("creationTime100ns")
    if type(pid) is not int or pid <= 0 or type(expected) is not int or expected <= 0:
        return False
    handle = _KERNEL32.OpenProcess(
        _PROCESS_QUERY_LIMITED_INFORMATION | _PROCESS_SYNCHRONIZE, 0, pid
    )
    if handle in (None, 0):
        return False
    try:
        creation = _FILETIME()
        exit_time = _FILETIME()
        kernel_time = _FILETIME()
        user_time = _FILETIME()
        if not _KERNEL32.GetProcessTimes(
            handle,
            ctypes.byref(creation),
            ctypes.byref(exit_time),
            ctypes.byref(kernel_time),
            ctypes.byref(user_time),
        ):
            return False
        if _filetime_value(creation) != expected:
            return False
        exit_code = ctypes.c_uint32()
        if not _KERNEL32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
            return False
        return int(exit_code.value) == _STILL_ACTIVE
    finally:
        _KERNEL32.CloseHandle(handle)


class JobHandle:
    """A named Windows Job Object handle used for evidence and termination."""

    def __init__(self, name: str, handle: int) -> None:
        self.name = name
        self.handle = handle

    @classmethod
    def open(cls, name: str) -> "JobHandle":
        if not IS_WINDOWS:
            raise OSError("Job Objects require Windows")
        handle = _KERNEL32.OpenJobObjectW(
            _JOB_OBJECT_QUERY | _JOB_OBJECT_TERMINATE,
            0,
            name,
        )
        if handle in (None, 0, _INVALID_HANDLE_VALUE):
            raise _win_error(f"cannot open Job Object {name}")
        return cls(name, int(handle))

    def active_process_count(self) -> int:
        info = _JOBOBJECT_BASIC_ACCOUNTING_INFORMATION()
        returned = ctypes.c_uint32()
        if not _KERNEL32.QueryInformationJobObject(
            ctypes.c_void_p(self.handle),
            _JOB_OBJECT_BASIC_ACCOUNTING_INFORMATION,
            ctypes.byref(info),
            ctypes.sizeof(info),
            ctypes.byref(returned),
        ):
            raise _win_error(f"cannot query Job Object {self.name}")
        return int(info.ActiveProcesses)

    def wait_empty(self, timeout: float) -> tuple[bool, int]:
        deadline = time.monotonic() + timeout
        last = -1
        while time.monotonic() < deadline:
            try:
                last = self.active_process_count()
            except OSError:
                return False, last
            if last == 0:
                return True, 0
            time.sleep(0.05)
        try:
            last = self.active_process_count()
        except OSError:
            pass
        return last == 0, last

    def terminate(self, exit_code: int = 1) -> None:
        if not _KERNEL32.TerminateJobObject(ctypes.c_void_p(self.handle), exit_code):
            raise _win_error(f"cannot terminate Job Object {self.name}")

    def close(self) -> None:
        if self.handle:
            _KERNEL32.CloseHandle(ctypes.c_void_p(self.handle))
            self.handle = 0

    def __enter__(self) -> "JobHandle":
        return self

    def __exit__(self, *_args: Any) -> None:
        self.close()


def replace_file_durable(source: Path, destination: Path) -> None:
    """Replace a file atomically and request write-through on Windows."""
    if not IS_WINDOWS:
        os.replace(source, destination)
        return
    if not _KERNEL32.MoveFileExW(
        str(source),
        str(destination),
        _MOVEFILE_REPLACE_EXISTING | _MOVEFILE_WRITE_THROUGH,
    ):
        raise _win_error(f"cannot replace {destination}")


def flush_path(path: Path) -> None:
    if not IS_WINDOWS:
        return
    handle = _KERNEL32.CreateFileW(
        str(path),
        _GENERIC_READ | _GENERIC_WRITE,
        _FILE_SHARE_READ | _FILE_SHARE_WRITE | 0x00000004,
        None,
        3,  # OPEN_EXISTING
        _FILE_ATTRIBUTE_NORMAL,
        None,
    )
    if handle in (None, 0, _INVALID_HANDLE_VALUE):
        raise _win_error(f"cannot open {path} for flush")
    try:
        if not _KERNEL32.FlushFileBuffers(handle):
            raise _win_error(f"cannot flush {path}")
    finally:
        _KERNEL32.CloseHandle(handle)


def terminate_windows_job(session: dict, timeout: float = 8.0) -> dict:
    """Wait for, then if needed terminate, a Job Object without PID walking."""
    if not IS_WINDOWS:
        raise OSError("Windows Job Objects require Windows")
    job = session.get("jobHandle")
    opened_here = False
    if job is None:
        name = (session.get("supervisorMeta") or {}).get("jobName")
        if not isinstance(name, str) or not name:
            name = session.get("expectedJobName")
        if not isinstance(name, str) or not name:
            if (
                session.get("launchAttempted") is not True
                and session.get("ctx") is None
                and not session.get("pid")
            ):
                return {
                    "exited": True,
                    "managedIdentities": [],
                    "remaining": [],
                    "job": {"available": False, "reason": "no process was spawned"},
                }
            return {
                "exited": False,
                "job": {"available": False, "reason": "job metadata missing"},
                "remaining": [],
            }
        try:
            job = JobHandle.open(name)
            opened_here = True
        except OSError as exc:
            return {
                "exited": False,
                "job": {"available": False, "reason": str(exc)},
                "remaining": [],
            }
    try:
        graceful, active = job.wait_empty(timeout)
        terminated = False
        if not graceful:
            try:
                job.terminate(1)
                terminated = True
            except OSError:
                pass
            graceful, active = job.wait_empty(timeout)
        return {
            "exited": graceful,
            "managedIdentities": session.get("managedIdentities", []),
            "remaining": [] if graceful else session.get("managedIdentities", []),
            "job": {
                "available": True,
                "name": job.name,
                "activeProcessCount": active,
                "terminateJobObject": terminated,
            },
            "sigterm": not terminated,
            "sigkill": terminated,
        }
    finally:
        if opened_here:
            job.close()
