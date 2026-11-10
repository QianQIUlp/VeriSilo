# V0.9: user-hosted remote environment control plane

V0.9 now includes an executable self-hosted control chain. The Windows desktop pairs with a Linux/Unix Remote Agent that the user runs. It uses HTTPS with ordinary PKI validation plus a certificate or SPKI SHA-256 pin. Client credentials, the remote binding and the monotonic sequence are stored in the desktop's encrypted Vault. The Agent stores authentication and environment control state locally with `0600`, `flock`, same-directory atomic replacement, and file and directory `fsync`.

This chain does not mean "the remote browser is already usable". The repository does not ship a real VM, container or browser Provider, and it has no screen media stream. The default deployment sample declares all nine lifecycle capabilities as `unavailable`. The Agent can honestly publish a capability only after an operator installs and pins a Provider that really implements it.

```text
VeriSilo Desktop
  ├─ Vault: TLS pin, short-lived client credential, Silo binding and monotonic sequence
  └─ reqwest/rustls: PKI + hostname + certificate validity period + mandatory pin
          │  HTTPS POST /, protocol v1, ≤64 KiB
          ▼
User-operated Remote Agent (Linux/Unix)
  ├─ One-time pairing token and short-lived bearer credential
  ├─ TTL, replay ledger, activity, authorization and typed Provider deletion receipt
  ├─ Pinned-SHA-256 stdio Provider boundary
  └─ Provider/Guest (not yet shipped in this repository)
       ├─ VM/process, encrypted volume and browser
       ├─ Network exit, DNS and WebRTC evidence
       └─ Screen and input transport
```

## Trust and transport boundary

The desktop accepts only:

- `ownership: user_self_hosted`;
- an HTTPS origin with no username, password, path, query or fragment;
- a nonzero, lowercase SHA-256 certificate pin or SPKI pin;
- the same authenticated leaf certificate passing the pin after ordinary Web PKI validation succeeds;
- HTTP/1.1, redirects forbidden, reading the system proxy forbidden, bounded connect/read/write timeouts and a 64 KiB limit.

The server opens only a rustls listener. It provides no plaintext HTTP and no certificate-challenge fallback. Each connection accepts only one `POST /`. It requires `application/json`, an exact `Content-Length` and `X-VeriSilo-Protocol: 1`. It rejects chunked encoding, upgrade, duplicate headers, Cookie, Proxy-Authorization and malformed bearer values. The configuration, certificate, private key and state paths must be canonical absolute paths. The configuration must not be group- or world-writable. The private key and existing state must be `0600`.

"Mutual authentication" here is a combination of two mechanisms. The server proves its identity to the desktop through PKI + pin. The desktop proves its identity to the Agent with a short-lived bearer credential issued after pairing. This is not client-certificate mTLS. Certificate issuance, DNS, renewal, firewall and public-entry rate limiting are the self-hosting operator's responsibility.

## Explicit pairing and the Vault

While the Agent is stopped, the operator runs:

```bash
verisilo-remote-agent init-token \
  --config /etc/verisilo-agent/server.json \
  --lifetime-seconds 300
```

The command requires an interactive terminal and refuses to redirect the plaintext token to a file. Disk stores only a domain-separated SHA-256 digest. A token lives at most five minutes and can be consumed only once. The desktop UI requires the user to tick an authorization box and submit the token ID, expiry time and token. A successful response returns the new client credential only once. The Tauri command writes it directly into the Vault and does not return it to the WebView, logs or reports.

The pairing state freezes the server ID, credential ID, node owner, data region, key ownership, cost description, nine capabilities and the client/server sequence. Every request uses a new UUID, nonce, timestamp and a strictly increasing sequence. The response must match the request ID, time window and server ID, and carry a higher sequence. Each lifecycle result also carries `lastActivityAtUnixMs` from the Agent's persistent record. The desktop syncs it to the stable binding and shows it explicitly. Revoking a pairing clears the application credential, the human/automation authorizations and the screen channel metadata in the desktop Vault. The stable Silo binding and the audit receipts are deliberately kept, for restoring after re-pairing or for explicit deletion. They are never described as the remote resource having been deleted. The operator can also stop the Agent and run `revoke-all` to invalidate all server-side credentials.

### Safe TLS pin rotation

Rotating the certificate or SPKI key does not require breaking the stable Silo bindings first. The desktop provides a separate action, “安全轮换” ("Secure rotation"), and requires all of the following:

- the old pairing credential still exists and has not expired;
- the client first completes one fixed rotation authorization, with sequence and nonce, using the old bearer over the PKI HTTPS channel of the old pin;
- the new endpoint and the old endpoint are the same HTTPS origin, and the pins really differ;
- the user separately confirms this rotation;
- a new one-time token issued by the Agent under the new pin, valid at most five minutes, is used;
- the `serverId` in the new pairing response is identical to the old pairing and to every stable binding.

