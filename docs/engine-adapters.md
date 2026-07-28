# V0.7 EngineAdapter

V0.7 now has a repository-side controlled-engine lifecycle and a real Windows
signed-manifest verifier. It still does **not** include a controlled Chromium or
Camoufox executable, a publisher certificate, private signing keys, browser
patches, or real browser evidence. Those are external release inputs, not test
fixtures and not implied by an `experimental` capability declaration.

The authoritative implementation is
`apps/desktop/src-tauri/src/engine.rs`. The package schema and deliberately
non-installable example are in `apps/desktop/src-tauri/resources`.

## Adapter and capability contract

Every adapter exposes descriptor/version/channel, capability negotiation,
install, update, shell-free launch planning, health, rollback, emergency
disable, identity validation, short-lived token derivation, and an
`observe → apply → verify → restore` control plan.

Capability availability and runtime state are independent:

- availability is `supported`, `experimental`, or `unavailable`;
- operation is `not_configured`, `configured`, `applied`, `verified`, or
  `failed`.

`experimental` never means verified. Applied, verified, failed, and restore
transitions require bounded direct evidence. `EngineControlExecution` enforces
phase order, exact per-capability evidence coverage, phase receipts, and
fallback receipts. A matching site fallback restores only the explicitly
listed experimental controls and returns `restore_then_reload`; wildcard rules
match subdomains, not the apex host.

The production desktop launch path now has a bounded receipt channel after its
strict bootstrap ACK. A controlled child must emit ordered, complete
per-capability `observe → apply → verify` receipts within the fixed launch
window before the adapter or any configured capability is marked `verified`.
The same reader accepts policy-matched site-fallback receipts while the process
runs and an optional final `restore` receipt before normal exit. Missing
Restore evidence is recorded as failed or unavailable; VeriSilo does not kill
an unrelated or ordinary browser to manufacture a restore claim.

Every runtime frame is strict JSON with a 32 KiB ceiling, protocol version,
monotonic sequence, a short issued/expiry window, and exact bindings to the
adapter, Silo, session, token identifier, engine version, artifact SHA-256,
verifier, and package verification time. Unknown fields, sequence gaps or
duplicates, stale/future receipts, wrong bindings, incomplete capability sets,
phase reordering, forged fallback matches/actions, early exit, and timeout all
fail closed. Launch failure terminates only the exact newly spawned child and
never falls back to stock. The opaque token value is never echoed in a receipt,
runtime activation, log, or persisted runtime record.

Stdout is a native control stream for externally packaged engines, not a page
message surface. One bounded reader owns it from ACK through Restore. Desktop
waits use fixed deadlines and never join that reader on timeout or process
exit, so a descendant that inherited stdout cannot hang launch, recheck, or
exit. The bounded receiver is drained into sanitized capability, phase, and
fallback records exposed by `RuntimeActivation`; wire bindings such as
`tokenId` are deliberately discarded after comparison.

TLS ClientHello and QUIC remain unavailable because this repository has no
controlled runtime or direct protocol capture. An identity template must use
`browser_default` for QUIC; asking this adapter to disable QUIC would otherwise
claim control that it does not implement.

## Stock Chrome and Edge

`StockChromiumAdapter` uses an explicit `BrowserDescriptor`, requires an
absolute regular executable with no link/reparse component, and emits an argv
array with an independent profile directory. It does not concatenate a shell
command. Vendor install, update, and rollback remain outside VeriSilo. Stock
browsers do not claim Canvas, WebGL, font, UA/UA-CH, Worker, TLS, or QUIC
control.

## External package layout and safety

`ExternalPackageEngineAdapter` accepts only `controlled-chromium` and
`camoufox`. A package root contains:

```text
engine-package.json
bin/chromium.exe       # controlled-chromium only
bin/camoufox.exe       # camoufox only
```

Manifest schema version 2 is defined by
`engine-package.schema.json`. The loader rejects relative roots, traversal,
backslash aliases, symlinks, Windows reparse points, paths escaping the
canonical root, unexpected executable names, empty executables, executables
over 4 GiB, manifests over 64 KiB, unknown JSON fields, duplicate or forbidden
capabilities, unpinned/non-SemVer versions, a major version outside 100–999,
wrong engine/channel/platform, and malformed hash/signature fields.

Packages must declare at least `identity_template` and `site_fallback`.
`profile_isolation`, network launch, TLS, and QUIC cannot be granted by a
package manifest. The adapter, launcher, or unavailable evidence gate owns
those surfaces.

## Signed-manifest protocol

The manifest uses `cms-detached-sha256`; Authenticode on the executable is not a
replacement for this signature. The signature authenticates the manifest,
which contains the exact SHA-256 of the executable.

The bytes to sign are deterministic:

1. Parse a schema-v2 manifest and replace only `signature.value` with the empty
   string.
2. Serialize compact UTF-8 JSON in this field order:
   `schemaVersion`, `engineId`, `engineVersion`, `channel`, `platform`,
   `executableRelativePath`, `artifactSha256`, `signature`, `capabilities`.
   The nested signature order is `algorithm`, `keyId`, `value`. Enum strings
   use their schema spelling. No trailing newline is present.
3. Prefix those JSON bytes with the ASCII domain separator
   `VeriSilo engine package manifest v2` followed by one NUL byte.
