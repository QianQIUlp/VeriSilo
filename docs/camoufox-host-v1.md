# VeriSilo M2.0.1 — Standalone Camoufox Host v1 (Linux stdio protocol)

Status: **Host v1 is a runnable Linux prototype with corrected lifecycle and
persistence evidence. Observations only — `verified: false` /
`observed-on-this-host` on every response. It is NOT yet a fully accepted
product Host: font isolation is not claimed and M2-W (Windows) / M3
(EngineAdapter/Tauri) are not allowed yet.**

## Scope

`apps/camoufox-host/host_v1.py` is a single-instance local Host that owns one
Camoufox browser session at a time over a JSON Lines stdio protocol. It does
not touch Tauri, the Rust EngineAdapter/Launcher, Vault, proxy secrets,
Windows packaging, production signing, auto-download, or `latest` versions.
The M0-verified pinned browser, controlled cache, explicit executable, and
DownloadGuard are used for every launch.

## Protocol

- One JSON object per line on stdin/stdout (LF-terminated).
- Maximum frame size: **32 KiB** (requests and responses). The stdin reader is
  **memory-bounded**: it buffers at most `MAX_FRAME_BYTES`, drains an
  oversized line to its LF terminator, and reports one `frame_too_large`
  error without unbounded allocation.
- stdout carries **only** protocol frames; all logs go to stderr.
- Frames are rejected for: duplicate JSON keys, unknown fields, oversized
  frames, invalid UTF-8, malformed JSON, non-object frames.

Commands:

| Command | Params | Result |
| --- | --- | --- |
| hello | — | protocol/hostVersion/roots/probePortPolicy/browserRelease/assetSha256/state |
| launch | artifactId, profileId, expectedArtifactFileSha256 | sessionId, state, digests, bootCount, managedPids, cookieEvidence, probePort |
| status | sessionId? | state machine snapshot |
| close | sessionId | exited + exitStatus + exitFileObserved + processTreeExit + cookieSqlite |
| shutdown | — | state shutdown + selfCheck (argv/stderr secret scan) |

Callers only pass `artifactId` / `profileId`; paths are never accepted.
`artifactId` must match `identity-*` and `profileId` is restricted to
`[a-z0-9][a-z0-9-]{0,63}`. Roots are fixed at process start with
`--artifact-root`, `--profile-root`, `--state-root` (defaults under
`tests/fixtures/camoufox` and `artifacts/camoufox-m2`). `--probe-port`
(default 0 = ephemeral) pins the probe origin so cookie/localStorage persist
across Host process restarts; after the first launch the Host remembers the
actual port for later launches in the same process.

## State machine

```text
idle -> starting -> running -> closing -> exited
                            \-> failed (browser crash)
```

Only one session is active at a time (`session_busy` otherwise). Profiles are
persistent directories guarded by an exclusive `flock`; a concurrent launch of
the same profile returns `profile_in_use`.

**Lock ordering (M2.0.1):** the profile lock is released LAST. Close/failure
paths first stop the monitor, close the Playwright context, terminate and
CONFIRM the entire managed process tree is gone, shut down the probe server
(including closing its listening socket) and Xvfb, and only then release the
flock. A second Host can never take over a profile while the old browser is
still alive.

## Launch guarantees (per launch)

1. Artifact bytes read exactly once; raw SHA == `expectedArtifactFileSha256`
   == sidecar.
2. Recursive strict schema validation: every closed object requires ALL
   declared fields (missing fields rejected), unknown fields rejected,
   `type(x) is int`/`bool`, policy/config consistency.
3. Browser binding check: archive SHA/size, BuildID, SourceStamp,
   properties.json SHA, generator versions.
4. Extraction tree verified against the tracked
   `browser-tree-manifest.json` (689 files / 1,284,408,846 bytes). Missing,
   extra, modified, **symlink, or non-regular entries are rejected** before
   launch; directory symlinks are never followed.
5. `deepcopy` of the resolved config; `configuredIdentityDigest` recorded
   before `launch_options()`; sent `CAMOU_CONFIG` must be byte-identical —
   any added/changed/removed key aborts before the browser starts.
