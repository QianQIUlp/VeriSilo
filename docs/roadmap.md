# VeriSilo roadmap to a commercial fingerprint browser

- Status: **Accepted plan**, 2026-11-10. It implements the [commercial direction decision](product-direction-decision-2026-11.md) under the [north star](identity-platform-north-star.md).
- This is a plan, not evidence. A work package is done only when its acceptance evidence exists. The current phase, the current work package and the results so far live on the [program status page](program-status.md).
- Code facts behind Phase 1 and Phase 3 are in the [architecture review](architecture-review-2026-11.md).

## 1. How to use this roadmap

Work proceeds one phase at a time, normally one phase per agent session. Inside a phase, each work package (WP) becomes one task started with `node scripts/agent-task.mjs start`. Work packages in different lanes may run in parallel.

### Phase kickoff prompt

Paste this into a new session to start or continue a phase. Replace `N`:

```text
Read AGENTS.md and docs/program-status.md. We are working on roadmap Phase N.
1. Read the Phase N section of docs/roadmap.md, then the owning code of its work packages.
2. Pick the next work package from the status page (or propose an order if none is set).
   Freeze its task contract with docs/agent-operating-model.md: goal, success criteria,
   allowed scope, forbidden scope, environment, acceptance matrix, stop conditions, deliverables.
3. Start the task with: node scripts/agent-task.mjs start --lane <lane> --task "<WP id> <title>" --name <slug>
4. Implement, run verify and check, commit, publish, then promote or hand to integration.
5. Update docs/program-status.md: work package state, evidence, next work package.
Ask me before: opening an RC or release gate, spending money (certificates, build machines,
hosting), changing the north star or an accepted decision, or any destructive or irreversible action.
```

### Definition of done for every work package

1. The acceptance criteria in its entry hold, with evidence at the stated verification level.
2. Tests cover the new behavior. Existing tests still pass at the lane's `verify` level.
3. No north-star invariant is weakened: separate lifecycles, fail-closed networking, the evidence vocabulary, acceptable-use limits.
4. User-visible text exists in English and Chinese once P2.1 has landed.
5. Affected documentation is updated in English, and the status page records the result.

### Verification levels

Use the lowest level that honestly proves the change ([AGENTS.md](../AGENTS.md) explains the cost rules):

| Level   | Meaning                                                                                                  |
| ------- | -------------------------------------------------------------------------------------------------------- |
| `unit`  | Unit or contract tests in the owning crate or package.                                                   |
| `cli`   | The real backend driven through the VeriSilo CLI or local API.                                           |
| `ui`    | UI Preview with mock data (`node scripts/dev-desktop.mjs <name> --preview`). Never runtime evidence.     |
| `dev`   | A Tauri dev instance against a real Vault (`node scripts/dev-desktop.mjs core ...`).                     |
| `win`   | A native Windows run with recorded evidence. Linux, WSL or Wine never substitutes.                       |
| `linux` | A native Linux run with recorded evidence.                                                               |
| `rc`    | A frozen release candidate installed in a clean environment. Only with the owner's explicit RC approval. |

Sizes: **S** is up to a week of focused work, **M** two to four weeks, **L** more than a month.

## 2. Starting point (2026-11-10, `main` at `1cf0f29`)

- Public prerelease `v0.1.0-rc5`. Windows NSIS (current user, outer Authenticode unsigned). Native Linux x86_64 Debian and AppImage, validated on Ubuntu 22.04 under X11.
- Standard Silos on system Chrome and Edge. Managed Silos on VeriSilo-built Camoufox 152.0.4 (Firefox) with 18 locale presets, 10 GPU presets and 22 timezones, Artifact schema v5 (v6 for network-bound identities).
- At most two concurrent Managed Silos (Camoufox with Direct or a fixed HTTP/SOCKS5 proxy). Standard, Clash-bound and other modes are exclusive.
- Local API on loopback with a bearer token, a CLI, and Camoufox page actions. No Playwright or CDP attach.
- Encrypted Vault, Vault backup, same-machine Managed cold backup. No cookie import/export, no cross-machine migration.
- No groups, tags, bulk UI operations, proxy library, auto-update, English desktop UI, team features or macOS build.

