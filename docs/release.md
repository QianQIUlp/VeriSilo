# Release gates

## Windows release candidates

The manual `Windows release candidate (unsigned)` workflow remains a deliberately
separate **unsigned** Windows x64 candidate. It requires an explicit desktop
version, the two public store extension IDs, the controlled-engine signer pin,
and an exact Hyper-V image source tuple. The image tuple is limited to a
same-repository Actions artifact ID, one strict lowercase VHDX leaf filename,
its lowercase SHA-256, and the exact
`I_HAVE_VERIFIED_REDISTRIBUTION_RIGHTS` acknowledgement. The workflow then:

1. runs the JavaScript, extension, Native Host source, and locked Rust checks;
2. compiles `verisilo-native-host.exe` with those IDs in its production allowlist;
3. downloads the image artifact through the repository-scoped GitHub API into a
   runner-temporary ZIP, rejects expired records, traversal, links, any archive
   with other entries, and a hash mismatch, then stages the VHDX and strict
   versioned manifest under the ignored Cargo target tree;
4. stages the Host and copies every release script into that tree for the loose
   audited candidate, then builds an explicitly desktop-only current-user NSIS
   installer by applying `tauri.unsigned.conf.json` after the release config;
5. excludes the Host sidecar, release resources, and Native Host installer hooks
   from that unsigned NSIS: unsigned PowerShell cannot satisfy the signed
   installer's `AllSigned` boundary, and the artifact must not pretend that it
   can register the Host;
6. builds the Companion directory and a deterministic ZIP32/store archive whose
   entries are byte-sorted and whose DOS timestamps come from
   `SOURCE_DATE_EPOCH`; `extension-zip-manifest.json` binds every source file and
   the archive hash, then generates dependency inventory plus
   CycloneDX/SPDX JSON and a lockfile-to-package-metadata license evidence
   report, checks for placeholders and secret-shaped material, and
   records the unsigned Authenticode state for every staged EXE and PS1;
7. writes a source-bound `promotion-status.json` with `NOT_PROMOTABLE`, then
   writes `SHA256SUMS` and `provenance.json`, verifies both, and uploads a
   short-lived Actions artifact whose name ends in `-unsigned`.

The separate `Windows release candidate (signed)` workflow is gated by the
`windows-signing` GitHub environment and refuses to proceed unless both
`VERISILO_AUTHENTICODE_PFX_BASE64` and `VERISILO_AUTHENTICODE_PASSWORD` secrets
exist. Its order is intentionally fixed:

1. build the Native Host and stage the sidecar plus release resources;
2. run `tauri build --no-bundle --no-sign` to create `verisilo.exe` without an
   installer;
3. Authenticode-sign and timestamp `verisilo.exe`, the staged Native Host
   sidecar, and all eight staged PS1 resources, then immediately verify signer and
   timestamp certificates;
4. run `tauri bundle --no-sign`, which bundles those already-signed inputs without
   rebuilding them;
5. sign and timestamp the resulting NSIS installer;
6. run `VerifySigned` over every staged EXE and PS1, require exact policy-report
   coverage, then generate SBOM, hashes, and provenance from the final bytes.

The certificate is decoded only under `RUNNER_TEMP`, its path is never inside the
uploaded directory, and an `always()` cleanup step removes it. The base64 and
password secrets are scoped only to the materialization/signing steps; the gate
does not print them or record the PFX filename. Certificate/key extensions are
also forbidden by the release policy scanner. The signed and unsigned artifacts
use distinct `-signed` and `-unsigned` names and must never be relabeled.
The unsigned artifact still carries the exact Host and scripts as loose audited
files for the external promotion harness, but its NSIS installer exercises only
the desktop shell and is not Native Host or integration evidence.

Neither workflow creates a GitHub Release, publishes either store extension, or
uploads to a public server. The unsigned workflow never signs a file. The store
IDs are public identifiers, not credentials, but a release approver must still
compare them with the real Chrome Web Store and Edge Add-ons listings. A
syntactically valid ID alone does not prove store ownership.

