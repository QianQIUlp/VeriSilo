# Architecture review, 2026-11

- Status: **review record** of `main` at `1cf0f29` (2026-11-10). It feeds roadmap Phase 1 and Phase 3. It does not change any Gate.
- Method: read-only code review with line counts and targeted reads. Items marked _unverified_ were not confirmed in code.

## 1. Components

| Component             | Location                                | Size (lines, incl. tests) | Role                                                                                       |
| --------------------- | --------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------ |
| Desktop backend       | `apps/desktop/src-tauri/src`            | about 61,500              | Tauri v2 app: Vault, launcher, engines, environments, network, Native Host, CLI, local API |
| Core test harness     | `crates/verisilo-desktop-core-harness`  | small                     | Compiles production modules without Tauri for tests                                        |
| Remote control plane  | `crates/verisilo-remote-backend`        | about 14,500              | Desktop-side controller and a self-hosted Agent (Unix only)                                |
| Desktop frontend      | `apps/desktop/src`                      | about 26,300 (with CSS)   | React 19 + Vite; `features/`, `shared/`, `workspace/`, `preview/`                          |
| Shared contracts      | `packages/contracts`                    | about 7,200               | zod schemas for Silo, network, engine, environment, remote and messaging                   |
| Companion extension   | `apps/extension`                        | about 9,200               | MV3 side panel, service worker, observers, Labs                                            |
| Camoufox Host         | `apps/camoufox-host`                    | about 46,000 Python       | Host protocol, identity policy, provisioning, builds, plus research runners                |
| Scripts and workflows | `scripts`, `tests`, `.github/workflows` | large                     | Release verifiers, SBOM, signing, Windows E2E, agent task routing                          |

## 2. Backend structure

```text
lib.rs (setup, tray, command registration)
 └─ commands.rs        thin async shims → spawn_blocking
     └─ application/   use cases shared by Tauri commands and the local API
         ├─ runtime.rs, silos.rs, environments.rs, remote.rs, vault.rs, engines.rs, identity.rs, network.rs, managed_backup.rs
         └─ DesktopCore: local_control, vault, runtime, local_runtimes, environments, environment_runtime, remote_control
 ├─ launcher.rs        RuntimeManager, Camoufox Host framing, stop paths (about 9,800 lines with inline tests)
 ├─ local_runtimes.rs  the two Managed slots
 ├─ local_api.rs       loopback HTTP API and discovery file
 ├─ vault.rs (+ vault/managed_backup.rs)
 ├─ engine.rs (+ engine_linux.rs)  EngineAdapter, packages, signatures, state
 ├─ environment.rs, environment_backend.rs   WSL, Windows Sandbox, Hyper-V
 ├─ mihomo.rs, proxy_relay.rs, native_host.rs, website_identity.rs, runtime_watchdog.rs, linux.rs
 └─ bin/verisilo-cli.rs, bin/verisilo-native-host.rs
```

Largest production modules (lines before the first top-level test module): `engine.rs` 5,985; `vault.rs` 4,633; `environment_backend.rs` 3,571; `domain.rs` 2,580; `vault/managed_backup.rs` about 2,100; `mihomo.rs` 2,008; `application/runtime.rs` 1,715; `native_host.rs` 1,622; `proxy_relay.rs` 1,410. `launcher.rs` mixes tests through the file, so its production size is _unverified_.

## 3. Re-check of earlier findings

An earlier review (on a two-month-old checkout) listed eight problems. Their state on `1cf0f29`:

