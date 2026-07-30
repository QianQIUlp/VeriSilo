# VeriSilo V0.9 self-hosted Remote Agent

This crate contains a TLS-only, operator-run control-plane service. The V0.9
hardened persistence target is Unix/Linux: it uses owner-only `0600` state,
`flock`, same-directory atomic rename, file and directory `fsync`. A non-Unix
binary exits immediately instead of claiming equivalent durability.

The service is deployable without a provider artifact by using the checked-in
`provider.mode = "unavailable"` example. That mode honestly advertises all nine
lifecycle operations as unavailable; it does not simulate a VM, browser,
encrypted volume, network evidence, logs, deletion proof, or screen stream.

## Build and local files

Build the operator binary from the repository root:

```sh
cargo build --release \
  --manifest-path crates/verisilo-remote-backend/Cargo.toml \
  --bin verisilo-remote-agent
```

Create dedicated, non-symlinked configuration and state directories owned by
the service account. Copy `verisilo-remote-agent.example.json` to an absolute
path and replace every example path and node disclosure. The configuration file
must not be group/world writable. The unencrypted PEM private-key file must be
exactly mode `0600`; existing auth and Agent state files must also be `0600`.
All configured paths are absolute and may not contain `.` or `..` components.
The auth store enforces its own canonical path, lock and mode. The executable's
strict configuration gate checks an existing Agent state/backup for canonical,
non-symlinked `0600` files before constructing the durable Agent store. State
commits sync the new file and parent-directory rename; interrupted backup
renames are recovered on reopen. Any uncertain durability result poisons that
in-process store and requires restart instead of reusing stale counters.

The certificate file must contain only a PEM leaf-first certificate chain, and
the key file exactly one unencrypted PKCS#8, RSA, or EC private key. Certificate
issuance, DNS, renewal and firewall policy remain operator responsibilities.
The desktop still performs ordinary PKI validation plus its configured pin.
The Agent never opens an HTTP fallback or a certificate-challenge listener.

## Pair and serve

Generate a one-time pairing token while the daemon is stopped:

```sh
target/release/verisilo-remote-agent init-token \
  --config /etc/verisilo-agent/server.json \
  --lifetime-seconds 300
```

`init-token` refuses redirected stdout. It prints the token ID, expiry and the
plaintext secret to the interactive terminal once; disk receives only a
domain-separated SHA-256 digest. Transfer those three values through the
operator's chosen secure channel and complete the explicit pairing action in
the desktop before expiry. Start the listener with:

```sh
target/release/verisilo-remote-agent serve \
  --config /etc/verisilo-agent/server.json
```

To invalidate every issued short-lived control credential, stop the daemon and
run:

```sh
target/release/verisilo-remote-agent revoke-all \
  --config /etc/verisilo-agent/server.json
```

The auth-state lock deliberately prevents `init-token`, `revoke-all`, and a
second daemon from mutating the same file concurrently.

## Fixed provider mode

To enable real operations, replace the provider object with strict local
configuration of this shape:

```json
{
  "mode": "stdio",
  "executablePath": "/opt/verisilo-agent/provider",
  "executableSha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "capabilities": []
}
```

Populate `capabilities` with each of the nine operations exactly once and mark
only implemented behavior available. `create` cannot be available unless
`destroy` is also available, because TTL maintenance needs a real deletion
path. The executable is launched directly with one fixed argument after its
SHA-256 is rechecked; no shell, caller path, arbitrary argument, or remote URL
is accepted. Browser-profile encryption and the truth of VM/network evidence
remain obligations of that locally installed provider and guest agent.

The provider `deleted` response is strict: `resourceDeletions` must contain
exactly one `compute_instance`, `persistent_volume`, `snapshot`, and
`ephemeral_key` item. Each item has `kind`, `status`, and an ID when status is
`deleted`; only `snapshot` may be `not_applicable`, in which case its ID must be
omitted. Compute, volume, and key IDs must equal the environment record. Legacy
`resourcesDeleted` strings and `volumeKeyDestroyed` booleans are rejected
fail-closed rather than treated as deletion evidence.

## Listener and deployment boundary

The listener accepts only one `POST /` HTTP/1.1 request per rustls connection,
strict `application/json`, protocol header `1`, exact `Content-Length`, and at
most 64 KiB. It rejects chunking, upgrades, duplicate headers, cookies, proxy
authorization and malformed bearer tokens. Header size is 16 KiB and the TLS +
HTTP read deadline is 15 seconds. Responses always request connection close and
do not log parser, token, credential, provider, or request-body details.

Auth and Agent mutations plus provider calls are serialized. The listener polls
accept every 100 ms and runs TTL maintenance at startup and every 30 seconds;
provider failures remain eligible for a later bounded-frequency retry, while a
durable-store failure stops further provider mutations until restart. One slow
provider operation can still delay other requests, and this daemon has no
distributed rate limiter. Put an untrusted-WAN deployment behind operator-owned
connection admission/rate limiting and monitor resource use without recording
headers or bodies.

`openScreen` returns only authenticated channel metadata. V0.9 does not include
or claim an implemented media stream, real WAN test, VM/browser provider,
certificate automation, system service package, or end-to-end screen/input
deployment.
