# Camoufox FP3 network identity contract

## FP3-0 scope

FP3-0 freezes a reproducible **configured input** for network-bound website identity. It does not
launch a browser or promote any value to `applied`, `observed` or `verified`.

| Concern | Owning lifecycle | FP3-0 state |
| --- | --- | --- |
| Proxy endpoint, credentials and required-routing policy | Silo `NetworkProfile` / Vault | Managed for Standard Silo; unavailable to Camoufox Host |
| Public exit and provider Geo observation | Runtime Evidence | `extension_asserted` observation only; not an Artifact or route receipt |
| Website timezone, locale, Geo coordinates and WebRTC public-address target | Resolved Identity Artifact v6 | Configured and immutable with the Artifact |
| Browser application of those values | Camoufox Engine runtime | Not observed in FP3-0 |
| Actual browser Geo and ICE/STUN results | Runtime Evidence | Unavailable in FP3-0 |

Profile storage, the Identity Artifact, Engine binding, Network Policy and Evidence remain separate
lifecycles. Artifact v6 keeps the existing immutable browser binding, but contains no proxy
endpoint, credential, Profile path, Engine package path or mutable runtime state.

## Versioned input

`NetworkCheckResult` v2 preserves a provider's latitude and longitude as a pair. Historical v1
results remain readable. A missing, non-finite or out-of-range pair is represented as unavailable;
v1 and v2 fields cannot be mixed.

The default Artifact writer remains v5. Opt-in Artifact/Policy v6 changes `timezoneMode` to
`network-bound` and freezes exactly these network identity fields:

- canonical global public IP address;
- two-letter country code;
- known IANA timezone;
- explicit canonical language-region locale;
- finite latitude and longitude;
- exactly one address-family-matching `webrtc:ipv4` or `webrtc:ipv6` value.

Artifact v6 also writes matching timezone, locale, Geo and WebRTC Camoufox configuration. Strict
validation rejects missing, extra or inconsistent fields. It does not alter DNS or routing policy.

## Evidence rules

- A Direct `NetworkProfile` requests no managed browser route. Launch success therefore leaves
  `browserRouting` at `not_requested`; it is not a route-application receipt.
- A provider's IP and Geo response can be `extension_asserted` and time-bounded. It does not prove
  browser Geolocation output, actual WebRTC candidates, DNS routing or proxy enforcement.
- Schema validation, deterministic generation and focused tests prove only that the configured
  input is closed and reproducible.
- `applied` requires a native Camoufox run with a value-specific engine receipt or direct browser
  observation. `observed` and `verified` require their own runtime evidence.

## Gate

FP3-0 is closed when Network Check v2 is backward-readable, Artifact v6 is deterministic and
strict, Direct no longer receives a false `applied` state, and focused contract tests pass.

The next Gate is **FP3-1 native Windows required FixedProxy application/observation discriminator**.
It requires a frozen proxy/network input, Camoufox Host routing support, browser launch and external
network evidence. Until that separately authorized Gate runs, proxy application, Geo accuracy,
browser Geolocation, ICE/STUN behavior, actual DNS path, cross-host replay and `verified:true` remain
unavailable or unverified.

## FP3-1 routing seam

The local routing seam is closed before native execution with these limits:

- Camoufox accepts only Direct(false), or a required HTTP/SOCKS5 `FixedProxy` whose
  `IdentityTemplate.network.proxyRequired` is true and whose Artifact binding is v6. Optional
  FixedProxy, PAC, HTTPS and SOCKS4 remain unavailable.
- The desktop reuses its existing authenticated `ProxyRelay`. The Host receives only
  `socks5://127.0.0.1:<ephemeral-port>`; upstream authority, credential reference and Vault
  credentials never enter the Artifact, serialized Engine plan, Host argv or persisted Host state.
- A proxy launch fails closed unless the verified on-disk Artifact and Policy are v6
  `network-bound`. The Host passes the exact loopback value to Camoufox `launch_options` and echoes
  it only after browser creation. The desktop requires the same value in launch and status results.
- An exact native Host receipt may make `browserRouting=applied`; it is not an exit, Geo, DNS,
  WebRTC or `verified` observation. A later Host status/close failure revokes the exact-runtime relay
  and marks browser routing failed. Fake-Host tests prove transport and cleanup only.
- Camoufox publishes no DNS/QUIC/WebRTC safeguards until those controls are directly established.
  Standard Silo keeps its existing safeguards. Direct remains `browserRouting=not_requested`.

The Host does not add a loopback bypass. Whether the existing local identity probe succeeds through
the required proxy is deliberately left to the native discriminator rather than hidden by a retry,
bypass or engine preference workaround.

## Frozen native discriminator

Before the separately authorized native run, freeze:

1. one reachable required HTTP or SOCKS5 proxy; credentials remain only in Vault;
2. a fresh Network Check v2 made through that proxy, including public address, country, timezone,
   locale and paired coordinates;
3. a deterministic Artifact v6 rebound from the selected v5 source, plus its raw SHA binding;
4. a direct-host public-address negative control distinct from the proxy address;
5. the existing pinned Host/browser package and a bounded observation page for public exit,
   timezone/locale, browser Geolocation and ICE candidates.

The discriminator passes only if the exact Host route binding remains present through status,
browser public exit equals the Artifact address and differs from the direct control, browser
timezone/locale/Geolocation match the Artifact, and ICE evidence contains the Artifact public
address with no contradictory direct public address. Every result records clean Host/process/relay
close. Actual DNS path remains `unavailable` unless separately observed; `verified:true`, cross-host
replay, package shipping and release remain out of scope.
