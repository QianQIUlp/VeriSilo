# VeriSilo environment implementation roadmap: browser engines, virtual machines and remote environments

> The product-wide plan is the [roadmap](roadmap.md). This document keeps the technical design of the engine and environment layers (V0.7 to V0.9). Its V0.8 and V0.9 environment work is frozen until roadmap Phase 7.

## Goal

VeriSilo does not treat a browser extension as the whole product. It also does not treat a separate `user-data-dir` as the end point of environment isolation. The product formally adopts four implementation layers:

| Layer                          | Version   | Status                                                                                                                                                                                                                                                                        | Main boundary                                                                                    |
| ------------------------------ | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Separate Silo                  | V0.1–V0.6 | Baseline partially implemented                                                                                                                                                                                                                                                | Separate browser Profile, site data, permissions, history and launch-level network configuration |
| Controlled browser engine      | V0.7      | M0–M2-W, M3-0, FP1–FP4 and clean M3-WI Attempt 4 are closed at their respective evidence layers. The production package, adapter, UI and NSIS installer are implemented. The current-source rc2 has completed post-install lifecycle acceptance in a pristine Windows Sandbox | Coordinates browser-visible signals, engine network capabilities and cross-context consistency   |
| Local virtual environment      | V0.8      | On the roadmap                                                                                                                                                                                                                                                                | Separate operating system, fonts, device view and per-environment network exit                   |
| Self-hosted remote environment | V0.9      | On the roadmap                                                                                                                                                                                                                                                                | Remote browser session, separate network stack, persistent environment and lifecycle audit       |

These layers are optional capabilities. Lightweight users can use only the extension or a separate Silo. Users who need a stronger boundary can upgrade one level at a time. The UI must always distinguish “当前可用” ("available now"), “已列入路线” ("on the roadmap"), “已配置” ("configured"), “已应用” ("applied") and “已验证” ("verified").

"Baseline partially implemented" describes only the code that exists today. It does not mean V0.1–V0.6 have been accepted as a whole. For item-by-item evidence and gaps, the [desktop completion audit](desktop-completion-audit.md) governs.

## V0.1–V0.6 frozen definition

Writing the six versions together as "separate Silo implemented" would hide gaps. For the first release, acceptance follows these boundaries:

| Version | Frozen delivery scope                                                                                                      | Direct evidence required to upgrade to complete                                                                                                                                 |
| ------- | -------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| V0.1    | Vault foundation, Chrome/Edge discovery, separate `user-data-dir`, Silo lifecycle, directory lock and single-active launch | Vault and state-machine unit tests, plus Win10/11 × Chrome/Edge E2E for A/B site-data isolation, unchanged default Profile and abnormal recovery                                |
| V0.2    | Optional Companion, production Native Host, fixed proxy/PAC/external Mihomo and fail-closed network path                   | Positive and negative protocol tests, HKCU install tests, degradation when the extension is missing, and disconnect and network-exit evidence collected from inside a real Silo |
| V0.3    | Modular observation, plain-language explanation, desktop evidence history, redacted JSON/HTML report                       | Collector failure isolation, coverage statement, desktop return/storage/migration tests and export confirmation tests                                                           |
| V0.4    | Stable per-Silo experiment control, `observe → apply → verify → restore`, per-site fallback                                | Short-lived derived configuration, cross-context consistency, failure rollback, restart recovery and compatibility matrix                                                       |
| V0.5    | High-risk experiments such as Cookie/Worker in `VeriSilo Labs`, with leak stop conditions                                  | Separate feature gate, off by default, item-by-item leak and compatibility tests. An experiment that has not met its stop conditions must not enter the default mode            |
| V0.6    | Capped capability report, Windows install/upgrade/uninstall, SBOM/license/checksum/signature and store release gates       | Traceable Windows artifact, E2E logs, supply-chain report, hash and signature verification. Roadmap documents or sample reports do not count as complete                        |

### V0.5 narrow implementation that can be checked directly today

V0.5 is no longer only roadmap text. Contracts, the Companion and the desktop UI share a versioned `LabsExperiment`/receipt model. Experiments are off by default and require explicit authorization for the current Silo plus site. They implement the `observe → apply → verify → restore`, expiry and stop-on-leak state machine. When no desktop Silo is available, only a bounded `local_temporary` run can be created. It cannot pose as Silo evidence.

