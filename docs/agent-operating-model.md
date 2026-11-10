# VeriSilo agent operating model

Status: **normative working protocol**. The goal is to match execution cost to task risk
without lowering product semantics, direct evidence, or fail-closed boundaries.
By default, choose the lightest process that reaches the goal. Do not copy the heaviest process used in the past.

## Roles

The lead agent understands the product goal, picks the most valuable problem right now, and rules on cross-module semantics and on what the evidence can support.
An execution agent implements and verifies work within an already defined scope. The lead agent should review key conclusions, but by default does not redo all of
the execution agent's work. When an execution agent hits an unknown that would change the plan, it should return to the lead agent instead of guessing.

Delegate only when parallel work really saves time, or when an independent view lowers the risk of a major judgment. Local edits,
single-file reviews, and mechanical tasks that can be finished directly are not delegated for the sake of "process completeness".

## Four execution levels

### L0: docs, comments, mechanical maintenance

Applies to: docs, links, formatting, renames, and plainly mechanical maintenance that do not change behavior or contract semantics.

- Read the target file and its direct references;
- Check the diff, links, or formatting;
- Do not create a run-id, claim, manifest, task contract, or full regression.

### L1: ordinary local implementation

Applies to: a bug fix or small feature that is reversible, has a clear scope, and involves no browser or external state.

- Check the owning seam, direct callers, and existing tests;
- Make the smallest change at the root cause;
- Run focused tests that would directly fail on this change;
- Widen the regression only when a shared schema/API/core execution path changes.

### L2: build, browser, and evidence tasks

Applies to: browser launch, engine builds, one-shot claims, platform-specific verification, or tasks that produce a formal capability conclusion.

- Freeze the inputs, environment, and stop conditions that directly affect the conclusion;
- Use the existing lock/result/schema. Do not build a separate general provenance platform;
- Keep the raw result, key logs, and artifact hashes needed to support the conclusion;
- After a failure, retry only once a new, reproducible root cause is found;
- Requalify only the binding or evidence boundary that changed.

### L3: product, architecture, and high-risk boundaries

Applies to: product direction, public data/API contracts, security boundaries, secrets, releases, destructive operations, or migrations that are hard to roll back.

- Read the north star and the relevant Accepted decisions;
- State the user-visible consequences and the alternatives;
- Obtain an explicit decision or approval when needed;
- Set verification strength by the actual blast radius, not by words such as "production" or "security".

Start at the lowest level by default. Raise the level only on direct evidence of higher risk or an explicit user request. That an earlier task used
L2/L3 does not mean an adjacent task inherits that level.

## When a task contract is needed

L0/L1 do not get a separate task contract by default. L2/L3, multi-person handoffs, and non-repeatable one-shot executions need only one short contract:

1. The one question to answer;
2. Allowed and forbidden scope;
3. Direct criteria for success and failure;
4. Evidence that must be kept;
5. Stop conditions.

Do not repeat the product history, every non-goal, the full Git lineage, or a theoretical failure matrix for each small phase. Link to the existing source of truth for
that material. Copy a field into the contract only when it changes the ruling for this task.

## Minimum sufficient evidence

- For each key claim, pick the most direct and economical signal;
- When the relevant inputs have not changed, reuse existing hashes, builds, and accepted evidence;
- A docs-only change does not run a browser or a full code regression;
- For a single-file logic fix, prefer a focused test. Do not run every historical suite by default;
- A successful build proves only the build. A runtime observation proves only the corresponding observation. Do not raise the wording;
- Add verification only for contradictory evidence, a failed check, a change to a key input, or an uncovered high-impact path.

Machine evidence lives in locks, results, manifests, tests, and Git. The status page keeps only the current decision and an index. It does not copy
the full SHA of every historical file. Git history itself audits document changes. The status page is not required to keep appending old text forever.

## Lead agent review

The lead agent confirms the following in a cost-controlled way:

1. Whether the result answers the right question;
2. Whether it crosses product, architecture, or forbidden scope;
3. Whether the core conclusion has direct evidence;
4. Whether failures and unverified items are stated accurately;
5. Whether any known direct blocker exists for the next ordinary use.

When nothing contradicts, do not rerun the full matrix, do not recompute all stable material, and do not build a second evidence framework.
List as blockers only the items that would overturn the current conclusion. Other findings go to the backlog or are explicitly dropped.

## Gates and stop rules

A Gate is used only for phase capabilities, L2/L3 execution, or multi-person handoffs. It is not used for every commit. The allowed conclusions are
`passed`, `failed`, `blocked`, or `inconclusive`, strictly limited to the evidence layer actually reached.

An immutable evidence attempt and the engineering Gate it belongs to are managed in separate layers. Keep every `failed` or
`inconclusive` result, but do not use it to end the original Gate or create a recovery Gate automatically. Within the original authorization and risk
scope, close out the causal blockers the evidence supports, validate locally, and then produce a new attempt. Do not retry unchanged inputs or pick samples.

Stop when the goal is complete, the critical path has been directly verified, no evidence-supported blocker remains, and the expected benefit of further checking
is lower than the cost in time, tokens, complexity, or maintenance. Do not keep going just because more checks can be imagined.

## Current project routing

The specific reading rules are in the repository root [AGENTS.md](../AGENTS.md). The current phase, work package and next task are on the [program status page](program-status.md); the plan for each phase, including a kickoff prompt for a new session, is in the [roadmap](roadmap.md). For Camoufox engine Gates and evidence, read the [Camoufox status and evidence index](camoufox-program-status.md). Read historical contracts only when investigating the corresponding run, claim, or decision.

When the owner starts a new session for a roadmap phase, the session's main agent acts as the lead agent: it freezes the task contract for each work package, delegates or executes it, reviews the result with the checks above, and updates the program status page.
