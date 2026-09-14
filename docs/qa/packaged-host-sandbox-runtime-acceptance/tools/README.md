# Windows Sandbox Acceptance Driver Tools

This directory contains the canonical, version-controlled PowerShell acceptance driver and fixtures used for Packaged-Host Windows Sandbox runtime acceptance.

## Directory Contents

- `bootstrap.ps1`: Logon script executed inside Windows Sandbox upon container startup. Waits for mapped volume availability and launches `sandbox-acceptance.ps1`.
- `sandbox-acceptance.ps1`: Standalone PowerShell acceptance orchestrator implementing:
  - Binary length-prefixed stdio protocol for `--provision-artifact`.
  - Stdio JSONL protocol over Win32 Named Pipe redirection for Host sessions.
  - 4-session acceptance matrix (1280x800, cold restart, 1024x768, legacy clamping & immutability).
  - WebGL2 vendor/renderer and acceptEncoding signal verification.
  - 6 window geometry invariant checks.
  - Page sentinel token and URL retention across reobservation.
  - Residual process inspection and cleanup.
  - `-SelfTest` parameter for headless framing and stdio self-check on the host without Windows Sandbox.
- `package-provenance.json`: Authoritative metadata and SHA-256 digests of the input Dev Engine Package.
- `legacy/`: Pre-generated legacy artifact fixture with deliberately out-of-bounds coordinates (`screenX: 2232, screenY: 140`) to verify runtime clamping and on-disk file immutability:
  - `identity-f49f1202d9ac4d79f991f794.json`
  - `identity-f49f1202d9ac4d79f991f794.json.sha256`
  - `legacy-meta.json`

## Verification & Self-Test

To run the local driver framing and stdio self-check without launching Windows Sandbox:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -Command "& 'docs/qa/packaged-host-sandbox-runtime-acceptance/tools/sandbox-acceptance.ps1' -PackageRoot '<path-to-engine-package>' -SelfTest"
```

To run the complete acceptance suite in Windows Sandbox:

```powershell
pwsh -NoProfile -ExecutionPolicy Bypass -File "scripts/run-sandbox-acceptance.ps1"
```
