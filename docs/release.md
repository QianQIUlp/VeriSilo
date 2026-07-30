# Release gates

## Before a public build

- [ ] Verify `pnpm check`, `pnpm test`, `pnpm build`, and `pnpm extension:verify`.
- [ ] Build the Rust desktop and Native Host using a pinned stable Rust toolchain on Windows 10 and Windows 11 x64.
- [ ] Run the local session fixture in two Silos: log into A, close it, open B, and confirm B has no A cookie or LocalStorage; reopen A and confirm its state persists.
- [ ] Confirm the user's default Chrome and Edge profiles were neither selected nor modified.
- [ ] Confirm a running Silo is not force-killed, and a stale/active `SingletonLock` causes a safe refusal.
- [ ] Verify fixed HTTP and SOCKS5 imports with and without credentials; confirm secrets do not appear in browser arguments or the plaintext Vault envelope.
- [ ] Stop the required proxy/Mihomo process during a Silo session and confirm public navigation fails without a host-network fallback.
- [ ] Verify external Mihomo rejects remote Controllers, unknown nodes, failed authentication, and selection readback mismatches before browser launch.
- [ ] Run the IP/public-DNS action inside the launched Silo and confirm it is labeled separately from the desktop controller check.
- [ ] Verify production Native Host manifests use only published Chrome/Edge IDs and user-level registry keys.
- [ ] Review every extension permission and store disclosure; no unused host or network permission may ship.
- [ ] Produce SBOM, dependency license report, checksums, reproducible-build notes, and signed Windows installer artifacts.

## Proxy wording

The UI may say `configured` after model validation, `endpoint reachable` after a connection/protocol check, `authentication verified` only after a protocol-level response (SOCKS5) or real proxied request (HTTP), and `browser routing applied` after process creation with the intended arguments. It may only say `exit verified` after a user-triggered test against a disclosed endpoint from the environment being described. Public DoH answer comparison must not be called DNS leak detection. The current stock-browser launcher must never claim that TLS, HTTP/2, HTTP/3, all DNS paths, or all WebRTC paths are controlled. A future engine, VM, or remote backend may only change that wording for an individual capability after it records direct runtime evidence.
