# Product scope

VeriSilo is a commercial-grade, multi-profile fingerprint browser. It runs many persistent, isolated browser identities (Silos), each with its own browser state, a coherent and stable fingerprint, and its own network exit, and it shows evidence of what actually ran. The intent and invariants are in the [north star](identity-platform-north-star.md); the reasons for the current direction are in the [commercial direction decision](product-direction-decision-2026-11.md); the plan is the [roadmap](roadmap.md); the current state is on the [program status page](program-status.md).

## Silo model

A complete Silo binds a Persistent Profile, a Resolved Identity Artifact when the engine supports one, an Engine Binding, a Network Policy and Runtime Evidence. These parts have separate schemas and lifecycles: a Profile is browser-owned state, an Artifact is a replayable identity configuration, an engine applies capabilities, the network policy selects the connection path, and evidence records what was configured, applied, observed, verified, failed or unavailable. Organization metadata (name, color, and later groups, tags and notes) and ownership never change the identity.

The desktop application is the isolation core. The browser extension is an optional companion for in-browser observations and explanations. A new Silo works before the extension is installed.

## Product layers

- **Standard Silo** uses the system Chrome or Edge. It provides an independent Profile, Vault-held secrets, a network policy, lifecycle control and evidence, and it never claims fingerprint control.
- **Managed Identity Silo** uses a pinned, signed, VeriSilo-built engine and a stable, coherent Resolved Identity Artifact. Camoufox (Firefox family) ships today. Controlled Chromium is the planned primary commercial engine (roadmap E0 and Phase 5).
- **Isolated Machine Silo** adds an operating-system or machine boundary. It is a later layer (roadmap Phase 7).

## What ships today

As of `v0.1.0-rc5` (a public prerelease) and `main` at `1cf0f29`:

- Windows (current-user NSIS, outer Authenticode unsigned) and native Linux x86_64 (Debian and AppImage).
- Standard Silos on Chrome and Edge; Managed Silos on Camoufox 152 with 18 locale presets, GPU presets, timezone choice and optional network-bound timezone, locale and geography.
- Managed identity creation shows the website-visible identity and lets the user adjust it before the first successful launch. After that launch the identity is locked; a new fingerprint means a new Silo.
- Up to two concurrent Managed Silos; other runtime modes are exclusive.
- Encrypted Vault with passphrase rotation, backup and restore; same-machine Managed cold backup.
- Direct, required fixed HTTP/SOCKS5 proxy, or a user-run Clash/Mihomo-compatible endpoint per Silo, with no direct fallback in required-proxy mode. VeriSilo encrypts credentials and controller secrets, leaves subscriptions with the user, sells no exits and assigns no "IP purity" scores. See [network identity providers](network-identity-providers.md).
- Runtime identity reconciliation (matched, mismatched, unavailable, stale), fresh re-observation, and one encrypted recent-run record per Silo.
- A loopback local API, a CLI, and page actions for Camoufox Silos.
- Strict standard-user install semantics, broad Windows edition coverage and outer code signing are not proven yet.

## What the product must never claim

The product states facts and evidence, not anonymity scores. A control is shown as protected only after it is applied and verified. VeriSilo never claims undetectability or anti-fraud bypass, never describes field rewriting as real hardware replacement, and never presents Standard Silo Profile isolation as fingerprint control.

## Public destinations

The source of truth is the GitHub repository. The product site is [verisilo.qiu.works](https://verisilo.qiu.works/).