## 3. Target: what "commercial" means in releases

| Release  | Content                               | Must be true                                                                                                               |
| -------- | ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| **v0.2** | Phase 1 + Phase 2: signed public beta | Signed installers, auto-update with rollback, English and Chinese UI, proven standard-user install on Windows 10 and 11.   |
| **v0.3** | Phase 3: workstation at scale         | 1,000 stored Silos, 20 running at once, groups, tags, bulk operations, proxy library, Silo packages, cookie import/export. |
| **v0.4** | Phase 4: automation                   | Local API v2, scoped tokens, Playwright attach for Camoufox, MCP server.                                                   |
| **v1.0** | Phase 5: Controlled Chromium          | A qualified, signed, auto-updating Chromium engine as the default managed engine on Windows and Linux.                     |
| **v1.x** | Phase 6: teams and sync               | End-to-end encrypted sync, roles and sharing, paid hosted service.                                                         |
| **v2.x** | Phase 7: platforms and environments   | macOS, isolated machines, remote environments.                                                                             |

## 4. Gap against the category

| Capability customers expect                  | VeriSilo now                                            | Closed by                       |
| -------------------------------------------- | ------------------------------------------------------- | ------------------------------- |
| Chromium fingerprint engine                  | absent (Firefox-family Camoufox only)                   | E0, Phase 5                     |
| Many profiles running at once                | 2 Managed; everything else exclusive                    | P3.2                            |
| Groups, tags, notes, search, bulk operations | name and color only; CLI `create-batch`                 | P3.4, P3.5                      |
| Proxy library, bulk import, checks           | one proxy per Silo; launch preflight                    | P3.6                            |
| Cookie import/export, profile transfer       | absent; same-machine cold backup only                   | P3.7                            |
| Automation API, Playwright/Puppeteer         | local API v1, Camoufox page actions                     | Phase 4, P5.8                   |
| Per-profile extensions                       | user installs inside each Silo by hand                  | P5.7                            |
| Team sharing, roles, sync                    | absent                                                  | Phase 6                         |
| Signed builds, auto-update                   | unsigned outer installer; no updater                    | P2.2, P2.3                      |
| English UI                                   | desktop UI and CLI are Chinese                          | P2.1                            |
| macOS                                        | absent                                                  | P7.1                            |
| Coherence and leak evidence                  | **ahead of the category** (reconciliation, fail-closed) | keep and extend: P3.9, CP chain |

## 5. Phase 1: engineering foundation

**Goal.** Make scale, a second engine and public releases safe to build. No user-visible capability claim changes.

**Exit criteria.** All work packages accepted. CI runs and passes the new jobs. A startup failure shows a message instead of a panic. Errors cross the Tauri boundary as typed codes. The release workflows' `pnpm format:check` passes.

### P1.1 Startup and lock-poison resilience

- Lane `core`. Size S. Verification `unit`, `dev`, `win` (dialog only).
- Problem: startup panics on recoverable conditions (`DesktopCore::open` calls `expect` at `apps/desktop/src-tauri/src/application/mod.rs:102-104`; `app_data_root().expect` at `lib.rs:152`; `.run(..).expect` at `lib.rs:283`; `LocalRuntimeSlot::new` at `local_runtimes.rs:63-66`). `snapshot.lock().unwrap()` in `local_runtimes.rs:80-147` turns a poisoned lock into a panic. About eighty sites map a poisoned lock to a permanent "unavailable" state.
- Scope: make `DesktopCore::open` return `Result`; show a native error dialog with the data path and log location, then exit non-zero; one lock helper that recovers cache-type locks with `PoisonError::into_inner` and fails state locks with a typed `state_poisoned` error that tells the user to restart.
- Out of scope: changing lock order or concurrency (P3.2).
- Acceptance: tests that panic while holding each lock class show that status reads keep working and state commands fail with `state_poisoned`; starting with an unwritable data directory shows the dialog and exits without a panic on Windows and Linux.

### P1.2 Generated cross-boundary types

