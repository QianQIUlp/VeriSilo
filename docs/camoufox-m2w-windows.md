# VeriSilo M2-W Windows Manual Gate

Status: host-local Windows evidence only. Every result remains
`verified: false` and uses `evidenceClass: observed-on-this-windows-host`.
This gate covers the standalone Camoufox Host only. It does not authorize
Tauri, EngineAdapter, desktop UI, installers, product protocol integration,
font isolation, Canvas identity, TLS/QUIC identity, or cross-host replay.

## Scope

The gate runs on a real Windows desktop session and proves three bounded
properties:

1. A persistent Camoufox profile survives a Host process restart.
2. Camoufox is owned and cleaned through a dedicated Windows Job Object.
3. Windows Artifact v3 fixtures can be replayed with stable website digests.

Linux fixtures, Linux asset locks, the Linux tree manifest, and the Linux
evidence manifest are independent and are not replaced by this gate.

## Windows Platform Boundary

`host_platform.py` owns the operating-system boundary. Windows profile leases
use a real file handle plus `LockFileEx`; the Host holds byte 0 and the native
supervisor holds byte 1 until the Job is empty or fail-closed. The lock file is
opened without delete sharing and reparse points are rejected.

The native supervisor in `windows-supervisor/` uses raw Win32 FFI and no
third-party runtime dependency. It creates a named Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, assigns the supervisor and browser before
resume, records PID plus process creation time, and watches its Playwright
parent. It forwards Playwright's Windows CRT stdio inheritance buffer
(`STARTUPINFO.lpReserved2`) so Firefox's juggler pipe remains intact.

Windows does not import `fcntl`, read `/proc`, or start Xvfb. Host stdio is set
to binary mode. Browser tree verification rejects missing, extra, modified,
symlinked, junction, mount-point, and other reparse entries, with normalized
relative paths and case-insensitive matching on Windows.

`navigator.maxTouchPoints` may be emitted by the pinned Camoufox/BrowserForge
runtime even though it is not part of the existing closed Artifact v3 config
contract. The Host strips only this runtime-only field and rewrites the exact
disk Artifact config before launch; no Artifact or ObservedWebsiteDigest schema
was changed.

## Pinned Windows Asset

- Release: `v152.0.4-beta.28`
- Platform: `windows-x86_64`
- GitHub asset ID: `482185262`
- Asset: `camoufox-152.0.4-beta.28-win.x86_64.zip`
- Official SHA-256: `386fc2f41139685f9a1a9cef0d024bc041d899c315ea538d561171b5b282e57d`
- Local SHA-256: same as official
- Archive size: `492370020`
- BuildID: `20260719045835`
- SourceStamp: `e39c605adc0fc049a165d7fe4a3f6517b761edf7`
- `properties.json` SHA-256: `c0573d7b47b3f4f217e459916f0feba461aba3816699727f216779a2c4988018`
- Browser tree: 493 files, 979350965 bytes
- Windows tree manifest: `tests/fixtures/camoufox/browser-tree-manifest-windows.json`

The lock is independent at
`apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-windows-x86_64.json`.
Runtime launch uses the already verified local archive and DownloadGuard;
automatic Camoufox webdl is not allowed.

## Fixtures

The Windows fixtures are `identity-win-a`, `identity-win-b`, and
`identity-win-c`. They use Artifact v3, `policy.targetOs=windows`, a Windows
browser binding, strict RFC3339-Z timestamps, raw SHA sidecars, and
`fontMode=inherit`. Profile paths, tokens, proxy secrets, display values, and
environment secrets are not recorded.

`test_identity_artifact.py` validates all three Windows fixtures independently
of browser launch. The generated-by marker identifies these files as M2-W
Artifact v3 fixtures; changing that marker requires regenerating their
canonical digests and raw SHA sidecars.

## Reproduction

From `apps/camoufox-host` on the same Windows host:

```powershell
uv sync --frozen
cargo build --release --manifest-path windows-supervisor/Cargo.toml
uv run python test_identity_artifact.py
uv run python test_windows_host.py
uv run python run_identity_spike.py stability --artifact ..\..\tests\fixtures\camoufox\identity-win-a.json --runs 5
uv run python run_identity_spike.py separation --artifacts ..\..\tests\fixtures\camoufox\identity-win-a.json,..\..\tests\fixtures\camoufox\identity-win-b.json,..\..\tests\fixtures\camoufox\identity-win-c.json
uv run python run_identity_spike.py tamper --artifact ..\..\tests\fixtures\camoufox\identity-win-a.json --out-dir ..\..\artifacts\camoufox-m2-windows-gate\tampered
```

`test_windows_host.py` includes separate junction and real volume mount-point
reparse tests. The empty-cache receipt distinguishes cache generation from
the subsequent warm replay: both paths must keep DownloadGuard untripped, and
the warm replay is the five-start stability receipt.

The Windows Gate report index records each run-id, report path, and report
SHA-256 in `tests/fixtures/camoufox/evidence-manifest-windows.json`. Raw
profiles and reports remain under gitignored `artifacts/`; the tracked
manifest records only their immutable receipt hashes. The manifest also
records the protected Linux fixture/lock/tree/evidence hashes and the exact
Windows implementation revision that produced the receipts.