The only selectable implementation today is a MAIN-world wrapper around the constructor for newly created Dedicated Workers. It handles only same-origin and blob classic Workers created after it is enabled. It uses short-lived random canaries for a Worker handshake and a same-origin iframe consistency self-check. Any of the following restores the original constructor, disables the site, and writes a local redacted receipt that contains no canary, Cookie or token: an observable leak across tabs, iframes, Service Worker URLs or page-visible Cookie canaries; a page or Worker exception; a timeout; a permission change; navigation; a scope violation; or a verification failure.

This entry point is injected after the user clicks, so it cannot be proven to run before page scripts. The success state can therefore only be `best_effort`, never `verified`. Existing Workers, module and cross-origin Workers, SharedWorkers and Service Worker memory are explicitly not covered. Cookie store virtualization and comprehensive Set-Cookie interception stay unselectable as `unsupported` in stock MV3. The current reliable alternative is still the separate `user-data-dir` of a desktop Silo. This closes the gaps in the feature gate, stop conditions and a narrow real implementation. It does not mean the full Windows and browser compatibility acceptance for V0.5 is complete.

Later layers cannot replace these base acceptance requirements. For example, the existence of a V0.8 VM backend does not automatically prove that the V0.1 default-Profile safety of stock Chrome/Edge holds.

## What to take from existing products and projects

The sources below are used to learn product layering, update mechanisms, configuration consistency and environment lifecycle. Their marketing claims do not automatically become evidence of VeriSilo capability.

