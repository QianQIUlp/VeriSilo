# Pure extension and launcher ceiling

| Capability | Observe | Configure | Verify | Tier | Boundary |
| --- | --- | --- | --- | --- |
| Separate browser data directory | Yes | Yes | Yes | reliable | Created and launched by the desktop application. |
| Browser-managed cookies and storage | Indirect | Yes, through separate directory | Yes, with local fixture | reliable | VeriSilo does not serialize or restore these values. |
| Fixed proxy launch argument | Yes | Yes | Partial | reliable | A TCP preflight is not proof of all browser traffic routing. |
| Exit IP | No, by default | No | User initiated only | best_effort | Requires a disclosed endpoint chosen by the user. |
| WebRTC privacy preference | Yes | Explicit optional extension control | Browser setting evidence | best_effort | `disable_non_proxied_udp` is reversible and marked not controllable if policy/another extension owns it; it is not a packet-level guarantee. |
| MAIN-world fingerprint API observation | Partial | N/A | Coverage recorded | best_effort | Injection may lose races and is visible to the page; accepted values are deliberately limited and labelled untrusted. |
| Window/iframe signal modification | Partial | Experimental | Per site only | best_effort | Cannot imply Worker or network-stack coverage. |
| Dedicated Worker modification | Partial | Research only | No general claim | best_effort | May cover narrow same-origin scenarios only. |
| SharedWorker / Service Worker modification | No | No | No | unsupported | Normal extensions have no generic injection target. |
| TLS, HTTP/2, HTTP/3, QUIC, GPU, OS fonts | Limited observation | No | No | unsupported | Requires a different browser/OS/VM layer. |
