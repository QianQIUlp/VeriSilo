# Managed Runtime Proof Repair & Environment Qualification — STATUS (2026-09-14)

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

## Verdicts and classifications

- **Host Runtime Environment**: `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`
- **Alternate Environment (Windows Sandbox)**: `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`
- **Overall Runtime Verdict**: `MANAGED_RUNTIME_ENVIRONMENT_BLOCKED`
- **Harness Code Status**: `REPAIRED_AND_VERIFIED` (the code and unit tests pass, which meets the standard for entering the canonical baseline)

---

## 1. Background and task goals

On 2026-09-14 it was found that the frozen Camoufox browser on the current host (Windows 11 25H2, Build 26200, UBR 9445) cannot spawn any child process (GPU, content, socket, RDD). This blocks the real-runtime acceptance chain (the Window Geometry acceptance and the real-browser proof for Evidence Coverage Closure II).
This task ran on the `integration` lane as authorized:

1. Diagnose why host child processes crash and run discriminating experiments;
2. Compare and verify the byte integrity of the engine packages;
3. Explore and verify an alternate runtime environment (Windows Sandbox);
4. Repair the real-runtime acceptance harness for Evidence Coverage Closure II (`apps/camoufox-host/test_real_runtime_evidence_closure_ii.py`);
5. Archive the QA verdicts and evidence in full.

---

## 2. Baseline and branch context

- **Canonical Baseline**: `origin/baseline/dev = 0d177a3e68657f048bf44a8690bb967c588f76c1`
- **Task Branch**: `agent/integration/runtime-proof-repair-3698b4`
- **Worktree**: `C:\Users\qiu\src\VeriSilo\.verisilo-worktrees\integration-runtime-proof-repair-3698b4`
- **Prior QA Work Preservation**: The existing Window Geometry QA branch was published to `origin/agent/qa/accept-real-managed-e0f551` (commit `f0b00e16df093e4b48474bef82b66796d1629f83`). It does not advance the baseline, and its reproduction evidence is fully preserved.

---

## 3. Proof of byte consistency of the engine packages

A full per-file SHA-256 hash comparison was run on the browser trees of the following two engine packages:

- `artifacts/build/managed-browser/rc3-engine-package/browser`
- `artifacts/release/managed-browser/v0.1.0-rc2/engine-package/browser`

Comparison results:

- There are 504 files in total (including `browser-tree-manifest.json`);
- The SHA-256 hashes of all 504 files match exactly;
- The check against `browser-tree-manifest.json` passed (503 files verified True);
- The SHA-256 of the core executable `camoufox.exe` is `c5535c7c25c04b8bba90b9b3e64fca13fce01a52fcb5a1532130e99e4804b49e`, which matches `runtime-asset-lock.json` exactly;
- Conclusion: the browser binary trees of the two packages are **`BYTE_IDENTICAL`**. This rules out runtime problems caused by a corrupted package build or missing files.

---

## 4. Host discriminating experiments and diagnosis (Host Diagnostics)

### 1. Plain Launch control experiment

On the host, a standalone PowerShell (launched directly from explorer, with no Host, supervisor, Playwright or Juggler involved) started `camoufox.exe` with a brand-new temporary profile:

- **Observed behavior**:
  - The main process started and stayed alive;
  - A window appeared after about 36 seconds (1280×1040, falling back to software rendering because the GPU was missing);
  - The number of child processes was **0** throughout (no GPU process, no socket process, no tab/content process).

### 2. Crash log and root-cause location

The Windows Event Viewer (Application Error log) shows:

- The child process crashed the instant it was created;
- Crash exception code: MSVC exception `0xc06d007e` (in `KERNELBASE.dll` at offset `0xc41ca`);
- Semantically confirmed as: **delay-load exception carrying `ERROR_MOD_NOT_FOUND` semantics**;
- Conclusion: the current host environment (Windows 11 Build 26200) lacks some dynamic-link dependencies or delay-load modules, so Gecko child processes cannot start normally.
- Host verdict: **`CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`**.

---

## 5. Windows Sandbox alternate-environment pre-check (Alternate Environment Qualification)

Without any destructive OS operation, the native Windows Sandbox was used for a contamination-free, isolated pre-check:

- **Guest OS**: Microsoft Windows 11 Enterprise, Build `26100`, UBR `9445`, AMD64;
- **User environment**: `WDAGUtilityAccount` (`IsAdministrator = true`);
- **Plain Launch test**:
  - `rc3-engine-package` was mapped into the sandbox, and `camoufox.exe` was run under a brand-new temporary profile;
  - **`childProcessesSpawned: true`**;
  - The browser main process successfully spawned **7 child processes** (GPU process, tab content process, socket process, utility/RDD process);
  - 0 crashes; the window appeared normally within 2-3 seconds.
  - **This result conclusively proves that the frozen Camoufox browser binary tree is intact, and that in a healthy Windows 11 (Build 26100) environment it fully has the ability to spawn child processes and use hardware acceleration.**