The repository defines the signed workflow but has not, by that fact, exercised
a real certificate, timestamp service, Windows installer, or standard-user E2E.
Those remain external release evidence. CI and both release workflows pin every
external action to a verified full commit SHA with its human-readable version in
a comment; version comments do not replace the SHA pin.

No VHD/VHDX is stored in this repository. The redistribution acknowledgement is
only an operator assertion that allows the fail-closed plumbing to run; it is
not license evidence, legal approval, or proof that Microsoft/Windows/browser
redistribution terms are satisfied. A release approver must obtain the external
image lawfully, preserve its license/source evidence outside the public
candidate, and approve redistribution for the intended audience. Until that
review and the real signed Hyper-V lifecycle matrix both pass, the legal guest
image remains a release blocker even when its artifact ID and SHA-256 validate.

## Exact-candidate Windows promotion

Uploading either Windows candidate never promotes it. The upload step reports
its Actions artifact ID, upload-artifact SHA-256 digest, and full source
revision, while the included `promotion-status.json` stays
`NOT_PROMOTABLE`. Run the independent reusable/manual Windows candidate
promotion gate (real E2E) workflow with those three exact values.

The gate checks out the provenance revision, downloads only that artifact ID
through the current repository API, rejects an expired or digest-mismatched ZIP,
extracts without traversal, links, or case collisions, and then verifies every
entry against `SHA256SUMS` and `provenance.json`. It runs a fixed four-cell
matrix: Windows 10 x64 and Windows 11 x64, each with Chrome and Edge, on the
fixed `verisilo-win10` / `verisilo-win11` self-hosted labels. Every harness
invocation uses `-RequireAll`; any `FAIL`, `BLOCKED`, `SKIP`, missing browser,
wrong OS, or missing result fails its matrix cell. Each cell uploads a
machine-readable v2 attestation binding candidate ID/digest/source revision,
candidate descriptor/checksum hashes, expected OS, browser, summary hash,
status counts, and an acceptance-driver receipt hash. The driver is built in a
fresh target directory from the exact descriptor revision with only the
`acceptance-tests` feature; its compile-time revision and receipt candidate
binding must match before the attestation can pass.

The workflow is deliberately independent because a hosted build job cannot
honestly stand in for separately administered Windows 10/11 real-device
runners. A queued job with no matching self-hosted runner is not a pass. The
acceptance-only driver exposes no production command/API: it reads a random
passphrase only from anonymous stdin, rejects non-temporary/default/production
roots unless a random sentinel matches, and exercises the real Vault/Silo/stock
launcher/recovery core. Raw browser baselines and mocks cannot satisfy its
canonical desktop results. The destructive V1/V2 retained-data NSIS lifecycle
is not fabricated by candidate promotion and remains a separate public-release
prerequisite.

## Self-hosted Remote Agent candidate

The manual `Self-hosted Remote Agent candidate (Linux x64)` workflow is separate
from the Windows product candidate. It requires a version that exactly matches
`crates/verisilo-remote-backend/Cargo.toml`, then runs locked Rust fmt/check/test
for all targets, strict Clippy/source invariants, and a release build of only the
fixed `verisilo-remote-agent` operator binary. It stages the binary together with
the strict unavailable-provider example, deployment boundary, licenses, SBOM,
dependency-license evidence, SHA-256 manifest, and provenance.

The result is an **unsigned, short-lived CI candidate**. The workflow uploads an
Actions artifact only: it does not open a port, configure DNS/TLS, install a
system service, deploy a VM/browser Provider, create a VeriSilo cloud, or publish
a release. ELF/package signing, distribution format, service sandbox, update
channel, production certificate lifecycle and real WAN/Provider acceptance are
still external release gates. The default example advertises every lifecycle
operation as unavailable, so installing the candidate alone cannot be described
as a working remote browser.

## Reproducible release metadata

The SBOM generator reads exact entries from `pnpm-lock.yaml` and `Cargo.lock`; it
does not infer a dependency from source imports or silently query a registry. It
emits:

