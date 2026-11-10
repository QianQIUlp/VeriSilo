# VeriSilo M1.1 + M2-0 + M2.0.1 + M2.0.2 + M2.0.3 — Resolved Camoufox Identity Artifact

> Historical stage report: the table below keeps the Gate verdicts of M2.0.3 as they were at that time. M2-W has since been Accepted, and PR #10 has been merged into `main`. For the current stage and the next task, the authority is the [Camoufox status page](camoufox-program-status.md).

Status: **M2.0.3 accepted on this host — observations only,
`verified: false` everywhere. Host v1 is a runnable Linux prototype; the
long-term schema/version contract is now frozen and old v2 artifacts are
explicitly rejected.**

## Gate summary

| Gate                                             | Verdict                                                                               |
| ------------------------------------------------ | ------------------------------------------------------------------------------------- |
| Linux replay correctness of a valid Artifact     | Passed (v3 Artifact / v2 ObservedWebsiteDigest)                                       |
| Stability and identity separation                | Passed (inherit fontMode, font widths not in the digest)                              |
| Recursive strict Artifact boundary               | Passed (nested required fields + unknown fields + top-level scalar closure)           |
| Schema/digest version freeze                     | **Passed (M2.0.2: v3/v2 upgrade + explicit rejection of old versions)**               |
| Host restart persistence                         | Passed (fixed origin + three kinds of evidence: Cookie API / page / cookies.sqlite)   |
| Normal lifecycle and process-tree cleanup        | Passed                                                                                |
| Fail-closed when an abnormal process cannot exit | **Passed (M2.0.3: whole-tree identity confirmation + three-state atomic quarantine)** |
| Strict Artifact JSON / RFC3339 boundary          | **Passed (M2.0.3)**                                                                   |
| Evidence frozen into Git                         | Passed (tracked fixtures + manifest + code revision)                                  |
| M2-W Windows manual Gate                         | **Allowed at the time of this document; since Accepted**                              |
| Integration into Tauri / EngineAdapter           | Not allowed yet                                                                       |

M0.1 keeps the tightened wording "local compatibility Gate passed":
`noCamoufoxWebdlAttemptObserved` proves only that `camoufox.webdl` was not
triggered (`outboundNetworkFullyObserved: false`), and the raw report is in a
gitignored directory. It is not a verification of the supply chain or of
network behavior.

## Version contract (M2.0.2)

- Artifact: `verisilo-camoufox-resolved-identity/v3`.
- Policy: `verisilo-camoufox-identity-policy/v3` (including the required `fontMode`).
- Projection: `verisilo-camoufox-stable-signal-projection/v3`.
- ObservedWebsiteDigest: `verisilo-camoufox-observed-website/v2`.
- M1 run report: `verisilo-camoufox-m1-run-report/v3`.
- An old `…/v2` Artifact returns `UnsupportedSchemaVersionError` (protocol
  code `unsupported_schema_version`) at the schema check stage. It is **not**
  handled as an ordinary missing-field case. Unknown and future versions are
  likewise rejected explicitly.

## The proven chain

```text
Artifact on disk (single byte read)
→ schema version contract check (old versions explicitly rejected)
→ expectedArtifactFileSha256 + sidecar check
→ unified top-level closed schema (including the 47-key resolvedConfig closure;
  nested required fields, unknown fields and types all validated recursively)
→ generatedBy non-empty string; generatedAtUtc strict RFC 3339 UTC (normalized to Z)
→ browser binding (archive SHA, BuildID, SourceStamp, properties.json SHA)
→ unpacked tree manifest check (689 files / 1,284,408,846 bytes;
  symlinks / non-regular files are likewise rejected)
→ deepcopy config → configuredIdentityDigest
→ launch_options() sends the exact same CAMOU_CONFIG (zero-mutation gate)
→ the website sees stable, distinguishable ObservedWebsiteSignals every time
→ ObservedWebsiteDigest v2 (no artifactId, no internal seed, no canvas,
  no font widths in inherit mode)
```

## Strict validation (M2.0.2)

- The top level uses a **unified closed schema**: schema, artifactId, policy,
  browserRelease, browserBinding, generatedBy, generatedAtUtc,
  generatorVersions, resolvedConfig, stableSignalsDeclared, exclusions,
  configuredIdentityDigest and canonicalDigest must all be present and have
  the correct type.
