# Transport Fingerprint Coherence Diagnostic — Managed Camoufox (TLS / HTTP2 / QUIC)

- Lane / branch: `qa` / `agent/qa/transport-fingerprint-d96e42`
- Baseline: `origin/baseline/dev` = `0d742906881263796d86018342699046c5e1b0cd` (exact-equal verified at task start)
- Date: 2026-09-13 (UTC timestamps in all logs)
- Engine under test: installed signed Camoufox package `152.0.4-beta.28` (engineId `camoufox`,
  cms-detached-sha256, keyId `57f3b44c…93`), staged into this worktree's dev resources tree and
  activated through the product's own `ensure_builtin_package` path during Silo creation
- Vault: task-local `qa-transport-fingerprint-d96e42`
- Scope: **read-only diagnostic.** No browser, Host, Core, or network product code was modified.

## Stop condition

**`TRANSPORT_COHERENCE_NO_CONTROL_BLOCKER_FOUND`.**

Every transport layer actually observed (TLS ClientHello, HTTP/2, QUIC attempts) is a genuine
Firefox/Gecko-family implementation and is coherent with the declared website-visible identity
(Firefox 152 family on Windows). No cross-layer contradiction was found. What the diagnostic did
confirm is exactly the alternative the task anticipated: the browser transport is coherent on its
own, while VeriSilo has no formal verifier/evidence surface for it (see VERIFIER_GAP below).

## Method (provenance chain)

All browser traffic was produced by real Managed Camoufox sessions launched through the product
path (dev desktop instance + `verisilo-cli` against the task vault), not by curl/Node/Python
clients. curl, Node, Python-ssl, and headless Edge were used **only as labeled control probes**
against the same endpoint to prove the observer distinguishes families; their captures are
excluded from all browser-fingerprint claims.

- Observation endpoint: loopback-only TLS terminator (Python `ssl`, self-signed test CA, SANs
  `localhost`, `localtest.me`, `127.0.0.1`, `::1`) that parses the raw ClientHello before
  handshaking, then either serves HTTP/1.1 or relays the decrypted plaintext HTTP/2 stream to a
  Node `http2` h2c logger that records the client's initial SETTINGS and header order. A UDP
  listener on 8447 recorded QUIC Initial datagrams (observation only; it never responds). A local
  HTTP-CONNECT test proxy recorded CONNECT targets for the FixedProxy path.
- Browser navigation was driven with the product's own Host `page` commands (`goto` / `click` /
  `evaluate` / `screenshot`), accepting the per-session certificate exception through the
  browser's own error page (shadow-DOM click of `#advanced-button` + `#exception-button`).
- No experimental browser switches were enabled for QUIC; the endpoint merely advertised
  standard `Alt-Svc: h3=":8447"` like ordinary websites do, and the browser's reaction was
  observed.
- No external fingerprint endpoint was used. External traffic during the diagnostic was limited
  to the managed browser's own launch probes (product behavior, e.g. `ipwho.is`,
  `api.ipify.org`) which tunneled through the local test proxy into the machine's pre-existing
  Clash instance, plus one public DNS name (`localtest.me`, A→127.0.0.1) used to exercise the
  proxy path. No Silo secrets, Artifacts, or personal data left the machine.

