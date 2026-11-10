# Store privacy form answers

- Status: **draft for pasting**
- Factual baseline: [`extension-data-flow-inventory.md`](extension-data-flow-inventory.md) and [`store-disclosure.md`](store-disclosure.md)
- Applies to: the privacy/permission forms of the Chrome Web Store Developer Dashboard and Microsoft Partner Center

## Chrome Web Store

### Privacy practices — Single purpose

> VeriSilo Companion helps you inspect and understand the observable exposure of your current browsing environment: what browser signals the page you are on can read, what your network exit looks like, and which temporary controls are active. All scans, network checks, and controls are user-initiated.

### Privacy practices — Permission justification

| Permission                      | Purpose statement (form text)                                                                                                              |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `storage`                       | Keeps the current scan report and a bounded, clearable local history (max 20 redacted reports, 30 days) inside trusted extension contexts. |
| `sidePanel`                     | Provides the Companion side-panel UI.                                                                                                      |
| `activeTab`                     | Lets the user scan only the page they opened the panel on; access expires when they leave.                                                 |
| `scripting`                     | Injects the read-only signal collectors on the page the user is scanning.                                                                  |
| `nativeMessaging`               | Talks to the separately installed VeriSilo Desktop's local Native Host for a redacted runtime status and user-submitted network results.   |
| `privacy` (optional)            | Requested only when the user explicitly enables temporary WebRTC / network-prediction controls.                                            |
| `http(s)://*/*` (optional host) | Requested only when the user invokes a scan or the network check that needs to contact the listed services.                                |

### Privacy practices — Data usage

- Data collected: summaries of browser-visible signals produced by user-triggered scans, and results of user-triggered network checks.
- Purpose: local display and interpretation. No upload to a server.
- Sold/transmitted: no. The network check sends fixed queries to ipwho.is, Cloudflare 1.1.1.1 and Google Public DNS. When ipwho.is is unavailable, it uses api.ipify.org or api.ip.sb as a fallback for the exit IP (the other side sees the request IP).
- Retention: session reports disappear with the session. Local history keeps at most 20 entries / 30 days. The user can clear it.
- Remote code: none (checked by the bundle gate).

## Microsoft Edge Add-ons (Partner Center)

### Single purpose (new privacy page)

> VeriSilo Companion is a single-purpose inspection tool: it helps users understand the observable exposure of their current browsing environment through user-initiated scans, an optional network check, and explicit temporary controls. Network checks and privacy controls serve this same purpose and never run automatically.

### Permission justification (per item; the same text as Chrome can be used)

Same as the table above. One extra Edge note: `proxy` / `declarativeNetRequest` are not declared (the bundle gate explicitly rejects them as optional).

### Remote code

> None. The extension ships all code in the package; it does not load or execute remote scripts. The store package is verified by a deterministic bundle gate.

### Data usage

- Same Data usage text as Chrome.
- Addition: Extension does not collect payment data, personal communications, or user activity for advertising.

## Manual review items before submission

- [ ] Compare the form text item by item with `docs/store-disclosure.md` (that document is checked by the bundle gate and is the most complete current disclosure).
- [ ] Confirm that the Network Check endpoints are still ipwho.is / api.ipify.org / api.ip.sb / cloudflare-dns.com / dns.google (ipify and ip.sb are only the exit-IP fallback when the primary service is unavailable) and that no URL was added (`scripts/verify-extension-bundle.mjs` already checks this; review it manually once).
- [ ] Enter `https://verisilo.qiu.works/privacy` as the privacy policy URL (after it goes live, verify that it is reachable over HTTPS).