- `generatedBy`: non-empty string.
- `generatedAtUtc`: strict RFC 3339 UTC, and it must end in `Z` (`+00:00`,
  non-UTC offsets and non-RFC3339 strings are all rejected).
- RFC3339 validation uses an **explicit regex + date parsing**: it accepts
  only `YYYY-MM-DDTHH:MM:SS[.fraction]Z`. Space-separated forms, the basic
  form, missing seconds and non-UTC representations are all rejected.
- `browserRelease`: non-empty string.
- `configuredIdentityDigest` / `canonicalDigest`: `sha256:<64 hex>`.
- Artifact JSON uses a **strict parser**: it recursively rejects duplicate
  keys (nested objects too) and NaN/Infinity, and the top level must be an
  object. Violations uniformly return `integrity_rejected`. They do not show
  up as `internal_error`, and the parser's choice among duplicate values
  creates no ambiguity (this paves the way for a future shared Rust/Python
  format).
- `type(x) is int` / `bool` are rejected. All nested objects reject unknown
  fields and require all required fields. The `.sha256` sidecar is required.
  Artifact bytes are read only once (no TOCTOU).

## Dual digest rules

Canonical JSON: UTF-8, recursively sorted keys, compact separators,
`ensure_ascii=false`, `allow_nan=false`.

- Artifact canonicalDigest: the digest of the canonical JSON after removing
  `canonicalDigest`.
- ConfiguredIdentityDigest (v1 semantics unchanged): the canonical digest of
  resolvedConfig (may contain the seed, does not contain artifactId).
- ObservedWebsiteDigest (v2): website-visible values only (no artifactId, no
  internal seed, no canvas, no Artifact font input; no font widths in inherit
  mode).

## Font policy

`policy.fontMode` has two values, `inherit` and `managed`:

- `inherit` (the current three fixtures): `fontUniverseWidths` is
  **host-bound** evidence and does not enter the ObservedWebsiteDigest.
  `fontNegativeControls` (forged fonts) are kept.
- `managed`: font widths enter the digest only when all host negative controls
  are unavailable in the page. Otherwise the browser is **launched
  temporarily and probed**, then cleaned up, and the launch fails before it
  enters `running` (`host_font_masking_failed`). This is **not** a "failure
  before the browser starts", because the detection must be done in a real
  page.

Host font masking is still unsolved. Therefore the digest can be stable on
the same Linux host, font-related digests are not necessarily stable across
hosts, and font isolation is **not claimed**.

## Process identity and quarantine (M2.0.2)

- `exit_supervisor` writes the supervisor PID, child PID, start-time ticks
  and process group. The Host uses them to establish the managed process
  identity, and no longer guesses the PID from the `/proc` cmdline.
- Before signaling, it checks **PID + start-time**. When a PID is reused but
  the start-time differs, it is not treated as the original process and is not
  killed by mistake.
- Normal close/fail: the profile lock is released only after the whole managed
  process tree is confirmed to have exited.
- When exit cannot be confirmed (`processTreeExit.exited !== true`): the state
  is **quarantined**, the current Host **keeps holding the profile lock**, and
  writes `stateRoot/quarantine/<profile>.json` (containing the PID,
  start-time and process group). It is never marked as exited.
- M2.0.3: `exited=true` is returned only after **all captured PID+start-time
  identities (root and descendants)** have disappeared. Surviving descendants
  are counted in `remaining` and written to quarantine.
- The quarantine record uses a **three-state read** (absent / valid /
  invalid): when the file exists but is truncated, is missing fields, has a
  type error or is unreadable, it **blocks takeover**. Writes are atomic
  replacements (same-directory temp file + fsync + `os.replace` + directory
  fsync), and a write/delete failure is fail-closed.
- A new Host can clean up the record and take over only after it has verified
  that all PID+start-time identities in the quarantine record no longer
  exist. Otherwise it returns `profile_quarantined`.

## Canvas classification (unchanged)

Raw pixels are stable, but seed noise was not exhibited by this probe
scenario. The export (toDataURL) is not stable across restarts. Neither is
counted in the ObservedWebsiteDigest.

## Test results (M2.0.2 final)

### Stability: identity-a × 5 cold starts