- Lane `integration` (shared contract). Size M. Verification `unit`.
- Problem: about 20 interfaces in `apps/desktop/src/desktop-api.ts` mirror Rust types by hand, and `invoke<T>()` only asserts types at compile time.
- Scope: generate TypeScript types from Rust DTOs (`specta` or `ts-rs`, chosen in the task contract); validate high-risk responses at runtime (Silo, runtime status, evidence) with schemas in `packages/contracts`; add a CI check that generated files are current; remove dead commands (`rename_silo`, `update_silo_network`, `update_silo_engine` and their wrappers) after confirming the CLI and local API do not use them.
- Acceptance: no hand-written duplicate DTO remains in `desktop-api.ts`; changing a Rust DTO without regenerating fails CI.

### P1.3 Typed error contract

- Lane `integration`. Size M. Depends on P1.2. Verification `unit`, `ui`.
- Problem: 60 command signatures in `commands.rs` and about 100 in `application/*` return `Result<_, String>`. The UI parses text; only Mihomo errors have codes.
- Scope: one `ErrorCode` enum with `{ code, message, detail?, retryable }` for Tauri commands, the local API and the CLI; Vault, launch, network, engine and backup errors first; the UI maps codes to text (this prepares P2.1).
- Acceptance: every command returns a typed error; `apps/desktop/src/user-errors.ts` matches on codes, not text; local API errors carry the same codes.

### P1.4 Platform modules and dependency hygiene

- Lane `core`. Size M. Verification `unit`, CI on Windows and Linux.
- Problem: three atomic-write implementations (`vault.rs`, `engine.rs`, `vault/managed_backup.rs`), at least five copies of reparse-point checks, a hand-written SHA-256 and HMAC in `engine.rs`, `ProxyScheme` defined twice (`domain.rs`, `environment_backend.rs`), a `domain` to `engine` type cycle, and `mihomo`/`proxy_relay` depending on `vault` for credential types.
- Scope: `fs_safety` module (atomic replace, directory sync, link and reparse checks); `sha2` and `hmac` crates behind the existing test vectors; `credentials` module; one `ProxyScheme`; move shared engine types so the cycle disappears.
- Acceptance: one implementation of each; all existing tests pass on both platforms; package verification digests are unchanged.

### P1.5 Cargo workspace and CI completeness

- Lane `integration`. Size M. Verification CI.
- Problem: four Cargo lockfiles and no workspace. CI does not lint `apps/desktop/src-tauri`, runs harness clippy without `-D warnings`, has no ESLint, does not run the Camoufox Host Python unit tests, and the release workflows run `pnpm format:check`, which fails on `main`.
- Scope: a Cargo workspace for the desktop, harness and remote crates with one lockfile (keep the Windows supervisor separate if its build needs it); update SBOM, license and release scripts that read lockfile paths; CI jobs for `src-tauri` clippy `-D warnings` on Linux and Windows, Host unit tests, `node --test scripts/agent-task.test.mjs`, ESLint (`typescript-eslint`, `react-hooks`) and `prettier --check`; add hash-bound evidence (`tests/fixtures/camoufox/**`, `tests/fingerprint-probe/**`, frozen evidence JSON) to `.prettierignore` instead of reformatting it.
- Acceptance: all jobs visible and green on a pull request; SBOM and license outputs unchanged apart from paths.

### P1.6 Vault and local-attack-surface hardening

- Lane `core`. Size M. Verification `unit`, `win`.
- Scope:
  - Raise Argon2id cost (currently m=19456 KiB, t=2, p=1 at `vault.rs:55-57`) to a calibrated target of about 0.5 s on a reference machine, with an envelope version bump and transparent rewrap on unlock.
  - Wrap passphrases in `Zeroizing` from the IPC and HTTP boundary inward.
  - Bind the no-auth loopback SOCKS5 relay (`proxy_relay.rs:735-745`) to the owning browser process tree by checking the peer process on accept (Windows TCP table, Linux socket inode). Chromium cannot authenticate to SOCKS5, so authentication is not an option.
  - Set an owner-only ACL on the data directory on Windows and `0700` on Linux.
- Acceptance: a process outside the Silo's browser tree cannot use a running relay; old Vaults unlock and rewrap; the threat model is updated.

### P1.7 Frontend workspace split

