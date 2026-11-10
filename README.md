# VeriSilo

VeriSilo is an open-source, local-first, multi-profile fingerprint browser that is being built to commercial grade. It runs many persistent, isolated browser identities called **Silos**. Each Silo has its own browser data, a coherent and stable fingerprint, and its own network exit, and VeriSilo shows evidence of what actually ran.

- Product intent: [north star](docs/identity-platform-north-star.md)
- Why this direction: [commercial direction decision](docs/product-direction-decision-2026-11.md)
- Plan: [roadmap](docs/roadmap.md)
- Current phase and next task: [program status](docs/program-status.md)
- For coding agents: [AGENTS.md](AGENTS.md)

## Current state

The current public build is the **v0.1.0-rc5 prerelease** ([release](https://github.com/QianQIUlp/VeriSilo/releases/tag/v0.1.0-rc5), source `7d0f83a04f1d7dc33a0a3c7ec2f93a5e5da99027`). It is a prerelease: the Windows installer is not Authenticode-signed, there is no auto-update, and strict standard-user installation is not proven yet. Engineering is in roadmap Phase 1, the foundation for a signed v0.2 public beta.

| Area          | Today                                                                                           | Planned                                             |
| ------------- | ----------------------------------------------------------------------------------------------- | --------------------------------------------------- |
| Platforms     | Windows x64 (current-user NSIS); Linux x86_64 (Debian, AppImage; validated on Ubuntu 22.04 X11) | Signed builds and auto-update (v0.2); macOS (later) |
| Standard Silo | System Chrome or Edge with a separate Profile. Fingerprint follows the real machine.            | Stays as the entry layer                            |
| Managed Silo  | VeriSilo-built Camoufox (Firefox family), 18 locale presets, Artifact-bound stable identity     | Controlled Chromium as the primary engine (v1.0)    |
| Concurrency   | Up to two Managed Silos at once                                                                 | 20+ at once (v0.3)                                  |
| Network       | Direct, required HTTP/SOCKS5 proxy with no direct fallback, or a user-run Clash/Mihomo endpoint | Proxy library, bulk import and checks (v0.3)        |
| Evidence      | Declared versus observed identity (matched, mismatched, unavailable, stale), recent-run records | Correlation report, Chromium qualification          |
| Data          | Encrypted Vault, Vault backup, same-machine Managed cold backup                                 | Silo packages, cookie import/export (v0.3)          |
| Automation    | Loopback local API, CLI, Camoufox page actions                                                  | API v2, Playwright/CDP attach, MCP server (v0.4)    |
| Organization  | Name and color                                                                                  | Groups, tags, notes, search, bulk operations (v0.3) |
| Teams         | None                                                                                            | End-to-end encrypted sync and roles (v1.x)          |
| Languages     | English only (since 2026-11)                                                                    | English only                                        |

## What VeriSilo does not claim

VeriSilo does not claim to be undetectable and does not promise to bypass anti-fraud systems. A Standard Silo isolates browser state; it does not change the real device fingerprint, TLS stack or hardware. Managed identities are reported with the evidence vocabulary `configured`, `applied`, `observed`, `verified` and `unavailable`, and observed equality is never presented as verification. VeriSilo never imports or modifies the user's default browser profile. See [non-goals](docs/non-goals.md) and the [threat model](docs/threat-model.md).

## Architecture

```text
Desktop core (Tauri v2, Rust)    Browser engines                    Companion and Native Host
──────────────────────────────   ────────────────────────────────   ─────────────────────────────
encrypted Vault                  Chrome / Edge (Standard)           optional MV3 extension
Silo lifecycle and runtime       Camoufox Host + engine (Managed)   page observations, exit checks
network relay, Mihomo adapter    signed engine packages             local redacted reports
local API and CLI                first-party identity probes        Native Messaging bridge
```

More detail: [architecture](docs/architecture.md), [architecture review (2026-11)](docs/architecture-review-2026-11.md), [engine adapters](docs/engine-adapters.md), [network identity providers](docs/network-identity-providers.md), [capability model](docs/capabilities.md), [compatibility matrix](docs/compatibility-matrix.md).

## Quick start for developers

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
pnpm native-host:verify
pnpm engine:verify
pnpm release:self-test
```

The desktop app also needs Rust 1.88 or newer and the native Tauri prerequisites. See the [development guide](docs/development.md), [Linux desktop build and verification](docs/linux-desktop.md), and the [Windows manual acceptance runbook](docs/acceptance/manual-windows-acceptance-runbook.md).

## Product site

The static Astro site lives in [`apps/site`](apps/site) and is published at [verisilo.qiu.works](https://verisilo.qiu.works/) in English.

```bash
pnpm site:dev
pnpm site:check
pnpm site:build
```

See the [Cloudflare Pages deployment runbook](docs/site-deployment.md).

## License

Source code is licensed under [MPL-2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file says otherwise.
