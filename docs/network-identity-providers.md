# VeriSilo network identity and exit providers

## Product principles

Network identity is not an "IP cleanliness" score. VeriSilo splits it into checkable facts: what the Silo is bound to, whether the endpoint is reachable, whether authentication has been verified by protocol, whether the browser has applied the routing, which network exit the current page actually sees, and how far the DNS/WebRTC evidence reaches.

- Local-first by default. VeriSilo does not operate a default public proxy or verification service.
- Users can use their own HTTP/HTTPS/SOCKS proxy, a residential, datacenter or VPS exit, or a Mihomo/Clash-compatible core that they run themselves.
- A long-lived Silo pins an endpoint or a Mihomo node by default. Rotation must be an explicit user change. There is no random background switching.
- "Required proxy" has no `DIRECT` fallback. If endpoint, authentication or Mihomo node binding fails, the Silo refuses to start. If the Controller, Secret, endpoint, node, `GLOBAL`/`global` configuration or exit evidence drifts while running, the loopback relay of the current runtime is revoked, and the browser can only get a network failure.
- IP, DNS, WebRTC, time zone and language each show their own evidence. A public DoH comparison is not mislabeled as proof of a DNS leak.

## Market demand conclusion

In public material, the common basis of mature products is not "automatically buy one VPS per Profile". It is that each Profile/Silo is bound to one proxy exit, owned by the user or provided by the platform, with import, checking, geographic coherence and explicit rotation. See the [GoLogin proxy FAQ](https://support.gologin.com/en/articles/14839275-faq-proxies), [Multilogin proxy FAQ](https://multilogin.com/help/en_US/all-about-multilogin-proxy/multilogin-proxy-faq), [AdsPower global settings](https://help.adspower.com/docs/global_settings), [Dolphin Anty proxy checker](https://docs.dolphin-anty.com/en/working-with-proxies/check-proxy-in-dolphin-anty), [Incogniton proxy integration](https://docs.incogniton.com/proxy-management/integrating-proxies) and [Octo Browser profile settings](https://docs.octobrowser.net/en/profiles/browser-profile-settings/).

VeriSilo adopts the same entry-point requirement that has already been validated. It does not repeat vendor marketing claims about anonymity, reputation or bypassing risk controls. Mihomo only forwards traffic and applies rules. What actually changes the public IP is still the remote node or proxy service the user chooses.

## Current implementation scope

| Stage                                        | Current status                    | Implemented                                                                                                                                                                                                                                                                                                                                   | Explicitly not claimed                                                                                                                                                                                                                                 |
| -------------------------------------------- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Stage 1: fixed proxy                         | Implemented                       | One-line import, a fixed endpoint per Silo, Vault credentials, port/protocol authentication preflight, random loopback relay, fail-closed launch arguments, layered evidence                                                                                                                                                                  | HTTP credentials show verified only when the Companion exit assertion of the same runtime and the relay challenge/accept/forwarded-byte receipt fall in the same check window. The built-in relay does not handle HTTPS/SOCKS4 upstream authentication |
| Stage 2: external Mihomo                     | Usable adapter implemented        | Connects to the user's already-running local Controller, reads selection groups, nodes, recent latency and, where available, Provider update times, encrypts the Controller Secret, pins `GLOBAL`/`global` and the loopback SOCKS port at launch, and lets the browser connect only to a random local relay. Runtime drift revokes that relay | Does not read or host airport subscriptions. Does not bundle a GPL core. A Controller or configuration readback is not proof of actual Windows traffic or network exit. Does not infer region from node names                                          |
| Stage 3: bundled core / WSL / TUN / parallel | Interface and read-only detection | Read-only check of WSL availability and distributions, Provider boundary and release gates                                                                                                                                                                                                                                                    | Mihomo is not yet distributed, system routes are not changed, TUN is not enabled, WSL Chromium is not started, and multiple Silos in parallel are not allowed                                                                                          |

## Stage 1 data flow

```text
User pastes host:port[:user:password]
  ├─ Non-secret endpoint → Silo network configuration (inside the encrypted Vault)
  └─ Username/password → independent random reference → inside the encrypted Vault

Start Silo
  ├─ Check Profile lock and network model
  ├─ Check the proxy endpoint
  ├─ SOCKS5: complete authentication negotiation (credentials can be verified before visiting a site)
  ├─ HTTP: confirm the endpoint is reachable; authentication stays "configured" and is not upgraded by any Companion success
  ├─ Start a random 127.0.0.1 SOCKS5 relay
  ├─ HTTP Basic: relay first obtains a 407 Basic challenge, then retries with the Vault credentials
  ├─ relay keeps only a bounded receipt in memory: relay/runtime, connection sequence number, monotonic time, upstream result, whether bytes were forwarded
  └─ Chrome/Edge receive only the loopback address, not the upstream username or password
```

The HTTP authentication receipt does not store the target, username, password, authorization header or response header. It also never enters logs, the UI, the Vault or exported reports. When an unauthenticated proxy accepts a CONNECT without credentials, that only proves the upstream accepted the connection. It does not prove the configured credentials are valid. If a credentialed request after a 407 still returns 407, it is recorded as `failed`. If the receipt has expired, the runtime does not match, no bytes were forwarded, no public IP was observed, or the relay does not exist, authentication stays `configured`. Authentication is not inferred from a Companion success assertion.

The required-proxy Chrome/Edge launch configuration contains:

- a single proxy address, with no `direct://`;
- `--proxy-bypass-list=<-loopback>`, which cancels Chromium's implicit loopback bypass;
- `--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE <proxy>`, so ordinary page domains are not resolved through host DNS;
- `--disable-quic`, to keep QUIC from bypassing the current TCP proxy path;
- `--webrtc-ip-handling-policy=disable_non_proxied_udp`, to limit non-proxied WebRTC UDP.

These arguments are launch-level protection. They do not mean TLS, DNS or WebRTC have packet-level proof. Chrome SOCKS/DNS behavior is regression-tested continuously against the [Chromium SOCKS proxy design](https://chromium.googlesource.com/website/%2B/refs/heads/main/site/developers/design-documents/network-stack/socks-proxy/index.md) and the [Chromium proxy documentation](https://chromium.googlesource.com/chromium/src/%2B/master/net/docs/proxy.md).

## Stage 2 external Mihomo adapter

The user first imports and updates airport subscriptions in their own Mihomo/Clash-compatible client. VeriSilo connects only to the explicitly entered `http://127.0.0.1:<port>/` or `http://[::1]:<port>/` Controller:

1. `GET /proxies` reads selection groups, the current node, optional alive status and recent latency. It optionally reads the Provider name, node count and update time from `/providers/proxies`.
2. The user pins a selection group and node to the Silo.
3. The Controller Secret is stored encrypted under a random reference.
4. Before each launch, VeriSilo confirms again that the node still exists.
5. `PUT /proxies/<group>` selects the node, and `GET /proxies` reads it back again.
6. required-proxy accepts only the `GLOBAL` selection group, `global` mode and a node that is not `DIRECT`/`REJECT`. It binds the redacted snapshot returned by `/configs` and the chosen loopback `socks-port`/`mixed-port` to this runtime.
7. The browser starts only after the endpoint protocol check succeeds.
8. While running, VeriSilo reads back the Controller, Secret, node and configuration, read-only. If any of them is lost or drifts, then in the same runtime state change VeriSilo closes the relay listener, terminates existing relay connections and revokes the in-memory credentials. It does not switch to host direct connection, reselect a node in the background, or end an unowned process.
9. Once the relay is closed because of a security failure, neither ordinary status refresh nor “复查” ("re-check") reopens the old port. The user must close that browser normally, then explicitly start again and verify again.

This closing behavior also applies to a fixed proxy that is not required but uses a managed relay because of credentials. Once the health or evidence of that managed path fails, the relay closes, and the status and all earlier `verified`/`observed` network evidence drop to failed or unavailable. A proxy that is not required and has no managed relay is not quietly advertised as verified. What fallback its browser itself may use remains a product boundary of that mode, so it must not be described as fail-closed.

The node "region" is evidenced by the country and city returned by the real exit check inside the Silo. Flags, abbreviations and marketing names in node names have no unified API semantics. They are shown as is and are not automatically treated as geographic fact.

This release does not accept a generic "change-IP URL". Such URLs often carry credentials, write state and take arbitrary target requests at the same time. Without a per-Provider domain allowlist, method/response protocol, SSRF protection and audit record, simply saving them and calling them in the background would widen the risk. If added later, the URL and token must go into the Vault, the call must be an explicit user action, and the result must still be re-checked through the real exit inside the Silo. The subscription URL of an external Mihomo continues to be stored and updated by the user's own client, so VeriSilo does not take over subscription secrets a second time.

Remote Controllers are rejected, so VeriSilo does not become a LAN/public SSRF or remote proxy-management entry point. For Controller API behavior, see the [Mihomo API](https://wiki.metacubex.one/en/api/). Nodes and subscriptions remain with the user's own external process.

## Evidence states

The launch card on the desktop shows, in order:

1. Network configuration;
2. Mihomo node binding (if applicable);
3. Proxy endpoint;
4. Proxy authentication;
5. Browser routing;
6. Silo actual exit;
7. DNS path evidence;
8. WebRTC path.

The desktop controller's own IP request cannot replace Silo evidence. The user starts the actual exit check from the Companion inside an already-started Silo. It can confirm the public IP, ASN, region and exit time zone that this browser environment saw for this request, and it gives a consistency suggestion against the browser time zone.

The current “公共 DNS” ("Public DNS") check only queries Cloudflare and Google public DoH for a fixed domain and compares the answers. It cannot see the recursive resolver actually used by the operating system, router, carrier or proxy, so it cannot prove "no DNS leak, poisoning or hijacking". A real DNS path verification needs a unique canary domain that the user self-hosts plus authoritative DNS logs, or a future controlled test inside a VM or remote Agent.

A Companion return is always `extension_asserted`, and the public exit is at most `observed`. If HTTP authentication meets the joint conditions above, the source of the authentication stage is additionally recorded as `relay_observed`. These two sources mean only a local joint observation within the same Silo/runtime and check window. The Native inbox does not independently authenticate the browser process, and the relay cannot see the extension identity. The UI and reports therefore must not describe this as an independently trusted end-to-end proof.

Network Check v2 also keeps the latitude and longitude returned by the Provider within that time limit. A coordinate that is missing, non-finite, out of range or unpaired is uniformly marked unavailable. Historical v1 results remain readable. Coordinates are only an `extension_asserted` Geo observation from an external Provider. They do not prove the output of the browser Geolocation API. A Direct configuration has no managed routing action, so a successful browser start does not upgrade `browserRouting` to `applied`.

The Companion's exit observation has a fixed validity period. An accepted observation that fails or expires is no longer shown as `verified`/`observed`. A required-proxy runtime revokes its relay, and background refresh does not restore it automatically. Rust regressions can prove only the local listener/connection state machine and launch arguments. They do not replace observation of DNS, WebRTC, QUIC and system traffic on real Windows Chrome/Edge.

For WebRTC, the current implementation can verify control of the browser privacy setting and its readback. It has no default public STUN service or packet-level observation, so it does not show “实际 WebRTC 出口已证明” ("actual WebRTC exit proven").

## Why stage 3 is released separately

- The Mihomo project uses GPL-3.0. Bundled distribution needs a separate component boundary, source and license obligations, an SBOM and an audit of the update chain.
- TUN changes system routing, DNS and firewall semantics and usually involves administrator privileges. Uninstall and crash recovery must first prove they do not cut the network or leak.
- A WSL Chromium Provider has to handle distribution installation, GUI, GPU/audio/clipboard integration, Windows/WSL network modes and browser updates. Currently only read-only detection with fixed arguments is allowed. It does not run arbitrary Linux commands.
- Running multiple Silos in parallel needs per-instance ports, processes, Vault unlock lifecycle and exit-evidence ownership to be verifiable. Until these interfaces are frozen, VeriSilo stays at a single Silo.

WSL is a replaceable `EnvironmentBackend` and not a hard VeriSilo dependency. The complete design still follows the [environment roadmap](environment-roadmap.md).

## Release acceptance

- Credentials, the Controller Secret and any future sensitive URL do not appear in launch arguments, logs, plaintext Vault envelopes or exported reports.
- A required-proxy configuration rejects bypass rules, and after the proxy process or endpoint stops, the test page cannot reach the public internet.
- For SOCKS5, a wrong username or password fails strict method negotiation before launch. For HTTP Basic, verification success shows only when all three hold: credentialed acceptance after the 407 challenge, forwarded bytes, and a same-runtime Companion exit observation. A second 407 shows failure. An unauthenticated proxy must not falsely prove credentials.
- The Mihomo Controller accepts loopback only. An unknown group or node, `DIRECT`/`REJECT`, non-`GLOBAL`/`global`, a port mismatch, 401, a timeout, and an inconsistent node or configuration readback all block launch or revoke the exact-runtime relay.
- Automated tests must prove that after drift the old port rejects new connections, existing connections close within a fixed bound, a wrong runtime ID does not affect another relay, forwarding cannot continue after the Controller process exits, and only an explicit new start after the browser is closed can establish a new port.
- The desktop controller check and the in-Silo check use different labels and cannot pass for each other.
- The coverage boundaries of DNS/DoH and WebRTC are consistent across the Chinese and English UI, store listings and exported reports.