The client first writes only the new token ID into the local replay ledger of the encrypted Vault. It then sends `authorize_tls_pin_rotation` to the old endpoint. This request contains only the old credential ID, the new pin and the token ID. It never contains the token secret. The Agent atomically verifies the old bearer, the request sequence and nonce, and the not-yet-consumed token ID. It persists a single-use challenge, valid at most 60 seconds, bound to the credential, token and new pin. Only after this step passes does the client connect to the new pin and submit, in the pairing request, the same token ID, the token secret and the challenge. The Agent must atomically consume a challenge that matches exactly and has not expired. The `serverId` returned by the new pairing must also match the old authorization and all bindings.

If the old authorization fails, the client never contacts the new endpoint. On a network failure, an Agent rejection, a wrong server, an origin swap, an expired old credential, or a failure of the final Vault commit, the old endpoint, the old credential identity and all bindings stay unchanged. The token ID still cannot be reused. An old request that was already sent with authentication keeps its advanced replay sequence, so the old bearer cannot reuse a sequence number later. On success, the new endpoint, the new pairing and the endpoint of every binding take effect together through one atomic Vault replacement. All local human sessions, automation authorizations and screen channels are cleared at the same time, so the new application credential does not inherit the old bearer capability. The deployment must keep the old-pin authorization and the new-pin pairing reachable in turn within this one-minute window.

## Lifecycle and persistent state

The protocol fixes nine operations:

| Operation          | Extra condition                                                                               | Success evidence                                                            |
| ------------------ | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `create`           | TTL, cost confirmation, network policy                                                        | Pinned binding/environment ID, encrypted-volume attestation, guest evidence |
| `start`            | An existing binding. For required proxy, the evidence is still valid                          | Guest evidence and started state                                            |
| `stop`             | An existing binding                                                                           | stopped state                                                               |
| `pause`            | The Provider explicitly supports it                                                           | paused state                                                                |
| `snapshot`         | The Provider explicitly supports it                                                           | snapshot receipt                                                            |
| `destroy`          | A new deletion needs separate confirmation. Recovering an existing receipt starts no deletion | Provider receipt bound to the server/Silo/binding/environment/volume/key    |
| `configureNetwork` | A fixed policy ID                                                                             | New policy and guest verification evidence                                  |
| `health`           | An existing binding                                                                           | Remote guest health and network evidence                                    |
| `logs`             | 1–200 entries, server cursor                                                                  | Bounded, redacted logs                                                      |

`create` can be published as available only when `destroy` is also available, so that a real cleanup path exists after the TTL expires. The Agent runs a TTL sweep at startup and every 30 seconds. A failed deletion does not forge a receipt and is retried on a later tick. A newly initiated deletion can call the Provider only with `confirmDestroy: true`. `confirmDestroy: false` only queries whether the same binding/environment already has a persisted deletion receipt. An active environment is always rejected. After a TTL, a Provider policy or a user-confirmed deletion, the Agent can repeatedly return the exact same `proofId`, Provider receipt, deletion time, reason and resource list. The client no longer wrongly rejects an old receipt for a "two-minute freshness" rule. Instead, it checks the current pinned server, the proof ID in the persistent environment record, the Silo/binding/environment, the original volume/key and the typed resource dispositions. Only if all of them match does it release the local binding.

The Provider deletion response no longer accepts arbitrary `resourcesDeleted` strings or a single `volumeKeyDestroyed` boolean. The strict wire schema requires each of the following four kinds to appear exactly once. Duplicates, missing entries, unknown kind/status, nil or wrong IDs, and the old fields all fail closed:

- `compute_instance`: `deleted`, and the ID must equal the remote environment ID;
- `persistent_volume`: `deleted`, and the ID must equal the original volume ID;
- `snapshot`: `deleted` + a non-nil ID, or, when there is no snapshot, an explicit `not_applicable` with no ID;
- `ephemeral_key`: `deleted`, and the ID must equal the original key ID.

The desktop calls this “已认证的 Provider 删除回执” ("authenticated Provider deletion receipt"). It proves that the pinned self-hosted Agent returned a Provider statement bound to the local identity. It does not pose as a third-party independent audit, a cloud vendor bill or an externally verifiable destruction proof. The old arbitrary-string deletion state is not migrated leniently. Strict deserialization rejects it and asks the operator to recover or re-check.

The Agent's authentication state and control state are persisted separately. Both reject unknown fields, over-limit collections, a regressed sequence and duplicate request IDs or nonces. A state replacement first syncs a temporary file, then atomically renames it and syncs the parent directory. If an indeterminate persistence error occurs near the commit point, the store enters a poisoned state and refuses further changes until a restart rereads the disk.

Agent state is not a browser Profile and not the encrypted volume itself. It stores only control metadata, authorizations, the replay ledger, activity and authenticated typed Provider deletion receipts. Real Profile encryption and key destruction are still performed by the pinned Provider, and its receipt is not a third-party independent audit.

### Forced detach for disaster recovery

Normal cleanup must keep using `destroy`. The local binding is removed only after the server, Silo, binding, remote environment, volume, ephemeral key and the four typed Provider dispositions have been checked. If the old pin, the credential and the Agent are all unrecoverable, the desktop offers a last resort, “强制分离” ("Force detach"). It requires two independent confirmations:

