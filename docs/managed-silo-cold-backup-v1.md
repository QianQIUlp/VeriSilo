# Managed Silo cold backup and original-identity restore v1

Goal of this round: complete the recovery chain for long-lived identity assets. A stopped local Managed Silo is restored together with its persistent browser data, original identity artifact, engine binding and required network configuration, without affecting another Silo. The starting point is `7b0a65dfd444582d6f9459f695f0417ab209480b`. The QA record of this round governs implementation and acceptance status.

## Scope and decisions

- Restore the existing original identity with the same UUID and seed on the same Windows machine, the same system user and the same Vault path. A lost Profile can be restored. If the Silo metadata is also deleted, first restore the existing Vault configuration backup.
- A cold backup requires the browser to be stopped and the Profile lock released. A separate backup password protects the archive. Windows user protection and the identity binding limit the restore scope. Rotating the Vault passphrase should not invalidate a cold backup.
- The full scope is the Profile persistent files, the original Artifact, the identity/engine binding and the required configuration and network secrets. It does not package the engine executable and does not save reusable current-run evidence.
- Before restore, inspect the backup and show its coverage. The confirmation is bound to the archive digest obtained by the inspection. The backend validates again, stages safely, then replaces. An ordinary failure or a process interruption must keep the original data or leave a clear recoverable state.
- Missing network secrets, an incompatible engine, a wrong password, corrupt input, an active Profile, a path escape and an identity conflict must be rejected before the original data is damaged. It must not quietly connect directly.
- Historical run records remain historical. After restore, the Silo must be started again to obtain current evidence. Login state revoked by a server is outside the restore guarantee.

The existing Vault backup still backs up only configuration. The new feature strengthens Identity Integrity, Execution Integrity, Network Integrity and Attribution. It does not widen any fingerprint claim or claim stronger anti-detection.

A Managed run actually uses `silos/<UUID>/profiles/`. It is not enough to copy the `browser-data` placeholder path in the Silo metadata. The run-time `engine-state` and the re-materializable `identity` cache should not be restored as current state. The original Artifact comes from the encrypted Vault record.

The Windows user and machine restrictions must both hold. The [Microsoft DPAPI documentation](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata) notes that a roaming user profile may decrypt CurrentUser data across machines. The archive key is therefore wrapped in two layers, user protection and machine protection. Machine protection alone also cannot replace the user restriction.

## Acceptance and stop conditions

In a separate development Vault, create A and B and write different Cookies, LocalStorage and IndexedDB to a local test site. Stop A and back it up, change its data, then restore and actually start it, reading back the state at backup time. Confirm that the UUID, original Artifact and bindings are kept and that B is unaffected. Use the existing focused tests to cover a wrong password, a corrupt backup, an active lock, incompatible input and failure rollback. The UI Preview checks selection, the inspection summary, overwrite confirmation, errors and secret cleanup. It does not replace Runtime Evidence.

A change to a shared DTO runs the full automated matrix under the integration workflow. The real run uses the existing pinned engine and the CLI with the real backend. It does not rebuild the core, and does not do RC, installer, production packaging or release acceptance. Publishing the development branch/baseline is separate from a product release.

Completion is judged by direct evidence of the user's restore action and the required failure protection. A commit, passing tests or a finished build cannot close the goal alone. If required access or authorization is missing, keep the item open. A tool failure does not make the task take over indefinitely.

## Entry points

After stopping the target Managed Silo, go to the Managed Silo cold backup area in 「保险库与数据」 ("Vault and Data") and choose the identity, the backup file path and a separate backup passphrase. To restore, first inspect the file and check the original identity, engine and data scope, then explicitly confirm the overwrite. After the restore completes, start it again to obtain current Runtime Evidence.

The CLI also provides `backup <name-or-id> <path>`, `backup-inspect <name-or-id> <path>` and `backup-restore <name-or-id> <path> --sha256 <inspected-digest> --yes`. The passphrase is passed through hidden input or standard input and is not put in command arguments. The backup file must be a new path. An existing file is not silently overwritten.

For the results of automation, UI Preview and native browser restore in this round, and the boundary of each, see the [acceptance record](qa/managed-cold-backup-2026-09-28.md).

## Execution constraints for this round

1. Each piece of work maps to a concrete product problem or a necessary uncertainty, and directly inherits valid results and evidence.
2. Each repair is followed by the lowest sufficient validation, preferring existing tools.
3. Without new evidence or a new hypothesis, do not repeat expensive builds, installs and acceptance.
4. A historical Sandbox is not the default point to resume from, and RC or production release flows are not opened automatically.
5. While necessary product work is incomplete, keep the goal open. Limited concurrency and later maintenance phases are not automatically included in this round.
