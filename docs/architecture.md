# Architecture

```text
Tauri desktop UI
  ├─ encrypted vault: Silo metadata + seed material + optional network secrets
  ├─ launcher: one managed Chrome/Edge process and one user-data directory
  ├─ network coordinator
  │    ├─ fixed HTTP/SOCKS endpoint + authenticated loopback relay
  │    ├─ external loopback Mihomo Controller + stable node binding
  │    └─ fail-closed launch safeguards and staged runtime evidence
  ├─ Native Messaging Host: origin-checked, size-limited handshake only
  └─ scheduled environment adapters
       ├─ V0.7 controlled browser engine
       ├─ V0.8 WSL Chromium / Windows Sandbox / Hyper-V
       └─ V0.9 self-hosted remote agent

MV3 companion extension (optional per Silo)
  ├─ side panel: beginner-facing local report
  ├─ service worker: schema validation and optional permissions
  ├─ isolated content script: page observation
  └─ MAIN-world script: explicitly partial, observable observation only
```

The desktop application has no browser-state import/export API. A browser's cookies, LocalStorage, IndexedDB, CacheStorage, service workers, and history remain browser-owned files beneath that Silo's `browser-data` directory.

The scheduled adapters are stronger environment backends, not claims about the current launcher. Their design, release gates, licensing rules, and evidence requirements live in [the environment roadmap](environment-roadmap.md). Stock Chrome/Edge remains a supported baseline after additional engines arrive.

The current network provider architecture, external Mihomo boundary, credential flow, fail-closed arguments, and three-stage release split live in [the network identity document](network-identity-providers.md). The browser never receives upstream proxy credentials: when authentication is needed, it connects to a random loopback SOCKS5 relay owned by the desktop process.

The companion never forwards raw cookies, local storage, indexed databases, credentials, or vault material through Native Messaging. The current-document copy of an observation report lives in `chrome.storage.session` and is cleared on navigation or tab close. A redacted local record is saved under its report ID in trusted `chrome.storage.local` on the same device, but is never reused as the current page's report. No report is synced or sent to a VeriSilo service. JSON and HTML export require a new explicit confirmation and redact high-sensitivity signal values by default.

Desktop network verification is also user initiated. The WebView may contact `ipwho.is`, Cloudflare 1.1.1.1, and Google Public DNS only after the user clicks the disclosed action; the parsed result is held only in React memory. It describes the desktop controller's request path, not a managed browser's configured proxy. A Silo exit can be verified only from inside that Silo, currently through its explicitly installed companion extension.

## Native Host installation

The consumer installer must install the Tauri desktop binary and register a per-user Native Messaging manifest under `HKCU`. It must never write enterprise policy or silently install an extension. The user opens the Chrome Web Store or Edge Add-ons listing _within the newly created Silo_ and confirms the companion installation.

The manifest's `allowed_origins` contains the two production store extension IDs. The host independently reads the same ID allowlist and fails closed if it is absent. The developer registration script exists solely for explicit local development.
