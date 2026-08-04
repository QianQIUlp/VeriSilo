# Camoufox M0 Compatibility Spike (Linux)

Status: **M0 spike complete on this host — observations only, nothing here is
release-grade verification.**

## Scope and constraints honored

This spike proves that one fixed Camoufox version can start repeatedly from a
persistent `user_data_dir` on the current 2C / 8GB Linux host, and that profile
state survives normal restarts. It deliberately does **not** modify the Tauri
UI, hook into any `EngineAdapter`, touch WSL/Hyper-V/Remote, build
Firefox/Camoufox from source, follow `latest`, mint a production signer, or
claim that Host-observed behavior is "verified".

Required reading before the spike was consulted:
`apps/desktop/src-tauri/src/engine.rs`,
`apps/desktop/src-tauri/src/launcher.rs`, `docs/engine-adapters.md`,
`packages/contracts/src/engine.ts`. The spike stays outside the production
protocol: it is a standalone probe, not an adapter implementation.

## M0.1 evidence traceability (revision)

Every run of `run_spike.py` now creates its own run-id directory under
`artifacts/camoufox-m0/runs/<run-id>/` containing:

- `report.json` — the full evidence report for that run;
- `report.sha256` — SHA-256 of that exact `report.json`;
- `run.log` — stdout/stderr of the run (including the browser process, which
  inherits those descriptors through the spike supervisor);
- `cycle-1..3-exit.json` — the real browser exit codes observed by the
  supervisor;
- `profile/` — the persistent `user_data_dir`, created fresh for this run and
  shared by all three cycles of this run.

The default profile is per-run and fresh, so repeating the same command never
reuses or pollutes a previous run's profile. The probe cookie value embeds the
run-id (`m0-<run-id>-cookie`), and cycle 1 first proves the cookie is absent
(both in the cookie API and on the page) before writing it.

This document references exactly one accepted run:
**`run-1785815310-886f0799`**. Its report lives at
`artifacts/camoufox-m0/runs/run-1785815310-886f0799/report.json` and its
`report.sha256` matches the file byte-for-byte.

Scope note: this is **host-local evidence traceability**, not a cross-machine
evidence freeze and not a supply-chain/network-behavior verification. The
reports live under the gitignored `artifacts/` directory; `report.sha256`
detects change but cannot make a file immutable.

## Pinned versions

| Component | Pin |
| --- | --- |
| Python | 3.12.11 (uv-managed, `.python-version`) |
| camoufox | 0.5.4 |
| playwright | 1.60.0 |
| browserforge | 1.2.4 |
| Browser release | v152.0.4-beta.28 (BuildID 20260719045650, Milestone 152.0.4) |
| Browser archive | `camoufox-152.0.4-beta.28-lin.x86_64.zip` |

All transitive Python dependencies are hash-pinned by `uv.lock` (42 packages).
Top-level pins and the browser asset lock live under
`apps/camoufox-host/lock/`.

## Browser asset integrity

The archive (663,387,175 bytes) was downloaded from the exact release URL and
its SHA-256 was computed by `apps/camoufox-host/fetch-browser.py` over the
bytes it received — the local digest is never copied from a vendor page:

```text
local sha256 = 924f3109ccd6d47cd6a0384d67a345fadf975d48b6319f8dbbd5954c588982bd
local size   = 663,387,175 bytes
```

The lock file
(`apps/camoufox-host/lock/camoufox-v152.0.4-beta.28-linux-x86_64.json`)
records GitHub's official asset metadata separately from the locally computed
values: asset id `482185256`, official digest
`sha256:924f3109ccd6d47cd6a0384d67a345fadf975d48b6319f8dbbd5954c588982bd`,
and official size `663,387,175` bytes (from the GitHub releases API).
`digestAgreement=true` in the lock is written only when the local digest and
the official digest — and both sizes — agree, which they do here.

The Camoufox package cache folder name (`152.0.4-beta.28-924f3109`) is chosen
by `run_spike.py` from the locally verified digest prefix; it is **not** an
independent cross-check of the archive. `run_spike.py` re-verifies the archive
hash and size against the lock before every run and seeds the package cache
only from that verified extraction (`artifacts/camoufox-m0/xdg-cache`,
gitignored).

## No unpinned downloads

Two automatic fetches in the stock Camoufox package were discovered during the
spike and are neutralized:

1. `add_default_addons()` downloads uBlock Origin from a `latest.xpi` URL
   (`https://addons.mozilla.org/firefox/downloads/latest/ublock-origin/latest.xpi`).
   The spike passes `exclude_addons=[DefaultAddons.UBO]`.
2. `get_env_vars()` looks up `fontconfig` in the package cache, which triggers
   a browser download when the cache is empty. The spike pre-seeds that cache
   from the verified archive (same bytes, no network) and sets a spike-owned
   `XDG_CACHE_HOME`.

As a hard guard, `run_spike.py` patches `camoufox.pkgman.webdl` and
`camoufox.addons.webdl` to raise; any residual unpinned download attempt fails
the run instead of being silently cached. The report records
`runtimeDownloadGuardInstalled: true` (the patch ran) and
`camoufoxWebdlAttempted: false` (it never tripped). This guard is **not** a
full network observation: outbound traffic of the browser process tree was not
captured at the socket level, and the report says so explicitly
(`outboundNetworkFullyObserved: false`). The spike never runs `camoufox
install`/`fetch` or `playwright install`.

## Launch harness

- Persistent context via `camoufox.AsyncNewBrowser(..., persistent_context=True)`
  with explicit `executable_path` (the extracted `camoufox-bin`) and explicit
  `user_data_dir` (same path for all three cycles).
