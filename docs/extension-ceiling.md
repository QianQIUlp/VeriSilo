# Pure extension and launcher ceiling

This document describes the MV3 extension and stock-browser launcher ceiling, not VeriSilo's permanent product ceiling. Capabilities that need an engine, VM, or remote environment are scheduled in [the environment roadmap](environment-roadmap.md).

| Capability | Observe | Configure | Verify | Tier | Boundary |
| --- | --- | --- | --- | --- |
| Separate browser data directory | Yes | Yes | Yes | reliable | Created and launched by the desktop application. |
| Browser-managed cookies and storage | Indirect | Yes, through separate directory | Yes, with local fixture | reliable | VeriSilo does not serialize or restore these values. |
| Fixed proxy launch argument | Yes | Yes | Partial | reliable | A TCP preflight is not proof of all browser traffic routing. |
| Exit IP | No, by default | No | User initiated only | best_effort | Requires a disclosed endpoint chosen by the user. |
| WebRTC privacy preference | Yes | Explicit optional extension control | Browser setting evidence | best_effort | `disable_non_proxied_udp` is reversible and marked not controllable if policy/another extension owns it; it is not a packet-level guarantee. |
| MAIN-world fingerprint API observation | Partial | N/A | Coverage recorded | best_effort | Injection may lose races and is visible to the page; accepted values are deliberately limited and labelled untrusted. |
| Window/iframe signal modification | Partial | Experimental | Per site only | best_effort | Cannot imply Worker or network-stack coverage. |
| New classic Dedicated Worker wrapper | Narrow current-site canary | Explicit Silo/site Labs gate, two minutes | Self-test only; state remains `best_effort` | best_effort | Only new same-origin/blob classic Dedicated Workers and a same-origin iframe probe. User-triggered injection does not prove document-start ordering. Existing/module/cross-origin Workers are excluded. |
| Cookie canary observation | Page-visible values only | No mutation | Leak stop signal only | best_effort | Does not see HttpOnly or prove Cookie Store, Worker, Service Worker, navigation, or network-response coverage. |
| Cookie repository virtualization | No complete observation | No | No | unsupported | A normal MV3 extension cannot provide a complete transparent cookie repository. Use a desktop Silo with an independent `user-data-dir`. |
| Complete Set-Cookie interception | No complete observation | No | No | unsupported | No `webRequestBlocking`, DNR rewrite, permanent host permission, or fake interception is shipped. Use an independent profile or a stronger engine/environment layer. |
| SharedWorker / Service Worker modification | Service Worker registration URL canary only | No | No general claim | unsupported | Normal extensions have no generic injection target or memory-inspection surface. |
| TLS, HTTP/2, HTTP/3, QUIC, GPU, OS fonts | Limited observation | No | No | unsupported | Requires a different browser/OS/VM layer. |

## V0.5 Labs gate

`VeriSilo Labs` is a separate, versioned, default-off gate. The selectable Worker experiment requires the current origin's optional host permission and an explicit click. When a fresh running desktop Silo is available, the authorization is bound to that Silo UUID and origin; otherwise it is labelled `local_temporary`, expires locally, and is never presented as a Silo result.

Every run records the ordered `observe → apply → verify → restore` phases. Stop conditions are machine-readable and all map to `restore_and_disable_site`: cross-tab, iframe, Worker, Service Worker URL, visible-cookie or Window canary exposure; page/Worker errors; timeout; permission takeover; navigation; scope violations; verification failure; or extension-context loss. Receipts retain only enum evidence, the site host, opaque run/Silo IDs, timestamps, coverage and restore outcome. Canary material, Cookie values and authentication tokens are not persisted.

The current Worker entry point is a user-triggered `chrome.scripting.executeScript` call. Even if it runs while `document.readyState` is `loading`, that is not evidence that it preceded every page script. Consequently, a passing constructor/Worker/iframe self-test is `best_effort`, never `verified`. The page wrapper has its own expiry and restores the original constructor on abnormal events so service-worker suspension does not turn a temporary run into a permanent page modification.