Accepted run: **`run-1786158540-228a3340`**
(report.sha256 `b15625fd…b323d`).

| Start | disk==sent | diff  | Exit code | exit file | Profile fresh | ObservedWebsiteDigest v2                                                  |
| ----- | ---------- | ----- | --------- | --------- | ------------- | ------------------------------------------------------------------------- |
| 1–5   | true       | empty | 0         | present   | yes           | `sha256:70f71b5ca7ee287a4d5e989086a990264433129615f0ee988aa18c135385d0ff` |

5/5 digests identical; `artifactFileSha256EveryStart` is the same all 5
times; `fontModeEveryStart` is `inherit` throughout. **Passed.**

### Separation: identity-a/b/c

Accepted run: **`run-1786158560-43ea2bd1`**.

| Artifact   | canonicalDigest        | ObservedWebsiteDigest v2 |
| ---------- | ---------------------- | ------------------------ |
| identity-a | `sha256:4f602179…58e8` | `sha256:70f71b5c…d0ff`   |
| identity-b | `sha256:0cb8e396…42f7` | `sha256:8559d3e0…6324`   |
| identity-c | `sha256:d6b8ad04…713f` | `sha256:f873b3dd…6de13`  |

All pairwise different. Exit code 0, exit file present, Profile fresh and
zero config mutation are all satisfied. **Passed.**

### Tamper: four modes rejected before launch

Accepted run: **`run-1786158573-fef25c08`**. digest / missing-field /
type-error / policy-mismatch are all rejected. The Host integration test
additionally verifies that deleting `policy.canonicalJsonRule` (with the
digest and sidecar recomputed) gives `integrity_rejected` before launch.

### Unpacked tree

`browser-tree-manifest.json` (689 files, 1,284,408,846 bytes, manifest SHA
`20807dd2…bae75`) is verified before every run. Missing / extra / modified
files, symlinks, FIFOs and other non-regular entries are all rejected.

## Unit and integration tests

- `test_identity_artifact.py`: 21/21 passed (new: old v2 Artifact →
  `UnsupportedSchemaVersionError`; non-string `generatedBy` rejected;
  `generatedAtUtc` bool / non-RFC3339 / non-UTC / non-Z normalization
  rejected; `browserRelease` and digest format rejected; loose RFC3339 forms
  rejected; the strict JSON parser rejects duplicate keys / NaN / non-object).
- `test_host_v1.py`: 19/19 passed (new: protocol-level v2 →
  `unsupported_schema_version`; after a managed font failure the browser is
  cleaned up and the state was never `running`; quarantine blocks takeover
  until the original process identity disappears; same PID with a different
  start-time is not treated as the original process; quarantine keeps the
  profile lock; counter-example with the root exiting and a SIGTERM-ignoring
  descendant surviving; a corrupt quarantine file blocks takeover; atomic
  write / failure is fail-closed; non-object / duplicate key / NaN Artifact
  gives protocol-level `integrity_rejected`).

## M2 gate

**Entry into M2-W (the Windows manual Gate) is allowed.** The Linux
standalone Host v1, as a "runnable Linux prototype", has passed the M2.0.2
corrections and acceptance (version contract freeze, top-level type closure,
process identity / start-time check, quarantine fail-closed). M3
(EngineAdapter/Tauri) is still not allowed. The order of Vault, proxy and UI
is unchanged.

## Reproduction

```bash
cd apps/camoufox-host
uv sync --frozen --offline
uv run python test_identity_artifact.py
uv run python test_host_v1.py

uv run python run_identity_spike.py stability --artifact ../../tests/fixtures/camoufox/identity-a.json --runs 5
uv run python run_identity_spike.py separation --artifacts ../../tests/fixtures/camoufox/identity-a.json,../../tests/fixtures/camoufox/identity-b.json,../../tests/fixtures/camoufox/identity-c.json
uv run python run_identity_spike.py tamper --artifact ../../tests/fixtures/camoufox/identity-a.json --out-dir ../../artifacts/camoufox-m1/tampered
```

Evidence index: `tests/fixtures/camoufox/evidence-manifest.json` (tracked;
contains run-ids, report digests, raw file SHAs, browserBinding, the unpacked
tree digest, the schema version contract and the code Git revision).
