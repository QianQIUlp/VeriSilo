# VeriSilo

VeriSilo is a Windows-first, open-source browser environment isolation and privacy-auditing platform for Chrome and Edge.

It creates a new, managed browser data directory for every **Silo**. Browser-owned state—cookies, storage, cache, service workers, permissions, and history—stays in that directory. VeriSilo never imports, clones, or mutates the user's default browser profile.

Current public milestone: [0.1 Identity Isolation Core](docs/milestones/0.1-identity-isolation-core.md). This is a source baseline, not a signed binary release.

## Architecture and roadmap

```text
Desktop core                 Browser and network               Companion & Native Host
─────────────────────────    ────────────────────────────      ─────────────────────────
encrypted Vault              dedicated user-data-dir           optional page observations
Silo lifecycle               direct · fixed proxy · Mihomo    user-triggered exit checks
runtime binding              fail-closed launch paths          local redacted reports
```

The desktop app owns the Silo lifecycle and runtime binding; Chrome or Edge owns the browser files; the optional Companion adds observations and local evidence without becoming the isolation mechanism. Every Silo keeps its own browser state and network profile, and the default browser profile is never imported or modified.

### Roadmap

| Stage                                                                                                  | Status                      |
| ------------------------------------------------------------------------------------------------------ | --------------------------- |
| **0.1 source milestone** — identity isolation core works end to end                                    | complete in source, 2026-08 |
| **Next** — browser-visible fingerprint consistency across Window / iframe / Worker / headers / network | planned, not implemented    |
| **Gated** — signed Windows distribution, controlled browser engine, stronger environments              | explicit future gates       |

See [the current milestone](docs/milestones/0.1-identity-isolation-core.md) for the exact source scope and [the environment roadmap](docs/environment-roadmap.md) for the stronger layers.

## What is implemented in this repository

- Tauri v2 desktop foundation with an Argon2id-protected local Silo vault:
  a password-derived wrapping key protects a random AES-256-GCM data key.
- Vault passphrase rotation, encrypted envelope backup/restore, schema migration,
  and explicit Silo edit/archive/restore/permanent-delete lifecycle controls.
- Safe browser discovery and argument-array launching with a separate `--user-data-dir` per Silo.
- A single-active-Silo runtime model that refuses to force-kill browser processes.
- Per-Silo fail-closed HTTP/SOCKS proxy binding with one-line imports, Vault-encrypted credentials, a random loopback authentication relay, and staged runtime evidence.
- An external Mihomo/Clash-compatible adapter that reads and fixes a node through a loopback-only Controller while leaving subscriptions and the GPL core under user control.
- A Native Messaging Host with fail-closed origin and schema validation.
- A bounded Companion-to-desktop evidence bridge: user-initiated Silo exit
  observations are freshness- and active-Silo-checked, labeled
  `extension_asserted` / `observed` (not process-authenticated), then persisted
  in the encrypted Vault with explicit DNS/WebRTC/QUIC coverage limits.
- A Manifest V3 companion extension with human-readable identity summaries, browser-backed temporary InPrivate separation, local signal observation, user-triggered IP/public-DNS checks, safe page-message validation, local-only redacted report exports, and reversible optional privacy controls.
- Product boundaries, capability states, threat model, release checks, and automated TypeScript tests.
- Reproducible lockfile SBOM generation, SHA-256/provenance tooling, Native Host
  current-user installer hooks, a clearly separated unsigned candidate workflow, and a
  certificate-secrets-gated signed workflow definition whose real Windows execution remains a
  release gate. Candidate builds also emit a lockfile-to-package-metadata
  license evidence report; every component remains pending explicit human
  license review.
- Four implemented control layers with explicit runtime gates: independent Silos; V0.7 stock and signed external EngineAdapters; V0.8 WSL/Sandbox/Hyper-V providers; and a V0.9 pinned self-hosted Remote Agent control plane.

Network identity design and exact Phase 1/2/3 boundaries are documented in [network identity providers](docs/network-identity-providers.md).

## Important boundary

The stock Chrome/Edge launcher provides browser-state separation and transparent privacy controls; it does not change TLS fingerprints or real hardware. Controlled-engine, local-environment, and self-hosted remote control paths now exist in the repository, but their actual availability remains gated by signed engine artifacts, supported Windows/virtualization hosts, legal guest images, and a real remote Provider. VeriSilo does **not** claim device impersonation, fraud bypass, universal Worker/Service Worker modification, or undetectability. See [the environment roadmap](docs/environment-roadmap.md).

## Quick start

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
pnpm native-host:verify
pnpm engine:verify
pnpm release:self-test
```

The desktop app additionally needs a Rust stable toolchain and the Windows Tauri prerequisites. See [the development guide](docs/development.md).
The current requirement-by-requirement evidence and remaining gaps are tracked in
[the desktop completion audit](docs/desktop-completion-audit.md).
The latest hands-on Companion results and the next Windows/desktop integration
gate are tracked in [the extension functional acceptance record](docs/acceptance/extension-functional-acceptance-2026-07-30.md)
and [the Windows desktop integration matrix](docs/acceptance/windows-desktop-integration-matrix.md).
Use [the step-by-step Windows manual acceptance runbook](docs/acceptance/manual-windows-acceptance-runbook.md)
for the exact Chrome, Edge, desktop, Native Host, and evidence-capture operations.

## Product site

The static Astro product site lives in [`apps/site`](apps/site) and includes English and Chinese routes.

```bash
pnpm site:dev
pnpm site:check
pnpm site:build
```

The site describes the current public-development state and deliberately does not present an installer or store listing before the public release gates are complete.

See the [Cloudflare Pages deployment runbook](docs/site-deployment.md) for the production build, preview, and custom-domain settings.

## License

Source code is licensed under [MPL-2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file says otherwise.
