# VeriSilo

VeriSilo is a Windows-first, open-source browser environment isolation and privacy-auditing platform for Chrome and Edge.

It creates a new, managed browser data directory for every **Silo**. Browser-owned state—cookies, storage, cache, service workers, permissions, and history—stays in that directory. VeriSilo never imports, clones, or mutates the user's default browser profile.

## What is implemented in this repository

- Tauri v2 desktop foundation with an Argon2id-protected local Silo vault:
  a password-derived wrapping key protects a random AES-256-GCM data key.
- Safe browser discovery and argument-array launching with a separate `--user-data-dir` per Silo.
- A single-active-Silo runtime model that refuses to force-kill browser processes.
- Per-Silo fail-closed HTTP/SOCKS proxy binding with one-line imports, Vault-encrypted credentials, a random loopback authentication relay, and staged runtime evidence.
- An external Mihomo/Clash-compatible adapter that reads and fixes a node through a loopback-only Controller while leaving subscriptions and the GPL core under user control.
- A Native Messaging Host with fail-closed origin and schema validation.
- A Manifest V3 companion extension with human-readable identity summaries, browser-backed temporary InPrivate separation, local signal observation, user-triggered IP/public-DNS checks, safe page-message validation, local-only redacted report exports, and reversible optional privacy controls.
- Product boundaries, capability states, threat model, release checks, and automated TypeScript tests.
- A committed four-layer environment roadmap: current independent Silos, a V0.7 controlled browser engine, V0.8 local virtual environments, and V0.9 self-hosted remote environments.

Network identity design and exact Phase 1/2/3 boundaries are documented in [network identity providers](docs/network-identity-providers.md).

## Important boundary

The current Chrome/Edge launcher provides browser-state separation and transparent privacy controls; it does not yet change TLS fingerprints or real hardware. Stronger controlled-engine, VM, and remote backends are scheduled, but VeriSilo does **not** claim device impersonation, fraud bypass, universal Worker/Service Worker modification, or undetectability. See [the environment roadmap](docs/environment-roadmap.md).

## Quick start

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
```

The desktop app additionally needs a Rust stable toolchain and the Windows Tauri prerequisites. See [the development guide](docs/development.md).

## License

Source code is licensed under [MPL-2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file says otherwise.