- Lane `ui`. Size M. Verification `unit`, `ui`.
- Problem: `apps/desktop/src/workspace/useDesktopWorkspace.ts` (about 1,500 lines) holds most state and two polling loops; `CreateSiloPanel.tsx`, `SiloList.tsx` and `ManagedSiloForm.tsx` are large.
- Scope: split into domain hooks (vault, silos, runtime, network, engines); component tests for create, edit, start, stop and recovery flows; remove source-string assertions where a component test covers the same behavior.
- Out of scope: event-driven status (P3.3), new features.

### E0 Controlled Chromium evaluation (parallel engine track)

- Lane `host` (research). Size M, time-boxed to about three weeks. Verification: written evidence and build logs.
- Scope:
  1. Inventory candidate bases (see D2): license and redistribution terms, patch quality, surface coverage, upstream cadence over the last six Chrome majors.
  2. Build the leading candidate for Windows x64 and Linux x64 on rented or owned build capacity; record time, resources and output size.
  3. Run a first-party probe against the build and list which coherence surfaces it controls, how deterministic the seed derivation is, and what VeriSilo would need to patch.
  4. Draft the Chromium Artifact family (fields only, no implementation).
  5. Write a decision addendum: chosen base, build infrastructure, rebase cadence and cost, or a no-go with reasons.
- Stop conditions: no candidate has an acceptable license; build cost exceeds the budget the owner approved.
- Ask the owner before renting build machines.

## 6. Phase 2: commercial single-user release (v0.2)

**Goal.** A stranger can download a signed build, install it as a standard user, use it in English or Chinese, and receive updates.

**Exit criteria.** Signed v0.2 installers for Windows and Linux; update and rollback rehearsed; Windows 10 and 11 client matrix passed with standard users; legal documents published; Companion listed in the Chrome and Edge stores.

### P2.1 Internationalization (English and Chinese)

- Lanes `ui` then `core`. Size L. Depends on P1.3. Verification `unit`, `ui`, `dev`.
- Scope: a typed message catalog with `en` and `zh-CN`; a language setting (default from the OS); migrate all desktop strings, the tray menu, Rust user-facing messages (through error codes), CLI and local API messages; a pseudo-locale check that fails on untranslated strings; Host reason strings (`apps/camoufox-host/identity_policy.py`, `host_v1.py`) become codes with catalog text.
- Acceptance: switching language changes every visible string without a restart; tests reference catalog keys, not literal Chinese.

### P2.2 Code signing

- Lane `integration`. Size M. Verification `rc`.
- Scope: obtain an Authenticode certificate or a managed signing service (owner approval and cost); run `windows-signed-release.yml` for real; sign the desktop executable, CLI, Native Host, installer and uninstaller; signed checksums and GPG signatures for Linux packages; a SmartScreen reputation plan.
- Acceptance: the release verifier checks every signature; a clean Windows 11 machine installs without an "unknown publisher" warning.

### P2.3 Auto-update and engine update channel

- Lanes `core` and `integration`. Size M. Depends on P2.2. Verification `rc`.
- Scope: the Tauri updater with a dedicated update-signing key separate from Authenticode; `stable` and `beta` channels; staged rollout; engine packages delivered through a separate signed channel that reuses the existing install, update, rollback and emergency-disable commands, so an app update does not re-download a 500 MB engine.
- Acceptance: upgrade from the previous version keeps Vault, Silos and Profiles; an interrupted or failed update leaves the old version working; engine rollback works; a revoked engine is disabled.

### P2.4 Install lifecycle proof

- Lanes `qa` and `integration`. Size M. Verification `rc`, `win`, `linux`.
- Scope: Windows 10 22H2 and Windows 11 (23H2, 24H2) clients with non-administrator users: install, first run, Standard Chrome and Edge Silos, Managed Silo, upgrade, uninstall with data kept, reinstall. Linux: Ubuntu 22.04 and 24.04, Debian 12, one RPM-based distribution if a package is added, X11 and Wayland.
- Acceptance: closes the `NOT_PROVEN` strict standard-user boundary for the tested editions; every result is recorded per environment.

### P2.5 Camoufox engine currency

