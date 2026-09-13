"""Direct Playwright control probe: launch the packaged supervisor without the Host.

Diagnostic control only — isolates whether the Juggler handshake stall is
environmental (Playwright/engine/session) or Host-source-specific.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import time
from pathlib import Path


async def run(args: argparse.Namespace) -> int:
    from playwright.async_api import async_playwright

    profile = Path(args.profile_dir)
    profile.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    async with async_playwright() as p:
        import os
        import uuid
        session_dir = profile.parent / "direct-probe-session"
        session_dir.mkdir(parents=True, exist_ok=True)
        env = dict(os.environ)
        if args.clean_env:
            keep = (
                "ALLUSERSPROFILE", "APPDATA", "COMPUTERNAME", "COMSPEC", "HOMEDRIVE",
                "HOMEPATH", "LOCALAPPDATA", "NUMBER_OF_PROCESSORS", "OS", "PATH",
                "PATHEXT", "PROCESSOR_ARCHITECTURE", "PROGRAMDATA", "PROGRAMFILES",
                "SYSTEMDRIVE", "SYSTEMROOT", "TEMP", "TMP", "USERDOMAIN",
                "USERDOMAIN_ROAMINGPROFILE", "USERNAME", "USERPROFILE", "WINDIR",
            )
            env = {key: value for key, value in env.items() if key.upper() in keep}
        env["VERISILO_REAL_EXE"] = str(args.real_exe)
        env["VERISILO_EXIT_FILE"] = str(session_dir / "exit.json")
        env["VERISILO_SUPERVISOR_FILE"] = str(session_dir / "supervisor.json")
        env["VERISILO_PROFILE_LOCK_PATH"] = str(profile.parent / "direct-probe.lock")
        env["VERISILO_JOB_NAME"] = f"Local\\VeriSiloCamoufox-{uuid.uuid4()}"
        try:
            ctx = await asyncio.wait_for(
                p.firefox.launch_persistent_context(
                    user_data_dir=str(profile),
                    executable_path=str(args.supervisor),
                    headless=False,
                    env=env,
                ),
                timeout=args.timeout,
            )
        except asyncio.TimeoutError:
            print(json.dumps({
                "outcome": "launch_timeout",
                "elapsedSeconds": round(time.monotonic() - started, 1),
            }))
            return 2
        except Exception as exc:  # noqa: BLE001
            print(json.dumps({
                "outcome": "launch_error",
                "elapsedSeconds": round(time.monotonic() - started, 1),
                "error": f"{type(exc).__name__}: {exc}",
            }))
            return 3
        elapsed = round(time.monotonic() - started, 1)
        pages = ctx.pages
        page = pages[0] if pages else await ctx.new_page()
        observation = await page.evaluate(
            "() => ({outerWidth: window.outerWidth, outerHeight: window.outerHeight,"
            " screenX: window.screenX, screenLeft: window.screenLeft,"
            " screen: {w: screen.width, h: screen.height,"
            " aw: screen.availWidth, ah: screen.availHeight,"
            " al: screen.availLeft, at: screen.availTop}})"
        )
        print(json.dumps({
            "outcome": "launched",
            "elapsedSeconds": elapsed,
            "pageCount": len(pages),
            "observation": observation,
        }, indent=2))
        await ctx.close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--supervisor", type=Path, required=True)
    parser.add_argument("--real-exe", type=Path, required=True)
    parser.add_argument("--profile-dir", type=Path, required=True)
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("--clean-env", action="store_true",
                        help="use a minimal Windows environment block")
    return asyncio.run(run(parser.parse_args()))


if __name__ == "__main__":
    raise SystemExit(main())
