#!/usr/bin/env python3
"""Record the frozen browser's native ELF dependency closure without rebuilding it."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def missing_libraries(output):
    return re.findall(r"^\s*(\S+)\s+=>\s+not found\s*$", output, re.MULTILINE)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser-root", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert missing_libraries("libxul.so => /browser/libxul.so (0x123)\n") == []
        assert missing_libraries("\tlibXt.so.6 => not found\n\tlibc.so.6 => /lib/libc.so.6 (0x123)\n") == ["libXt.so.6"]
        assert missing_libraries("liba.so => not found\nlibb.so => not found\n") == ["liba.so", "libb.so"]
        print("Native browser dependency report self-test passed.")
        return 0
    if sys.platform != "linux":
        parser.error("ELF dependency checks must run on native Linux")
    if args.browser_root is None or args.out is None:
        parser.error("--browser-root and --out are required")

    root = args.browser_root.resolve(strict=True)
    if not (root / "camoufox-bin").is_file():
        parser.error("browser root must contain the frozen camoufox-bin")
    binaries = []
    for path in sorted(root.rglob("*")):
        if path.is_file():
            with path.open("rb") as stream:
                if stream.read(4) == b"\x7fELF":
                    binaries.append(path)
    env = {**os.environ, "LC_ALL": "C"}
    env.pop("LD_PRELOAD", None)
    directories = sorted({str(path.parent) for path in binaries})
    env["LD_LIBRARY_PATH"] = os.pathsep.join(directories)
    report = {
        "browserRoot": str(root),
        "libraryDirectories": directories,
        "elfCount": len(binaries),
        "passed": False,
        "missingDependencies": [],
        "commandErrors": [],
        "entries": [],
    }
    for path in binaries:
        relative = path.relative_to(root).as_posix()
        entry = {"path": relative}
        for name, command in [
            ("file", ["file", "--brief", "--", str(path)]),
            ("readelf", ["readelf", "--file-header", "--dynamic", "--wide", "--", str(path)]),
            ("ldd", ["ldd", "--", str(path)]),
        ]:
            try:
                result = subprocess.run(command, env=env, capture_output=True, text=True, errors="replace", timeout=60)
                output = result.stdout + result.stderr
                entry[name] = {"exitCode": result.returncode, "output": output}
                static = name == "ldd" and ("not a dynamic executable" in output or "statically linked" in output)
                if result.returncode != 0 and not static:
                    report["commandErrors"].append({"path": relative, "command": name, "exitCode": result.returncode})
                if name == "ldd":
                    for library in missing_libraries(output):
                        report["missingDependencies"].append({"path": relative, "library": library})
            except (OSError, subprocess.TimeoutExpired) as error:
                entry[name] = {"error": str(error)}
                report["commandErrors"].append({"path": relative, "command": name, "error": str(error)})
        report["entries"].append(entry)
    report["passed"] = bool(binaries) and not report["missingDependencies"] and not report["commandErrors"]
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf8")
    for failure in report["missingDependencies"]:
        print(f"Missing native browser dependency: {failure['path']} requires {failure['library']}", file=sys.stderr)
    for failure in report["commandErrors"]:
        print(f"Native browser dependency inspection failed: {failure}", file=sys.stderr)
    print(f"Native browser ELF closure: {len(binaries)} files; passed={report['passed']}; report={args.out}")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
