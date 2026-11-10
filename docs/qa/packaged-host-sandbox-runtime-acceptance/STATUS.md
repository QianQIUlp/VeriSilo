# Packaged-Host Windows Sandbox Runtime Acceptance — STATUS (2026-09-14)

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

## Verdicts and classifications

- **Host Runtime Environment**: `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`
  - On the host (Windows 11 Build 26200), Gecko child processes crash with the MSVC `0xc06d007e` missing delay-load module exception, so they cannot be spawned natively.
- **Alternate Environment (Windows Sandbox)**: `ALTERNATE_ENVIRONMENT_ACCEPTED`
  - The native Windows Sandbox (Windows 11 Enterprise Build 26100, UBR 9445) environment is healthy and supports full multi-process tree spawning and hardware acceleration.
- **Packaged Host Runtime Acceptance**: `CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`
  - The self-contained Dev Engine Package built from the current source passed all 4 real-browser session verifications and process-tree cleanup inside the healthy Windows Sandbox.
- **Overall Verdict**: **`PASS`**

---

## 1. Task background and acceptance goals

The previous phase (`managed-runtime-proof-repair`) established the host environment blocked verdict (`CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`). It also confirmed in a pristine Windows Sandbox image that the frozen browser binaries are intact and can spawn multiple processes. But the sandbox lacked a controlled test toolchain, so it stayed at `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`.

This task, on the `integration` lane, completes the minimal closed loop of the runtime proof chain:

1. Build the Supervisor fresh from the current canonical baseline (`521ae2496a90fd45d001e6b2864f20e67ea4586e`) and produce a unique, source-bound, standalone Dev Engine Package (without overwriting the historical `rc2`/`rc3`, and with code signing forbidden);
2. Build a self-contained Windows Sandbox automated acceptance driver (PowerShell native named-pipe IPC + independent binary/JSONL channels);
3. Run the 4-stage real-browser acceptance matrix in the healthy Windows Sandbox isolated container, collect the security evidence in full and write it to disk.

---

## 2. Baseline, artifacts and code provenance

- **Canonical Baseline**: `521ae2496a90fd45d001e6b2864f20e67ea4586e` (`origin/baseline/dev`)
- **Integration Task Branch**: `agent/integration/packaged-host-windows-6ea6c8`
- **Worktree**: `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-packaged-host-windows-6ea6c8`
- **Dev Engine Package**: `artifacts/qa/dev-engine-package-521ae24`
  - Package Tree SHA: `cf002e2c2c77f87b6b26efe31a8228ac733b0f36dd0c55d7c749fe8c827492ee`
  - Browser Tree Manifest SHA: `d77002d0f872a1ca57675d9b3bc2f9d88769e406d2d082cd94b07d78c9f075e6`
  - Host Executable (`camoufox-host.exe`) SHA: `8a554956e59555e9d6f6b95001830f2a14ae95ed41e755e98245b6b1461c4611`
  - Windows Supervisor (`verisilo-camoufox-supervisor.exe`) SHA: `d9c2e0c7c1a5efab1bfb4a89210bd37ad59342bc80e54bf2c72394a8602fad87` (fresh Release build, built from the current worktree)
  - Manifest (`engine-package.json`) SHA: `64eb046db3ecd91b7689ad393f15860e4e5a1d8b86ae056e324526b470b1fe3c`
  - Probe HTML (`probe.html`) SHA: `20306c39a9ca3c1c9960547066ca083068fc0d5123c4fd628a7d87c10531a08d`
  - Classification: `UNSIGNED_DEV_ENGINE_PACKAGE` (code signing is strictly forbidden; no signing credentials were queried or mocked)
  - _Provenance reconciliation note_: `package-provenance.json` in the same directory is the authoritative record of the input artifacts as loaded through the sandbox mapping. The Package Tree and Host SHA recorded in an earlier document draft were mixed up with historical rc2 artifact hashes. They have been fully corrected and aligned.
