# Architecture

```text
Tauri desktop UI
  ├─ encrypted vault: Silo metadata + seed material only
  ├─ launcher: one managed Chrome/Edge process and one user-data directory
  └─ Native Messaging Host: origin-checked, size-limited handshake only

MV3 companion extension (optional per Silo)
  ├─ side panel: beginner-facing local report
  ├─ service worker: schema validation and optional permissions
  ├─ isolated content script: page observation
  └─ MAIN-world script: explicitly partial, observable observation only
```

The desktop application has no browser-state import/export API. A browser's cookies, LocalStorage, IndexedDB, CacheStorage, service workers, and history remain browser-owned files beneath that Silo's `browser-data` directory.

The companion never forwards raw cookies, local storage, indexed databases, credentials, or vault material through Native Messaging. The current-document copy of an observation report lives in `chrome.storage.session` and is cleared on navigation or tab close. A redacted local record is saved under its report ID in trusted `chrome.storage.local` on the same device, but is never reused as the current page's report. No report is synced or sent to a VeriSilo service. JSON and HTML export require a new explicit confirmation and redact high-sensitivity signal values by default.

## Native Host installation

The consumer installer must install the Tauri desktop binary and register a per-user Native Messaging manifest under `HKCU`. It must never write enterprise policy or silently install an extension. The user opens the Chrome Web Store or Edge Add-ons listing _within the newly created Silo_ and confirms the companion installation.

The manifest's `allowed_origins` contains the two production store extension IDs. The host independently reads the same ID allowlist and fails closed if it is absent. The developer registration script exists solely for explicit local development.
