# Clean M3-WI native Windows Desktop / Real Host contract

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical contract/evidence record.** The test-only qualification of Attempt 4 is
> complete. This document is not the current RC, the current product acceptance, or the
> current next task. For current routing, see the [Camoufox status page](camoufox-program-status.md).

- Status: **Attempts 1–3 immutable Failed; Attempt 4 corrected input frozen**
- Freeze date: 2026-08-28
- Execution branch: `codex/camoufox-m3-engine-adapter`
- Definition baseline: `02e5bad7962aabcf44839e3a69c7c5d21d7b7927`
- Historical boundary: the [old M3-WI contract and investigation](camoufox-m3-wi-windows-task.md)
  stays Failed / Inconclusive. Its 1–16 matrix, its R1/R2/R2H runners, freezer, schema, and
  evidence manifest are not revived.

This document freezes only one new question, its inputs, its direct criteria, and its stop
conditions for the clean M3-WI. The product direction and the existing facts are governed by the
[identity platform north star](identity-platform-north-star.md), the
[Camoufox-first decision](camoufox-managed-engine-decision.md), and the
[current status page](camoufox-program-status.md) respectively.

## The only product question

On native Windows, can the existing desktop-backend `RuntimeManager`, through a test-only exact
`EngineAdapter` plan, the existing `camoufox-host-jsonl-v1` transport, and the real
`host_v1.py` implementation, drive the Formal-v3 Engine and Artifact v6 that already passed
FP2/FP3/FP4 through two sequential `launch → running → clean stop` cycles on the same Silo /
Persistent Profile? While doing so, can it keep the bindings and the layering of the Artifact,
Engine, Network Policy, and Runtime Evidence honest, and avoid passing off a standalone
`IdentityTemplate` declaration as a fact that the Artifact applied?

This is a desktop backend integration Gate. It is not a UI, production package, signing,
installer, or release Gate. Even if Passed, it must keep:

- `productionPackageVerified=false`, `shipped=false`, `verified=false`;
- `packageVerification=not_requested`, `hostLaunch=observed`, `verifiedAdapter=null`;
- Host self-reports support at most the corresponding `applied/observed` and cannot be
  promoted to `verified`.

FP4 is the last Gate of the fingerprint qualification. It answers only whether the frozen
candidate can complete the core tasks of representative ordinary sites. It does not prove
"undetectable", universal compatibility, commercial product readiness, or desktop integration.
An FP4 Passed therefore supports continuing Camoufox-first, but does not replace this Gate. No
FP5 is created here, and the live-site matrix is not rerun.

## Identity authority and preconditions for execution

The only runtime identity authority of Camoufox is the **Resolved Identity Artifact**.
BrowserForge and desktop templates can at most produce candidates or policy input. What the
Host actually receives and applies is the exact Artifact ID, raw SHA, and schema. The
`IdentityTemplate` is not a second identity that can take effect independently.

The current owning seam does not yet guard this boundary. `SiloEngineConfig::Camoufox`
validates the Template and the Artifact binding separately and establishes no semantic binding
between the two. The Host launch plan also configures identity capabilities from the Template
but hands only the Artifact binding to the Host. After the Host enters running,
`apply_camoufox_host_capability_evidence` currently uses one generic running piece of evidence
to promote all Template-derived capabilities from `configured` to `applied`. The frozen FP3
Template even declares a `1920×1080` screen while the exact Artifact declares `4300×1800`, so
this is not a theoretical risk.

Before a native attempt begins, a minimal local fix must be made and pass a focused test:

- The generic Host running evidence may promote only the directly bound `ProfileIsolation`
  from `configured` to `applied`.
- The capabilities for `IdentityTemplate`, UA/UA-CH, language/timezone, screen, Canvas, WebGL,
  fonts, media, and realm stay `configured`, unless per-item Artifact-aware direct evidence
  exists in the future.
- The `configured/reachable/applied` state of the required proxy is still given by the
  separate Network Evidence seam and must not be mixed into identity capabilities.
- `hostLaunch=observed`, `verifiedAdapter=null`, and `verified=false` stay unchanged. The Stock
  and Controlled Chromium paths are unchanged.

The focused test only needs to prove that the Profile binding can be promoted and that
Template-derived identity claims are not promoted when the Host is running. No new Artifact
parser, Template→Artifact projection/schema, or generic evidence framework is added here. If the
product in the future still needs the Template to be a visible identity input, it should
separately remove the dual authority or build an explicit, verifiable projection.

## Evidence already accepted and reused directly