- **Acceptance Driver & Fixtures**: `docs/qa/packaged-host-sandbox-runtime-acceptance/tools/`
  - The full driver suite that actually passed the acceptance tests is now version-tracked (including `bootstrap.ps1`, `sandbox-acceptance.ps1`, `package-provenance.json`, the `legacy/` fixture and its loading logic).

---

## 3. Windows Sandbox acceptance matrix results

The sandbox acceptance script `scripts/run-sandbox-acceptance.ps1` drove the native Windows Sandbox to map the inputs automatically and execute. The whole run took 157.3 seconds, and the results are summarized in `results.json`:

### 1. Session 1: main functional acceptance and Evidence Coverage Closure II (1280x800)

- **Artifact Provision**: `identity-86e45f432062e595dc257f24` (preset: `balanced-en-us`, window: 1280×800, fontMode: `managed`)
- **Spawn Time**: 31.8s (including the supervisor, the Gecko main process and 7 child processes)
- **Signals**:
  - `webgl2Vendor`: `Google Inc. (Intel)` (`state: matched`, consistent with the Artifact declaration, **PASS**)
  - `webgl2Renderer`: `state: unavailable` (`unavailableCorrect: true`, strictly conforms to the contract, **PASS**)
  - `acceptEncoding`: `gzip, deflate, br, zstd` (`state: matched`, consistent with the Host contract, **PASS**)
- **Window Geometry Invariants**:
  - `screenX == screenLeft`: True (0 == 0)
  - `screenY == screenTop`: True (0 == 0)
  - `screen.availLeft <= screenX`: True (0 <= 0)
  - `screenX + outerWidth <= availWidth`: True (0 + 1166 <= 1280)
  - `screen.availTop <= screenY`: True (0 <= 0)
  - `screenY + outerHeight <= availHeight`: True (0 + 632 <= 800)
  - Geometry invariants verdict: **PASS**