- Lane `host`. Size M. Verification `win`, `linux`.
- Problem: the engine is pinned to Camoufox `v152.0.4-beta.28`. Firefox moves every four weeks, so a public release on an old engine ships known security fixes late and presents an aging version.
- Scope: rebase the VeriSilo patch series (0000 to 0007) onto the current upstream Camoufox and Firefox release; rebuild; re-sign; re-run only the qualification stages whose inputs changed; document the rebase procedure so it repeats every release cycle.
- Acceptance: new engine package passes the affected FP stages and the Managed lifecycle on Windows and Linux.

### P2.6 First-run experience and UX polish

- Lane `ui`. Size M. Verification `ui`, `dev`.
- Scope: an onboarding flow (create Vault, choose engine with a plain explanation of Standard versus Managed, create the first Silo); Silo templates; empty, loading and error states; keyboard shortcuts; basic accessibility (focus order, labels, contrast).

### P2.7 Diagnostics and support

- Lanes `core` and `ui`. Size S to M. Verification `unit`, `dev`.
- Scope: structured logs with rotation; an in-app log viewer; a redacted diagnostic bundle the user exports on purpose (no secrets, cookies or Vault content, verified by tests); local crash capture with an opt-in upload.

### P2.8 Companion store release

- Lanes `qa` and `integration`. Size S. Verification `rc`.
- Scope: publish to the Chrome Web Store and Edge Add-ons using the prepared `docs/store-*` material; put the production extension IDs into the Native Host manifest.

### P2.9 Legal, site and documentation

- Lane `integration`. Size M. Needs a qualified reviewer.
- Scope: terms of service, the acceptable-use policy from D7, a privacy policy, a completed third-party license review and notices; site download page for signed builds; a user guide in English and Chinese.

## 7. Phase 3: profile management at scale (v0.3)

**Goal.** Hundreds of Silos stay manageable and tens run at once.

**Exit criteria.** 1,000 stored Silos; 20 concurrent sessions in any engine mix during an 8-hour soak on a 32 GB Windows 11 machine with no leaked processes and responsive UI; all P3 features available in UI, CLI and local API.

### P3.1 Vault storage v2

- Lane `core`. Size L. Verification `unit`, `cli`, `win`.
- Problem: the Vault is one encrypted blob rewritten on every change (`persist_data` in `vault.rs`), with a 256-Artifact cap (`MAX_IDENTITY_ARTIFACTS`) and a 64 MiB backup cap.
- Scope: per-record encrypted storage (for example SQLite rows or per-Silo envelopes under the existing data key); per-record revision, update time and device ID (needed by Phase 6 sync); crash-safe migration from every supported schema with an automatic backup; streaming backup format v2.
- Acceptance: migration tests from every schema; with 1,000 Silos a single update writes in under 50 ms on the reference machine; one corrupted record does not block the others.

### P3.2 Concurrent runtime registry

- Lane `core`. Size L. Depends on P1.1. Verification `unit`, `cli`, `win`, `linux`.
- Problem: a single legacy runtime plus a two-slot Managed set (`LOCAL_MANAGED_SESSION_LIMIT = 2` in `domain.rs`); a global `local_control` reservation serializes lifecycle work; the Native Host snapshot rejects more than two sessions (`native_host.rs`).
- Scope: one registry keyed by Silo for every engine and network mode; per-Silo reservation, relay, watchdog and recovery record; a configurable limit with a default derived from RAM; Native Host snapshot v2 for N sessions; start and stop of different Silos run in parallel.
- Acceptance: 20 concurrent mixed sessions; independent start and stop; one crash does not affect the others; recovery after a desktop restart; evidence never crosses Silos.

### P3.3 Event-driven status

- Lane `integration`. Size M. Verification `unit`, `dev`.
- Scope: the core emits runtime and evidence events; the UI subscribes instead of polling (`useDesktopWorkspace.ts` polling loops); incremental status updates.
- Acceptance: idle CPU stays flat with 20 running Silos; UI updates within 250 ms of a state change.

### P3.4 Organization

- Lanes `integration` then `ui`. Size M. Verification `unit`, `ui`.
- Scope: groups, tags, notes, custom fields and favorites as organization metadata that never touches the Artifact; search, filter and sort; a virtualized list for 1,000 rows; column configuration.

