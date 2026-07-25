# Release gates

## Before a public build

- [ ] Verify `pnpm check`, `pnpm test`, `pnpm build`, and `pnpm extension:verify`.
- [ ] Build the Rust desktop and Native Host using a pinned stable Rust toolchain on Windows 10 and Windows 11 x64.
- [ ] Run the local session fixture in two Silos: log into A, close it, open B, and confirm B has no A cookie or LocalStorage; reopen A and confirm its state persists.
- [ ] Confirm the user's default Chrome and Edge profiles were neither selected nor modified.
- [ ] Confirm a running Silo is not force-killed, and a stale/active `SingletonLock` causes a safe refusal.
- [ ] Verify production Native Host manifests use only published Chrome/Edge IDs and user-level registry keys.
- [ ] Review every extension permission and store disclosure; no unused host or network permission may ship.
- [ ] Produce SBOM, dependency license report, checksums, reproducible-build notes, and signed Windows installer artifacts.

## Proxy wording

The UI may say `configured` after argument validation and `preflight passed` after a required fixed-proxy TCP check. It may only say `exit verified` after a user-triggered test against a disclosed endpoint. It must never claim that DNS, TLS, HTTP/2, HTTP/3, QUIC, or all future fallback paths are controlled.
