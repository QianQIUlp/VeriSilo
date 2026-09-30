#!/usr/bin/env python3
"""Linux browser exit supervisor (and historical standalone Windows helper).

Playwright 1.60's Python API does not expose the browser process or its exit
code for persistent contexts. This wrapper is passed as executable_path: it
spawns the real Camoufox binary with the exact arguments Playwright provides,
hands stdin/stdout/stderr straight through, forwards termination signals, and
records the real browser process's exit code to VERISILO_EXIT_FILE.

On Linux the supervisor owns a separate process group and watches the exact
Host PID/start time. Host death reclaims that group even if the desktop or
Playwright driver can no longer run the ordinary shutdown path.
"""

from __future__ import annotations

import json
import os
import signal
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path

from host_platform import IS_WINDOWS, ProfileLock, process_creation_time


def main() -> int:
    real_exe = os.environ.pop("VERISILO_REAL_EXE", None)
    exit_file = os.environ.pop("VERISILO_EXIT_FILE", None)
    supervisor_file = os.environ.pop("VERISILO_SUPERVISOR_FILE", None)
    host_pid = os.environ.pop("VERISILO_HOST_PID", None)
    host_start = os.environ.pop("VERISILO_HOST_START_TICKS", None)
    profile_lock_path = os.environ.pop("VERISILO_PROFILE_LOCK_PATH", None)
    if not real_exe or not exit_file:
        print("exit_supervisor: VERISILO_REAL_EXE and VERISILO_EXIT_FILE required", file=sys.stderr)
        return 2

    child_argv = [real_exe, *sys.argv[1:]]
    if not IS_WINDOWS and os.getpgrp() != os.getpid():
        os.setsid()
    profile_lease = None
    if not IS_WINDOWS and profile_lock_path:
        path = Path(profile_lock_path)
        profile_lease = ProfileLock.acquire(path.with_name(path.name + ".supervisor"))
    child_env = os.environ.copy()
    child_env.pop("VERISILO_REAL_EXE", None)
    child_env.pop("VERISILO_EXIT_FILE", None)
    child_env.pop("VERISILO_SUPERVISOR_FILE", None)
    if not IS_WINDOWS and getattr(sys, "frozen", False):
        original = child_env.pop("LD_LIBRARY_PATH_ORIG", None)
        if original is None:
            child_env.pop("LD_LIBRARY_PATH", None)
        else:
            child_env["LD_LIBRARY_PATH"] = original

    proc = subprocess.Popen(
        child_argv,
        cwd=os.path.dirname(real_exe),
        stdin=sys.stdin.fileno(),
        stdout=sys.stdout.fileno(),
        stderr=sys.stderr.fileno(),
        env=child_env,
        # The Playwright driver passes an extra protocol fd to the spawned
        # process; closing it (Python's default) makes camoufox-bin exit 0
        # during startup. Inherit all fds like a shell would.
        close_fds=False,
        start_new_session=False,
    )

    watcher = None
    if not IS_WINDOWS and host_pid and host_start:
        try:
            identity = (int(host_pid), int(host_start))
            if identity[0] <= 0 or identity[1] <= 0:
                raise ValueError("invalid Host process identity")
        except ValueError:
            proc.terminate()
            proc.wait()
            return 2

        def watch_host() -> None:
            while proc.poll() is None:
                if starttime_ticks(identity[0]) != identity[1]:
                    # The supervisor is the live group leader, so the group
                    # cannot be confused with an unrelated reused PID.
                    os.killpg(os.getpid(), signal.SIGTERM)
                    time.sleep(1)
                    os.killpg(os.getpid(), signal.SIGKILL)
                    return
                time.sleep(0.2)

        watcher = threading.Thread(target=watch_host)

    if supervisor_file:
        try:
            with open(supervisor_file, "w", encoding="utf-8") as fh:
                if IS_WINDOWS:
                    metadata = {
                        "supervisorPid": os.getpid(),
                        "supervisorCreationTime100ns": process_creation_time(os.getpid()),
                        "childPid": proc.pid,
                        "childCreationTime100ns": process_creation_time(proc.pid),
                        "observedAtUtc": datetime.now(timezone.utc).isoformat(),
                    }
                else:
                    metadata = {
                        "supervisorPid": os.getpid(),
                        "supervisorStartTimeTicks": starttime_ticks(os.getpid()),
                        "supervisorProcessGroup": os.getpgid(os.getpid()),
                        "childPid": proc.pid,
                        "childStartTimeTicks": starttime_ticks(proc.pid),
                        "childProcessGroup": os.getpgid(proc.pid),
                        "observedAtUtc": datetime.now(timezone.utc).isoformat(),
                    }
                fh.write(json.dumps(metadata) + "\n")
        except OSError:
            pass

    def forward(signum: int, _frame) -> None:
        if proc.poll() is None:
            proc.send_signal(signum)

    signals = (signal.SIGTERM, signal.SIGINT)
    if not IS_WINDOWS:
        signals += (signal.SIGHUP,)
    for sig in signals:
        signal.signal(sig, forward)
    if watcher is not None:
        watcher.start()

    code = proc.wait()
    try:
        with open(exit_file, "w", encoding="utf-8") as fh:
            fh.write(
                json.dumps(
                    {
                        "exitCode": code,
                        "observedAtUtc": datetime.now(timezone.utc).isoformat(),
                    }
                )
            )
    except OSError:
        pass
    if profile_lease is not None:
        # Parent-death cleanup keeps the interpreter alive until its watcher
        # kills the owned group, preserving this lease through the grace period.
        if watcher is None or not watcher.is_alive():
            profile_lease.release()
    return code


def starttime_ticks(pid: int) -> int:
    """Field 22 of /proc/<pid>/stat (starttime in clock ticks)."""
    try:
        text = Path(f"/proc/{pid}/stat").read_text()
    except OSError:
        return -1
    fields = text.rsplit(")", 1)
    if len(fields) != 2:
        return -1
    parts = fields[1].split()
    if not parts or parts[0] == "Z":
        return -1
    try:
        return int(parts[19])  # 22nd field overall
    except (IndexError, ValueError):
        return -1


if __name__ == "__main__":
    raise SystemExit(main())
