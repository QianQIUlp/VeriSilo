# Product scope

VeriSilo is a local-first persistent browser identity management and privacy-auditing product. A complete Silo binds a Persistent Profile, a Resolved Identity Artifact when the selected engine supports one, an Engine Binding, a Network Policy, and Runtime Evidence. These parts have separate schemas and lifecycles: a Profile is browser-owned state, an Artifact is a replayable identity configuration, an engine applies capabilities, network policy selects the connection path, and evidence records what was configured, applied, observed, verified, failed, or unavailable.

The desktop application is the isolation core. The extension is an optional companion for browser-context observations, verification, and explanations. A new Silo works before its extension is installed.

The user-facing product has three durable Silo layers:

- **Standard Silo** uses stock Chrome or Edge and provides an independent Profile, Vault, network policy, lifecycle, and evidence without claiming hardware-fingerprint control.
- **Managed Identity Silo** adds a pinned controlled engine and a stable, coherent Resolved Identity Artifact. Camoufox is the current first engine.
- **Isolated Machine Silo** adds an operating-system or machine boundary for workloads that require stronger host isolation.

Standard remains a permanent baseline. Current engineering priority is nevertheless the missing Managed Identity execution layer: the standalone Camoufox vertical slice has accepted Linux and native Windows M2-W evidence, and [M3-0](camoufox-m3-engine-adapter-task.md) accepted its contract-level EngineAdapter/RuntimeManager connection at `e96ef3f`. The real Windows M3-WI seam remains test-only and `experimental`; its Gate is Failed after an inconclusive second-Host investigation, with no production fix or signed release package. Work therefore proceeds through FP1 deterministic Artifact projection, FP2 cross-realm consistency, FP3 network/region coordination, and FP4 site compatibility before a clean M3-WI is frozen again. Local virtual and self-hosted remote environments remain later, optional upgrades rather than substitutes for Profile or fingerprint semantics. See the [identity platform north star](identity-platform-north-star.md), [Camoufox engine decision](camoufox-managed-engine-decision.md), [current Camoufox status](camoufox-program-status.md), and [environment roadmap](environment-roadmap.md).

The product must state facts and evidence, not anonymous-looking scores. A control is only presented as protected after it is both applied and verified.

Every Silo can bind a user-owned fixed proxy or a user-run external Mihomo/Clash-compatible endpoint. Managed Identity Silos use the same network choices: Direct, a local Clash/Mihomo mixed port with optional controller group/node lock, or a required HTTP/SOCKS5 proxy. VeriSilo encrypts optional credentials and local Controller secrets, refuses direct fallback in “required proxy” mode, and leaves subscription ownership with the user. It does not sell exits or assign “IP purity” scores. See [network identity providers](network-identity-providers.md).

Managed Identity creation shows the website-visible identity (UA, language, timezone, screen, CPU cores, WebGL, and proxy-exit geography when bound) and lets the user adjust those fields before the first successful launch. After that launch the identity stays locked; a new fingerprint is a new Silo.

The current public project destination is the GitHub repository. `https://verisilo.qiu.works/` is the planned product-domain candidate; it must not replace the working fallback until DNS, HTTPS, and a real landing page are available.
