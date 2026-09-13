"""One-shot --provision-artifact run against a packaged host exe (control only)."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host-exe", type=Path, required=True)
    parser.add_argument("--package-root", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--window", default="1280x800")
    parser.add_argument("--preset", default="balanced-en-us")
    parser.add_argument("--artifact-root", type=Path, required=True)
    parser.add_argument("--state-root", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args()

    width, height = (int(part) for part in args.window.lower().split("x", 1))
    args.out_dir.mkdir(parents=True, exist_ok=True)
    log_file = (args.out_dir / "provision-control.stderr.log").open("wb")
    process = subprocess.Popen(
        [str(args.host_exe), "--provision-artifact",
         "--package-root", str(args.package_root),
         "--artifact-root", str(args.artifact_root),
         "--state-root", str(args.state_root)],
        cwd=str(args.host_exe.parent), stdin=subprocess.PIPE,
        stdout=subprocess.PIPE, stderr=log_file,
    )
    request = json.dumps({
        "seed": args.seed, "preset": args.preset, "window": [width, height],
    }).encode("utf-8")
    assert process.stdin is not None
    process.stdin.write(len(request).to_bytes(4, "big") + request)
    process.stdin.flush()
    header = process.stdout.read(4)  # type: ignore[union-attr]
    if len(header) != 4:
        raise SystemExit(f"host closed stdout before header (exit pending)")
    payload = process.stdout.read(int.from_bytes(header, "big"))  # type: ignore[union-attr]
    process.stdin.close()
    process.wait(timeout=120)
    log_file.close()
    response = json.loads(payload.decode("utf-8"))
    (args.out_dir / "provision-control.response.json").write_text(
        json.dumps(response, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps({
        "ok": response.get("ok"),
        "error": response.get("error"),
        "artifactId": (response.get("result") or {}).get("artifactId"),
    }))
    return 0 if response.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
