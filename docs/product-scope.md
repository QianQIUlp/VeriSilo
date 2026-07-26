# Product scope

VeriSilo is a local browser-environment isolation and privacy-auditing product. A Silo is a managed Chrome or Edge user-data directory plus an explicit network profile, stable seed, capability state, and optional companion-extension connection.

The desktop application is the isolation core. The extension is an optional companion for browser-context observations, verification, and explanations. A new Silo works before its extension is installed.

The product has four committed environment layers: the current independent Chrome/Edge Silo, a V0.7 controlled-engine layer, V0.8 local virtual environments, and V0.9 self-hosted remote environments. These layers are optional upgrades, not extension claims. Their implementation gates and evidence requirements are defined in [the environment roadmap](environment-roadmap.md).

The product must state facts and evidence, not anonymous-looking scores. A control is only presented as protected after it is both applied and verified.

Every Silo can bind a user-owned fixed proxy or a user-run external Mihomo/Clash-compatible endpoint. VeriSilo encrypts optional credentials and local Controller secrets, refuses direct fallback in “required proxy” mode, and leaves subscription ownership with the user. It does not sell exits or assign “IP purity” scores. See [network identity providers](network-identity-providers.md).

The current public project destination is the GitHub repository. `https://verisilo.qiu.works/` is the planned product-domain candidate; it must not replace the working fallback until DNS, HTTPS, and a real landing page are available.
