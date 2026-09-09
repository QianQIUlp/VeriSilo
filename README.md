# VeriSilo

VeriSilo is a local-first, open-source platform for managing persistent browser identities, network policy, and privacy evidence. Its current Windows desktop baseline uses stock Chrome and Edge with an independent browser data directory for each Silo.

It creates a new, managed browser data directory for every **Silo**. Browser-owned state—cookies, storage, cache, service workers, permissions, and history—stays in that directory. VeriSilo never imports, clones, or mutates the user's default browser profile.

Current product state: **v0.1.0-rc2 is a current-source locally accepted installed candidate in a pristine Windows Sandbox**. It is not tagged, publicly released, or shipped; strict non-admin install/reinstall/uninstall semantics remain unproven. The [0.1 Identity Isolation Core milestone](docs/milestones/0.1-identity-isolation-core.md) is a historical source checkpoint, not the current product stage or a signed binary release.

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

| Stage                                                                                                  | Status |
| ------------------------------------------------------------------------------------------------------ | ------ |
| **Standard Silo Windows Profile Isolation**                                                            | closed |
| **Camoufox M0–M2-W, M3-0, FP1–FP4, and clean M3-WI Attempt 4**                                        | closed at their documented evidence layers |
| **Managed Engine production adapter, Formal-v3 package/signing, Managed Silo UX, and current-user NSIS** | implemented; release checks closed |
| **Current product phase**                                                                               | rc2 current-source installed candidate accepted in an exact Windows Sandbox; next direction is the accepted differentiated roadmap |
| **Current source-bound RC**                                                                              | v0.1.0-rc2, source `c1688d5a392ffa69ae77c246bcb4bb78b083e26f`; locally accepted, not public |

See [the historical 0.1 source milestone](docs/milestones/0.1-identity-isolation-core.md) for that checkpoint and [the environment roadmap](docs/environment-roadmap.md) for the stronger layers.

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
  certificate-secrets-gated signed workflow definition. Candidate builds also emit a
  lockfile-to-package-metadata license evidence report; every component remains pending
  explicit human license review.
- Four control-plane layers represented in code, each with independent runtime and release gates: independent Silos; V0.7 stock plus the production `ExternalPackageEngineAdapter` and signed Formal-v3 package path; V0.8 WSL/Sandbox/Hyper-V providers; and a V0.9 pinned self-hosted Remote Agent control plane.

Network identity design and exact Phase 1/2/3 boundaries are documented in [network identity providers](docs/network-identity-providers.md).

## Important boundary

The stock Chrome/Edge launcher provides browser-state separation and transparent privacy controls; it does not change TLS fingerprints or real hardware. The controlled-engine path is implemented with a pinned Formal-v3 package and production adapter, and the current-source rc2 installed lifecycle passed in a pristine Windows Sandbox. Strict unelevated standard-user semantics, broader Windows compatibility, and outer Authenticode signing remain separate boundaries; the internal engine CMS signature is not outer Authenticode. Local-environment and self-hosted remote paths remain gated by their own artifacts, hosts, guest images, and Providers. VeriSilo does **not** claim device impersonation, fraud bypass, universal Worker/Service Worker modification, or undetectability. See [the environment roadmap](docs/environment-roadmap.md).

## Identity platform direction

The durable product model, current engine choice, Agent workflow, and changing delivery state are recorded separately:

- [Identity platform north star](docs/identity-platform-north-star.md) defines Standard, Managed, and Isolated Silos and separates Profile, Fingerprint, and Environment concerns.
- [Camoufox-first Managed Engine decision](docs/camoufox-managed-engine-decision.md) records the stable reason for using a pinned Camoufox/BrowserForge execution layer before broader engine work.
- [Agent operating model](docs/agent-operating-model.md) defines how architecture, execution, evidence, and stage Gates are delegated and reviewed.
- [Camoufox program status](docs/camoufox-program-status.md) is the only mutable checkpoint page for this workstream.

The current source includes the accepted standalone Camoufox Host and v3 Identity Artifact lineage, the M3-0 contract, FP1–FP4 qualification, clean M3-WI Attempt 4 evidence, a production `ExternalPackageEngineAdapter`, an internally CMS-signed Formal-v3 package path with a public signer pin, Managed Silo product flow, current-user NSIS packaging, and the rc2 current-source installed acceptance record. That record covers the exact pristine Windows Sandbox environment and does not prove universal Windows, strict non-admin, public-release, site-compatibility, or undetectability claims. Current routing, rc2 provenance, the accepted differentiated product direction, and the remaining unverified boundaries live in the [Camoufox program status](docs/camoufox-program-status.md) and [Simprint source due diligence](docs/simprint-source-due-diligence-2026-09-09.md).

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
The historical requirement-by-requirement audit is retained in
[the desktop completion audit](docs/desktop-completion-audit.md). Current
product facts, release-readiness status, and the next task are tracked in
[the Camoufox program status](docs/camoufox-program-status.md). The hands-on
acceptance records remain evidence for their named runs and environments.
Use [the step-by-step Windows manual acceptance runbook](docs/acceptance/manual-windows-acceptance-runbook.md)
for the exact Chrome, Edge, desktop, Native Host, and evidence-capture operations.

## Product site

The static Astro product site lives in [`apps/site`](apps/site) and is live at [verisilo.qiu.works](https://verisilo.qiu.works/) with English and Chinese routes.

```bash
pnpm site:dev
pnpm site:check
pnpm site:build
```

The site describes the current public-development state and deliberately does not present an installer or store listing before the public release gates are complete.

See the [Cloudflare Pages deployment runbook](docs/site-deployment.md) for the production build, preview, and custom-domain settings.

## License

Source code is licensed under [MPL-2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file says otherwise.