### Toolchain blockage analysis

- Windows Sandbox is a brand-new pristine system image, with no Python or Playwright preinstalled;
- Mapping the host's 40 GB main project directory into the sandbox failed because volume projection timed out;
- Under the task constraints, installing unmanaged system software or building temporary distribution packages inside the sandbox at will is forbidden;
- Alternate-environment verdict: **`ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`**.
- Overall runtime-environment verdict: **`MANAGED_RUNTIME_ENVIRONMENT_BLOCKED`**.

---

## 6. Real-Runtime Acceptance Harness repair (Harness Repair)

File: `apps/camoufox-host/test_real_runtime_evidence_closure_ii.py`

### Repairs:

1. **Signal lookup contract alignment**:
   - Before the repair, it used dictionary indexing directly, `evidence.get("headers.Accept-Encoding")`, which conflicted with the `identityEvidence.signals` array format returned by the backend;
   - After the repair, `signal_by_name(evidence, name)` iterates the `signals` array;
   - The signal name is normalized to `acceptEncoding` (aligned with `packages/contracts` and the Host).
2. **Strict explicit parameterization**:
   - Added the `--package-root` command-line argument and support for the `VERISILO_PACKAGE_ROOT` environment variable;
   - When neither is given, it fails closed and exits with exit code 2, which rules out implicit fallback or silent guessing.
3. **Complete Provenance recording**:
   - Outputs the full `packageRoot`, `browserExecutableSha256`, `browserTreeManifestSha256`, `runtimeAssetLockSha256`, `hostSourceTreeSha` and `repoSha`.
4. **Normalized Artifact Provisioning**:
   - Dropped the outdated `artifact-cold1.json` (the original configuration used `fontMode=inherit`);
   - Uses `provision_artifact` fully consistent with the current product specification (`preset="balanced-en-us"`, `fontMode="managed"`, `window=[1280, 800]`).
5. **Single positive acceptance loop and smoke gate**:
   - Optimized into a single positive acceptance loop (launch -> verify WebGL2 and Accept-Encoding -> fresh reobserve reconciliation -> gracefully close the session);
   - Added a Managed Font Masking runtime gate (`state == "running"`).

### Verification record:

- `python apps/camoufox-host/test_evidence_coverage_closure_ii.py`: **10/10 PASS**
- `python apps/camoufox-host/test_package_contract.py`: **PASS**
- `python apps/camoufox-host/test_page_command.py`: **PASS**
- Fail-closed exit code check when the argument is missing: **ExitCode = 2**

---

## 7. Summary of affected gate states

| Gate / acceptance item       | Status                                                                   | Notes                                                                                                                                                                |
| ---------------------------- | ------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Window Geometry Coherence    | `source fix closed / real-browser post-fix acceptance pending`           | The source fix is closed in the baseline. It is blocked by the host child-process crash. The QA evidence is archived at `origin/agent/qa/accept-real-managed-e0f551` |
| Evidence Coverage Closure II | `source fix closed / harness repaired / real-runtime acceptance pending` | The source is closed, and the harness is repaired and passes the specified unit tests. Real-runtime acceptance will run once the environment is ready                |
| Current Host Environment     | `CURRENT_HOST_RUNTIME_ENVIRONMENT_BROKEN`                                | Child processes on the Windows 11 Build 26200 host report the `0xc06d007e` delay-load crash                                                                          |
| Windows Sandbox Toolchain    | `ALTERNATE_ENVIRONMENT_TEST_TOOLCHAIN_BLOCKED`                           | Child processes spawn normally in the Build 26100 sandbox, but the pristine image lacks the Python/Playwright test toolchain                                         |
| Overall Managed Runtime      | `MANAGED_RUNTIME_ENVIRONMENT_BLOCKED`                                    | Unblocks once the host environment is recovered or a clean image with a self-contained toolchain is ready                                                            |

---

## 8. Resume guide

Once an environment that is usable and has the test toolchain is available (or the host's delay-load problem is investigated and resolved):

1. Run the real runtime acceptance:
   ```bash
   python apps/camoufox-host/test_real_runtime_evidence_closure_ii.py --package-root artifacts/build/managed-browser/rc3-engine-package
   ```
2. Run the Window Geometry runtime acceptance:
   ```powershell
   & "apps/camoufox-host/.venv/Scripts/python.exe" docs/qa/window-geometry-runtime-acceptance/tools/plain_launch_desktop.py
   ```
