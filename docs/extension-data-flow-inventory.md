# VeriSilo Companion data-flow inventory

- Status: **mechanical fact baseline for store disclosure**
- Generated: 2026-08-15, checked item by item against the extension source at `main@45bfea1`
- Purpose: the only source of truth for the Chrome Web Store / Edge Add-ons privacy form and the privacy policy. Any "local only" or "no data collected" statement must be covered by the tables below.

## 1. Trigger model

No data collection runs automatically. The user triggers every action explicitly in the side panel:

| Action                                       | How it is triggered                                                                          | Happens by default |
| -------------------------------------------- | -------------------------------------------------------------------------------------------- | ------------------ |
| Page scan (observation)                      | The user clicks the toolbar icon on the target web page to open the side panel → clicks Scan | No                 |
| MAIN-world observation                       | Part of the scan (`executeScript`, best-effort)                                              | No, runs with scan |
| Network Check                                | The user clicks → grants an optional host permission → clicks to confirm                     | No                 |
| Labs web leak experiment                     | The user turns it on explicitly                                                              | No                 |
| Privacy controls (WebRTC/network prediction) | The user turns it on explicitly; restorable                                                  | No                 |
| Report export (JSON/HTML)                    | The user clicks export and confirms                                                          | No                 |
| Open project page                            | The user clicks the desktop project button                                                   | No                 |

## 2. Signals read by the page scan (the `collect` list in `content.ts`)

All reads happen in the single tab the user actively scans, and are read-only:

| Signal group                     | Content                                                                                     | Sensitivity | Stability |
| -------------------------------- | ------------------------------------------------------------------------------------------- | ----------- | --------- |
| `navigator`                      | userAgent, platform, language, languages, hardwareConcurrency, deviceMemory, maxTouchPoints | medium      | stable    |
| `ua_ch`                          | User-Agent Client Hints                                                                     | medium      | stable    |
| `timezone`                       | Time zone resolved by `Intl`                                                                | medium      | stable    |
| `screen`                         | Screen size, avail size, color depth                                                        | medium      | session   |
| `canvas_hash`                    | Canvas fingerprint digest                                                                   | high        | session   |
| `webgl`                          | WebGL renderer/vendor digest                                                                | high        | session   |
| `webgpu`                         | WebGPU digest                                                                               | high        | session   |
| `audio`                          | AudioContext fingerprint digest                                                             | high        | session   |
| `fonts`                          | Font enumeration digest                                                                     | high        | session   |
| `media_devices`                  | Device enumeration digest                                                                   | high        | session   |
| `permissions`                    | Site permission state digest                                                                | medium      | session   |
| `storage`                        | Storage state such as `cookiesEnabled`                                                      | medium      | session   |
| `webrtc`                         | WebRTC digest                                                                               | high        | session   |
| `window_iframe`                  | iframe context observation                                                                  | medium      | session   |
| `dedicated_worker`               | Dedicated Worker self-test                                                                  | medium      | session   |
| `main_world_navigator_untrusted` | Page-visible navigator (explicitly labeled `page_observable_untrusted`)                     | medium      | stable    |

**Not read**: Cookie values, LocalStorage/IndexedDB contents, page body text, form input, passwords, browsing history.

## 3. Where data is stored (all local)

| Storage                               | Content                                                                              | Retention policy                            | Access restriction                                   |
| ------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------- | ---------------------------------------------------- |
| Memory (content script)               | Signals of the scan in progress                                                      | Lost on page navigation or close            | Page isolated world                                  |
| `chrome.storage.session`              | Current report (`report:<tabId>`), Network Check result and handoff state            | Session lifetime; the UI has a clear button | `setAccessLevel("TRUSTED_CONTEXTS")`                 |
| `chrome.storage.local`                | Up to 20 redacted historical reports (`saved-report:*`, not saved in incognito mode) | 30-day TTL, trimmed before writing          | `setAccessLevel("TRUSTED_CONTEXTS")`, not synced     |
| Outside the extension (desktop Vault) | Network Check results submitted by the user                                          | See item 10 of `docs/store-disclosure.md`   | Only when the Vault is unlocked and the Silo matches |

## 4. Network requests (Network Check only, user-triggered)

| Endpoint                                                                                    | Request                                                              | What is sent                                        | What is received                 | Retention                                   |
| ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | --------------------------------------------------- | -------------------------------- | ------------------------------------------- |
| `https://ipwho.is/`                                                                         | GET, `credentials: omit`, `no-referrer`, 10s timeout                 | No request body; the third party sees the source IP | Exit IP / geolocation / ASN JSON | Stored in `storage.session`, can be cleared |
| `https://api.ipify.org?format=json`                                                         | GET, exit IP fallback used only when the main service is unavailable | Same as above                                       | Exit IP JSON                     | Same as above                               |
| `https://api.ip.sb/geoip`                                                                   | GET, exit IP fallback used only when the main service is unavailable | Same as above                                       | Exit IP / geolocation JSON       | Same as above                               |
| `https://cloudflare-dns.com/dns-query?name=example.com&type=A&do=true`                      | GET, `Accept: application/dns-json`                                  | Same as above                                       | A record of a fixed domain       | Same as above                               |
| `https://dns.google/resolve?name=example.com&type=A&do=true&edns_client_subnet=0.0.0.0%2F0` | GET                                                                  | Same as above                                       | A record of a fixed domain       | Same as above                               |

- A request is never sent automatically. After the optional host permission is granted, the user must still confirm before it is sent.
- Responses are limited to 64 KiB, and the result is built after JSON parsing.
- There are no network calls outside the five domains (ipwho.is / api.ipify.org / api.ip.sb / cloudflare-dns.com / dns.google). The bundle gate checks each URL.
- Opening the project page navigates to `https://github.com/QianQIUlp/VeriSilo` (`packages/contracts/src/product.ts`).

## 5. Native Messaging (`io.verisilo.host`)

Available only when the VeriSilo desktop is installed on this machine and the Host is registered. Any failure degrades to `local_only` and does not affect use of the extension.

| Message                   | Payload                                                                                        | Direction                                                            |
| ------------------------- | ---------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `get_runtime_status`      | `protocolVersion`, `requestId` (UUID)                                                          | Extension → Host → returns a redacted snapshot (expires in 45s)      |
| `submit_network_evidence` | `siloId`, `runtimeId`, `networkCheck` (the local result just produced), `coverage` declaration | Extension → Host (temporary inbox, 16 KiB / 32 entries / 10 minutes) |
| `open_desktop`            | No parameters                                                                                  | Extension → Host → wakes `verisilo.exe`                              |

**Never sent**: Cookies, LocalStorage/IndexedDB, credentials, browsing history, the full observation report, Vault keys. The snapshot contains no messages, proxy labels, Silo metadata or browser-owned data (see line 58 of `docs/architecture.md`).

## 6. Export (user-triggered)

JSON/HTML export redacts high-sensitivity signal values by default (`redactObservationReport` in `report-export.ts`). The file name contains the reportId.

## 7. Mapping to store questions

| Store question              | Basis for the answer                                                                                                           |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| What data is collected      | The signal list in section 2; all through a user-triggered scan                                                                |
| Does data leave the machine | Only the three Network Check requests (section 4) and the optional native bridge (section 5). Observation data is not uploaded |
| Remote code                 | None. The bundle gate verifies there is no `eval`, `new Function` or dynamic `import`                                          |
| Sold or shared              | No. There is no VeriSilo server                                                                                                |
| Retention and deletion      | Section 3. The user can clear session results and local history at any time                                                    |
