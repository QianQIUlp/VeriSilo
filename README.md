# VeriSilo

VeriSilo is a Windows-first, open-source browser environment isolation and privacy-auditing platform for Chrome and Edge.

It creates a new, managed browser data directory for every **Silo**. Browser-owned state—cookies, storage, cache, service workers, permissions, and history—stays in that directory. VeriSilo never imports, clones, or mutates the user's default browser profile.

## What is implemented in this repository

- Tauri v2 desktop foundation with an Argon2id-protected local Silo vault:
  a password-derived wrapping key protects a random AES-256-GCM data key.
- Safe browser discovery and argument-array launching with a separate `--user-data-dir` per Silo.
- A single-active-Silo runtime model that refuses to force-kill browser processes.
- A Native Messaging Host with fail-closed origin and schema validation.
- A Manifest V3 companion extension with local signal observation, safe page-message validation, a beginner-facing side panel, local-only redacted report exports, and explicit optional privacy controls.
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

## License

Source code is licensed under [MPL-2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file says otherwise.
