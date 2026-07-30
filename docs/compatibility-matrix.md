# Compatibility matrix

| Target                    | Status                         | Notes                                                                                                       |
| ------------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| Windows 10 x64            | planned validation target      | Desktop installer and browser discovery require Windows end-to-end verification.                            |
| Windows 11 x64            | planned validation target      | Desktop installer and browser discovery require Windows end-to-end verification.                            |
| Chrome Stable             | supported target               | Uses Chromium command-line profile isolation and Manifest V3 companion APIs.                                |
| Edge Stable               | supported target               | Uses the same MV3 source with its own Store ID and Native Messaging registration.                           |
| Fixed HTTP/SOCKS5 proxy   | implemented, needs Windows E2E | Optional credentials use a loopback relay; SOCKS5 auth is checked before launch, HTTP auth by real request. |
| External Mihomo/Clash API | implemented adapter            | User-run core only; loopback Controller, stable selector/node binding, no bundled GPL binary.               |
| Controlled engine adapter | V0.7 scheduled                 | Stock Chrome/Edge stays supported; Chromium and Camoufox tracks require separate evidence.                  |
| WSL Chromium              | V0.8 scheduled option          | Current build performs only a read-only WSL/distribution check; no Chromium lifecycle or TUN yet.           |
| Windows Sandbox           | V0.8 scheduled option          | Disposable lab only; unavailable on Home and currently limited to one running instance.                     |
| Hyper-V                   | V0.8 scheduled option          | Persistent VM backend; requires Pro/Enterprise, virtualization support, admin enablement, and restart.      |
| Self-hosted remote agent  | V0.9 scheduled option          | User-owned node first; no default VeriSilo public cloud or implicit data upload.                            |
| macOS/Linux               | not yet supported              | Deferred until Windows release behavior and installer safety are validated.                                 |
| Multiple concurrent Silos | not yet supported              | Intentionally blocked by the runtime manager in V0.1.                                                       |
