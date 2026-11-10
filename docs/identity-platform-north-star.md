# VeriSilo north star

- Status: **normative product intent**. Rewritten in English on 2026-11-10 at the owner's direction. It replaces the earlier Chinese "identity platform north star" (Git history at `1cf0f29`).
- This page states what VeriSilo is for and which principles must not drift between milestones. The reasons behind the current direction are in the [commercial direction decision](product-direction-decision-2026-11.md). The plan is in the [roadmap](roadmap.md). The current phase and next task are on the [program status page](program-status.md).

## Product statement

**VeriSilo is a commercial-grade, multi-profile fingerprint browser.** It lets a person or a team run many persistent, isolated browser identities. Each identity has its own browser state, a coherent and stable fingerprint, and its own network exit. VeriSilo also shows evidence of what actually ran.

The question VeriSilo answers better than other products in its category is: **did the declared identity, engine and network policy actually run as declared?** Feature breadth gets VeriSilo into the category. Integrity and evidence are why a customer should choose it.

## Who it is for

VeriSilo serves people who treat browser identities as long-lived assets and lose money or time when identities leak into each other:

| Customer                                     | Job to be done                                                                                        |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| E-commerce sellers with several stores       | Keep each store's logins, cookies, fingerprint and exit separate and stable for years.                |
| Ad, affiliate and social media agencies      | Operate client accounts from one workstation and hand them to team members without sharing passwords. |
| Growth, community and support teams          | Run many brand or regional accounts with the right language, timezone and region.                     |
| QA, ad-verification and localization testers | See a site as users in another region, browser or device class see it, reproducibly.                  |
| Data and research teams                      | Run lawful automation against their own or permitted targets with stable, attributable identities.    |
| Privacy-conscious individuals                | Separate personal, work and research browsing into identities that do not correlate.                  |

## What "commercial grade" means

A feature is not done until it meets these bars on the supported platforms:

1. **Reliable.** No profile corruption, no lost logins, no orphaned browser processes. Crashes are rare, recoverable and diagnosable.
2. **Scales.** Hundreds of Silos stay manageable. Tens run at once on an ordinary workstation, and the UI stays responsive.
3. **Coherent fingerprints.** Every website-visible surface of one identity agrees with every other: JavaScript in windows, iframes and workers; HTTP headers; TLS and HTTP/2 shape; screen, fonts, GPU, locale, timezone and network location. The identity stays the same across restarts.
4. **Network integrity.** A Silo that requires a proxy never leaks to a direct connection. DNS and WebRTC follow the declared path or are reported as not covered.
5. **Secure.** Secrets are encrypted at rest. Builds are signed. Updates are signed and can roll back. Automation access is scoped, revocable and audited.
6. **Distributable.** Signed installers, automatic updates, upgrades that keep user data, and an English user interface. Windows and Linux first, macOS later.
7. **Operable at work.** Groups, tags, search and bulk operations; a reusable proxy library; import and export; a local API and Playwright/CDP automation; team sharing with roles.
8. **Honest.** The product says what it controls, what it only observes and what it cannot control. It never inflates a configured value into a verified one.

## Differentiation

- **Integrity first.** Identity Integrity, Execution Integrity, Network Integrity, Runtime Evidence and Attribution are product features, not internal QA. Every Silo can show declared versus observed values, with `matched`, `mismatched`, `unavailable` and `stale` states.
- **Fail closed.** When a required proxy, engine package, identity binding or ownership check fails, VeriSilo stops instead of degrading silently.
- **User-owned data.** Local-first by default. Sync is optional and end-to-end encrypted. No telemetry by default.
- **Open core.** The core application is open source (MPL-2.0). Customers can audit what runs.

## Domain model

A Silo is a set of separately versioned parts with explicit bindings:

```text
Silo
= Persistent Profile        browser-owned state: cookies, storage, cache, service workers, permissions, history, settings
+ Resolved Identity Artifact the resolved, replayable, website-visible identity configuration (never a Profile, never a secret)
+ Engine Binding            the exact browser family, version and verified package that applies the identity
+ Network Policy            direct, fixed proxy or user-run provider, plus region/timezone/language coupling rules
+ Runtime Evidence          what was configured, applied, observed, verified, failed or unavailable
```

Two concerns sit beside the Silo and never change its identity:

- **Organization metadata:** name, color, group, tags, notes and custom fields.
- **Ownership:** local, or held under a team lease so that only one device runs a Silo at a time.

These parts keep separate schemas and lifecycles. Merging any two of them into one opaque directory requires an explicit decision.

## Three isolation concerns

| Concern               | Problem it solves                                                       | Typical mechanism               |
| --------------------- | ----------------------------------------------------------------------- | ------------------------------- |
| Profile isolation     | Cookies and browser state do not cross between identities.              | Separate user data directory    |
| Fingerprint control   | Websites see a stable, coherent browser and device that fit the engine. | Managed engine plus Artifact    |
| Environment isolation | The browser is separated from host files, processes, OS or hardware.    | Virtual machine, remote machine |

```text
a separate Profile ≠ a fingerprint browser
a fingerprint browser ≠ a virtual machine
a virtual machine ≠ a coherent website identity
```

