# Threat model

## Scope of this baseline

This threat model applies to the implemented stock Chrome/Edge Silo, local proxy/Mihomo path, Native Messaging Host, optional Companion, V0.7 adapter/package control plane, V0.8 local-environment controllers, and V0.9 desktop/Remote-Agent control plane. It distinguishes those executable control planes from external artifacts and machines: no trusted controlled-engine package, legal VM image, real remote browser Provider, media stream, production certificate, or real Windows/WAN acceptance result is bundled merely because its interface exists.

## Protected goals

- Prevent accidental sharing of browser-managed state between separately created Silos.
- Avoid modifying a user's default Chrome or Edge profile.
- Keep Silo metadata and secrets out of page contexts and out of plain-text exports.
- Keep proxy credentials and Mihomo Controller secrets out of browser arguments, logs, and the plaintext Vault envelope.
- In required-proxy mode, revoke the exact runtime relay and its credentials when the upstream endpoint, user-run Mihomo process, binding/config, or time-bounded exit evidence fails; never repair that runtime by selecting `DIRECT` or another node.
- Prevent a controlled-engine package, guest script, local backend or remote caller from turning typed configuration into a shell command or caller-selected executable path.
- Keep long-term Silo seed material out of browser arguments, environment variables, page contexts and external engine stdin; only a short-lived session-bound derivative may cross the engine bootstrap boundary.
- Authenticate self-hosted remote nodes with PKI plus a mandatory certificate/SPKI pin, authenticate desktop requests with a short-lived credential, and make replay/rollback state survive restart.
- Require user confirmation and bounded authorization for remote cost, destruction, human session, automation and input operations.
- Make browser, extension, proxy, and page-observation limits visible to the user.

## Explicitly out of scope for the stock layer

- Changing the real device, operating system, GPU, OS font installation, or the stock browser's TLS/HTTP/2/HTTP/3 implementation. A future controlled engine, VM, or remote backend may expose a different, explicitly identified environment only after direct per-capability verification; it still must not describe field rewriting as real hardware replacement.
- Bypassing fraud controls, account restrictions, or website security controls.
- A guarantee that a proxy prevents every browser-version-specific DNS, extension, OS, LAN, or future network fallback path; each path needs separate evidence.
- Generic modification of third-party Worker, SharedWorker, or Service Worker execution contexts.
- Protection against an attacker who controls the local operating system, browser binary, or installed extensions.
- Trusting an unsigned/unpinned third-party controlled-engine package, unknown VM image, or arbitrary remote Provider. Those components remain disabled or unavailable until the operator supplies reviewed, fixed artifacts.
- Treating a Provider's encrypted-volume or guest-network attestation as cryptographic proof against a malicious Provider. A fixed hash establishes which Provider ran; independent guest/runtime acceptance is still required.
- Availability against a volumetric network attack. The V0.9 Agent has strict size/deadline bounds but an internet-facing deployment still requires operator-owned connection admission and rate limiting.

No current or future layer promises arbitrary device impersonation, fraud-control bypass, or undetectability. TLS and QUIC may move from “not controllable in stock” to a narrower engine/backend capability only when a real ClientHello or protocol observation proves the exact claim.

## Trust boundaries