1. The user confirms that only the local stable binding is removed.
2. The user confirms that the remote environment, volume and other resources may still exist, keep running and keep incurring charges.

Force detach does not touch the network, does not call `destroy`, and does not generate or reuse a deletion proof. It never calls the local removal a remote deletion. The Vault permanently keeps a separate orphan receipt: the receipt, Silo, binding, remote environment, server, the endpoint/pin at that time and the detach time. This receipt allows the local Silo to be permanently deleted afterward. The receipt is not deleted with the Silo. It stays visible in the remote panel, so the user can contact the self-hosting operator for cleanup and reconciliation.

## Provider boundary and network evidence

The production Agent can be configured with one local stdio Provider. Its executable path comes from the operator's `0600` or otherwise controlled configuration. Before launch it is canonicalized again and its full SHA-256 is checked. The Agent launches that file directly with only the fixed `--verisilo-provider-v1`, and exchanges bounded JSON over stdin/stdout. A remote request cannot choose an executable, shell, arguments, file path, image, host, port or URL.

For required proxy, `create`, `start`, `configureNetwork` and `health` can succeed only when all of the following evidence is fresh and correctly bound:

- the exact policy has been executed in the guest;
- the guest has observed the network exit address;
- the guest has observed the DNS resolver and the leak result;
- the guest has observed the WebRTC candidate and the leak result;
- the guest agent version and health check are valid.

Missing, expired, UUID-mismatched, `unavailable` or failed evidence, or a detected leak, must all fail closed. The desktop also checks the stored evidence before sending `start`. That cannot replace the same rule enforced by the server and Provider before the browser process starts.

The repository's default `provider.mode = "unavailable"` generates no VM, volume, network exit, DNS, WebRTC, log, Provider deletion receipt or screen stream. For deployment steps and a strict example, see [`crates/verisilo-remote-backend/DEPLOYMENT.md`](../crates/verisilo-remote-backend/DEPLOYMENT.md).

## Human session, automation and screen input

The desktop and the Agent implement six typed interaction commands: open/close a human session, grant/revoke automation, open a screen channel, and send bounded keyboard/mouse/text events.

- A human session needs explicit user confirmation and takes priority over automation. Automation input is rejected during an active human session.
- Automation must be confirmed separately and limited by scope and validity period. The scope and period returned by the Agent must not exceed the desktop request.
- The authorization, environment and expiry time of a screen channel must be bounded by the parent authorization.
- Input events have no shell, script or arbitrary-command field.

Currently `openScreen` only returns authorized channel metadata. The desktop UI can show, refresh and close the authorization, and can call the typed input commands. It does not decode or render video or audio. The Provider also does not yet ship a real encrypted media and input transport. "The remote desktop picture is usable" therefore remains partially implemented and is not a verified capability.

## Automated evidence and release artifact

The repository's automated tests cover: protocol version, unknown fields and size limits, replay and sequence, pairing token and credential, atomic replacement of all bindings during pin rotation, wrong server/origin/expired credential/transport failure, token replay, final Vault write failure, disaster-recovery orphan receipt, persistence across restart, TTL sweep → retrieving the old Provider receipt without a new deletion confirmation → releasing the binding, forged server/environment/proof/reason, typed resource missing/duplicate/unknown kind/status, last activity, capability negotiation, required-proxy fail-closed, human priority, automation scope, screen/input authorization, the HTTP parser, TLS pin and the pinned Provider bridge.

Commands for re-checking:

```bash
cargo fmt --manifest-path crates/verisilo-remote-backend/Cargo.toml -- --check
cargo test --locked --all-targets \
  --manifest-path crates/verisilo-remote-backend/Cargo.toml
pnpm remote-agent:verify
pnpm release:self-test
```

The `Self-hosted Remote Agent candidate (Linux x64)` workflow builds an unsigned Linux x64 operator candidate. It comes with a deployment sample, SBOM, dependency license evidence, SHA-256 and provenance. It only uploads a CI artifact. It does not deploy a service, does not create a public endpoint, and does not call an unsigned artifact an official release.

## Gates that still need external or later implementation

The current local repository tests cannot forge these items into completion:

1. A reviewed real VM/container/browser Provider with a pinned artifact hash, and its lawful base image.
2. Inside the Provider: a real encrypted persistent volume, a per-environment network stack, a browser Profile, a guest Agent and a deletion-key flow.
3. A real authenticated screen media and input transport. Currently there are only the authorization protocol and channel metadata.
4. The user's domain, DNS, a valid certificate, renewal, firewall and entry rate limiting, plus rotation and rollback drills in a real certificate/SPKI key change.
5. The matrix of pairing, creation, disconnect, restart, concurrency, TTL, proxy failure, evidence, destruction and recovery on a real WAN.
6. An independent security audit of the Agent/Provider/guest boundary and an operational recovery drill.

Until these gates are complete, the UI and release notes must call V0.9 “可执行的自托管控制面 + 待安装真实 Provider” ("an executable self-hosted control plane + a real Provider still to be installed"). They must not call it a delivered cloud browser, a complete remote desktop or an undetectable environment.