- The M3-0 Accepted checkpoint `e96ef3ff3d2a43a46fd39b5e90029aad3e1faccd` already closed the
  package/transport/fail-closed/evidence contract of the fake Host. It does not prove a real
  browser.
- The FP3 authoritative result
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp3-result.json`,
  SHA-256 `8a821eca7b9e11716668d6742ac356743b7438ab2b9a7ca8b0d604264be86e62`,
  has directly proven one native Windows
  `RuntimeManager → required FixedProxy → real Host → Formal-v3` cycle, the observations of
  the external network exit/Geo/ICE, and a clean stop, with `verified:false`.
- The FP4 authoritative result
  `apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-verisilo-r1-formal-v3-fp4-result.json`,
  SHA-256 `14c7de3a8a14b8037cf0e16ec7b5dc213294b68050665a57513dea79efd8f2de`,
  has directly proven the two sequential lifecycles of the current Host/Engine, the Profile
  replay, and the frozen V5 ordinary-site matrix, with `verified:false`.
- Between the FP3 terminal code revision and the definition baseline, `engine.rs`,
  `launcher.rs`, `launcher_m3_wi_windows_tests.rs`, `domain.rs`, `lib.rs`, and `host_v1.py`
  have no code difference. The `run_spike.py` behavior after FP4 is covered by the FP4 result
  above.

The clean M3-WI therefore does not rerun the FP2 cross-realm, the FP3 external network
exit/Geo/ICE, or the FP4 live-site matrix. What is newly added and not yet directly covered
by the same piece of evidence is only this: **the desktop evidence no longer wrongly promotes
Template claims, and, after the current final inputs go through the desktop composition,
whether the second brand-new RuntimeManager / Host cycle can continue to succeed and release
cleanly.**

## Frozen inputs

- Platform: native Windows interactive desktop. Linux, WSL, Wine, and container results cannot
  stand in for it.
- Silo: ID `74444444-4444-4444-8444-444444444444`, seed reference
  `75555555-5555-4555-8555-555555555555`, the same `SiloEngineConfig::Camoufox` and the same
  run-owned Profile. Phase B must create a new `RuntimeManager` and Host child and must not
  reuse the Phase A process.
- Compatibility Template: reuse the following `IdentityTemplate` of FP3 exactly. It only
  satisfies the existing adapter validation and produces `configured` policy/capability input.
  It is not the runtime identity authority. The whole value is frozen by the SHA of this
  contract:

```json
{
  "schemaVersion": 1,
  "templateId": "73333333-3333-4333-8333-333333333333",
  "os": { "family": "windows", "version": "11", "architecture": "x64" },
  "browser": {
    "family": "firefox",
    "majorVersion": 152,
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:152.0) Gecko/20100101 Firefox/152.0",
    "uaCh": null
  },
  "languages": { "primary": "en-US", "accepted": ["en-US"] },
  "timezone": "Asia/Hong_Kong",
  "screen": {
    "width": 1920,
    "height": 1080,
    "availableWidth": 1920,
    "availableHeight": 1040,
    "devicePixelRatio": 1.0,
    "colorDepth": 24
  },
  "render": { "canvas": "native", "webGlVendor": null, "webGlRenderer": null },
  "fonts": { "families": ["Segoe UI"] },
  "media": {
    "microphones": 1,
    "cameras": 1,
    "speakers": 1,
    "labelsExposed": true
  },
  "network": {
    "proxyRequired": true,
    "countryCode": "HK",
    "timezone": "Asia/Hong_Kong",
    "locale": "en-US",
    "desiredQuic": "browser_default"
  }
}
```

- Resolved Artifact:
  `artifacts/camoufox-fp3-1b-attempt-7/identity-fp3-1b-formal-v3-a.json`, SHA-256
  `8a4cd0d10a0a456678d1f3b4beb1515195d5d171742c4695c2d909132a26e722`; the sidecar file SHA-256
  is `e027eb101fa2783adbc697fa8b47a339e7d66bf00170eacdca7b71a8983f8b86`, and its content must
  also match exactly.
- Engine: `verisilo-camoufox-152.0.4-beta.28-r1-formal-v3`; runtime asset lock
  `artifacts/camoufox-fp2-formal-r1-attempt-8/formal-v3-runtime-asset-lock.json`, SHA-256
  `81e73a69347272d0b770bfa3c9b3eb07449bb165efb0c16948eece2e5a0678ce`; the browser-tree
  manifest raw / canonical SHA-256 are respectively
  `8434ab9925bf0f7d95cc4ff06fe94b7dcf9963a0691f37638469d68cda58ace2` /
  `68d78d0f414d90545691560858b46ed179ee163b7258306c44f0d850bcde6204`; `camoufox.exe`
  SHA-256 `b147602826db5bf852e5777f56cd56036dc04e8ea8868a8e55f8b08744f142a6`.
- Host source: `apps/camoufox-host/host_v1.py`, SHA-256
  `b3b313d4cf6d2eaadceaff4320e5a6bb8afb5d39212652b2c51474eb6809aad0`. Attempt 4 enters
  through `apps/camoufox-host/run_m3_wi_clean_host.py` (SHA-256
  `2015e91bf0902cc6b7276aadb6e8589ca728eb0dc11791a457d0b9744bae5ee8`). This thin entry point
  only reuses the Formal-v3 exact lock/tree verification hooks of the existing
  `apps/camoufox-host/run_fp3_1b_windows.py` (SHA-256
  `73a4fe9b20a95588d8bd03335aeffddf2a93b53cd0ddf9c24301ea99d6437785`), and then restores the
  base `host_v1.CamoufoxHost`. It does not use the `FP3ManagedHost` of FP3, so it does not run
  the network exit, Geo, Geolocation, or ICE/STUN observation. Python dependency lock
  `apps/camoufox-host/uv.lock`, SHA-256
  `41f63b2c12c3102573266b4d9ac002fbd29f7f95cc3d291b8a41d09e411f8f6f`.
- Network Policy: required unauthenticated SOCKS5 `127.0.0.1:7897`. Before execution, run
  `Test-NetConnection 127.0.0.1 -Port 7897` exactly once. If it fails, do not create an
  attempt and do not launch the browser.
- Attempts: `clean-m3-wi-attempt-1`, `clean-m3-wi-attempt-2`, and `clean-m3-wi-attempt-3` stay
  immutable Failed. The corrected new code/input uses `clean-m3-wi-attempt-4`. This is not a
  retry/recovery Gate, and it introduces no delay, sample rotation, cache workaround, or site
  fallback.

Attempt 4 continues to use the native Windows browser authorization that this Gate already
obtained. This Gate neither needs nor visits the exit IP, Geo, Geolocation, ICE/STUN, or
ordinary-site external check services.

## Direct conclusions of Attempts 1–3 and the correction boundary

The `run-report.json` and `native-evidence.json` of Attempt 1 stay immutable Failed. It failed
directly before the Host hello of Phase A, and the browser did not launch. A separate
hello-only diagnostic proved that the `run_spike` asset allowlist of the base Host accepts
only the official/old canvas-v1 lock, and therefore rejects the Formal-v3 runtime lock frozen
by this contract. The resulting `SystemExit` was written to the OS stderr that the launcher
had discarded, so it surfaced finally only as stdout EOF. The engineering Gate stays open.

Attempt 2 passed the Formal-v3 Host preparation above and returned a parseable hello, but was
rejected by the strict root binding before `launch` was sent. The browser still did not
launch. A direct probe proved that the Rust `join("runtime/app")` of the clean test keeps the
`/`, while Python `Path.absolute()` normalizes the same path to `\`. The validator makes an
exact string comparison of the three typed roots, so they are necessarily unequal.

Attempt 3 changed only that test-owned root to a component-by-component
`join("runtime").join("app")`. The clean-only entry point still only reuses the existing
Formal-v3 strict lock/tree verification of FP3 and restores the base `CamoufoxHost`. It does
not widen the trust allowlist of the shared `run_spike`, does not change the Profile,
Artifact, Engine, or Network lifecycle, and does not reuse the FP3 external network
collection.

Attempt 3 then completed the Host `hello → launch → status` and the browser observation of
cycle 1. But the shared `assert_running_evidence` hardcoded the base implementation source
`host_v1.py` as the expected argv, while the launch receipt correctly recorded the thin entry
point `run_m3_wi_clean_host.py` frozen by the contract, so it panicked before the failure
context was recorded. The `RuntimeGuard` drop still closed the exact runtime. The leftover
session directly recorded `state=exited`, `exitStatus=0`, process tree `remaining=[]`, and
Job active count `0`. Attempt 3 stays Failed according to the native evidence.

Attempt 4 only lets the shared assertion accept the actual `host_script` typed input of the
test adapter and makes an exact raw-string comparison on the receipt. It no longer treats the
implementation source as the execution entry point, and it does not do one-sided Windows
`\\?\` canonicalization. The runner still freezes the SHA-256 of the thin entry point, the
Formal input binder, and the base Host source separately, so the execution and origin
bindings are not weakened.

## Owning seam

```text
RuntimeManager::launch_with_identity_deriver
→ cfg(test, windows) exact Camoufox adapter plan
→ desktop ProxyRelay + bind_camoufox_host_proxy
→ spawn_engine_child / spawn_camoufox_host
→ CamoufoxHostTransport hello / launch / status
→ clean-only Formal-v3 input binder → real host_v1.py / base CamoufoxHost
→ exact Formal-v3 browser tree

