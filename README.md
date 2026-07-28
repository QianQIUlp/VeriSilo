# VeriSilo

VeriSilo is a Windows-first, open-source browser environment isolation and privacy-auditing platform for Chrome and Edge.

It creates a new, managed browser data directory for every **Silo**. Browser-owned state—cookies, storage, cache, service workers, permissions, and history—stays in that directory. VeriSilo never imports, clones, or mutates the user's default browser profile.

## What is implemented in this repository

- Tauri v2 desktop foundation with an Argon2id-protected local Silo vault:
  a password-derived wrapping key protects a random AES-256-GCM data key.
- Safe browser discovery and argument-array launching with a separate `--user-data-dir` per Silo.
- A single-active-Silo runtime model that refuses to force-kill browser processes.
- A Native Messaging Host with fail-closed origin and schema validation.
- A Manifest V3 companion extension with human-readable identity summaries, browser-backed temporary InPrivate separation, local signal observation, user-triggered IP/public-DNS checks, safe page-message validation, local-only redacted report exports, and reversible optional privacy controls.
- Product boundaries, capability states, threat model, release checks, and automated TypeScript tests.

## Important boundary

VeriSilo provides browser-state separation and transparent privacy controls. It does **not** claim to provide device impersonation, fraud bypass, TLS/QUIC modification, hardware isolation, or universal Worker/Service Worker fingerprint modification.

## Quick start

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
```

The desktop app additionally needs a Rust stable toolchain and the Windows Tauri prerequisites. See [the development guide](docs/development.md).

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