- `dependency-inventory.json` — every pnpm/Cargo lock entry, including build,
  development, optional, target-specific, and transitive entries;
- `bom.cyclonedx.json` — CycloneDX 1.6 JSON;
- `bom.spdx.json` — SPDX 2.3 JSON.

`dependency-licenses.json` is generated separately from `pnpm licenses list`
and target-filtered `cargo metadata`, then cross-checked against both lockfiles.
It strips runner-local package paths, lists metadata that did not match a locked
component, and keeps unmatched locked components visible. Every component is
deliberately marked `requiresHumanReview`; declared SPDX expressions and license
file names are evidence, not a legal conclusion. Collection happens through
fixed, shell-free child-process argument arrays; runner-local paths never enter
the normalized report or its deterministic metadata digests.

Lockfiles do not prove license declarations, so SPDX license fields remain
`NOASSERTION`. This is intentional and must not be relabeled as a completed legal
review. `THIRD_PARTY_NOTICES.md`, upstream metadata, and the exact shipped binary
set still require human license review before a public release.

`SOURCE_DATE_EPOCH` and `VERISILO_SOURCE_REVISION` are set from the checked-out
commit in the workflow. When absent locally, generators use the explicit,
non-release values `0` and `unversioned-source`; they never invent a commit or
current timestamp. This makes the JSON reproducible from the same inputs. It does
**not** claim the NSIS/PE binaries or whole candidate are bit-for-bit
reproducible: the hosted Windows runner image, PE/NSIS toolchain output, tool
downloads, and signing timestamp are not yet hermetic. The Companion ZIP is the
narrower exception: the repository writer uses stored entries, byte-sorted UTF-8
paths, fixed `SOURCE_DATE_EPOCH` timestamps, CRC-32, and an exact content/hash
manifest. `provenance.json` nevertheless records
`build.reproducibility.hermetic: false` and lists the remaining limitations for
the whole candidate.

Local generator checks:

```text
pnpm engine:verify
pnpm environment:verify
pnpm hyperv-image:self-test
pnpm extension:package:self-test
pnpm promotion:self-test
pnpm remote-agent:verify
pnpm release:self-test
pnpm sbom:generate
pnpm sbom:check
pnpm licenses:report
pnpm licenses:check
pnpm release:metadata
pnpm release:metadata:check
```

`generate-release-metadata.mjs --check` recomputes every staged file hash,
rejects symlinks, and verifies that provenance names the same artifact set.
`verify-release-policy.mjs --check` separately checks source/extension version
families, exact config fields, known placeholder IDs, expected artifacts, embedded
Host IDs, and secret/key/credential-bearing URL patterns. Pattern scanning is a
release gate, not proof that a binary contains no undiscovered sensitive data.

## Tauri/NSIS Native Host staging

The normal development config remains buildable without published store IDs. A
release build first runs `stage-windows-bundle.mjs`, which copies the already-built
Host, verified config, and exact release-script allowlist under the ignored Cargo
`target` directory. The signed release workflow then merges
`tauri.release-reset.conf.json` followed by `tauri.release.conf.json`. The reset
replaces the normal config's source-resource map before the release map is added;
this prevents an unsigned source PS1 with the same bundle destination from
surviving a deep config merge. The resulting release config has these properties:

- the target-triple Host becomes the sibling `verisilo-native-host.exe` sidecar;
- production install/verify/uninstall scripts and the verified ID config are
  bundled from staging under `native-host/`;
- environment PS1 and guest-agent shell resources are bundled from the same
  staging tree under `environment/`;
- the verified Hyper-V manifest is bundled under `environment/` and its only
  declared VHDX under `environment/images/`; the compile-time filename/SHA-256
  values are the same values rechecked by artifact policy;
- `windows/release-hooks.nsh` registers and verifies the Host after install and
  unregisters it before uninstall;
- a failed registration/verification stops installation instead of reporting a
  false success; rollback calls the idempotent uninstaller.

