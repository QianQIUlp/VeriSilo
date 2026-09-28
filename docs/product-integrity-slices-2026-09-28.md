# Product integrity slices — 2026-09-28

Status: implementation and scoped integration verification complete. This is normal product development, not a QA, fingerprint or release Gate.

## Product boundary

This batch closes three concrete gaps: explaining current runtime evidence consistently; confirming the resolved Managed identity after creation; and retaining one recent run per Silo for later attribution. The current baseline at task start is `26d683c79293ae7e9788f56f2432a7a66ed4ca4e`.

Existing reconciliation, Fresh Recheck, profile ownership, identity locking, proxy fail-closed behavior and valid qualification evidence are inherited. Observed equality does not become verified. Missing observations and unsupported comparisons remain unavailable. Identity staleness remains a binding decision, distinct from the age of an observation and network evidence expiry.

## Recent-run decision

Keep at most one most recent local Standard or Managed run per Silo in the existing encrypted Vault. This is a bounded product snapshot, separate from the runtime recovery file. Capture the declaration at the time of the run and only the evidence actually available for that run. Do not reconstruct historical declarations from later Silo settings. Unknown times or missing evidence remain explicitly unknown. Archive preserves the record; deleting its Silo removes it. Vault lock hides it. A record is historical/last known and never supplies current-runtime verification.

Update on meaningful run/evidence/lifecycle changes, not on every status-poll timestamp. No new database, unbounded timeline, cloud service, export format, kernel, probe or environment backend is included.

## Execution constraints

1. Every work item addresses a specific product problem or necessary uncertainty. Inherit existing results and valid evidence.
2. Verify each repair using existing tools and the minimum sufficient checks. Tool failures must not take over the task indefinitely.
3. Reassess expensive retries. Do not repeat complete builds, installs or acceptance without new evidence or a new hypothesis.
4. Historical Sandbox work is not the continuation point. Do not resume RC, production packaging or release workflows.
5. Completion means the product problems are handled. Commits, passing tests and generated packages alone are insufficient; necessary unfinished work remains open.

## Acceptance

- Current evidence has one verdict source and understandable scalar/complex differences, timestamps, attribution and unavailable boundaries.
- A failed re-observation does not relabel an old observation as new.
- Network policy, applied stages and observed results stay distinct, including Direct, proxy, Clash and expiry.
- Creation makes follow-network precedence clear and shows the resolved identity before the optional first launch.
- Recent runs remain attributable across A/B runs and Desktop/Vault reload, with old Vault compatibility, lock and deletion behavior.
- UI checks use Preview; backend persistence and lifecycle use owning tests/CLI where sufficient. No installation or fingerprint requalification is part of this task.

## Validation

- Desktop UI: 160 tests passed, including current evidence, readable complex differences, summary/detail network consistency, resolved creation, history presentation, and Vault UI session isolation. Recursive `pnpm check` passed; the final Desktop type check also passed after the notice adjustment.
- Core: five focused recent-run tests passed. These cover actual encrypted Vault writes, unchanged-poll no-write behavior, separate A/B records, archive/delete/lock, cross-runtime rejection, old payloads without `recentRuns`, early launch failure, Desktop reopen, and stopped-state reconciliation. A simulated filesystem save failure leaves authoritative runtime status readable, emits a typed warning, and recovers on a later successful save.
- Browser UI Preview: configured Japanese/Asia-Tokyo creation receipt; follow-proxy priority disables manual language; matched with unavailable fields; equal-count voice differences show Voice A vs Voice B; failed recheck preserves the earlier observation time; cross-runtime identity/network separation; expired network details; A/B history switch; read/save failures distinct from empty history; lock hides records; stop preserves the selected Silo and shows its ended record. This uses real React components with synthetic API data, not runtime evidence.
- One full integration attempt was run because this slice changes shared contracts and encrypted persistence. It found two obsolete UI copy assertions and one Vault schema-matrix assertion (reported by both Rust test targets). The copy assertions were updated to the unified formal-evidence UI. The schema test now distinguishes allowed fields from required fields: `recentRuns` is explicitly optional for existing schema-v9 Vaults; the prior required-field, unknown-field and downgrade checks remain. The initial failed log is retained locally; no unchanged full matrix was rerun.
- Follow-up validation: recursive `pnpm test` passed (Desktop 160, contracts 66, extension 56, plus session fixture and development-entry self-tests). The repaired schema-matrix test passed in both Desktop and core-harness targets; all five recent-run core tests passed again, including old-v9 payload deserialization through the actual Vault parser.
- Inherited passing groups from that same full attempt: managed contract consistency, recursive type checks, Desktop Rust check, Host package/page-command self-tests and 40 task-routing tests. The Desktop Rust run passed 272 tests and the core run passed 267; their sole failed schema assertion was repaired and separately revalidated as above. Each Rust target retained its existing five ignored tests and three excluded live-Mihomo tests. These are composed verification results, not a claim that the initial full command exited successfully.

No browser kernel, Host reconciliation, probes or qualification inputs are changed. Existing native Managed lifecycle evidence is inherited within its original boundary. No production build, installer or release acceptance is part of this slice.
