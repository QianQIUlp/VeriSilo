# Architecture

```text
Tauri desktop UI
  ├─ encrypted vault: Silo metadata + seed material + optional network secrets
  ├─ launcher: one managed process and one user-data directory
  │    ├─ stock Chrome/Edge adapter
  │    └─ verified external controlled-engine adapter (optional artifact)
  │         └─ M3-0 Camoufox Host contract (accepted)
  │              └─ real Windows seam: test-only, experimental, not shipped
  ├─ network coordinator
  │    ├─ fixed HTTP/SOCKS endpoint + authenticated loopback relay
  │    ├─ external loopback Mihomo Controller + stable node binding
  │    └─ fail-closed launch safeguards and staged runtime evidence
  ├─ Native Messaging Host: production-ID allowlist, bounded protocol,
  │    desktop wake-up, and short-lived redacted runtime snapshots
  ├─ V0.8 local environment controller
  │    ├─ WSL Chromium
  │    ├─ Windows Sandbox laboratory
  │    └─ Hyper-V persistent VM controller
  └─ V0.9 remote controller
       └─ pinned HTTPS + Vault credential/binding + typed lifecycle

User-operated Remote Agent (optional, Linux/Unix)
  ├─ TLS-only strict request router and one-time pairing
  ├─ durable auth/control state, TTL, authorization and deletion proof
  └─ fixed-hash local Provider bridge
       └─ real VM/browser/screen Provider is not bundled yet

MV3 companion extension (optional per Silo)
  ├─ side panel: beginner-facing local report
  ├─ service worker: schema validation and optional permissions
  ├─ isolated content script: page observation
  └─ MAIN-world script: explicitly partial, observable observation only
```

The product layers are Standard Silo, Managed Identity Silo, and Isolated Machine Silo. They do not collapse the underlying domains: a Persistent Profile owns browser state, a Resolved Identity Artifact owns replayable website-visible configuration, an Engine Binding names the executable asset that can apply it, Network Policy owns the connection path, and Runtime Evidence records capability state. The normative model is defined in [the identity platform north star](identity-platform-north-star.md).

The current `main` branch contains the stock launcher, the external-package `EngineAdapter` control contract, and the standalone Python Camoufox Host plus accepted Linux/native-Windows M0–M2-W evidence merged by [PR #10](https://github.com/QianQIUlp/VeriSilo/pull/10). [M3-0](camoufox-m3-engine-adapter-task.md) accepted the package-entrypoint, dedicated Host transport, Artifact binding, lifecycle, and honest capability/evidence contract at `e96ef3f`. A test-only Windows seam can pass the real Host plan through `RuntimeManager`, but M3-WI remains Failed after an inconclusive second-Host investigation and is still `experimental`. Release Tauri has no trusted signed Camoufox Host package or signer pin, so this is neither a production launch path nor a shipped capability.

Managed Engine work now proceeds through FP1 deterministic Artifact projection, FP2 cross-realm consistency, FP3 network/region coordination, and FP4 site compatibility before a newly frozen clean M3-WI integration Gate. Current checkpoints are maintained in [the Camoufox program status](camoufox-program-status.md).

The desktop application has no browser-state import/export API. A browser's cookies, LocalStorage, IndexedDB, CacheStorage, service workers, and history remain browser-owned files beneath that Silo's `browser-data` directory.

The optional adapters are stronger environment backends, not claims about the stock launcher. V0.7 has a versioned adapter, package-signature/update/rollback state machine, per-Silo configuration and a short-token bootstrap path, but no trusted controlled-engine artifact is bundled. V0.8 has executable backend controllers and fixed scripts, while real WSL/Sandbox/Hyper-V browser evidence still requires compatible Windows hosts and legal guest images. V0.9 has a real desktop control plane and self-hosted TLS Agent, while the actual VM/browser/media Provider remains an external implementation gate. Their exact evidence states and release requirements live in [the environment roadmap](environment-roadmap.md). Stock Chrome/Edge remains a supported baseline after additional engines arrive.

The Remote Agent is never a default VeriSilo cloud. Pairing is explicit; the desktop authenticates the self-hosted endpoint with normal PKI plus a mandatory certificate/SPKI pin, and the Agent authenticates the client with a short-lived credential stored in the encrypted desktop Vault. Remote requests contain only fixed typed operations. They cannot select a shell, executable, arbitrary argument list, filesystem path, image or URL. See [the V0.9 control-plane boundary](remote-environment-v0.9.md).

The current network provider architecture, external Mihomo boundary, credential flow, fail-closed arguments, and three-stage release split live in [the network identity document](network-identity-providers.md). The browser never receives upstream proxy credentials: when authentication is needed, it connects to a random loopback SOCKS5 relay owned by the desktop process.

The companion never forwards raw cookies, local storage, indexed databases, credentials, or vault material through Native Messaging. The current-document copy of an observation report lives in `chrome.storage.session` and is cleared on navigation or tab close. Up to 20 redacted local records are kept for at most 30 days in trusted `chrome.storage.local`; the user can inspect and clear that history, and no saved record is reused as the current page's report. No report is synced or sent to a VeriSilo service. JSON and HTML export require a new explicit confirmation and redact high-sensitivity signal values by default.

Desktop network verification is also user initiated. The WebView may contact `ipwho.is`, Cloudflare 1.1.1.1, and Google Public DNS only after the user clicks the disclosed action; the parsed result is held only in React memory. It describes the desktop controller's request path, not a managed browser's configured proxy. A Companion check inside a Silo is persisted only as `extension_asserted` / `observed`: the Native Host validates protocol, active-Silo binding and freshness, but it does not cryptographically authenticate the browser process. Only a fixed Guest/Engine source with stronger provenance may promote an exit to `verified`.

## Native Host installation

The consumer installer must install the Tauri desktop binary and register a per-user Native Messaging manifest under `HKCU`. It must never write enterprise policy or silently install an extension. The user opens the Chrome Web Store or Edge Add-ons listing _within the newly created Silo_ and confirms the companion installation.

Chrome and Edge receive separate manifests whose `allowed_origins` each contain only that store's published extension ID. The same IDs are embedded into the Host through `VERISILO_CHROME_EXTENSION_ID` and `VERISILO_EDGE_EXTENSION_ID` at build time; a build with missing or malformed values authorizes no production extension. The Host never trusts an install-time ID file in release mode. Debug builds may additionally read the explicitly generated development allowlist.

The bridge accepts only the strict protocol in `packages/contracts`, rejects messages over 16 KiB and fields associated with cookies, credentials, browser storage, seed material, or Vault contents. `open_desktop` resolves only `verisilo.exe` beside the installed `verisilo-native-host.exe` and passes no caller-controlled arguments. Runtime status is read from a desktop-written snapshot that expires after 45 seconds and omits messages, proxy endpoint labels, Silo metadata, and all browser-owned data. A snapshot is a local UI status channel, not proof of network isolation.

After a user explicitly runs Companion's disclosed network check, the extension may submit that already-local result to a temporary Native Host inbox. The Host accepts it only while the Vault is unlocked and the submitted Silo ID matches a fresh, running desktop snapshot. Paths and filenames are Host-generated; entries are limited to 16 KiB, 32 pending files, and a ten-minute lifetime. The fixed coverage declaration distinguishes a third-party HTTPS IP observation and a public DoH answer comparison from unobserved DNS routing, WebRTC, and QUIC. Draining rechecks the active Silo and deletes malformed, expired, or unauthorized files. Inbox acceptance is not a `verified` capability result.