- **Fresh Recheck & Page Sentinel reconciliation**:
  - After the start page's first screen stabilized, the Page Sentinel Token was injected: `window.__verisilo_sentinel`
  - Sent the `reobserve_identity` command: re-read the Artifact from disk, started a temporary loopback probe page to complete an independent reconciliation, and closed the temporary page
  - After reconciliation, the user's current page state was checked:
    - `timestampAdvanced`: True (`2026-09-14T12:04:51.782551Z` > `2026-09-14T12:04:38.588065Z`)
    - `sentinelIntact`: **True** (the Sentinel Token stayed undamaged)
    - `urlIntact`: **True** (the URL of the user's active page stayed unchanged)
    - `webgl2VendorReobserved`: **True**
    - `webgl2RendererReobserved`: **True**
    - `acceptEncodingReobserved`: **True**
  - Session 1 overall verdict: **PASS**

### 2. Session 2: cold restart and Profile reuse (1280x800)

- **Profile Reuse**: reused the Profile directory `profile-1280-a` and the Artifact from Session 1 for a cold restart;
- **Window Geometry**: observed `outerWidth=1166, outerHeight=632, screenX=0, screenY=0`; all 6 geometry invariants **PASS**;
- Closed gracefully and exited. Session 2 overall verdict: **PASS**.

### 3. Session 3: geometry coherence at a second size (1024x768)

- **Artifact Provision**: `identity-01d09cbe572a2405074aaba2` (window: 1024×768);
- **Window Geometry**: observed `outerWidth=1024, outerHeight=632, screenX=0, screenY=0`; all 6 geometry invariants are fully satisfied on a screen of a different size, with no false clamp failure;
- Session 3 overall verdict: **PASS**.

### 4. Session 4: Legacy Artifact coordinate correction and immutability of the on-disk file

- **Staged Legacy Artifact**: a conforming Legacy Artifact `identity-f49f1202d9ac4d79f991f794` was generated in advance by the host Python `compute_artifact_digest` + `write_artifact_with_sidecar`, with severely out-of-bounds coordinates `screenX: 2232, screenY: 140` injected;
- **Launch & Clamping**:
  - SHA-256 of the on-disk file before launch: `2edabe511a057dc165a5c469df697b4ecaaf2cdbc134700f2d6cf1a0c653e103`;
  - The browser started and, in interactive mode (`VERISILO_INTERACTIVE=1`), automatically corrected the window coordinates:
    - Actually observed: `screenX = 114, screenY = 140`
    - Computed maximum usable boundary of the virtual screen: `maxX = 1280 - 1166 = 114`
    - `screenX` was successfully clamped dynamically from the out-of-bounds `2232` to `114`, inside the virtual screen boundary;
    - All 6 geometry invariants **PASS**;
  - After the process exited gracefully, the SHA-256 of the on-disk file was read again: `2edabe511a057dc165a5c469df697b4ecaaf2cdbc134700f2d6cf1a0c653e103`;
  - `artifactFileUnchanged: True` (the on-disk Artifact bytes did not change at all. The correction took effect only in runtime memory, which also proves the on-disk Artifact was not touched);
- Session 4 overall verdict: **PASS**.

### 5. Cleanup: process and lock cleanup check

- After the 4 sessions ended, it waited 2 seconds and then checked the sandbox for leftover processes:
  - `camoufox.exe` leftover count: **0**
  - `camoufox-host.exe` leftover count: **0**
  - `verisilo-camoufox-supervisor.exe` leftover count: **0**
- The Profile locks and Job Objects were released normally. Cleanup verdict: **PASS**.

---

## 4. Security and data isolation guarantees

1. **Zero secret exfiltration**:
   - The temporary Profile, Cookies, Sqlite cache and temporary Artifact were all stored in the sandbox's local temporary directory (`C:\Users\WDAGUtilityAccount\AppData\Local\Temp\verisilo-acceptance-*`) and were fully removed before the test ended;
   - The files mapped back to the host directory (`docs/qa/packaged-host-sandbox-runtime-acceptance/`) contain only safe summaries and structured observation results (`results.json`, `geometry-observation.json`, `identity-evidence-summary.json`, `environment.json`, `package-provenance.json`, `driver.log`, `host-stderr.log` and the sentinel of each stage). They contain no seed, no raw cookie and no personal data.
2. **Zero damage to the host environment**:
   - No global package was installed on the host system, and no host system font or registry was modified.
3. **No RC / Release Gate opened**:
   - The outputs of this run are used only for QA / Integration Acceptance. They do not modify the UI, do not touch production packaging and do not open a release gate.

---

## 5. Final conclusion and proof chain status

| Gate / proof item            | Status                                                                 | Proven fact                                                                                                                                                                                 |
| ---------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Window Geometry Coherence    | **`ACCEPTED_IN_WINDOWS_SANDBOX`**                                      | The four scenarios 1280x800, 1024x768, cold restart and out-of-bounds Legacy Artifact passed all invariants and the dynamic clamping check in a real browser                                |
| Evidence Coverage Closure II | **`ACCEPTED_IN_WINDOWS_SANDBOX`**                                      | The WebGL2 Vendor/Renderer and Accept-Encoding signals, Managed font masking, and the preservation of the Page Sentinel and URL were fully verified in real-browser runs and re-observation |
| Current Host Environment     | `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`                              | The known fault of the Windows 11 Build 26200 host environment is kept on record                                                                                                            |
| Windows Sandbox Environment  | **`ALTERNATE_ENVIRONMENT_ACCEPTED`**                                   | The native Windows Sandbox Build 26100 passed the full runtime acceptance                                                                                                                   |
| Packaged-Host Runtime Proof  | **`CURRENT_SOURCE_PACKAGED_HOST_RUNTIME_ACCEPTED_IN_WINDOWS_SANDBOX`** | The Host packaged from the current source completed the full closed loop in a healthy Windows environment                                                                                   |