- Web pages and content-script inputs are untrusted.
- MAIN-world code is observable by the page and never receives vault secrets.
- Browser profile contents are owned by Chrome or Edge and are not copied into the VeriSilo vault.
- The random loopback relay is trusted with upstream proxy credentials only while the Vault is unlocked and an uncompromised Silo runtime is launching/running. The browser sees only its ephemeral local endpoint. A terminal health/drift/evidence failure closes that exact listener, shuts down tracked connections with a fixed bound, and drops its credential material; refresh never reopens it. The endpoint is still a credential-hiding bridge, not a same-user authorization boundary: another local process that discovers the port can use it while active. The relay caps concurrent clients at 64, but stronger per-browser isolation would require an OS-enforced broker/ACL/WFP design.
- An external Mihomo Controller is accepted only on loopback. Required mode additionally pins `GLOBAL`/`global`, a non-`DIRECT` node, the SOCKS listener port and a redacted config snapshot for the exact runtime. Its subscription, node provider, remote exit and core binary remain outside VeriSilo's trust boundary; API readback is not packet-level or real-Windows proof.
- IP/DoH verification providers are third parties contacted only after the disclosed user action. Their response is evidence for that request, not a trust anchor or DNS-path proof.
- A Native Messaging Host accepts only the browser-provided allowed extension origins configured by the per-user installer.
- A controlled-engine package and its signer are separate supply-chain trust boundaries. Production accepts only the frozen manifest schema, canonical files, exact hashes and a pinned signing certificate; an empty production signer set means the engine stays unavailable.
- WSL, Windows Sandbox and Hyper-V are separate execution boundaries with different persistence, device, networking and administrator properties. The desktop controller and signed fixed scripts cannot prove a guest image or hypervisor is trustworthy.
- The V0.9 desktop stores the node pin, client credential, remote binding, sequence and authorization metadata in the encrypted Vault. The WebView can submit an explicit pairing token but never receives the issued client credential back.
- A self-hosted Remote Agent is trusted to enforce lifecycle and TTL policy. Its fixed local Provider is additionally trusted to create the claimed VM/process, encrypted volume, browser, network and deletion result. The repository's default unavailable Provider makes no such claim.
- Screen-channel metadata is an authorization object, not a media stream. Any future transport and decoder add separate confidentiality, integrity, resource-exhaustion and input-injection boundaries.

## V0.7–V0.9 abuse cases and controls

| Abuse case                                              | Current control                                                                                                                                                       | Residual gate                                                                                                          |
| ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Manifest traversal, package substitution or downgrade   | Strict schema, canonical package root, exact executable/file hashes, CMS detached signature, signer pin, version lock, update receipt, rollback and emergency disable | Real signed engine package and signer lifecycle                                                                        |
| Seed or stable identifier leaks into process inspection | Long-term seed remains in Vault; a session/site/adapter-bound short token is framed on stdin and zeroized                                                             | Real engine must prove it does not persist or expose the bootstrap                                                     |
| Controlled-engine failure silently opens stock Chrome   | Selected controlled adapter fails closed; stock is a separate explicit Silo setting                                                                                   | Windows launch/rollback acceptance with the real artifact                                                              |
| Guest script becomes arbitrary command execution        | Backend operations map to fixed scripts/subcommands and typed UUID/config fields; no shell string concatenation                                                       | Authenticode and real Windows privilege-boundary review                                                                |
| Required proxy silently falls back to host network      | No `DIRECT` bypass; exact-runtime relay/credential revocation on Controller, node, config, listener or exit-evidence failure; terminal latch requires a new launch    | Real Windows Chrome/Edge plus WSL/VM DNS, WebRTC, QUIC and outage tests; Controller readback alone is not packet proof |
| Same-user process borrows the credential relay          | Random loopback-only port, Silo-lifetime shutdown, handshake deadlines and a 64-client cap reduce exposure/resource exhaustion                                        | Stock Chromium cannot present an unexposed per-process SOCKS secret; OS-enforced process ACL/WFP remains unsupported   |
| Remote endpoint interception or downgrade               | HTTPS-only, ordinary PKI/hostname validation, mandatory cert/SPKI pin, no redirect or ambient proxy                                                                   | Real certificate rotation and hostile-network tests                                                                    |
| Stolen/replayed pair or control request                 | Interactive one-time token, short credential, timestamp window, request UUID, nonce, monotonic client/server sequence and durable ledgers                             | Endpoint compromise and OS-level secret theft remain out of scope                                                      |
| TTL or deletion state lost on crash                     | Atomic state replacement, file and directory sync, poisoned state after ambiguous commit, provider-bound deletion proof                                               | Real provider crash/power-loss and disaster-recovery exercise                                                          |
| Automation takes control from the user                  | Separate scoped authorization, explicit approval, bounded expiry and human-session priority                                                                           | Real input/media Provider and UI safety testing                                                                        |
| Slow client/provider starves Agent                      | 64 KiB/header bounds, 15-second connection deadline, serialized mutation and documented outer rate-limit requirement                                                  | Production reverse proxy/admission policy and load test                                                                |

V0.7–V0.9 source and state-machine tests reduce implementation risk; they do not convert an absent external artifact or environment into runtime evidence. The final completion audit must continue to use “部分实现” or “外部条件阻塞” for those gaps.