4. Produce a DER detached CMS SignedData object with exactly one signer and a
   SHA-256 signer digest. Base64-encode the DER using the standard alphabet and
   padding, then store it in `signature.value`.
5. Set `signature.keyId` to lowercase SHA-256 of the signer certificate's exact
   DER bytes.

The encoded field is limited to 60,000 characters and decoded CMS input to
48 KiB. The verifier recomputes the executable SHA-256 from a bounded streaming
read and rejects a file that changes while being hashed.

On Windows, verification calls Crypt32 directly; it starts no command shell or
child process. It checks the CMS signature, requires exactly one signer,
requires SHA-256, requires the certificate to be currently within its validity
period, requires the explicit Code Signing EKU (`1.3.6.1.5.5.7.3.3`), hashes
the returned signer certificate, and compares that hash with both `keyId` and
the build's signer-pin policy. Non-Windows builds fail closed.

### Trust, expiry, chain, and revocation boundary

The exact certificate SHA-256 pin is the trust anchor. The verifier does not
substitute the ambient Windows root store for that pin, perform online chain or
revocation retrieval, or accept a timestamp to resurrect an expired
certificate. This avoids a network-dependent install decision, but it makes
pin rotation and revocation an application-release responsibility:

- remove a compromised/revoked pin in an application update;
- persistently emergency-disable the affected adapter/version immediately;
- sign replacement manifests with a newly pinned, currently valid code-signing
  certificate.

`engine-trusted-signers.json` is intentionally empty in this source tree
because no release signer has been supplied. A release build may embed one or
more comma-separated lowercase certificate hashes through the compile-time
`VERISILO_ENGINE_SIGNER_SHA256` value, or a maintained source policy can hold
public certificate pins. Private keys never belong in the repository. With no
pin, the production verifier is present but every external package is rejected.

## Durable lifecycle state

On Windows, `production_prototype` installs the production verifier and uses
`%LOCALAPPDATA%\VeriSilo\engine-state`. State contains only schema/adapter,
canonical package root plus pinned version for active and rollback packages,
emergency state/reason, and update time. It does not cache a successful
verification as authority.

State updates are written to a same-directory temporary file, flushed, and
atomically replaced with `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` before
memory is changed. Install verifies the incoming package; update re-verifies
the active package and requires a strictly newer SemVer before verifying the
incoming package; launch, negotiation, identity validation, control planning,
and health re-verify the active package; rollback re-verifies both active and
rollback packages before swapping them. Re-enabling an emergency-disabled
adapter also re-verifies its active package.

Emergency disable is the intentional safety exception: it remains persistable
even after artifact tampering, so corruption cannot prevent the kill switch.
While disabled, install, update, launch, and rollback are blocked.

## Identity boundary

Identity templates constrain Windows version/architecture, browser family and
package major, UA and UA-CH, languages, timezone, screen, Canvas/WebGL pairing,
fonts, media devices, and network locale/timezone. Templates contain no seed.
The derivation context contains opaque IDs and a lifetime of at most one hour;
the launch plan carries only a short-lived token identifier and requires token
delivery through secure stdin before navigation. It never places the token or
a long-term seed in argv or the page main world.

At spawn, the native launcher writes a length-prefixed, strict bootstrap that
binds the Silo/session, constrained template/control plan, opaque token, and
freshly reverified package digest. A partial write, malformed/missing ACK, or
timeout terminates only that exact newly spawned child and never falls back to
stock Chrome/Edge. Package signature verification, bootstrap ACK, and runtime
receipts remain separate evidence. A receipt proves only that the freshly
verified, bound engine process made a specific evidence declaration. It does
**not** by itself prove that Canvas output, TLS ClientHello bytes, QUIC
behavior, or any other browser behavior is truthful; that still requires an
audited controlled artifact and independent observations or captures.

These checks and the control plan do not prove that an external browser patch
actually applied all signals. Runtime receipts are necessary to move each
capability upward, never sufficient evidence of the underlying signal's
truthfulness.

## Source and release gates

Run the dependency-free source gate with:

```bash
node scripts/verify-engine-source.mjs
node scripts/verify-engine-source.mjs --release
```

The normal gate validates the manifest example, trust policy, no-shell source
boundary, and absence of controlled-engine executables/archives/signatures in
desktop resources. It reports the missing signer pin as an explicit blocker.
The release gate fails until at least one valid certificate pin is configured.

Focused Rust tests cover exact SHA-256 vectors, canonical signed bytes, package
tamper, wrong hash/signer/signature/version/platform, traversal and symlink
rejection, unknown manifest/state fields, update/rollback persistence,
emergency-disable persistence and re-enable refusal after tamper, identity
constraints, strict evidence phases, and site fallback.

## External release blockers

V0.7 cannot be called a shipped controlled browser until separately supplied
and audited work provides licensed reproducible Chromium/Camoufox artifacts,
patch sources, SBOM and license review, protected signing service and public
certificate pin, update hosting and rollback retention, secure token transport
integration, real Window/iframe/Dedicated Worker consistency evidence,
site-compatibility regression results, Canvas/WebGL/font observations, TLS
ClientHello captures, and direct QUIC observations. Tests and placeholder JSON
must never be presented as those artifacts or evidence.

In particular, this source tree still has no real signed engine artifact or
signer pin. Production external-engine launch therefore remains blocked even
though the receipt transport and fake-engine E2E harness exist.