### P3.5 Bulk operations

- Lane `integration`. Size M. Depends on P3.2 and P3.4. Verification `unit`, `cli`, `ui`.
- Scope: multi-select; bulk create from a template (count, naming pattern, preset mix, proxies from the library); bulk start and stop through a queue that respects the concurrency limit; bulk move, tag, archive and delete with confirmation; the same operations in the CLI and local API.

### P3.6 Proxy library

- Lane `integration`. Size M. Verification `unit`, `cli`, `ui`.
- Scope: reusable proxy entries stored in the Vault; bulk import (`host:port:user:pass`, URLs, CSV); checks for reachability, authentication, exit IP, geography and latency through disclosed endpoints; an assignment view; a warning when several Silos share one exit; provider templates for sticky-session usernames as user-entered data.
- Rule: results use the existing evidence words (`reachable`, `observed`) and never `verified`.

### P3.7 Silo packages and cookie import/export

- Lane `integration`. Size L. Verification `unit`, `cli`, `win`, `linux`.
- Scope: a written package format (password-derived key, chunked AES-256-GCM, per-part hashes, separate Profile, Artifact, Engine Binding, Network Policy and evidence parts); export and import of stopped Silos with "import as new" or "replace" choices; secrets excluded unless chosen; cookie import and export in Netscape and JSON formats for Standard and Managed Silos; imported Managed Silos start with `stale` evidence until re-observed.
- Acceptance: a Silo exported on one Windows machine and imported on another keeps its logins; tampered packages are rejected; the threat model covers packages.

### P3.8 Window management

- Lanes `core` and `ui`. Size M. Verification `win`, `linux`.
- Scope: a per-Silo window badge or title prefix in the Silo color; arrange and tile; bring to front; minimize all. Windows first, then X11; Wayland only where the compositor allows it.

### P3.9 Coherence and correlation assistant

- Lanes `core` and `ui`. Size M. Verification `unit`, `ui`.
- Scope: warnings when a Silo's language, timezone or geography does not fit its proxy exit (hints only for Standard Silos); a correlation report that lists Silos sharing an exit IP, an Artifact digest, or an unusual combination of screen and GPU. The report states facts and never shows a score.

### P3.10 Scale soak and performance budget

- Lane `qa`. Size S. Depends on P3.1 to P3.5. Verification `win`, `linux`.
- Scope: the 8-hour, 20-session soak from the exit criteria; memory and CPU per session; UI interaction latency; recorded evidence.

## 8. Phase 4: automation and integrations (v0.4)

**Goal.** Developers and AI assistants can drive Silos from code, safely and with attribution.

**Exit criteria.** Local API v2 published with an OpenAPI description; Playwright attach works for Camoufox Silos from Python and Node; the MCP server works with scoped tokens; the threat model covers every surface.

### P4.1 Local API v2

- Lane `integration`. Size L. Verification `unit`, `cli`.
- Scope: versioned `/v2` with a generated OpenAPI 3.1 document; full CRUD for Silos, organization, proxies and bulk operations; scoped, expiring, revocable tokens managed in the UI; an audit log in the Vault; rate limits; `Host` and `Origin` header checks so a web page cannot reach the API through DNS rebinding; discovery-file permissions checked on Windows.
- Acceptance: contract tests generated from the OpenAPI document; negative tests for missing, expired and wrong-scope tokens, non-loopback peers, and browser-origin requests.

### P4.2 Playwright attach for Camoufox Silos

- Lanes `core` and `host`. Size M. Verification `cli`, `win`, `linux`.
- Scope: an authenticated per-Silo endpoint that lets Playwright connect to a running Camoufox Silo without changing its identity; detach leaves the Silo running; examples in Python and Node.
- Acceptance: the first-party probe shows the same observed identity with and without an attached client.

### P4.3 CLI v2 and SDKs

- Lane `integration`. Size M. Verification `unit`, `cli`.
- Scope: an English CLI with JSON output for every v2 endpoint; thin Python and Node SDKs generated from the OpenAPI document.

### P4.4 MCP server