RuntimeManager::stop_managed_camoufox
→ Host close / shutdown
→ exact Host child exit
→ relay + Profile lease release
```

The test-only adapter replaces only the trusted Host package / signer inputs that do not yet
exist. It must generate a shell-free, strictly typed plan with `package_verification=None`,
and it must not enter a non-test build. After the plan is formed, it must go through the
existing production `RuntimeManager`, relay, transport, binding validation, and stop path. A
direct Python driver cannot stand in for this Gate.

## The only execution sequence

1. From the clean synced Attempt 4 implementation commit, create one run-owned root and copy the
   exact Artifact and sidecar. Record the code revision/tree/origin, the contract SHA, the
   Python path/version, and all the frozen input hashes.
2. Phase A: a new `RuntimeManager` starts the same Silo. Require `Running`, the exact
   Artifact/Profile/Engine binding, the single relay URI in the Host status,
   `ProfileIsolation=applied`, and the Template-derived identity capabilities still
   `configured`. The desktop network evidence is `configured/reachable/applied`, while
   exit/DNS/WebRTC stay `not_requested`. The Vault identity deriver is not called.
3. Phase A clean stop: Host `close/shutdown` succeeds, exit code 0,
   `processTreeExit.exited=true`, Job active count 0, no quarantine/forced cleanup, the exact
   child exits, the relay closes, and the Profile lease is released.
4. Phase B: from the same app root, create another `RuntimeManager` and a different Host PID,
   using the same Silo, Artifact, Profile, and Network Policy. Require the boot count `1 → 2`
   and that the managed cookie and `observedWebsiteDigest` continuity hold.
5. Phase B also clean-stops. All owned PIDs of both cycles go to zero, no unrelated process is
   operated on, and no secret/token enters argv, the wire, the runtime activation, the Host
   state, or the evidence.

Add only one Windows-only ignored focused test (it may live in the existing private test
module or an adjacent clean module) and the minimum code needed to save the direct evidence
above. Do not run the old soak, the failure matrix, desktop-drop, crash, EOF, concurrent, or
ten-cycle scenarios. These are not the current new uncertainty.

## Adjudication

### Passed

The focused evidence-semantics test before execution and all five steps above hold directly,
the inputs/hashes/evidence receipts are complete, the two lifecycles are clean, and all the
honesty boundaries stay `verified:false`. The conclusion can only be **the clean M3-WI
integration slice Passed on this native Windows host**.

### Failed

If the focused test does not pass, the implementation precondition is not met: do not create
an attempt, do not generate a Failed result, and the engineering Gate stays open. Only after
the frozen inputs are available and an attempt has begun, and any required criterion
(RuntimeManager/adapter/relay/JSONL Host binding, the second cycle, Profile continuity, or
clean stop) directly fails, is that attempt adjudicated Failed. A Failed attempt stays
immutable. After a new, evidence-backed root cause is found, any fix constitutes new
code/input and a new attempt. No Failed attempt is rewritten, and no recovery Gate is created.

### Inconclusive

The attempt has begun, but the code/input binding, the raw evidence, or the environmental
attribution is insufficient to adjudicate. Missing evidence cannot be treated as Passed. An
unreachable SOCKS5 port before execution is an unmet input, and in that case no attempt is
created.

## Minimal evidence and stop conditions

An attempt saves only:

- `native-evidence.json`: the direct observations of the two phases, the bindings, the
  lifecycle, the PID/Job/relay/lease, and the honest evidence state;
- `run-report.json`: the inputs/hashes, the execution command/exit, the native evidence SHA,
  the adjudication, and the limitations;
- SHA-256 sidecars for the two JSON files, and the bounded stderr necessary to explain a
  direct failure. No new schema/freezer/manifest framework is created.

The definition/refreeze closes once this contract and the status page pass the focused
document/hash/diff checks, form a clean commit, and are pushed. Then stop. The next task is to
implement this one focused test and, under a new explicit authorization, run the native
attempt. Do not claim in advance that M3-WI Passed, that the production package is verified,
that Managed Identity is shipped, or `verified:true`.
