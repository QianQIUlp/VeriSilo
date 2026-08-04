# VeriSilo M2 — Standalone Camoufox Host v1 (Linux stdio protocol)

Status: **Host v1 integration acceptance passed on this host — observations
only, `verified: false` / `observed-on-this-host` on every response.**

## Scope

`apps/camoufox-host/host_v1.py` is a single-instance local Host that owns one
Camoufox browser session at a time over a JSON Lines stdio protocol. It does
not touch Tauri, the Rust EngineAdapter/Launcher, Vault, proxy secrets,
Windows packaging, production signing, auto-download, or `latest` versions.
The M0-verified pinned browser, controlled cache, explicit executable, and
DownloadGuard are used for every launch.

## Protocol

- One JSON object per line on stdin/stdout (LF-terminated).
- Maximum frame size: **32 KiB** (requests and responses).
- stdout carries **only** protocol frames; all logs go to stderr.
- Frames are rejected for: duplicate JSON keys, unknown fields, oversized
  frames, invalid UTF-8, malformed JSON, non-object frames.

Commands:

| Command | Params | Result |
| --- | --- | --- |
| hello | — | protocol/hostVersion/roots/browserRelease/assetSha256/state |
| launch | artifactId, profileId, expectedArtifactFileSha256 | sessionId, state, digests, bootCount |
| status | sessionId? | state machine snapshot |
| close | sessionId | exited + exitStatus |
| shutdown | — | state shutdown + selfCheck (argv/stderr secret scan) |

Callers only pass `artifactId` / `profileId`; paths are never accepted.
`artifactId` must match `identity-*` and `profileId` is restricted to
`[a-z0-9][a-z0-9-]{0,63}`. Roots are fixed at process start with
`--artifact-root`, `--profile-root`, `--state-root` (defaults under
`tests/fixtures/camoufox` and `artifacts/camoufox-m2`).

## State machine

```text
idle -> starting -> running -> closing -> exited
                            \-> failed (browser crash)
```

Only one session is active at a time (`session_busy` otherwise). Profiles are
persistent directories guarded by an exclusive `flock`; a concurrent launch of
the same profile returns `profile_in_use`. On crash the lock is released
immediately and the profile can be relaunched.

## Launch guarantees (per launch)

1. Artifact bytes read exactly once; raw SHA == `expectedArtifactFileSha256`
   == sidecar.
2. Recursive strict schema validation (types, unknown fields, policy/config
   consistency).
3. Browser binding check: archive SHA/size, BuildID, SourceStamp,
   properties.json SHA, generator versions.
4. Extraction tree verified against the tracked
   `browser-tree-manifest.json` (689 files / 1,284,408,846 bytes; missing,
   extra, or modified files rejected before launch).
5. `deepcopy` of the resolved config; `configuredIdentityDigest` recorded
   before `launch_options()`; sent `CAMOU_CONFIG` must be byte-identical —
   any added/changed/removed key aborts before the browser starts.
6. Explicit executable / profile / controlled cache; DownloadGuard installed.
7. Probe page reports ObservedWebsiteDigest (fixed font universe, no canvas,
   no internal seeds, no artifact-supplied font input).

## Evidence and self-check

Each session writes `stateRoot/<sessionId>/session.json` (state, digests,
exit status, failure) and `observed.json` (full probe + projection), plus
`browser.log`. `shutdown` returns `selfCheck` with matches from a secret
pattern scan of the Host argv and the stderr log.

## Test results

`apps/camoufox-host/test_host_v1.py` (7/7 passed):

- hello returns fixed protocol/version binding (`verisilo-camoufox-host/v1`,
  hostVersion 0.1.0, browserRelease v152.0.4-beta.28).
- launch → status running → close with exit code 0.
- Host process restart preserves Cookie/LocalStorage (bootCount 0→1, then
  1→2) with identical ObservedWebsiteDigest.
- Three Host-managed cold starts of identity-a produce the same
  ObservedWebsiteDigest (`sha256:1bfa0ca0…cd905`).
- Concurrent launch of the same profile returns `profile_in_use`.
- Wrong expected SHA / missing config field / extraction-tree tamper are all
  rejected before launch (`integrity_rejected`).
- Browser crash transitions to `failed`, releases the profile lock, and the
  same profile relaunches successfully.
- stdout stayed pure protocol JSON in every test; shutdown selfCheck found no
  artifact seeds or secrets in argv/stderr.

## Known limitation (font masking)

The fixed-universe font probe and host-font negative controls are implemented.
On this host the page can still see host-installed DejaVu/Liberation families
through `document.fonts.check` even when they are not in the artifact font
list — the injected list does not fully mask the host font set in this
configuration. This is recorded as evidence (`hostFontMasking` failures), does
not gate the digest-based stability/separation results, and must be addressed
before font isolation is claimed (M2+/M2-W).

## Reproduce

```bash
cd apps/camoufox-host
uv sync --frozen --offline
uv run python test_identity_artifact.py
uv run python test_host_v1.py
```

Example session:

```bash
printf '%s\n' \
  '{"id":"1","command":"hello"}' \
  '{"id":"2","command":"launch","params":{"artifactId":"identity-a","profileId":"p1","expectedArtifactFileSha256":"<64-hex>"}}' \
  '{"id":"3","command":"status","params":{"sessionId":"<sessionId>"}}' \
  '{"id":"4","command":"close","params":{"sessionId":"<sessionId>"}}' \
  '{"id":"5","command":"shutdown"}' \
  | uv run python host_v1.py
```
