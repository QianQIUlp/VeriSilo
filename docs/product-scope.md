# Product scope

VeriSilo is a local-first persistent browser identity management and privacy-auditing product. A complete Silo binds a Persistent Profile, a Resolved Identity Artifact when the selected engine supports one, an Engine Binding, a Network Policy, and Runtime Evidence. These parts have separate schemas and lifecycles: a Profile is browser-owned state, an Artifact is a replayable identity configuration, an engine applies capabilities, network policy selects the connection path, and evidence records what was configured, applied, observed, verified, failed, or unavailable.

The desktop application is the isolation core. The extension is an optional companion for browser-context observations, verification, and explanations. A new Silo works before its extension is installed.

The user-facing product has three durable Silo layers:

- **Standard Silo** uses stock Chrome or Edge and provides an independent Profile, Vault, network policy, lifecycle, and evidence without claiming hardware-fingerprint control.
- **Managed Identity Silo** adds a pinned controlled engine and a stable, coherent Resolved Identity Artifact. Camoufox is the current first engine.
- **Isolated Machine Silo** adds an operating-system or machine boundary for workloads that require stronger host isolation.

Standard remains a permanent baseline. Current engineering priority is nevertheless the missing Managed Identity execution layer: a standalone Camoufox vertical slice has accepted Linux evidence and now awaits its native Windows Gate before it can be connected to the existing desktop control plane. Local virtual and self-hosted remote environments remain later, optional upgrades rather than substitutes for Profile or fingerprint semantics. See the [identity platform north star](identity-platform-north-star.md), [Camoufox engine decision](camoufox-managed-engine-decision.md), and [environment roadmap](environment-roadmap.md).

The product must state facts and evidence, not anonymous-looking scores. A control is only presented as protected after it is both applied and verified.

Every Silo can bind a user-owned fixed proxy or a user-run external Mihomo/Clash-compatible endpoint. VeriSilo encrypts optional credentials and local Controller secrets, refuses direct fallback in “required proxy” mode, and leaves subscription ownership with the user. It does not sell exits or assign “IP purity” scores. See [network identity providers](network-identity-providers.md).

The current public project destination is the GitHub repository. `https://verisilo.qiu.works/` is the planned product-domain candidate; it must not replace the working fallback until DNS, HTTPS, and a real landing page are available.
