# Store release runbook: Native Host dual store IDs

- Status: **P5/P6 operations runbook**
- Prerequisite: Chrome Web Store and Microsoft Edge Add-ons each have a Draft item, and both store extension IDs have been obtained
- All related capabilities are implemented: `scripts/prepare-native-host-release.mjs`, `scripts/install-native-host.ps1`, `scripts/verify-native-host-install.ps1`, and the compile-time/runtime checks in `apps/desktop/src-tauri/build.rs` and `native_host.rs`. This runbook covers only the order of steps.

## 0. Why this cannot be skipped

The two stores assign **different** extension IDs to the same ZIP, and the Native Host `allowed_origins` cannot use wildcards. The Chrome side accepts only `chrome-extension://<Chrome ID>/`, and the Edge side accepts only `chrome-extension://<Edge ID>/`. Before release, both IDs must therefore be: written into the release config, compiled into the Host, and written into the two HKCU-registered manifests.

## 1. Get the IDs (P5, account holder)

- Chrome: Developer Dashboard → Add new item → upload `VeriSilo-Companion-0.2.11-chrome-edge.zip` (for its SHA-256 see [`store-release-evidence.md`](store-release-evidence.md)) → save the Draft → note the Item ID.
- Edge: Partner Center → Microsoft Edge → create an Extension → upload the same ZIP → save the Draft → note the extension ID.
- Note: this plan uses the **key that the CWS generates automatically**. The unpacked ID from local sideloading therefore differs from the store ID, and a local positive handshake with the store ID cannot be done before submission. The positive handshake is scheduled for after the store version can be installed (unlisted / trusted testers / review passed but not published).

## 2. Generate the release config (P6)

```powershell
$env:VERISILO_CHROME_EXTENSION_ID = '<Chrome Store Item ID>'
$env:VERISILO_EDGE_EXTENSION_ID = '<Edge Add-ons ID>'
node scripts/prepare-native-host-release.mjs --out artifacts/native-host
```

The script rejects empty values, values that do not match `[a-p]{32}`, placeholder IDs, and IDs with too little character diversity. The generated `artifacts/native-host/native-host-release-config.json` holds public identifiers, not secrets. But **do not commit the generated output before the real store IDs are obtained** (see [`development.md`](development.md)).

## 3. Build the Host in the same environment

```powershell
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --features native-host --bin verisilo-native-host
```

`build.rs` rebuilds when the env changes. `native_host.rs` validates the compiled-in IDs again at runtime, and refuses to authorize any production extension when they are missing or malformed.

## 4. Install and verify

Put `verisilo-native-host.exe` and `verisilo.exe` in the same directory (the Host may only wake the `verisilo.exe` in its own directory), then run as a **standard user**:

```powershell
pwsh -File scripts/install-native-host.ps1 `
  -HostPath '<install-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
pwsh -File scripts/verify-native-host-install.ps1 `
  -HostPath '<install-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
```

`install-native-host.ps1` writes `HKCU:\Software\Google\Chrome\NativeMessagingHosts\io.verisilo.host` and `HKCU:\Software\Microsoft\Edge\NativeMessagingHosts\io.verisilo.host`. Each points to a manifest that contains only that store's ID. On failure the transaction is rolled back. The verification script checks the manifest fields, the exact origin match, the registry targets and the consistency of the install-record, and it rejects reparse points.

## 5. Negative tests (can be done locally, no store ID needed)

Check against items 331–335 of [`release.md`](release.md):

1. The production Host rejects a wrong or missing ID, an unauthorized origin, unknown fields, secret-shaped fields, a protocol mismatch and messages larger than 16 KiB;
2. `open_desktop` starts only the `verisilo.exe` in the same directory, with no arguments. An expired or malformed snapshot is never returned as the current state;
3. Run `uninstall-native-host.ps1` twice in a row: both browser registrations and manifests are gone, while the Vault, the reports and the Silo Profile directories are kept.

## 6. Positive handshake test (needs a store-side installable version)

Suggested order: Chrome first goes through trusted testers or Deferred Publishing (a passed review does not publish automatically), and Edge uses the visibility option of the draft. After installing the store version once in each browser, verify:

1. The in-extension state changes from `local_only` to the bound state (desktop app running, Vault unlocked, Silo matched);
2. The Network Check result enters the desktop Vault history through `submit_network_evidence`;
3. Without the desktop app, or when locked, it degrades gracefully to `local_only` and does not report "the feature is broken".

## 7. Closure

- Add the two store IDs and the verification results to [`store-release-evidence.md`](store-release-evidence.md) (IDs are public identifiers and may go into the repository).
- Check against the store listings that each of the two IDs really belongs to its own listing (syntactically valid ≠ ownership, see [`release.md`](release.md)).
- Only after that is Submit for Review allowed (for Chrome, Deferred Publishing is recommended).