| Finding                                        | State            | Evidence                                                                                                                                                                                                                                                                                                           |
| ---------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| All Tauri commands run on the main thread      | **Fixed**        | 73 of 74 commands are `async` and use `spawn_blocking` (`commands.rs:34-45`). The remaining sync command only validates input. Lifecycle work is still serialized by shared mutexes (`application/mod.rs:79-96`).                                                                                                  |
| Lock poisoning and startup panics              | **Partly fixed** | Startup moved into `tauri::Builder::setup`, but `expect` remains at `lib.rs:152-153`, `application/mod.rs:102-104`, `lib.rs:283` and `local_runtimes.rs:63-66`. New `lock().unwrap()` calls in `local_runtimes.rs:80-147` panic on poison. Poisoned locks still map to permanent "unavailable" in about 80 places. |
| Only one Silo can run                          | **Partly fixed** | `LOCAL_MANAGED_SESSION_LIMIT = 2` (`domain.rs:1366`); only Camoufox with Direct or fixed HTTP/SOCKS5 qualifies (`local_runtimes.rs:38-52`); every other mode is exclusive (`application/runtime.rs:1325-1347`).                                                                                                    |
| Module cycle and duplicated platform code      | **Still true**   | `domain.rs:17` ↔ `engine.rs:22`; `mihomo.rs` and `proxy_relay.rs` import credential types from `vault.rs`; three atomic-write implementations; five or more reparse checks; hand-written SHA-256/HMAC in `engine.rs`; `ProxyScheme` at `domain.rs:359` and `environment_backend.rs:319`.                           |
| Hand-mirrored TS and Rust types, string errors | **Mostly true**  | `desktop-api.ts` casts 74 `invoke` results without runtime checks; 60 `Result<_, String>` signatures in `commands.rs`; only Mihomo errors carry codes (`user-errors.ts:51-60`).                                                                                                                                    |
| Vault KDF cost, plain passphrases, open relay  | **Still true**   | Argon2id m=19,456 KiB, t=2, p=1 (`vault.rs:55-57`); IPC passphrases are plain `String` (`commands.rs:389-437`); the loopback SOCKS5 relay accepts no-auth from any local process (`proxy_relay.rs:735-745`). The local API wraps its passphrase in `Zeroizing`.                                                    |
| Tooling gaps                                   | **Partly fixed** | React component tests now exist (`ManagedSiloForm`, `CurrentSessionIntegrity`, `SiloList`). Still missing: ESLint, Python Host unit tests in CI, `-D warnings` on CI clippy, `src-tauri` lint in CI. Four separate `Cargo.lock` files. `pnpm format:check` fails, and both release workflows run it.               |
| Dead commands                                  | **Partly fixed** | `rename_silo`, `update_silo_network` and `update_silo_engine` are still registered (`lib.rs:243-246`) with no UI use. Whether the CLI or local API use `list_silos` is _unverified_.                                                                                                                               |

## 4. Risks by scaling axis

### Many concurrent profiles (20 to 100)

1. The two-session limit is part of the Native Host snapshot protocol (`native_host.rs:1287`), not only a constant.
2. Each slot carries a full `RuntimeManager`, a mutex and a watchdog thread; the relay uses a thread per connection.
3. One Vault mutex and one `local_control` reservation held across slow launches serialize lifecycle work.
4. Every Vault change re-encrypts and rewrites the whole payload (`persist_data`, `vault.rs:2715-2744`), and the Vault caps stored Artifacts at 256 (`MAX_IDENTITY_ARTIFACTS`).
5. The UI polls full status on intervals (`workspace/useDesktopWorkspace.ts`).

Roadmap: P1.1, P3.1, P3.2, P3.3.

### A Chromium managed engine beside Camoufox

1. Camoufox is assumed across the stack: `supports_managed_concurrency` requires the Camoufox adapter, lifecycle helpers are named for Camoufox (`active_managed_camoufox_silo_id`), and the `EngineAdapter` trait has a Camoufox-specific default method (`camoufox_host_launch_plan`).
2. Reusable pieces already exist: `EngineAdapterId::ControlledChromium`, `SiloEngineConfig::ControlledChromium`, package schema v2, `NativeBootstrapV1` transport, package verifiers on Windows and Linux, engine state with rollback and emergency disable.
3. Missing: any Chromium source, patches, build, Artifact family, probes or qualification.

Roadmap: E0, P5.1 to P5.9.

### Team sync

1. The single encrypted blob has no per-record revision or device metadata.
2. The remote crate keeps its own stores, separate from the Vault.
3. Hand-mirrored types and string errors make a second client likely to drift.

Roadmap: P1.2, P1.3, P3.1, Phase 6.

### Public release with auto-update

1. No updater plugin; engine packages have their own install, update and rollback commands that must be coordinated with app updates.
2. Startup panics exit without a message.
3. Signing runs only on manual dispatch and has never produced a signed public build.
4. The desktop UI and tray are Chinese only.
5. The Camoufox engine is pinned to Firefox 152.0.4.

Roadmap: P1.1, P1.5, P2.1 to P2.5.

## 5. Quick wins (small and low risk)

These are folded into the Phase 1 work packages:

1. Replace `lock().unwrap()` in `local_runtimes.rs` with `PoisonError::into_inner` for snapshot locks (P1.1).
2. Make `DesktopCore::open` return `Result` and remove the `expect` at `lib.rs:153` (P1.1).
3. Merge the two `ProxyScheme` types; extract `fs_safety` and `credentials` modules (P1.4).
4. Wrap IPC passphrases in `Zeroizing` (P1.6).
5. Remove the unused command wrappers after checking the CLI and local API (P1.2).
6. Add `-D warnings` to CI clippy and add ESLint in report-only mode first (P1.5).
7. Run the Camoufox Host unit tests in CI (P1.5).
8. Replace the hand-written SHA-256/HMAC with `sha2` and `hmac` behind the existing test vectors (P1.4).