- Virtual headful via a spike-owned `Xvfb` (`-screen 0 1280x800x24`, display
  `:0`) with software GL (`LIBGL_ALWAYS_SOFTWARE=1`).
- Cookie is written through `context.add_cookies()` with a 30-day expiry; the
  value embeds the run-id. LocalStorage is written by the minimal probe at
  `tests/fingerprint-probe/` (`verisilo.bootCount`, incremented each cycle).
- Playwright 1.60's Python API does not expose the browser process or exit
  code for persistent contexts, so `apps/camoufox-host/exit_supervisor.py`
  (spike harness only) is used as the spawned executable: it execs the real
  `camoufox-bin` with Playwright's exact args, hands stdio through, forwards
  signals, and records the real browser process's exit code to a file. One
  subtlety found during the spike: Python's `subprocess` default `close_fds`
  drops an extra protocol fd that the Playwright driver passes to the spawned
  process, which makes `camoufox-bin` exit 0 during startup; the supervisor
  sets `close_fds=False` (shell inheritance semantics).
- Memory is sampled every 0.25s over the browser process tree
  (`/proc/<pid>/task/*/children`, sum of `VmRSS`); the peak is reported per
  cycle. Sampling starts after launch returns, so the startup ramp itself is
  not fully captured (see Known issues).

## Actual run

```bash
cd apps/camoufox-host
uv sync --frozen --offline        # installs the exact uv.lock set, no network
uv run python fetch-browser.py    # verifies archive vs local pin + official digest
uv run python run_spike.py        # fresh per-run profile, 3 cycles, runs/<run-id>/report.json
```

Accepted run **`run-1785815310-886f0799`** on 2026-08-04, host
`Linux 6.17.0-1021-azure x86_64`, 2 cores, 7,883 MiB total RAM, exit code 0:

| Cycle | Boot count before → after | Cookie (API / page) | Profile arg observed | Start (spawn / page ready) | Close | Exit code | Peak RSS | Screen (page probe) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 0 → 1 | true / true | true | 2.081s / 0.149s | 0.451s | 0 | 452,804 KiB (~442 MiB) | 683×384 |
| 2 | 1 → 2 | true / true | true | 1.234s / 0.078s | 0.357s | 0 | 425,476 KiB (~415 MiB) | 1366×768 |
| 3 | 2 → 3 | true / true | true | 1.268s / 0.086s | 0.232s | 0 | 400,692 KiB (~391 MiB) | 683×384 |

Profile evidence: the same fresh `user_data_dir` (`runs/<run-id>/profile`) was
used in all cycles, the browser's argv contained `-profile <userDataDir>` each
cycle, and the profile contains `cookies.sqlite`, `prefs.js`, and `storage/`
after the run. The JSON report for the accepted run (including full probe
snapshots, argv snapshots, log scans, and the run-log SHA-256 sidecar) is at
`artifacts/camoufox-m0/runs/run-1785815310-886f0799/report.json` (gitignored
artifact). The random fingerprint means screen values differ per launch; the
values above are the ones actually observed in this accepted run.

## Observed vs not-yet-verified

**Observed on this host:** three clean persistent-context launches; Cookie and
LocalStorage values survive all three normal closes/restarts; the cookie value
contains the run-id and cycle 1 proved it absent before writing; profile path
is honored; exit code 0 per cycle; peak sampled RSS ≈ 391–442 MiB; no unpinned
download attempt observed via `camoufox.webdl` (guard installed and never
tripped; acceptance field `noCamoufoxWebdlAttemptObserved`, not a claim that
the process tree made no outbound requests); no secret-like patterns found in
the spike argv, browser argv snapshots, or run logs; no secret inputs were
supplied by the spike.

**Not yet verified (explicitly out of M0):** authenticity/signing of the
Camoufox release chain; **per-file verification of the extracted run tree**
(only the archive hash/size and executable presence are checked); **outbound
network traffic of the process tree** (only `webdl` calls are guarded);
**the initial pin is TOFU** — GitHub's official digest now agrees with the
locally computed digest, but there is no out-of-band attestation of the
original pin; Canvas/WebGL/font/media-device output truthfulness; TLS
ClientHello and QUIC behavior; Windows platform behavior; the production
EngineAdapter bootstrap/receipt protocol; long-run memory stability under real
site workloads. The report's `conclusion.verified` is `false` by design.

## Known issues / limitations

- The probe page loads only from `http://127.0.0.1`; no real-site navigation
  was exercised.
- Memory sampling starts after `launch_persistent_context` returns, so the
  launch-time RSS ramp is under-measured; the reported peak is the sampled
  session peak. Even at 2× headroom the observed footprint stays well under
  this host's 8GB budget.
- Screen values are random per launch (accepted run: 683×384, 1366×768,
  683×384). This is expected Camoufox behavior, not a failure, and nothing
  here claims those values are truthful.
- Exit-code observation depends on the spike supervisor; Playwright 1.60's
  Python API itself exposes no browser process handle for persistent contexts.
- The browser archive and extracted bundle live under gitignored `artifacts/`;
  the committed pins are the lock JSON + `uv.lock`.

## M1 recommendation

Yes — the candidate pins (`camoufox 0.5.4`, `playwright 1.60.0`,
`browserforge 1.2.4`, `v152.0.4-beta.28`) are mutually compatible and pass M0
acceptance on this host. M1 must add independent artifact provenance,
release-grade evidence (Canvas/WebGL/TLS/QUIC, Windows), and the production
protocol work; this spike is not a substitute for any of that.