The staging self-test checks the full resource mapping, and `--check` compares
every staged byte with its source before signing. Signed builds mutate only the
staged copies, so source PS1 bytes and source-derived provenance remain truthful;
there is no backup/restore window that can strand signed source files after a
failure. This wiring still requires a real standard-user NSIS
install/upgrade/uninstall run on Windows 10 and Windows 11. Merely constructing
the installer from signed inputs is not that E2E evidence.

The unsigned workflow adds `tauri.unsigned.conf.json` last. That override removes
the inherited `externalBin` key with an RFC 7396 `null`, and clears `resources`
and `installerHooks`, producing a desktop-only NSIS
instead of invoking unsigned PS1 under `AllSigned`. The loose Host/resources are
still staged and hashed for the exact-candidate promotion harness, which
registers the Host explicitly and does not treat the unsigned installer as an
integration result.

The Native Host Cargo binary is also gated by the non-default `native-host`
feature. Host staging enables that feature explicitly; ordinary Tauri builds do
not, so Tauri cannot auto-bundle the `src/bin` target as an additional project
binary after the external sidecar override has been cleared.

## Authenticode boundary

`authenticode-gate.ps1` treats EXE and PS1 as one signable boundary. Its modes are
deliberately separate:

- `Unsigned` fails unless every selected EXE and PS1 reports `NotSigned`, then records
  `signingState: unsigned`;
- `DryRunSigning` requires an existing certificate input, an HTTPS timestamp URL,
  `signtool.exe`, and a password environment variable, but performs no signing and
  records `dry-run-inputs-validated-not-signed`;
- `SignAndVerify` requires a real PFX/private key, signs EXE with `signtool.exe`
  and PS1 with PowerShell Authenticode, timestamps both, and immediately requires
  a valid signer and timestamp certificate for every selected input;
- `VerifySigned` never signs and records `signed-and-verified` only after Windows
  reports a valid signer and timestamp certificate for every selected EXE and PS1.

`-IncludeRelativePath` is an explicit, duplicate-free allowlist constrained to the
declared release root. It is used for the pre-bundle inner inputs and outer
installer signing phases. The final call omits it so recursive verification and
the release policy must cover the complete staged EXE/PS1 set exactly once.

No certificate or password is stored in this repository or used by the unsigned
workflow. A missing certificate, timestamp, or complete final coverage report can
therefore never become a “signed” status.

## Vault compatibility release gate

Schema 7 is the only write format. Schemas 1–6 are supported import formats and
must continue to pass the real Argon2id/AES-256-GCM fixture matrix before a
release candidate is promoted.

| Source schema | State that must survive migration                                            | Safe default / rejection boundary                                                          |
| ------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| 1             | Silo identity, network profile, seed reference and 32-byte seed              | Credentials, network history, and remote state default empty.                              |
| 2             | Schema 1 plus proxy credential reference/value                               | Mihomo secret and later state default empty.                                               |
| 3             | Schema 2 plus Mihomo Controller secret reference/value                       | Network history and remote state default empty.                                            |
| 4             | Schema 3 plus sanitized network inbox history                                | Missing runtime binding remains nil/unbound and cannot become current `verified` evidence. |
| 5             | Schema 4 plus endpoint, pairing/replay ledger, binding, and operation result | Deletion proof and orphan receipts default absent/empty.                                   |
| 6             | Schema 5 plus authenticated deletion proof                                   | Orphan receipts default empty.                                                             |
| 7             | All current fields, including force-detach orphan receipts                   | Current schema; unknown or missing required fields are corruption, not migration.          |

The gate must also reject unknown envelope/payload/Silo fields, unsupported
envelope or payload versions, a lower schema number carrying later-schema
fields, metadata/ciphertext tampering, and the wrong passphrase without putting
sensitive fixture values in errors. For every source schema, verify atomic
rewrite to schema 7 and stable reopen. Separately verify a legacy encrypted
backup through restore-and-migrate, passphrase rotation after migration, and a
schema 7 backup/restore containing an orphan receipt. This is local persistence
evidence only; it does not satisfy the Windows disk-failure, permissions,
upgrade, or installer-retained-data matrix.

