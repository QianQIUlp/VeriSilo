# Desktop development journey — 2026-09-24

- Source: `9d7c238c4a0442333af07eab5454311495bfe3f7` (`origin/baseline/dev` at task start)
- Host: native Windows 11 Pro `10.0.26200`, x64
- Mode: Tauri dev with Vite HMR and real Rust backend, port `15544`
- Isolation: named Vault `qa-desktop-dev-journey-e690d6`; default Vault was not used

## Reproduction and observations

1. Started `node scripts/dev-desktop.mjs core --port 15544 --vault qa-desktop-dev-journey-e690d6`. The Tauri dev executable started and the local CLI returned `vault.state=uninitialized` and `activation.state=idle`.
2. Initialized the named Vault with a generated, transient passphrase. Created a direct Standard Silo named `qa-standard-journey` using the installed Microsoft Edge executable. CLI listed the new Silo and its isolated profile directory. Locked and unlocked the same Vault successfully.
3. Ran `verisilo-cli --vault qa-desktop-dev-journey-e690d6 --json start qa-standard-journey`. The backend returned `state=running`, `activeSiloId` matching the created Silo, and `browserVerification.state=verified` for Edge `153.0.4234.48`.
4. Standard `stop` refused to terminate the browser and instructed the user to close its window. Located the Edge root process by the unique QA Vault path in its command line, then called `CloseMainWindow` on that exact process. The backend reported `state=stopped` and `activeSiloId=null`.
5. Started the same Silo a second time, observed `running` and `browserVerification.state=verified` again, closed its exact window, and observed `stopped` with no active Silo. Stopped the QA desktop service and dev process cleanly.

## Coverage boundary

This is real native Windows development runtime evidence for the named Vault and Standard Silo lifecycle through the CLI and Rust local API. It is not GUI interaction acceptance, an installed application test, or an RC/release candidate. The dev worktree had no staged `target/verisilo-managed-browser-resources/engine-package`, so no current-source Managed provision/launch claim follows from this run. The installed engine package elsewhere on this machine was not substituted for the missing staged package.