6. Explicit executable / profile / controlled cache; DownloadGuard installed.
7. Managed process identity comes from the supervisor's own
   `supervisor.json` (supervisor PID, child browser PID, start times, process
   groups). The Host never guesses a PID by scanning `/proc` cmdlines.
8. Probe page reports ObservedWebsiteDigest (no canvas, no internal seeds, no
   artifact-supplied font input; font widths only in `managed` font mode).
9. First boot writes `verisilo_probe_cookie` (value embeds the session id),
   reads it back via the cookies API and `document.cookie`, and close()
   records `cookies.sqlite` evidence (file + `moz_cookies` row). Later boots
   prove the cookie and bootCount persisted from the previous Host process.

## Font policy (M2.0.1)

`policy.fontMode` is `inherit` or `managed`.

- `inherit` (current fixtures): font widths are **host-bound**; they never
  enter ObservedWebsiteDigest. Host-font negative-control failures are
  recorded as evidence (`hostFontMasking`) but do not gate stability.
- `managed`: font widths enter the digest ONLY if every host negative control
  is unavailable in the page; otherwise launch fails with
  `host_font_masking_failed` before acceptance.

Current artifacts are `inherit` because host-font masking is NOT solved on
this host (several host-installed families remain visible through
`document.fonts.check`). Font isolation is not claimed.

## Evidence and self-check

Each session writes `stateRoot/<sessionId>/session.json` (state, digests,
managed PIDs + supervisor metadata, exit status/file, processTreeExit,
cookie evidence, cookie sqlite, fontMode, probe port) and `observed.json`
(full probe + projection), plus `browser.log`. `shutdown` returns `selfCheck`
with matches from a secret pattern scan of the Host argv and the stderr log.

## Test results

`apps/camoufox-host/test_host_v1.py` (11/11 passed) and
`test_identity_artifact.py` (17/17 passed):

- hello returns fixed protocol/version binding (`verisilo-camoufox-host/v1`,
  hostVersion 0.1.0, browserRelease v152.0.4-beta.28).
- launch → status running → close with exit code 0, exit file observed,
  process tree confirmed gone.
- **Real cross-process persistence**: fresh temp roots, same probe origin —
  bootCount 0→1 then 1→2, cookie present via API/page on both boots,
  `cookies.sqlite` contains the `verisilo_probe_cookie` row after close,
  ObservedWebsiteDigest identical across both Host processes.
- Three Host-managed cold starts of identity-a produce the same
  ObservedWebsiteDigest (`sha256:6206f58d…96f03`, inherit font mode).
- Concurrent launch of the same profile returns `profile_in_use`.
- Wrong expected SHA / missing top-level config field / missing NESTED
  required field (policy.canonicalJsonRule) with recomputed digests /
  extraction-tree tamper are all rejected before launch
  (`integrity_rejected`).
- Browser crash (real child PID killed via supervisor.json) → `failed` →
  lock released → same profile relaunches.
- **SIGTERM, SIGINT, and stdin EOF with an active session all cleanly close
  the browser tree and exit 0; the same profile relaunches.**
- An oversized (>32 KiB) frame is rejected with `frame_too_large` and the
  Host remains usable.
- stdout stayed pure protocol JSON in every test; shutdown selfCheck found no
  artifact seeds or secrets in argv/stderr.

## Known limitations (carried forward)

1. **Font masking is NOT solved.** Host-installed families remain visible;
   current artifacts are `inherit` and font widths are excluded from the
   digest. `managed` mode requires all host negative controls unavailable.
2. Host-local Linux prototype only. M2-W (Windows manual gate) is next, then
   M3 (EngineAdapter/Tauri). Windows profile locking / process lifecycle /
   Windows-bound artifacts are not implemented.
3. Raw reports and profiles live under gitignored `artifacts/`; the tracked
   `tests/fixtures/camoufox/evidence-manifest.json` is the sanitized evidence
   index.

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
