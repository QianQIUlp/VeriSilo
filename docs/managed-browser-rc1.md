# VeriSilo Managed Browser v0.1.0 RC1

This RC is a self-contained Windows x64 build for local evaluation. It includes
the fixed VeriSilo Formal-v3 Camoufox engine package; the target computer does
not need Git, Python, uv, Node, Rust, or a separately installed Camoufox.

The release directory is `artifacts/release/managed-browser/v0.1.0-rc1` and
the installer is exactly
`VeriSilo-Managed-Browser-v0.1.0-rc1-x64-setup.exe`.

The RC installer and `verisilo.exe` are intentionally not Authenticode-signed
for this local RC; `authenticode-status.json` records that unsigned outer
boundary. The bundled engine package has a separate mandatory detached CMS
signature and is rejected before launch if its signer, manifest, Host, or
browser tree does not match the pin embedded in the Desktop build.

The release folder also contains the signed `engine-package/**` tree,
`SHA256SUMS`, `provenance.json`, `authenticode-status.json`, pending
`windows-acceptance-report.json`/`.md`, SBOM/license evidence, and this user
guide as `README.txt`. It does not contain Hyper-V/VHDX, environment scripts,
extension files, Native Host binaries, or installer hooks.

The generated Windows acceptance report is `Pending` with no runtime result;
that status changes only when the clean Windows 11 runbook supplies evidence.

## Install and start

1. Run `VeriSilo-Managed-Browser-v0.1.0-rc1-x64-setup.exe` as the current user.
2. Open VeriSilo and create or unlock the local Vault.
3. Choose **Create Silo** → **Managed identity browser**.
4. Enter a name and color, then choose one identity preset.
5. Choose **Direct** or a required HTTP/SOCKS5 fixed proxy. Proxy credentials,
   when supplied, are stored only in the encrypted Vault.
6. Create the Silo and select **Start**. VeriSilo verifies the complete engine
   package before opening Camoufox.

Only one browser Silo can run at a time in v0.1. Close or stop the active Silo
before starting another one.

## Close and reopen

Close the Camoufox window normally or use **Stop Silo** in VeriSilo. Wait until
the Silo reports stopped before exiting the Desktop application. Starting the
same Silo later reuses its private Profile and exact saved Identity Artifact,
so ordinary site state remains available. A second Silo uses a different
Profile and identity seed.

Closing the VeriSilo window keeps it available in the Windows notification
area. To end the Desktop process, choose **退出 VeriSilo** from the tray menu.
If a Managed Silo is active, VeriSilo first confirms the Host and browser tree
closed cleanly; a failed or timed-out cleanup cancels exit and reopens the main
window with the retained failure state.

Required proxy mode is fail-closed. If its upstream proxy cannot be reached or
the observed route does not match the saved network binding, browser launch
fails instead of switching to the direct connection.

## Status wording

- **Configured** means a setting is saved and valid.
- **Reachable** means the configured endpoint answered the bounded preflight.
- **Applied** means that exact setting was supplied to the running Host/browser.
- **Observed** means the running browser produced matching evidence.
- **Verified** is reserved for a cryptographically or directly verified result.
- **Unavailable** means the current build cannot honestly establish the result.

Package verification does not imply that every browser identity signal or
website is verified. This RC does not claim universal compatibility,
undetectability, CAPTCHA/payment bypass, or control of every DNS/TLS/QUIC path.

## Reinstall and uninstall

Installing the same RC again repairs the application while preserving the
Vault and Silo data under `%LOCALAPPDATA%\io.verisilo.app`. Uninstall removes
the application but does not silently delete that data. Permanently delete a
Silo from the application's archived-Silo view before uninstalling if that is
the intended outcome.

Keep the release directory's `engine-package/**`, `SHA256SUMS`,
`provenance.json`, SBOM, license evidence, and Windows acceptance reports with
the installer.