## Desktop core Rust evidence

`crates/verisilo-desktop-core-harness` is the required Tauri-free drift gate for
the production desktop core. Its independent lockfile is development/test
infrastructure only. CI must run its locked fmt, check, test, and
`clippy --all-targets -- -D warnings` gates on both Ubuntu and Windows; local
release preparation should run `pnpm desktop-core:verify` with the exact lock
already available offline.

The Ubuntu result proves only that the actual platform-independent Rust modules,
loopback relay/Mihomo tests, Vault/engine/environment models, and remote-backend
path crate compile and pass without GTK/WebKit. It is not a Tauri build, Windows
`cfg` result, real-browser result, or OS-isolation result. The Windows harness
adds native Windows core compilation/tests, but does not prove Tauri packaging,
NSIS behavior, a real Chrome/Edge network path, or Windows 10/11 acceptance.

## Before a public build

- [ ] Verify `pnpm check`, `pnpm test`, `pnpm build`, `pnpm extension:verify`, `pnpm native-host:verify`, `pnpm engine:verify`, `pnpm environment:verify`, `pnpm remote-agent:verify`, and `pnpm release:self-test`.
- [ ] Verify `pnpm desktop-core:verify` from the locked offline cache and review both Ubuntu and Windows desktop-core CI results; record the exact passed-test count for each target without treating either as Tauri or real-browser evidence.
- [ ] Run the Vault schema 1–6 unlock/restore migration matrix plus schema 7 rotation/backup/reopen tests; inspect the plaintext envelopes to confirm that no Silo name, seed, credential, remote application credential, or orphan-receipt detail appears.
- [ ] Build the Rust desktop and Native Host using a pinned stable Rust toolchain on Windows 10 and Windows 11 x64.
- [ ] Run the local session fixture in two Silos: log into A, close it, open B, and confirm B has no A cookie or LocalStorage; reopen A and confirm its state persists.
- [ ] Confirm the user's default Chrome and Edge profiles were neither selected nor modified.
- [ ] Confirm a running Silo is not force-killed, and a stale/active `SingletonLock` causes a safe refusal.
- [ ] Verify fixed HTTP and SOCKS5 imports with and without credentials; confirm secrets do not appear in browser arguments or the plaintext Vault envelope.
- [ ] Stop the required proxy/Mihomo process during a Silo session and confirm public navigation fails without a host-network fallback.
- [ ] Verify external Mihomo rejects remote Controllers, unknown/`DIRECT`/`REJECT` nodes, non-`GLOBAL`/`global` mode, listener-port/config drift, failed authentication, and selection readback mismatches before browser launch.
- [ ] While a required Mihomo Silo is running, change each of Controller reachability, Secret, selected node, listener port and config independently; confirm the exact runtime enters the blocked state, its old loopback port rejects new connections, existing relay connections close within the implementation bound, and another runtime/relay is unaffected.
- [ ] Restore Mihomo after a terminal drift and confirm status refresh, runtime recheck and node rebind do not reopen the old listener; close the browser normally, launch again explicitly, and only then accept a newly verified relay.
- [ ] Repeat the drift/child-exit matrix on real Windows 10/11 Chrome and Edge and capture host/DNS/WebRTC/QUIC traffic. Passing Rust listener tests is not evidence that real Windows has no browser- or OS-level fallback.
- [ ] Run the IP/public-DNS action inside the launched Silo and confirm it is labeled separately from the desktop controller check.
- [ ] For authenticated HTTP Basic, confirm the same runtime produces a 407 Basic challenge, credentialed 2xx CONNECT, relayed bytes, and a Companion public-IP observation inside the fixed check window before authentication becomes `verified`; confirm a second 407 becomes `failed`.
- [ ] Confirm stale/wrong-runtime/missing relay receipts, a Companion success without a receipt, and a proxy that accepts unauthenticated CONNECT all leave configured HTTP credentials unverified; inspect logs, UI, receipt state, Vault history, and exports for credential or `Proxy-Authorization` leakage.
- [ ] Verify production Native Host manifests use only published Chrome/Edge IDs and user-level registry keys.
- [ ] Generate `native-host-release-config.json` from explicit store IDs, build the Host in the same environment, then run `install-native-host.ps1` and `verify-native-host-install.ps1` as a standard user.
- [ ] Confirm an absent/wrong production ID, unauthorized origin, unknown field, secret-shaped field, protocol mismatch, and message over 16 KiB are rejected.
- [ ] Confirm `open_desktop` can only start the sibling `verisilo.exe` without arguments, and a stale or malformed runtime snapshot is never returned as current status.
- [ ] Run `uninstall-native-host.ps1` twice and confirm both browser registrations/manifests are gone while Vault, reports, and Silo Profile directories remain.
- [ ] Review every extension permission and store disclosure; no unused host or network permission may ship.
- [ ] Generate and verify dependency inventory, CycloneDX/SPDX SBOMs, `SHA256SUMS`, provenance, and the Authenticode status report from the final staged artifacts.
- [ ] Obtain the Hyper-V base image lawfully, review its exact license and redistribution terms, retain external source/license evidence, and compare its same-repository Actions artifact ID/leaf filename/SHA-256 with the bundled strict manifest; the workflow acknowledgement alone is insufficient.
- [ ] Review `dependency-licenses.json` against the exact shipped graph, resolve every unmatched or ambiguous entry, copy required license/notice/source materials, and record legal approval; metadata and lockfile `NOASSERTION` are not a legal conclusion.
- [ ] Run the NSIS current-user install/upgrade/uninstall/data-retention E2E on Windows 10 and Windows 11.
- [ ] Run the exact uploaded candidate ID/digest/revision through all four Windows 10/11 x64 Chrome/Edge promotion cells with `RequireAll`; review each machine-readable attestation and treat a missing runner/browser, `SKIP`, or `BLOCKED` as failure.
- [ ] Provision the authorized Authenticode PFX/password in the protected `windows-signing` environment, run the signed workflow, review every signer/timestamp entry, and keep the unsigned candidate clearly separate.
- [ ] Compare both extension IDs with their published store listings and run final-host positive handshakes for each origin plus an unauthorized-origin negative test.
- [ ] For a Remote Agent candidate, independently review the fixed Provider and guest artifact, prove its image/license provenance, exercise PKI + pin rotation on a real endpoint, and run create/network/outage/TTL/destroy/screen-input scenarios over a real WAN. Do not promote the unavailable-provider candidate as a remote browser.