- Lane `integration`. Size M. Verification `unit`, `cli`.
- Scope: MCP tools over the v2 scopes (list, create, start, stop, page actions, evidence); destructive tools require a scope the user grants explicitly; documentation for common assistants.

### P4.5 Automation attribution

- Lane `core`. Size S. Verification `unit`, `ui`.
- Scope: every automation session is recorded with token, scope, start and end time in the Silo's run record and shown in the UI.

## 9. Phase 5: Controlled Chromium engine (v1.0)

**Goal.** A qualified Chromium Managed engine is the default for new Managed Silos on Windows and Linux.

**Entry.** The E0 decision addendum is accepted. Camoufox is in maintenance mode for the duration.

**Exit criteria.** CP1 to CP4 passed on native Windows and native Linux; signed engine packages with monthly rebases; CDP attach and per-Silo extensions; Chromium is the default engine in the create flow.

### P5.1 Build infrastructure

- Lane `host`. Size M. Verification build logs.
- Scope: a dedicated or rented builder; pinned toolchains; a source lock (Chromium tag, base patches, VeriSilo patches with per-file seam hashes, toolchain digests); a build-result lock; a dispatchable CI job; a build cache.
- Acceptance: two builds from the same lock produce the same recorded inputs and a reproducible package tree.

### P5.2 Patch set

- Lane `host`. Size L. Verification `unit`, probe runs.
- Scope: apply the chosen base; add VeriSilo patches for Artifact-bound deterministic identity, secure delivery of the identity configuration (never on a visible command line), disabled update and telemetry paths, and branding.

### P5.3 Chromium Artifact family v1

- Lanes `host` and `integration` (shared contract). Size L. Verification `unit`.
- Scope: schema `verisilo-chromium-resolved-identity/v1` covering the user agent and full client hints, navigator fields (including `deviceMemory`), screen and device pixel ratio, WebGL, WebGL2 and WebGPU adapter data, Canvas and audio policy, managed fonts, media devices, voices, timezone, locale and geography, WebRTC policy and `Accept-Language`; a generator from real-device distributions; strict validation and digest rules modeled on the Camoufox family.

### P5.4 Adapter and runtime integration

- Lane `core`. Size L. Depends on P3.2. Verification `unit`, `dev`, `win`, `linux`.
- Scope: implement the `ControlledChromium` adapter behind `EngineAdapter` (`engine.rs`); a family-neutral engine package schema; process ownership with Job Objects on Windows and process groups on Linux; Profile lock; relay integration; first-party probe reconciliation into Runtime Evidence.

### P5.5 Qualification chain CP1 to CP4

- Lanes `qa` and `host`. Size L. Verification `win`, `linux`.
- Scope: CP1 deterministic Artifact projection; CP2 consistency across window, iframe, dedicated worker, service worker and request headers; CP3 network coherence (exit, timezone, locale, geography, WebRTC ICE, DNS path); CP4 a frozen ordinary-site compatibility matrix that now includes login flows with test accounts. Record the observed TLS and HTTP/2 shape against stock Chrome of the same version.
- Rule: results stay `observed` unless a capability-specific verification contract exists.

### P5.6 Packaging, signing and rebase cadence

- Lane `integration`. Size M. Verification `rc`.
- Scope: signed engine packages through the P2.3 engine channel; emergency disable; a documented monthly rebase that follows Chrome stable within one major version.

### P5.7 Per-Silo extensions

- Lanes `core` and `ui`. Size M. Verification `dev`, `win`.
- Scope: install extensions per Silo from a store package or an unpacked folder (a VeriSilo-built Chromium can load extensions that branded Chrome 137 and later refuse); an inventory per Silo; updates; Camoufox extensions through policies.

### P5.8 CDP attach

- Lane `core`. Size M. Depends on P4.1. Verification `cli`, `win`, `linux`.
- Scope: Playwright and Puppeteer attach over an authenticated CDP proxy that VeriSilo owns (the browser uses a pipe, never a public debugging port); the observed identity does not change when a client attaches.

### P5.9 Make Chromium the default