## Product layers

### Standard Silo

- Uses the system Chrome or Edge with a separate Profile, Vault-held secrets, a network policy, lifecycle control and local evidence.
- Hardware and browser fingerprints follow the real machine. VeriSilo reports them as `native` or `unavailable` and never claims to control them.
- Stays as a permanent, low-cost entry layer.

### Managed Identity Silo

- Uses a pinned, signed, VeriSilo-built browser engine that applies a stable Resolved Identity Artifact.
- **Camoufox** (Firefox family) is the current shipping managed engine.
- **Controlled Chromium** is the planned primary commercial engine, because most of the web, most customer extensions and most automation tools expect Chromium. It is built and qualified under the [commercial direction decision](product-direction-decision-2026-11.md).
- An identity always matches its engine family. A Firefox engine never claims to be Chrome, and the reverse is also true.
- Identities are checked by first-party probes for stability across restarts and for separation between Silos.

### Isolated Machine Silo

- Runs the browser in a pinned guest image or a remote machine with its own virtual network.
- Solves host isolation, not everyday account management. It is a later, optional layer.

## Identity policy: coherence first

Changing more fields does not make a better fingerprint. The default identity must keep the OS, browser version, user agent and client hints, language, timezone, screen, rendering, fonts, GPU and network location consistent with each other, and keep them stable for the life of the Silo.

- Identities are sampled from real-device distributions and constrained to be internally consistent.
- The default presents the same OS family as the host, because cross-OS identities leak through rendering and fonts unless the engine can prove otherwise.
- Changing an identity after its first launch creates a new identity revision with a visible record. Per-launch or per-call randomization is never the default.

User-facing policy expresses intent, not seeds:

| Policy                       | Meaning                                                           |
| ---------------------------- | ----------------------------------------------------------------- |
| `inherit` / `native`         | Follow the real machine or browser value.                         |
| `coherent` / `common_bucket` | Choose a common value that fits the rest of the identity.         |
| `fixed`                      | Use the value the user or the resolved Artifact specifies.        |
| `disabled`                   | Turn a controllable surface off on purpose.                       |
| `unavailable`                | The current engine cannot control or check this surface reliably. |

The Resolved Identity Artifact is the low-level replay record. It is not the user-facing configuration model.

## Evidence vocabulary

- `configured`: the configuration exists and is valid.
- `applied`: the controlled execution layer reports that it applied the configuration.
- `observed`: a named observation channel saw the result. It is not proof of process origin.
- `verified`: every direct-evidence and binding condition of the capability's verification contract holds.
- `unavailable`: the build cannot honestly establish the result. This is a normal result and is never replaced by a default or a guess.

Observed equality does not become `verified` by itself. Stronger verification is added capability by capability.

## Network policy

- Every Silo binds to direct access, a user-owned fixed proxy, or a user-run Clash/Mihomo-compatible provider. A reusable proxy library and bulk assignment are part of the product.
- In required-proxy mode there is no direct fallback, ever.
- Timezone, language and geolocation can follow the proxy exit. WebRTC follows the exit or is restricted.
- VeriSilo does not operate a proxy exit marketplace or sell "IP purity" scores. Third-party proxy providers can be used with the customer's own account.

## Data, sync and privacy

- All data is local by default. The Vault encrypts secrets, identity Artifacts and evidence.
- Team sharing and multi-device use go through optional, end-to-end encrypted sync. A self-hostable sync server comes first. A VeriSilo-hosted instance may follow as a paid service, and its operator must not be able to read customer data.
- No telemetry by default. Crash reports and diagnostics are opt-in and redacted.

## Honest claims and acceptable use

- VeriSilo does not promise to be "undetectable" and does not promise to bypass anti-fraud systems. No single detection test site is treated as overall acceptance.
- VeriSilo does not build CAPTCHA solving, synthetic "human-like" input meant to defeat bot detection, payment or identity-verification fraud tools, or automated mass account registration.
- Customers must accept terms of service and an acceptable-use policy that forbid fraud and other unlawful use.

These limits keep VeriSilo distributable through code-signing authorities, payment processors and browser extension stores, and they make its claims credible.

## Sequencing principles

- Only one controlled engine is in active build-out at a time. The other managed engine receives maintenance, security rebases and regression fixes.
- WSL, Hyper-V, VMware and remote environments stay frozen until the managed engines and the workstation features reach their roadmap exits.
- Native Windows conclusions require native Windows evidence. Linux, WSL or Wine results never stand in for them.
- A test pass, a build or a configuration is never presented as a runtime or product Gate that has not been met.

## Change rules

These changes require a new explicit, recorded product or architecture decision. A single task must not make them silently:

- changing which managed engine is the primary engine, or starting a third engine family;
- making WSL, Hyper-V, VMware or a remote machine the default execution layer for ordinary Silos;
- merging the lifecycles of Profile, Identity Artifact, network secrets or evidence;
- weakening the distinction between `configured`, `applied`, `observed`, `verified` and `unavailable`;
- replacing stable coherent identities with per-launch randomization;
- adding default telemetry, a proxy marketplace, or a service that can read customer data;
- relaxing the acceptable-use limits above.