## Proxy wording

The UI may say `configured` after model validation, `endpoint reachable` after a connection/protocol check, and `browser routing applied` after process creation with the intended arguments. SOCKS5 authentication may say `verified` only after strict method selection and username/password protocol success. HTTP Basic authentication may say `verified` only when the exact active relay and runtime record a 407 Basic challenge followed by credentialed upstream acceptance and relayed bytes within the same bounded window as a valid public-IP observation from that runtime. A direct unauthenticated 2xx, a generic Companion success, a stale or wrong-runtime receipt, or an accepted handshake without relayed bytes is not credential evidence; a credentialed 407 is `failed`. The Companion fact remains `extension_asserted` and the relay fact remains `relay_observed`; their combination is not an independently trusted browser-process attestation. The exit itself remains `observed`, never process-authenticated `verified`. A terminal runtime drift must say that the network path is blocked and the old port will not be reopened; it must not keep a running/verified badge merely because the browser process is still alive. Expired/failed evidence must be cleared. Only a fixed Guest/Engine source with authenticated provenance may use `verified` for that exact result. Public DoH answer comparison must not be called DNS leak detection. The current stock-browser launcher must never claim that TLS, HTTP/2, HTTP/3, all DNS paths, or all WebRTC paths are controlled. A future engine, VM, or remote backend may only change that wording for an individual capability after it records direct runtime evidence.