- Lanes `ui` and `integration`. Size S. Depends on P5.5. Verification `ui`, `rc`.
- Scope: Chromium becomes the default engine for new Managed Silos; the identity editor shows Chromium fields; existing Camoufox Silos are unchanged.

## 10. Phase 6: teams, sync and commerce (v1.x)

**Goal.** Teams share Silos safely across devices; VeriSilo has a paid hosted service.

**Entry.** Phase 3 storage v2 is complete. D6 legal and payment checks are done.

### P6.1 Sync protocol and key design

- Lane `integration`. Size L. Verification design review, `unit`.
- Scope: per-record sync on Vault storage v2; key hierarchy (user, team, per-Silo); device enrollment and revocation; single-holder lease; conflict rules; chunked, encrypted Profile sync for stopped Silos that skips caches; a threat model.

### P6.2 Self-hostable sync server

- Lane `integration` (new crate). Size L. Verification `unit`, `linux`.
- Scope: an open-source server (storage, lease service, invitation flow) with container deployment; it stores only ciphertext.

### P6.3 Team management and permissions

- Lanes `integration` and `ui`. Size M.
- Scope: members, roles, invitations, per-Silo permissions (launch only, no export, hide proxy credentials). The UI must state that hiding credentials from someone who runs the Silo on their own machine is a convenience, not a security boundary.

### P6.4 Team audit log

- Lane `integration`. Size S.
- Scope: who launched, exported, shared or changed which Silo and when, synced and tamper-evident.

### P6.5 Commerce

- Lane `integration`. Size M. Needs owner decisions on pricing.
- Scope: accounts for the hosted service only (the local app keeps working without an account); entitlements; billing through the approved merchant of record; a pricing page.

### P6.6 Hosted operation

- Lane `integration`. Size M.
- Scope: deployment, backups, monitoring, incident response, data deletion, and data-processing terms.

## 11. Phase 7: platforms and environments (v2.x)

- **P7.1 macOS** (Apple Silicon and Intel): Tauri build, notarization, both engines, keychain-backed Vault option, macOS identity coherence (fonts, rendering).
- **P7.2 Isolated Machine Silo on Hyper-V:** the V0.8 control plane exists; it needs a lawful guest image, a reviewed guest agent and native Windows acceptance.
- **P7.3 Remote environments:** the V0.9 control plane exists; it needs a real Provider and screen transport.
- **P7.4 WSL browser environments:** decide to keep, finish or remove.

These stay frozen until Phase 5 exits, per the sequencing rule.

## 12. Parallel maintenance tracks

- **Camoufox maintenance.** Rebase with each Firefox release cycle after P2.5; fix regressions; keep evidence current. During Phase 5 this is the only Camoufox work.
- **Security.** Dependency updates (Vite, Vitest, zod, TypeScript, Rust crates) in small batches; OSV scanning already runs in release workflows; respond to advisories within one release cycle.
- **Documentation.** Every change keeps agent-facing documents in English.

## 13. Risks

| Risk                                    | Effect                         | Mitigation                                                                 |
| --------------------------------------- | ------------------------------ | -------------------------------------------------------------------------- |
| Chromium build and rebase cost          | Phase 5 slips; stale engine    | E0 measures cost first; rented builders; a monthly rebase procedure        |
| Native Windows validation bottleneck    | Gates cannot close             | Dedicated Windows 10 and 11 test machines or a self-hosted runner          |
| Payment, signing or store refusal       | No paid offering or no listing | D7 limits; honest copy; early provider checks in P2.9 and P6.5             |
| Attack surface from automation and sync | Session theft from customers   | Scoped tokens, loopback only, audit logs, threat model before each release |
| Single maintainer                       | Work stalls                    | One phase at a time; lanes in parallel only where they do not share seams  |
| Upstream engine abandonment             | Security exposure              | Two engine families; documented rebase; emergency disable                  |

## 14. Not on this roadmap

- "Undetectable" claims, anti-fraud bypass promises, CAPTCHA solving, human-like input for bot evasion, payment or KYC fraud tooling, automated mass account registration.
- A VeriSilo-operated proxy marketplace or "IP purity" scores.
- Default telemetry.
- Per-launch fingerprint randomization.
- Mobile device emulation and cloud phones.
- More than two controlled engine families.
