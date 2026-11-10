# Development

## Prerequisites

- Node.js 22 or newer and pnpm 11 or newer.
- Rust 1.88 or newer, Cargo, and the native Tauri v2 prerequisites for the desktop build. See [Linux desktop builds](linux-desktop.md) for Linux dependencies and Managed engine packaging.
- Chrome and/or Edge for manual Windows validation.

The current development environment may build and test the TypeScript workspace without Rust. Desktop compilation is intentionally not faked when the Rust toolchain is unavailable.

## Commands

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
pnpm desktop:dev ui --port 1421 --preview # Mode A: real React UI + mock API + Vite HMR, no Tauri or Vault.
pnpm desktop:dev core --port 1422 # Mode B: real Tauri desktop dev; frontend HMR, Rust changes rebuild and restart the app.
```

## Development loop

Use the cheapest mode that exercises the changed behavior.

| Mode            | Command                                              | Use it for                                                                            | It cannot prove                                                    |
| --------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| A: UI Preview   | `pnpm desktop:dev ui --port 1421 --preview`          | CSS, layout, components, copy, form UX, loading/error/empty/running states            | that the Vault was written, a command is correct, or an engine ran |
| B: Desktop Dev  | `pnpm desktop:dev core --port 1422 [--vault <name>]` | Tauri commands, application logic, Vault, runtime, front-to-back behavior             | installer or production packaging behavior                         |
| C: RC / release | release workflow                                     | installer, upgrade, uninstall, production packaging; only after the owner opens an RC |                                                                    |

- Mode A opens `http://127.0.0.1:1421/preview.html?scenario=overview`. It uses real React components with an in-memory mock API and Vite HMR. It starts no Rust backend, no Vault and no browser engine. Scenarios: overview, empty list, locked, loading, first use, running, launch failure, and the managed creation form. A refresh discards the mock data. The network exit check button still makes a real public network query after explicit consent.
- Mode B runs real React → Tauri dev → the real application against a named Vault (`dev-<name>`, never the default Vault). Frontend edits hot-reload in the WebView. Rust edits trigger an incremental rebuild and an automatic app restart; no installer is built.
- Each running instance needs its own name and port. `--dry-run` prints the arguments without starting. The script does not change system environment variables, install the product, copy user Profiles, or build an engine package.
- Prefer the CLI against a real backend for Vault, Silo lifecycle, Managed, network, persistence and recovery behavior. Use Preview for what the CLI cannot express.
- A real Managed launch needs an existing, verified development engine package. Preview cannot replace it, and ordinary development does not build a package for it.

RC installer builds and acceptance follow the [release process](release.md) and the
[Windows acceptance runbook](acceptance/manual-windows-acceptance-runbook.md).

## Code map

| Work                                                | Owning code                                                                 | Minimum check                                       |
| --------------------------------------------------- | --------------------------------------------------------------------------- | --------------------------------------------------- |
| Pages, forms, interactions                          | `apps/desktop/src/features/{silos,identity,vault,network,environments,cli}` | desktop check/test; Preview of the affected pages   |
| Page assembly                                       | `apps/desktop/src/App.tsx`                                                  | desktop check/test; Preview                         |
| Cross-page session, refresh, invalidation, ordering | `apps/desktop/src/workspace/useDesktopWorkspace.ts`                         | Vault UI tests                                      |
| Shared business operations                          | `apps/desktop/src-tauri/src/application/`                                   | core harness `application::` tests                  |
| Window, tray, Tauri entry point                     | `apps/desktop/src-tauri/src/lib.rs`, `commands.rs`                          | desktop Rust check                                  |
| CLI and local HTTP API                              | `local_api.rs`, `bin/verisilo-cli.rs`                                       | desktop Rust check/test                             |
| Lifecycle, package verification, network, storage   | `launcher.rs`, `engine.rs`, `mihomo.rs`, `proxy_relay.rs`, `vault.rs`       | the owning module's tests                           |
| Camoufox Host and engine                            | `apps/camoufox-host/`                                                       | Host tests; native runtime evidence when it matters |
| User acceptance and release                         | `tests/windows/`, `docs/acceptance/`, release scripts                       | the matching acceptance on the exact candidate      |

`application` does not depend on Tauri and does not start an HTTP service. Tauri commands and the local API call the same operations. The frontend reaches the backend only through `desktop-api.ts`. A change to `desktop-api.ts`, `packages/contracts`, a Rust DTO, the Host protocol or a data format lands as one commit that updates both sides.

For a full Windows manual pass, including the exact Rust checks, browser
profiles, Native Host registration, and evidence files, follow the
[step-by-step Windows acceptance runbook](acceptance/manual-windows-acceptance-runbook.md).

## Companion extension manual installation

Build the unpacked extension first:

```bash
pnpm extension:build
pnpm extension:verify
```

Then open `chrome://extensions` or `edge://extensions`, enable Developer mode,
choose **Load unpacked**, and select the generated
`apps/extension/dist` directory. Confirm that version `0.2.5` loads without a
manifest or Service Worker error, record the development extension ID, and
open VeriSilo from the toolbar action. The action grants one-tab access and
opens the Side Panel; granting a site's longer-lived optional host permission
is a separate, reversible operation inside the panel.