Observed identities (from the product's own recheck/`identity` output and from in-page JS):

- Silo `transport-diag-direct` (balanced-en-us): UA `Mozilla/5.0 (Windows NT 10.0; Win64; x64;
  rv:152.0) Gecko/20100101 Firefox/152.0`, platform `Win32`, oscpu `Windows NT 10.0; Win64; x64`,
  en-US, TZ America/New_York, 16 cores, UA-CH absent (Firefox behavior).
- Silo `transport-diag-direct-2` (balanced-zh-cn): same UA family; zh-CN, Asia/Shanghai, 8 cores.
- Silo `transport-diag-proxy` (balanced-en-us, fixed proxy): same UA family; en-US,
  America/New_York, 12 cores.
- Error-page chrome confirms build `Firefox/152.0.4-beta.28` / `WINNT`.

## Results

### TLS — ClientHello

- **observed** = yes. 15 browser ClientHello captures across: same-session loads on four origins
  (`127.0.0.1:8443/8444/8445/8446`), one SNI variant (`localhost:8443`), cold restart of the same
  Silo/Profile/Artifact, a second Silo with a different identity seed, and the proxied path
  (`localtest.me:8443` via CONNECT tunnel). Direct evidence files:
  `runtime/tls-observations.jsonl` (event `client_hello`), negotiated cipher suites logged per
  connection.
- **stable** = yes, at three distinct levels:
  - **Constant across every capture** (the fingerprint core): 16 cipher suites in exact
    NSS/Firefox order `0x1301 0x1303 0x1302 0xc02b 0xc02f 0xcca9 0xcca8 0xc02c 0xc030 0xc00a
    0xc013 0xc014 0x009c 0x009d 0x002f 0x0035` (note `0x1303` before `0x1302` and the legacy
    `0xc00a` — NSS orderings that Chromium does not produce); fixed extension order
    `extended_master_secret, renegotiation_info, supported_groups, ec_point_formats,
    session_ticket, alpn, status_request, delegated_credentials, signed_certificate_timestamp,
    key_share, supported_versions, signature_algorithms, [compress_certificate], ECH(0xFE0D)`
    (with `server_name` first when present, `pre_shared_key` last on resumption);
    supported_groups `X25519MLKEM768Draft06, x25519, secp256r1, secp384r1, secp521r1, ffdhe2048,
    ffdhe3072` (ML-KEM hybrid first, no GREASE groups); 11-entry NSS signature_algorithms list;
    ALPN `h2, http/1.1`; supported_versions `TLS 1.3, 1.2`; `psk_key_exchange_modes=[psk_dhe_ke]`;
    and **zero GREASE anywhere** (Gecko does not GREASE — Chromium does).
  - **Legitimate session-variable values** (not drift): `pre_shared_key` appears only on TLS 1.3
    session-resumption handshakes (ja3 `0761506b…` vs full-handshake shapes); SNI present only
    for hostname origins; and the `compress_certificate` extension offer alternates per
    connection (parallel connections within the same second were observed with and without it;
    ja3 variants `859fa094…`, `6447ab08…`, `dfc1768f…` differ *only* by this extension). TLS
    random and key-share contents change per handshake by design and were excluded.
  - **Cross-boundary stability**: same ja3 shapes recur across cold restart, a second Silo with a
    different identity, and the proxied path. Transport values are engine-level constants and do
    **not** follow per-Silo identity variation (CPU/locale/timezone differ per Silo by design;
    the ClientHello does not).
- **coherent with declared Firefox identity** = yes. The declared UA/platform is Firefox 152 /
  Win32; the transport shows a genuine NSS/Firefox 152 ClientHello including current Gecko
  features (TLS 1.3, ML-KEM hybrid key share `X25519MLKEM768Draft06` 1216 bytes, ECH offer,
  delegated_credentials, certificate compression) and none of the Chromium markers (no GREASE, no
  ALPS/application_settings, different cipher and extension ordering). Controls confirm the
  observer separates families: headless Edge produced Chromium-shape handshakes (GREASE-varying
  ja3 across parallel connections) and a Node client produced a 52-cipher handshake.
- **direct evidence** = `runtime/tls-observations.jsonl` (raw ClientHello hex + full parse +
  ja3 string/md5 per capture), `runtime/screenshot-*.png`, tooling in `tools/tls_observer.py`.

### HTTP/2

- **observed** = yes. ALPN negotiated `h2` on every successful browser load (7 completed h2
  sessions: 8444, 8443×2, 8445, 8446, localhost:8443, localtest.me via proxy tunnel). One early
  session's h2 frames were lost to a tooling log-file handle issue; the value was reconfirmed on
  all later sessions (honest coverage note).
- **stable** = yes. Client initial SETTINGS identical in every session:
  `HEADER_TABLE_SIZE=4096, ENABLE_PUSH=true, INITIAL_WINDOW_SIZE=65535, MAX_FRAME_SIZE=16384,
  MAX_CONCURRENT_STREAMS=100, MAX_HEADER_LIST_SIZE=2^32-1, MAX_HEADER_SIZE=2^32-1,
  ENABLE_CONNECT_PROTOCOL=false` — the known Gecko/neqo SETTINGS shape (Chromium differs, e.g.
  large initial window via WINDOW_UPDATE and no ENABLE_PUSH=true). Pseudo-header order constant:
  `:method, :path, :authority, :scheme` (Chromium sends `:method, :scheme, :authority, :path`;
  the Node control sent `:path, :method, …`).
- **coherent with declared Firefox identity** = yes — same conclusion as TLS, independently
  corroborated; request headers carry exactly the declared UA (`request_headers` capability
  applied), Firefox-style Accept lists, no Chromium header shapes.
- **direct evidence** = `runtime/h2-observations.jsonl` (per-session remoteSettings + full
  ordered header blocks).

### QUIC / HTTP-3

- **observed** = yes, at attempt level. After any page load that advertised
  `Alt-Svc: h3=":8447"`, the managed browser sent QUIC **v1 Initial datagrams** (version field
  `0x00000001`, 1252-byte padded Initials, 8-byte DCIDs, long-header packets under header
  protection) to the advertised UDP port — 4 separate attempt bursts from 3 distinct browser
  sessions (initial session, cold-restarted session, second Silo), ~34 datagrams total, with
  retransmission backoff visible. The browser therefore ships with its QUIC/HTTP-3 stack
  **enabled** (Gecko/neqo defaults; Host code contains no http3/quic pref manipulation — verified
  by grep) and actively probes h3 upgrades like ordinary Firefox. No QUIC server existed in the
  observation chain, so a completed h3 transport session was **not** observed and is not claimed;
  on the proxied path (HTTP CONNECT) no QUIC attempt occurred, consistent with Firefox not
  upgrading to h3 through an HTTP proxy.
- **stable** = n/a at fingerprint level (no QUIC handshake parameters captured; not honestly
  claimable from this chain). Attempt behavior is stable across sessions.
- **coherent with declared Firefox identity** = yes — h3 probing is current-Firefox default
  behavior; nothing about it contradicts the declared Firefox 152 identity.
- **direct evidence** = `runtime/tls-observations.jsonl` (events `quic_datagram`), tooling
  `tools/tls_observer.py` (UDP listener).

### DNS

- **observed** = partially, proxied path only. On the FixedProxy path the browser handed the
  **hostname** (`localtest.me:8443`) to the routing chain — the CONNECT line arrives at the test
  proxy with the hostname intact, and resolution was performed by the test proxy itself
  (`upstream_resolution` events). The desktop's loopback relay likewise forwarded the hostname
  without resolving it. Direct observation, honestly labeled: on proxied sessions the browser
  delegates name resolution to the proxy chain; neither the browser nor the VeriSilo relay
  resolves upstream hostnames locally.
- **not observed** = the browser's own resolver behavior on the direct path. All direct-path
  navigations used IP-literals or `localhost` (no DNS involved), and this machine's system DNS is
  intercepted by the user's Clash fake-ip mode (198.18.0.0/8), which would make any passive DNS
  observation describe Clash, not the browser. Per the task rule, this is recorded as
  NOT_OBSERVED rather than inferred.

## Classification

| Layer | Classification | Note |
| --- | --- | --- |
| TLS ClientHello | **COHERENT_AND_OBSERVED** | Genuine Firefox/NSS 152 shape, stable core, session-variables explained; plus a standing **VERIFIER_GAP** (below) |
| HTTP/2 | **COHERENT_AND_OBSERVED** | Gecko SETTINGS + pseudo-header order, stable across every session and path |
| QUIC | **COHERENT_AND_OBSERVED** (attempt level) | QUIC v1 Initials observed from the managed browser; h3 session completion NOT_OBSERVED (no QUIC server in chain) |
| DNS | **NOT_OBSERVED** (direct browser resolver) | Proxied-path hostname delegation observed; direct-path resolver behavior had no honest capture path on this machine |
| Proxy (FixedProxy path) | **COHERENT_AND_OBSERVED** | Proxy does **not** terminate TLS: ClientHello with SNI `localtest.me` observed end-to-end from the browser through the loopback relay + CONNECT tunnel; same ja3 shapes as direct |

## Capability-matrix implications (VERIFIER_GAP, no contradiction)

- The declared capability state (`apps/desktop/src-tauri/src/engine.rs:2664-2671`) marks
  `TlsClientHello` and `Quic` as `Unavailable` with reason *"No signed controlled build plus
  direct ClientHello/QUIC observation evidence is configured."* The installed engine package
  manifest likewise declares neither capability. Both declarations remain **honest**: they claim
  no control, and the product had no observation evidence — until now this diagnostic supplies
  exactly the missing observation evidence (on this host, this package, this date).
- New factual inputs for the owning layer:
  1. The engine's transport is *natively* coherent with the Firefox identities VeriSilo projects
     — TLS/HTTP2/QUIC-attempt all match the declared family without any spoofing. This is a
     strength, not a defect.
  2. Transport values are engine-level constants and intentionally not identity-derived. Two
     Silos on one machine emit byte-identical ClientHello cores (ja3-identical). Websites can
     correlate Silos through identical transport fingerprints; whether that matters is a product
     decision, not a coherence failure.
  3. QUIC is *enabled and probing* by default. `DesiredQuic` is schema-only by design (anything
     other than `browser_default` is rejected as an unsupported control, `engine.rs:4752-4757`),
     so today no identity can ask for QUIC-off, and no evidence pipeline observes it. If a
     future requirement wants QUIC under identity/network control or evidence, that is new
     product work — out of scope here, no recommendation implemented.
- Minimal owning-layer follow-up (recommendation only, not implemented): when a future task
  wants to close the verifier gap, the natural seam is a signed verifier/observation step that
  records a ClientHello/SETTINGS digest into Runtime Evidence for the managed session — reusing
  the existing capability `transition → Verified` machinery. This diagnostic's JSONL format is
  directly reusable as the observation format.

## Residual boundaries

- All conclusions are bound to: this host (Windows 11, this machine's Clash-intercepted DNS),
  this engine package (`152.0.4-beta.28`, the currently installed signed package), and
  2026-09-13. They are runtime observations, not product gates; nothing here changes any
  capability state or claim.
- HTTP/2 fingerprint coverage is limited to what Node's http2 API exposes (initial SETTINGS,
  header order/content). WINDOW_UPDATE/PRIORITY-frame patterns were not captured; no claim is
  made about them.
- QUIC: no h3 transport completion, and no QUIC ClientHello/transport-parameter capture (would
  require a QUIC server; not in scope). "QUIC attempted" is proven; "QUIC works end-to-end" is
  not claimed.
- The `compress_certificate` offer alternation mechanism (per-connection randomization vs NSS
  session-state gating) was not determined from external observation; both shapes are genuine
  browser emissions (captured in parallel within the same second).
- Camoufox's cert-error flow required per-origin session exceptions (through the browser's own
  error page). No CA was imported into the browser profile, the Windows store, or any managed
  profile — the browser trust boundary is untouched.