| Source                                                                                                                    | Idea worth borrowing                                                                                           | VeriSilo constraint                                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [GoLogin Orbita](https://gologin.com/docs/orbita-browser)                                                                 | Treat a modified Chromium as a browser product with continuous updates, not as a one-off script injection      | Every field still needs cross-context verification. Do not repeat marketing conclusions such as "undetectable"                                         |
| [GoLogin Cloud Browser](https://gologin.com/docs/api-reference/cloud-browser/what-is-gologin-cloud-browser)               | Lifecycle model for persistent remote Profiles, remote sessions and automation connections                     | V0.9 targets user-hosted nodes by default and does not build a default VeriSilo public cloud                                                           |
| [Multilogin Mimic / Stealthfox](https://multilogin.com/help/cn_CN/how-to-use-mimic-and-stealthfox)                        | Multi-engine adaptation and maintenance tiers for primary and legacy engines                                   | Each engine publishes its own compatibility matrix and maintenance status                                                                              |
| [Multilogin X feature overview](https://multilogin.com/help/en_US/introduction-to-multilogin-x/multilogin-x-top-features) | Product structure of Profile templates, proxy binding, and local or cloud storage                              | Stay local-first by default. Any remote storage needs its own threat model and an explicit user choice                                                 |
| [Multilogin Cloud Phones](https://multilogin.com/help/en_US/how-to-create-mobile-profiles)                                | Provide a full operating-system environment directly when browser field control is not enough                  | Mobile or real-device environments are not in the V0.8 first release. Evaluate them separately after V0.9                                              |
| [Donut Browser](https://github.com/zhom/donutbrowser)                                                                     | Product orchestration of Profile, proxy, browser engine and local automation entry point                       | AGPL-3.0 code must not be copied into the MPL-2.0 core. Only general ideas may be implemented independently                                            |
| [Camoufox](https://github.com/daijro/camoufox)                                                                            | Firefox engine-level control, configuration injection and BrowserForge distributions                           | The current first Managed Engine route. Pin assets and collect evidence stage by stage. Maintenance gaps and coherence risks must be visible in the UI |
| [BrowserForge](https://github.com/daijro/browserforge)                                                                    | Generate mutually constrained request-header and browser-feature templates that follow realistic distributions | Pin the version and record the source. Templates first pass rule constraints and verification. Do not randomize blindly                                |
| [FingerprintJS](https://github.com/fingerprintjs/fingerprintjs)                                                           | Modular collectors, error tolerance and stable signal extraction                                               | A hash is not an absolute identity. Use collection only for explanation and regression verification                                                    |
| [CreepJS](https://github.com/abrahamjuliot/creepjs)                                                                       | Look for self-contradiction across Window, iframe, Worker and rendering signals                                | Borrow only the contradiction rules and test structure. Do not treat a single-site result as proof of "undetectable"                                   |

Any third-party compatibility code must first record its version, license, attribution, modifications and SBOM entry.

## V0.7: controlled browser engine

### Current priority

Standard Silo remains the long-term base layer of the product. The V0.7 Camoufox qualification chain is complete. The production package and adapter, the Desktop signer pin, the Managed Silo creation and run path, and the current-user NSIS installer are implemented. The old M3-WI failure investigation and the clean Attempt 4 keep their historical evidence boundaries. The current-source `v0.1.0-rc2` has completed packaged runtime and post-install lifecycle acceptance in a pristine Windows Sandbox. Strict unelevated standard-user semantics, a broader Windows matrix, outer Authenticode and public release remain unproven. The product direction since 2026-11-10 is the commercial fingerprint browser described in the [north star](identity-platform-north-star.md) and planned in the [roadmap](roadmap.md); identity, execution, network, Runtime Evidence and attribution integrity remain the differentiator. For the current evidence and unverified boundaries, see [Camoufox Managed Engine status](camoufox-program-status.md). For the reasons behind the stable route, see the [Camoufox-first decision](camoufox-managed-engine-decision.md).

Controlled Chromium was not developed in parallel with Camoufox under the 2026-08 decision. **Update 2026-11-10:** the [commercial direction decision](product-direction-decision-2026-11.md) makes Controlled Chromium the planned primary commercial engine. Its evaluation (E0) and build-out (Phase 5) follow the [roadmap](roadmap.md). Only one controlled engine is in active build-out at a time, and V0.8/V0.9 environment work stays frozen until roadmap Phase 7.

### Delivery scope

1. Freeze the `EngineAdapter` interface: install, update, signature verification, launch arguments, identity template, capability probe, health check and rollback.
2. Keep Chrome/Edge Stable as the `stock-chromium` baseline adapter. It promises only a separate Profile and supported launch configuration.
3. Maintain the completed Camoufox Managed Engine vertical path: pinned Firefox-family engine assets, generating and replaying constrained Artifacts, persistent Profile, strict Host lifecycle, the EngineAdapter contract, and the production package/signing seam. It is not a Chrome emulation. It also does not automatically become an accepted release because any standalone, contract, build or static release check passes.
4. Freeze constrained identity templates: operating system, browser version, UA/UA-CH, language, time zone, screen, rendering, fonts, media devices and network settings must satisfy explicit rules.
5. Every control follows `observe → apply → verify → restore`. A long-lived seed never enters the page main world. The page receives only a short-lived derived token or an already-constrained configuration.
6. Engine packages must support signature verification, pinned versions, reproducible metadata, SBOM, a license list, rollback on incremental-update failure, and emergency disable.

### Release gate

- The same field has no unexplained contradiction across Window, iframe, Dedicated Worker, request headers and the engine network layer.
- Canvas/WebGL/font templates are stable across restarts of the same Silo and, as configured, distinguishable between different Silos.
- QUIC shows “已验证关闭” ("verified off") only after both the launch configuration and protocol observation confirm it. TLS uses the real ClientHello and negotiation result as evidence.
- Self-built test pages, FingerprintJS-style collectors and CreepJS-style contradiction rules all enter the compatibility matrix. Passing a single test site cannot serve as overall acceptance.
- A website compatibility failure can fall back per site, and the UI clearly shows which controls are currently rolled back.

## V0.8: local virtual environment

### Backend selection

- [WSL 2](https://learn.microsoft.com/en-us/windows/wsl/about) is a lighter Linux Chromium Provider. It can provide a separate Linux user space, file system, font set and browser Profile. By default it still shares or passes through the Windows network, GPU and WSLg integration, so it cannot be advertised as a fully independent device. The repository already has evidence for a pinned guest agent, exact Profile/PID lifecycle, and a user-hosted loopback SOCKS5H network exit with proxy DNS. The guest OS resolver and OS-level enforcement remain unavailable, and real Windows/WSLg acceptance is still a release gate. Network modes are verified against [Microsoft WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking).
- [Windows Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/) is a disposable compatibility lab. It suits repeatable clean tests, but its environment is temporary by default and it cannot currently run multiple instances at once, so it cannot be a persistent backend for multiple Silos. The pinned controller proves only the exact host process lifecycle. There is no reliable guest return channel, so guest health, network, DNS and browser readiness stay unavailable. For what a `.wsb` file can control, the [Microsoft configuration documentation](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file) governs.
- [Hyper-V](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/overview) is the persistent VM backend. It provides a separate guest operating system, disk and network boundary. The controller in the repository pins the VM GUID/name, generation, image hash and failure receipts. A valid VHDX, a pinned guest Agent and real signed lifecycle acceptance remain externally blocking conditions. Control-plane receipts cannot replace them.

Before enabling, the UI must detect the Windows version, virtualization capability, administrator requirement and restart requirement. Differences in system requirements, concurrency and available versions between Windows Sandbox and Hyper-V must not be hidden. Per the [Microsoft Hyper-V installation requirements](https://learn.microsoft.com/en-us/windows-server/virtualization/hyper-v/get-started/Install-Hyper-V), Hyper-V should not be shown as directly enableable on Windows Home.

### Delivery scope

1. Freeze the `EnvironmentBackend` interface: create, start, pause, snapshot, destroy, network configuration, health check and log export.
2. Provide a signed base-image manifest and reproducible configuration. Do not distribute system images of unknown origin directly.
3. By default, turn off host writable-directory mapping, clipboard, camera, microphone, USB and unnecessary GPU passthrough. When the user enables one, show its effect on the isolation boundary.
4. Each VM stores its own browser Profile, font set and network configuration. Network-exit verification must start from inside the guest environment.
5. "Real hardware has changed" is never a default conclusion. Passthrough GPU, virtual CPU and virtual devices must all be shown as they are.

WSL, Sandbox and Hyper-V all connect through the same `EnvironmentBackend` abstraction. WSL does not become a core VeriSilo dependency. Each backend's network exit and DNS must be verified from inside that backend and cannot reuse requests from the desktop controller. For the current implementation boundary of proxies and external Mihomo, see [Network identity and exit providers](network-identity-providers.md).

### Release gate

- Guest and host access to Cookies, disks, clipboard, mapped directories, ports and devices follows the default-deny policy.
- A snapshot restore does not merge the keys, seeds or browser data of two Silos.
- When the network is off or proxy verification fails, a required-proxy environment cannot enter a normal browsing session.
- Uninstalling VeriSilo does not silently delete a VM the user is still using. Deleting an environment needs separate confirmation and a statement of the data impact.

## V0.9: self-hosted remote environment

The first remote mode of V0.9 is a user-owned server or an organization-hosted node. It is not a default VeriSilo public cloud. The remote session lifecycle can draw on [Browserless session management](https://docs.browserless.io/baas/session-management), but VeriSilo defines and verifies the protocol, encrypted volumes, browser control and audit itself.

### Delivery scope

1. A minimal remote Agent: mutual authentication, version negotiation, environment creation, session start, health check, TTL and destruction.
2. Each remote environment uses a separate process or VM boundary, an encrypted persistent volume and its own network configuration. Exit IP, DNS and protocol checks run from inside the remote side.
3. Provide a browser display/input channel and an optional Playwright-style automation entry point. Authorize human sessions and automation separately.
4. The desktop explicitly shows data region, node owner, key ownership, online cost, last activity time and deletion status.
5. By default, do not copy Cookies, credentials or reports to a VeriSilo service. Import and export both require an explicit user action and an audit record.

### Release gate

- After the control plane is lost, the remote session locks or expires according to user policy and is not exposed indefinitely.
- The Agent rejects protocol downgrade, unknown fields, oversized messages and unauthorized environment IDs.
- A delete operation can prove that the compute instance, persistent volume, snapshots and temporary keys each reached the expected state.
- Desktop assertions and remote evidence correspond to each other. Configuration, application and actual network-exit verification must not be merged into one state.

## After V0.9

Real mobile devices, device farms, Linux/macOS hosts and organization-level scheduling can be evaluated once the interfaces above are stable. They do not block the Windows first release, and the extension UI will not present them as existing.

## License and supply-chain rules

- The VeriSilo core stays MPL-2.0. Documentation stays CC BY 4.0.
- MIT code of the FingerprintJS/CreepJS kind, BrowserForge's Apache-2.0 code and Camoufox's MPL-2.0 code may enter a distribution only after version pinning, NOTICE, attribution, modification records and SBOM are complete.
- The CreepJS name is subject to a trademark policy. VeriSilo may describe its test ideas or record compatibility results. It must not name its own components, services or public test sites CreepJS.
- Donut Browser's AGPL-3.0 code must not be copied or rewritten into the MPL core. Only independent implementation of general product ideas from public behavior is allowed.
- A third-party browser engine must be a replaceable adapter. Install, upgrade, rollback, end of maintenance and security advisories must all be visible in the UI.

## Overall statement

Dedicated engines, VMs and remote environments raise the isolation level and configuration coherence. They do not guarantee bypassing risk controls, and they do not remove correlation caused by behavior, account relationships, payment information, server-side history or human actions. VeriSilo's release standard is "a clear boundary with evidence", not "absolutely undetectable".
