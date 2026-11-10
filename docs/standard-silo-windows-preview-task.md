# Standard Silo Windows runnable vertical slice

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical task record.** This document freezes the dedicated preview result for Standard Silo
> Profile Isolation. It does not represent the current global phase or the next task. For current
> product routing, see the [Camoufox status page](camoufox-program-status.md).

Status: **completed and frozen**. The Profile Isolation Windows local Preview passed at
`aa72eadaf8300d1cd33a2c32173c06e3e677ca89` / tree
`cd126770be02a33c6bb698853813512748b894c8`.

## Closure status

- The current deliverable is an unsigned local Preview. It is not a signed installer or a shipped release.
- It has been verified on native Windows Server. The normal Windows 10/11 client release matrix is still to be added.
- The capability boundary stops at separate Profile, website-state persistence, single-active ownership and the local Chrome/Edge
  lifecycle. It does not include Managed Identity, fingerprint virtualization, proxy isolation or whole-machine virtualization.
- Camoufox continues to be investigated in the separate Managed Engine worktree. It is not part of this product line's integration.
- The Profile isolation layer is now frozen. It is not expanded further except for real regression defects.

## Goal

Let an ordinary user on native Windows, without understanding EngineAdapter, WSL, proxies or Profile directories,
complete the shortest Standard Silo journey:

```text
Initialize Vault
→ Use the auto-discovered Chrome or Edge
→ Create a Local + Direct Standard Silo
→ Start the separate Profile
→ User closes the browser
→ Desktop returns to idle
→ Start the same Silo again, and the state of that Profile is kept
```

This task optimizes the existing product path. It does not create a new browser backend. Standard declares only separate Profile, login
state and local lifecycle. Hardware and browser fingerprints continue to be written as `native`, `inherit` or
`unavailable`.

## Product results

1. The default main path of “创建 Silo” ("Create Silo") is native Windows, Direct and one auto-discovered system browser.
   The user only needs to name the Silo and confirm to create it.
2. WSL, proxy, manual browser path and other advanced configuration do not take the default main path. Existing capabilities can sit behind
   an explicit advanced entry, but the backend semantics must not change.
3. The main path and the Silo card state clearly that website data and login state are independent, and that device and browser fingerprints still follow
   the local machine. Do not use “受控身份” ("controlled identity"), “指纹隔离” ("fingerprint isolation") or `verified` in a way that misleads Standard users.
4. After launch, clearly tell the user that closing that Silo's browser window stops it. The desktop checks the real process and
   Profile lock and then returns to idle. It does not offer a “强制停止” ("Force stop") button that could kill the everyday browser by mistake.
5. Produce a locally runnable unsigned desktop-only Windows preview. It is not a signed release package
   and must not be called an official installer or a shipped product.

## Allowed scope

- The Standard Silo first-time creation, overview, status and honest copy in `apps/desktop/src/**`;
- Minimal fixes in `apps/desktop/src-tauri/src/**` that relate directly to stock Chrome/Edge, Vault, separate Profile and local
  lifecycle only;
- Necessary reuse of existing desktop tests, `windows_acceptance_driver.rs`,
  `tests/windows/Invoke-VeriSiloWindowsE2E.ps1` and the session-site fixture;
- Synchronizing this task document and the existing manual acceptance guide.

First run and understand the existing path. Fix only what actually blocks, or what completes the explicit product results above. Do not
rewrite working code just to show there were changes.

## Forbidden scope

- Do not modify `apps/camoufox-host/**`, the Camoufox/Controlled Chromium adapter, a Managed
  Artifact or any M0–M3 evidence;
- Do not implement or expand proxy, Mihomo, PAC, WSL, Remote, Sandbox, Hyper-V or installer signing;
- Do not add a production dependency, and do not change the public protocol or data semantics;
- Do not create a new runner, freezer, finalizer, evidence schema or multi-layer manifest;
- Do not touch, delete or migrate the user's existing VeriSilo, Chrome or Edge data. Do not close the user's everyday browser.

## Environment and safety

- Native interactive Windows. Use the current original checkout and do not create a worktree.
- The starting point must be a clean checkpoint dispatched by the lead agent. Stop immediately if any change not belonging to this task is found.
- All automation app data, Profiles, reports and session fixtures live in a single temporary or task directory.
- When starting the preview, use a run-owned `LOCALAPPDATA` to isolate the real `%LOCALAPPDATA%\VeriSilo`.
- Use only an installed and auto-discovered Chrome or Edge. At least one available is enough. Do not install a new browser.
- Do not use real accounts, passphrases, Cookies, proxy secrets or real business websites.

## Acceptance

The execution agent runs the final candidate acceptance only once. It does not pick historical green results to cover a single failure:

1. UI/contract tests prove that the default main path is Local + Direct + auto-discovered browser, that advanced configuration needs
   explicit expansion, and that the Standard boundary copy exists.
2. Reuse the existing Windows desktop-core acceptance driver. On a run-owned root, pass through a real
   Vault, Silo creation, separate `user-data-dir`, stock launch, Profile lock, single-active rejection and
   exact process recovery.
3. Reuse one installed-browser unit of the existing Windows browser E2E to confirm A/B Profile isolation,
   Cookie/localStorage/IndexedDB retention across cold starts, and an unchanged default Profile. A missing second browser
   does not block this slice.
4. Build an unsigned desktop-only Tauri preview. Start the real executable with a run-owned `LOCALAPPDATA`
   and confirm the window process can start with no immediate crash. If there is no reliable desktop automation for the full click journey,
   hand it back explicitly for a manual smoke test. Do not fake it as an automatic pass.
5. At the end, run the regular `pnpm check`, `pnpm test`, `pnpm build` only once, and the desktop Rust
   fmt/test/clippy. Reuse the repository's locked dependencies and do not update the lockfile.
6. At the end the worktree is clean, and no test Chrome/Edge, fixture, Tauri or build process remains. The user's everyday
   browser and default Profile were not operated.

The machine-readable result reuses the existing acceptance receipt/summary. This task returns only a short execution summary and the
preview path. It does not create a new evidence supply chain.

## Stop conditions

- Changing the Managed Engine, network backend, release signing or public protocol, or adding a production dependency, becomes necessary;
- The only way to continue is to close the user's everyday browser, read a real Vault/Profile, or relax a security boundary;
- A real stock browser or the desktop core exposes Profile cross-use, a default Profile modification, or a wrongly killed unrelated process;
- The core journey of the same final candidate fails. On failure, keep the environment as it was left and return to the lead agent. Do not split a new sub-Gate.

## Delivery

- `passed`, `failed` or `blocked`. Do not declare the product Accepted yourself;
- The starting point, the final commit/tree, the modified files and the worktree;
- The actual change to the user path and the boundary copy;
- The desktop-core receipt, the Windows browser summary, and the preview executable / NSIS path;
- Regular validation counts, the manual smoke test not completed, and a check of residual processes and the forbidden scope.