## Environment & privacy notes

- FixedProxy test used a local HTTP-CONNECT test proxy (`tools/connect_proxy.py`) pinned to
  resolve the diagnostic name `localtest.me` to `127.0.0.1` because this machine's system DNS
  returns Clash fake-ip addresses; the pinning decision is logged in the proxy evidence. The
  silo's launch probes tunneled through this test proxy into the machine's pre-existing Clash
  instance (observed domains are in `runtime/proxy-observations.jsonl`; none carry Silo data).
- No external fingerprint service was contacted by this diagnostic. The only external DNS name
  introduced was `localtest.me` (public A→127.0.0.1 service).
- Dev-machine resources created for this task (not committed): 1.1 GiB engine package staging
  copy at `apps/desktop/src-tauri/target/verisilo-managed-browser-resources/` (required for the
  dev instance's `ensure_builtin_package`; derivable from the installed package), task vault
  `qa-transport-fingerprint-d96e42` with three diagnostic Silos (vault passphrase intentionally
  not persisted anywhere), and a 10-day self-signed test CA + leaf (SANs `localhost`,
  `localtest.me`, `127.0.0.1`, `::1`; openssl-generated at run time, kept outside the committed
  tree, trusted by nothing — the managed browser only accepted per-session exceptions through
  its own error page).

## Evidence index

| File | Content |
| --- | --- |
| `runtime/tls-observations.jsonl` | 15 browser ClientHello parses (full hex, ja3 string/md5, sha256), negotiated TLS/ALPN per connection, QUIC Initial datagrams, control probes (Edge/Node/curl) |
| `runtime/h2-observations.jsonl` | 8 h2 sessions (7 browser + 1 Node control): initial client SETTINGS, ordered headers |
| `runtime/proxy-observations.jsonl` | CONNECT targets, resolutions, tunnel establishment for the FixedProxy path |
| `runtime/screenshot-proxy-silo-localtest.png` | Managed browser (proxy Silo) rendering the diagnostic page over the CONNECT tunnel |
| `runtime/screenshot-session1-cert-error-page.png` | Camoufox's own cert-error page (untrusted-cert flow, build banner) |
| `tools/tls_observer.py`, `tools/h2_relay_server.js`, `tools/connect_proxy.py`, `tools/report_page.html`, `tools/navigate.sh` | Read-only observation tooling (test control plane) |

Browser capture timeline (UTC): session 1 `09:26:57–09:43:52` (origins 8443/8444/localhost:8443,
includes TLS-1.3 resumption captures), cold restart `09:44:01–09:45:08` (port 8445), Silo 2
`09:49:21–09:49:43` (port 8446), proxy Silo `09:52:15–09:52:17` (localtest.me tunnel), controls
`09:53:49` (headless Edge), `09:54:09` (Node h2 client).