Reload the unpacked extension after every rebuild. Reloading destroys the old
extension context, so an active Labs experiment must recover fail-closed and
its restore receipt must be checked before continuing acceptance.

## Vault schema compatibility

Vault payload schema 7 is current. Unlock and encrypted-backup restore support
every older encrypted payload schema from 1 through 6 and atomically rewrite a
successfully validated payload as schema 7. These are on-disk compatibility
boundaries, not claims that every schema shipped in a public Windows release.

| Payload schema | Authenticated payload introduced by that schema                                         | Unlock / restore behavior                                                                                                |
| -------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| 1              | Silo metadata, network profile, and identity seed reference/material                    | Supported; newer fields become empty or otherwise fail-closed defaults.                                                  |
| 2              | Encrypted proxy credentials referenced by the Silo network profile                      | Supported; credential references and values are preserved.                                                               |
| 3              | Encrypted external-Mihomo Controller secrets                                            | Supported; proxy and Controller credentials remain reference-only outside the decrypted Vault.                           |
| 4              | Sanitized Native Messaging network inbox observations                                   | Supported; a legacy record without a runtime binding remains an unbound observation and is never promoted to `verified`. |
| 5              | Self-hosted endpoint, pairing/replay ledger, stable binding, and operation result state | Supported; missing later receipt fields use empty/`None` defaults.                                                       |
| 6              | Authenticated remote deletion proof retained with the operation result                  | Supported; proof identities and resource bindings are revalidated.                                                       |
| 7              | Force-detach orphan receipts that explicitly do not claim remote deletion               | Current write format.                                                                                                    |

Envelope version 1 (password-derived data key) remains import-only and is
rewrapped with a random DEK in envelope version 2. Passphrase changes rotate
that DEK for future Vault state; historical backups remain independently
decryptable with their original passphrase. Envelope and payload DTOs
reject unknown fields, unsupported versions, cross-schema downgrade shapes,
missing fields required by the source schema, corrupted AEAD data, and an
incorrect passphrase. A successful migration is persisted by same-directory
atomic replacement and must reopen without another rewrite. Backups contain
the encrypted envelope only, never browser Profile contents; restore rebases
Profile paths to the destination's managed root.

The normal Rust checks are:

```bash
cargo metadata --locked --no-deps --format-version 1 \
  --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml vault --lib
```

On Linux the second command also needs the Tauri/WebKit system libraries,
including GLib. The permanent development/test-only core harness avoids that
system dependency while still compiling the real production Rust modules:

```bash
pnpm desktop-core:fmt
pnpm desktop-core:check
pnpm desktop-core:test
pnpm desktop-core:clippy
```

`crates/verisilo-desktop-core-harness` path-loads the production domain, engine,
environment/backend, launcher, Mihomo, Native Host model, HTTP/SOCKS relay, and
Vault sources and includes the real remote-backend path crate. It does not copy
their logic and has no Tauri, GTK, or WebKit dependency. Its own `Cargo.lock` is
test infrastructure only; the scripts run dependency-using commands with both
`--offline` and `--locked`, while CI first fetches that exact lock and then uses
`--locked` on Ubuntu and Windows.

A passing Linux harness is evidence for platform-independent core compilation
and tests, including loopback HTTP relay/Mihomo behavior. It is not evidence for
a Tauri build, Windows `cfg` paths, Windows packaging, a real Chrome/Edge
process, browser DNS/WebRTC/QUIC behavior, or OS isolation. Native Windows CI
runs the same harness to cover Windows compilation and tests, but still does not
replace real-browser or release-candidate acceptance.

## Native Messaging development

The production installer must write user-level host manifests that contain only the published Chrome Web Store and Edge Add-ons extension IDs. The development registration script accepts IDs explicitly and is never a release-installation mechanism.

Build and registration are intentionally separate:

```powershell
$env:VERISILO_CHROME_EXTENSION_ID = '<published Chrome ID>'
$env:VERISILO_EDGE_EXTENSION_ID = '<published Edge ID>'
node scripts/prepare-native-host-release.mjs --out artifacts/native-host
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --release --features native-host --bin verisilo-native-host
pwsh -File scripts/install-native-host.ps1 `
  -HostPath '<installed-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
pwsh -File scripts/verify-native-host-install.ps1 `
  -HostPath '<installed-dir>\verisilo-native-host.exe' `
  -ReleaseConfigPath 'artifacts\native-host\native-host-release-config.json'
```

`prepare-native-host-release.mjs` fails before writing output unless both IDs are valid. Run Cargo in the same environment so the Host embeds exactly those IDs. Do not commit the generated release configuration before the real store IDs exist.

For an unpacked extension and debug Host only, register each browser with the
development ID shown on its own extensions page:

```powershell
pwsh -File scripts/register-native-host.ps1 `
  -Browser chrome `
  -ExtensionId '<chrome://extensions development ID>' `
  -HostPath '<debug-dir>\verisilo-native-host.exe'
pwsh -File scripts/register-native-host.ps1 `
  -Browser edge `
  -ExtensionId '<edge://extensions development ID>' `
  -HostPath '<debug-dir>\verisilo-native-host.exe'
```

The desktop process must refresh the redacted Native Messaging status snapshot at least every 30 seconds while it is running and remove it during a clean shutdown. The Host rejects snapshots older than 45 seconds, snapshots with unknown fields, and expired `unlocked` Vault states.
