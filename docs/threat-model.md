# Threat model

## Protected goals

- Prevent accidental sharing of browser-managed state between separately created Silos.
- Avoid modifying a user's default Chrome or Edge profile.
- Keep Silo metadata and secrets out of page contexts and out of plain-text exports.
- Keep proxy credentials and Mihomo Controller secrets out of browser arguments, logs, and the plaintext Vault envelope.
- In required-proxy mode, prevent intentional `DIRECT` fallback when the upstream endpoint or user-run Mihomo process fails.
- Make browser, extension, proxy, and page-observation limits visible to the user.

## Explicitly out of scope

- Device, operating-system, GPU, font, TLS, HTTP/2, HTTP/3, or QUIC impersonation.
- Bypassing fraud controls, account restrictions, or website security controls.
- A guarantee that a proxy prevents every browser-version-specific DNS, extension, OS, LAN, or future network fallback path; each path needs separate evidence.
- Generic modification of third-party Worker, SharedWorker, or Service Worker execution contexts.
- Protection against an attacker who controls the local operating system, browser binary, or installed extensions.

## Trust boundaries

- Web pages and content-script inputs are untrusted.
- MAIN-world code is observable by the page and never receives vault secrets.
- Browser profile contents are owned by Chrome or Edge and are not copied into the VeriSilo vault.
- The random loopback relay is trusted with upstream proxy credentials only while the Vault is unlocked and a Silo is launching/running. The browser sees only its ephemeral local endpoint.
- An external Mihomo Controller is accepted only on loopback. Its subscription, node provider, remote exit and core binary remain outside VeriSilo's trust boundary.
- IP/DoH verification providers are third parties contacted only after the disclosed user action. Their response is evidence for that request, not a trust anchor or DNS-path proof.
- A Native Messaging Host accepts only the browser-provided allowed extension origins configured by the per-user installer.
