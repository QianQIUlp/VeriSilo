# M3-0 Camoufox Host / EngineAdapter integration task contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical Accepted contract**: M3-0 is closed. The required-reading list and the
> acceptance matrix in this document do not apply to the current task. Read it only when
> investigating the `e96ef3f` contract evidence. For the current Gate, see the
> [status page](camoufox-program-status.md).

- Status: **Main-brain Gate Accepted on 2026-08-09**
- Freeze date: 2026-08-09
- `main` starting baseline: `8de389db366d1d9ff510b1e885fab7f49a89aad0` (PR #10 merge commit)
- Execution branch: `codex/camoufox-m3-engine-adapter`
- Frozen implementation starting point: `b3d094cb0a7f3b7f9c113c53e4c4575d16babb67`
- Accepted checkpoint: `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`
- Previous Gate: M2-W **Accepted**. The Linux and Windows standalone evidence stays `verified: false`.

This document is both the repository task card for M3-0 and the prompt handed to the
execution agent. Before execution, read the following in order:

1. [Identity platform north star](identity-platform-north-star.md);
2. [Camoufox-first decision](camoufox-managed-engine-decision.md);
3. [Agent operating model](agent-operating-model.md);
4. [Camoufox current status](camoufox-program-status.md);
5. This document;
6. [EngineAdapter current state](engine-adapters.md), [Host v1](camoufox-host-v1.md), and the [M2-W Gate](camoufox-m2w-windows.md).

## Gate result

The original candidate `bc65e07fbc21ee8581b3ac60c91afd15d0effa20` did not pass the lead
agent's review and cannot be used as the accepted checkpoint. The execution agent closed the
gaps (the package/browser tree separation, the secret persistence scan, and the real
`RuntimeManager` fake Host vertical slice) with the follow-up commits
`fc67418b946c2f82f69e13ec5751cff24f3f1e7f`,
`b50765986c7da0bf081ff0a9057dc38073813311`, and the final
`e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd`. The lead agent checked each item against the
frozen contract and did a small read-only seam check, and accepted the final checkpoint. The
full test matrix was not rerun.

The accepted evidence is contract-level fake Host tests and commit binding. It contains no
real-browser run ID:

- `pnpm check`, `pnpm build`, and `pnpm engine:verify` passed; `pnpm test` ran `150` tests.
- Desktop `cargo test --locked` gave `184 passed / 0 failed`, and fmt and clippy passed.
- Artifact-locked environment tests: `25/25`.
- The production signer pin is still not configured and stays fail-closed.
- Host evidence is still `observed-on-this-host`, `verified: false`, and `verifiedAdapter = null`.

This Gate accepts only the package/transport/lifecycle/evidence contract. It did not launch
a real Camoufox. It did not prove a signed Host package, a published runtime, an installer,
or a shipped Managed Identity. The follow-up task is
[M3-WI native Windows real Host integration](camoufox-m3-wi-windows-task.md).

## Role and task boundary

You are the **execution agent** for this phase, not the lead agent who re-decides the project
direction. Your job is to inspect the real code within the frozen scope, implement, test,
commit, and return an evidence package. Do not expand M3-0 into UI, an installer, network
identity, release signing, or another browser-engine project.

If the baseline does not match, the task requirements conflict with each other, or you would
have to change the forbidden scope to continue, stop and report to the lead agent. Do not lower
the Gate, forge a receipt, or merge `configured`, `applied`, `observed`, and `verified` just to
"look finished".

## Current real gap

The two existing protocols in the repository cannot be assumed to be compatible:

- The desktop external-engine path treats a newly started child process as a controlled
  engine. It writes one length-prefixed bootstrap to stdin and then reads an ACK and ordered
  phase receipts from stdout.
- The standalone Camoufox Host v1 is a long-lived bidirectional JSON Lines control process
  that supports `hello/launch/status/close/shutdown`. `launch` accepts only
  `artifactId/profileId/expectedArtifactFileSha256`.
- External package schema v2 treats `bin/camoufox.exe` as the raw browser executable and
  binds only that file. It defines no Host entrypoint, no Host/runtime tree, and no
  package-level binding for the browser tree.
- The current Camoufox Silo configuration stores a generic `IdentityTemplate`. It does not
  yet store an explicit ID + raw SHA binding for an Artifact file. The real replay authority
  is the v3 Resolved Identity Artifact.

The purpose of M3-0 is not to hide these gaps behind naming or an adapter wrapper. It is to
form a narrow, strict, testable desktop integration seam.

## Core goal

Deliver a **contract-level vertical slice that does not launch a real Camoufox**: the existing
Tauri `RuntimeManager` can drive a fake Camoufox Host v1 over a dedicated, bounded transport. It
completes package re-verification, a strict `hello`, an Artifact-bound `launch`, `status`,
exact `close/shutdown`, failure mapping, and honest capability/evidence status.

Passing M3-0 only opens the later native Windows M3-WI integration Gate. It does not prove
that a real desktop Camoufox is connected, and it is not a releasable Managed Identity Silo.

## Frozen architecture choices

### 1. The Host is the Camoufox package entrypoint

- The Camoufox package entrypoint is the Host managed by VeriSilo, not the `camoufox.exe` in
  the browser tree.
- One active Camoufox Silo corresponds to one Host child process that the desktop starts and
  holds exactly. The Host in turn owns the browser Job/process tree.
- Do not add a resident global daemon. Do not share a Host across different Silos. Do not take
  over an old process by port discovery or by guessing from the process name.
- The existing native bootstrap/receipt v1 path of Controlled Chromium stays unchanged.

### 2. Use a separate transport for the Host, and do not forge a generic receipt

Add an explicit transport discriminant to the Rust launch plan / runtime. It must at least
distinguish:

- stock / no controlled protocol;
- the existing `native-bootstrap-v1`;
- `camoufox-host-jsonl-v1`.

The Camoufox transport must directly implement the Host v1 bounded JSON Lines
request/response, strict request ID correlation, a single stdout reader, timeouts, and EOF
handling. It is forbidden to forge a Host `launch` result into an
`observe → apply → verify` receipt that the Host did not send. It is also forbidden to let the
two framings share one ambiguous parser.

### 3. The Artifact is the Camoufox identity authority

Introduce a strict `CamoufoxArtifactBindingV1` (the name may follow repository convention, but
the semantics must not change). It must contain at least:

- `artifactId`;
- a 64-hex `artifactFileSha256`;
- the fixed Artifact schema `verisilo-camoufox-resolved-identity/v3`.

This binding must be persisted in the engine configuration of the Camoufox Silo. An old
Camoufox configuration without the binding may still deserialize, but launching must fail
closed with an explicit `identity_artifact_unavailable`-class error. It must not generate,
guess, or select an Artifact on the fly at launch time.

The `IdentityTemplate` kept for the transition period may only serve as a declaration of the
old control plane. It cannot become the Host replay authority, cannot be sent to the browser,
and cannot by itself raise a capability to `applied` or `verified`. Do not modify the schema,
canonicalization, or digest rules of Artifact v3, Policy v3, Projection v3, or
ObservedWebsiteDigest v2.

The desktop resolves artifact/profile/state/tree paths from fixed roots that the application
owns. The Silo, the API, and the page must not supply arbitrary paths. `profileId` must be
derived deterministically from the Silo ID. Host `launch` still receives only the ID and the
expected SHA.

### 4. Do not send the Silo seed or proxy secrets into the Host

The Camoufox Host transport does not use the existing Vault-seed-derived token as an identity
input. The Silo seed, the Artifact's internal seed, proxy passwords, the Controller secret,
and bearer tokens must not appear in Host/browser argv, logs, or protocol evidence.

The existing short-token bootstrap continues to serve `native-bootstrap-v1`. Do not push it
into the Camoufox Artifact replay chain for the sake of code reuse.

### 5. Versioned Host package contract

Define a new, strict package manifest version for the Camoufox Host. Do not silently
reinterpret the `bin/camoufox.exe` of schema v2. The new manifest must bind at least:

- `entrypoint.kind = camoufox-host-v1`;
- a fixed relative path for the Host entrypoint;
- Host protocol `verisilo-camoufox-host/v1`;
- the entrypoint SHA-256;
- the relative path and SHA-256 of the package tree manifest;
- engine/version/platform/channel, the capability declarations, and the existing CMS signer
  policy.

The CMS canonical payload must cover the new bindings. The loader must reject, layer by layer:
traversal, unknown/duplicate fields, symlinks/reparse points, missing/extra/changed tree
members, and wrong schema/engine/entrypoint combinations.

The Controlled Chromium behavior of schema v2 stays compatible. A schema v2 Camoufox package
must be rejected with an explicit unsupported entrypoint/schema error. M3-0 uses only
fake/test packages. It adds no release signer pin, does not bypass the production verifier,
and commits no real browser archive, Python runtime, or private key.

### 6. Map runtime evidence honestly

Fixed launch order:

```text
reverify signed package contract
→ spawn exact Host child with typed argv
→ hello (validate protocol/host version/platform/asset/tree binding)
→ launch (artifactId/profileId/raw SHA)
→ validate response binding and state=running
→ RuntimeManager enters Running
```

The Host currently returns `verified: false` / `observed-on-this-host`. Therefore:

- The exact child, the package verification, the Artifact raw SHA, and the Host launch
  response may be recorded.
- `launchedAdapter` may be Camoufox.
- `verifiedAdapter` must stay empty.
- Host launch evidence maps to `applied` / `observed` at most. It cannot map to `verified`.
- `runtimeReceipts` must not be marked as a verified generic phase receipt.
- TLS ClientHello, QUIC, Canvas stable identity, managed fonts, and cross-host reproduction
  stay `unavailable` or unverified.
- `site_fallback` has no real implementation in Host v1. The Camoufox fallback rules must be
  empty and explicitly unavailable.
- M3-0 does not connect a proxy. Camoufox allows only a Direct network policy. A required
  proxy fails before spawn, and `launch_network` stays unavailable.

If the existing capability model cannot express `observed`, prefer keeping the capability
conservatively at `applied` and writing the source into bounded evidence. Do not widen the
whole public capability enum for this task. Any public contract version bump must have
compatibility tests and an explicit migration. It must not happen as a side effect.

### 7. Lifecycle must use Host ownership

- The desktop holds the Host stdin/stdout and the exact child handle. It must not close a
  process by PID enumeration or name matching.
- The watchdog/status updates state through bounded `status` requests. A wrong ID, a timeout,
  a malformed response, and Host EOF/exit all fail closed and cannot fall back to stock.
- For an active Camoufox Silo, the existing `stop_silo` may send `close`, wait for
  `state=exited` and `processTreeExit.exited=true`, then send `shutdown` and wait for the exact
  Host child. The Stock Chrome/Edge semantics "the user closes the window themselves" are
  unchanged.
- `quarantined`, `profile_quarantined`, or an unconfirmed full-tree exit must enter
  `VerificationFailed` / `RecoveryRequired`. Keep the boundary information. Do not mark it
  `Stopped` or release it as safe to reuse.
- The cleanup semantics for a Host crash, desktop shutdown, and stdin EOF must be covered by
  fake-host counterexamples. The Job Object/lock logic already verified in M2-W must not be
  weakened.

## Allowed modifications

- `packages/contracts/src/engine.ts` and its tests, plus the corresponding dist generated by
  repository scripts;
- `apps/desktop/src-tauri/src/engine.rs`, `launcher.rs`, `domain.rs`, `lib.rs`, and targeted
  tests;
- `apps/desktop/src-tauri/resources/engine-package*.json` and the source verifier;
- a new test-only fake Host / package fixture;
- `apps/camoufox-host/host_v1.py`: minimal, backward-compatible protocol fields or test
  fixes, only when the desktop cannot establish a required binding from the existing response;
- `docs/engine-adapters.md`, this task card, and the status page, consistent with the real
  implementation.

TypeScript UI is allowed only the neutral adaptation needed to compile the existing model. Do
not add an interface, an entry point, marketing copy, or a way for users to choose a Camoufox
package that cannot yet run.

## Forbidden to modify or implement early

- The Artifact/Policy/Projection/ObservedWebsiteDigest schemas and the accepted
  fixtures/evidence manifests;
- The M2-W Job Object, file lock, reparse point, and tree-verification guarantees;
- Managed Identity generation UI, the installer, automatic download, the update service, and
  release-signing infrastructure;
- Proxy injection, network-region linkage, site fallback, the extension ecosystem, TLS/QUIC;
- Controlled Chromium, WSL, Hyper-V, VMware, Remote;
- Real Camoufox/Python archives, private keys, tokens, seeds, or proxy secrets;
- Changing `verified: false` to `true`, or writing fake-host tests up as real browser evidence.

## Acceptance matrix

### Contract / package

1. The Camoufox Artifact binding strictly rejects an empty ID, a wrong schema, a bool/non-string
   SHA, a non-64-hex value, unknown fields, and missing fields.
2. The new package version strictly rejects a wrong entrypoint/protocol, a tree manifest
   digest mismatch, path traversal, link/reparse, missing/extra/tampered members, and
   duplicate/unknown fields.
3. Schema v2 Controlled Chromium keeps passing. Schema v2 Camoufox is explicitly rejected.
4. The production verifier still fails closed without a signer pin. Tests must not add a hidden
   bypass.

### Fake Host integration

5. The normal `hello → launch → status → close → shutdown` completes, and the request ID,
   Artifact ID/raw SHA, profile ID, and Host child are consistent throughout.
6. After a successful `launch` the Runtime is Running, but `verifiedAdapter` is empty and the
   capability shows no false `verified`.
7. An Artifact/SHA/profile response mismatch and a wrong Host protocol/version/asset binding
   are rejected before Running.
8. Duplicate keys, unknown fields, invalid UTF-8, an oversized frame, an out-of-order response,
   a duplicate/wrong ID, EOF, early exit, and timeout all fail within bounds.
9. `profile_in_use`, `profile_quarantined`, `quarantined`, and `processTreeExit.exited=false`
   map to cannot take over / cannot stop safely.
10. A normal close releases the desktop runtime/profile ownership only after the Host confirms
    that the whole tree has exited. Failure paths do not kill unrelated processes.
11. A Direct Camoufox can form a plan. A required proxy and non-empty fallback rules are
    rejected before the Host spawn.
12. The argv/log/protocol snapshot scan contains no Vault seed, Artifact seed, token value, or
    proxy secret.

### Regression

13. The Stock Chrome/Edge launch/stop semantics are unchanged.
14. The Controlled Chromium native bootstrap, ACK, ordered receipt, site fallback, and package
    schema v2 tests keep passing.
15. The existing Host v1 standalone unit/integration tests do not regress because of M3-0. The
    long real-browser tests need not be rerun.

## Required verification

Run and report at least:

```bash
pnpm check
pnpm test
pnpm build
pnpm engine:verify
cargo fmt --manifest-path apps/desktop/src-tauri/Cargo.toml -- --check
cargo test --locked --manifest-path apps/desktop/src-tauri/Cargo.toml
cargo clippy --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --all-targets -- -D warnings
```

If the current Linux cannot complete the Windows-only verifier path, the existing GitHub
Windows CI must cover it and you must give the check URL. Do not use Linux/WSL to stand in for
native Windows M3-WI.

## Stop conditions

Stop and return to the lead agent on any of the following. Do not expand on your own:

- Integration would require changing Artifact v3 or the M2-W accepted evidence.
- The Host transport cannot be added without weakening Controlled Chromium v1 or the
  production verifier.
- A real signer, a published runtime, a browser archive, UI/installer, or a proxy would be
  needed for the contract tests to hold.
- The generic receipt and the Host evidence cannot be mapped honestly, and a new cross-product
  evidence schema decision would be needed.
- It would be necessary to delete, rewrite, or regenerate the accepted M0–M2-W history.

## Delivery and result package

The execution agent must finally return:

1. The starting commit, platform, branch, and ending commit;
2. The acceptance result for each of items 1–15;
3. Key implementation notes on the transport, Artifact binding, package v3, lifecycle, and
   evidence mapping;
4. The list of modified files and an explicit conclusion on the diff against the forbidden
   scope;
5. The test commands actually run, the counts, the GitHub checks, and the repair record after
   failures;
6. The location of the fake Host evidence, and the malformed/timeout/quarantine/secret-scan
   counterexamples;
7. The worktree, leftover processes, external side effects, and whether anything was pushed
   or a PR opened;
8. The unverified boundaries and the real Windows task inputs that M3-WI needs;
9. The execution conclusion may only be `passed`, `failed`, or `blocked`. Do not declare on
   your own that M3 or a product Gate has passed.

After receiving the result, the lead agent makes the Gate decision under the
[cost-controlled review protocol](agent-operating-model.md) and does not redo the whole
execution task by default.
