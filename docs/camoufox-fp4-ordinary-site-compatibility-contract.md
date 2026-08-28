# Camoufox FP4 ordinary-site compatibility contract

## Product question

FP4 asks one bounded go/no-go question: can the exact Formal-v3 browser and Artifact v6 that passed
FP2/FP3 complete the frozen `fp4-ordinary-sites-v3` capability matrix on native Windows through the
required SOCKS5 route, preserve one non-sensitive site preference across a clean browser restart, and
then close every owned lifecycle cleanly?

This is Runtime Evidence for the existing Camoufox Engine binding. Profile, Artifact, Engine,
Network Policy and Evidence remain separate lifecycles. FP4 carries the already-qualified Artifact
and route bindings forward; it does not rerun FP2/FP3 or add a desktop/Host product API.

## Frozen input and availability

- exact Formal-v3 browser tree already qualified by FP2 and FP3;
- exact FP3 Artifact v6 copy, raw SHA binding and its matching Host-required SHA sidecar;
- one fresh temporary Profile reused by two sequential Host launch/close phases;
- required unauthenticated SOCKS5 endpoint `127.0.0.1:7897`;
- native Windows, one immutable attempt, no retry or runtime sample rotation;
- site matrix `fp4-ordinary-sites-v3` below.

The unchanged selections retain their 2026-08-28 availability evidence. React remains selected after
its pre-V2 HTTP 200 freeze. Attempt 9 directly recorded OSM asynchronously relocating to the Hong Kong
search result after the result list appeared; V3 changes only the graphics markers to wait for that
semantic relocation and measure one relative zoom increment. No site may switch after launch.

## Frozen capability matrix

| Capability | Frozen selection and required task | Predeclared alternate |
| --- | --- | --- |
| Document/navigation | `https://en.wikipedia.org/w/index.php?search=camouflage+animals+military&title=Special%3ASearch&ns0=1`: require the search-results heading, open exact result `Military camouflage`, require its heading, scroll to the `History` section, go back to the search results, then forward to the same article | `https://en.wiktionary.org/w/index.php?search=browser+internet&title=Special%3ASearch&ns0=1` with the same search-result/article/history semantics |
| Complex JavaScript app | `https://react.dev/reference/react`: open site search, filter for `useState`, and open the matching client route | `https://github.com/microsoft/playwright/issues`: its former exact dialog marker drifted in Attempt 7 and is not selected |
| Interactive graphics | `https://www.openstreetmap.org/#map=12/51.5074/-0.1278`: require completed 256px map tiles, search exact place `Hong Kong`, require results and the map settled over Hong Kong, pan at that zoom, zoom in exactly one level, open layers, select `CyclOSM`, require the checked layer and newly completed tiles | `https://www.google.com/maps?hl=en`: search exact place `Hong Kong`, require the `/place/Hong+Kong/` route and rendered map, then pan, zoom and select the Satellite layer |
| Audio/video | `https://commons.wikimedia.org/wiki/File:Big_Buck_Bunny_keyframe_strobing_example.webm`: start the video by user action, require ready media and advancing `currentTime`, pause, seek, and require the new media time | `https://commons.wikimedia.org/wiki/File:Big_buck_bunny_720p_5mb.webm` with the same media-state markers |
| Form/state | `https://en.wikipedia.org/wiki/Web_browser`: change the anonymous Appearance font-size preference to `Large`, require it after reload, then require it again after a clean Host/browser restart using the same Profile and restore `Standard` | `https://en.wiktionary.org/wiki/browser` with the same anonymous Appearance preference markers |

The frozen semantic markers are:

- document primary: initial `h1=Search results`; exact `Military camouflage` link; final path
  `/wiki/Military_camouflage` and `h1=Military camouflage`; visible `History` heading; back restores
  `h1=Search results`, and forward restores the same final path/heading. The fallback clicks the first
  `.mw-search-result-heading a`, freezes its text during the task, and requires the final `h1` to equal
  that text plus the same first-`h2`, back and forward relationships;
- complex-JS selection: open the visible `Search` control, fill exact `useState`, activate a visible
  result whose link path is `/reference/react/useState`, and require that final path with
  `h1=useState`;
- graphics selection: at least one `img.leaflet-tile` with `complete=true` and `naturalWidth=256`
  before interaction; `#query` is filled with exact `Hong Kong`, yields at least one
  `.search_results_entry`, and the hash settles to latitude 21.5–23.5 / longitude 113–115.5; one
  `ArrowRight` changes the hash while preserving that settled zoom; one `.zoom .plus-lg` click adds
  exactly one zoom level; `#map-ui-layer-cyclosm` becomes checked; and at least one completed
  post-selection tile has a source not present before selection. The fallback fills exact
  `Hong Kong` in `#searchboxinput`, activates `#searchbox-searchbutton`, requires a final path beginning
  `/maps/place/Hong+Kong/` and a rendered map canvas, then requires one drag, one Zoom-in action and the
  Satellite layer to change the visible map state;
- media primary: `video.vjs-tech`, `readyState >= 2`, duration from 19 through 21 seconds, at least
  one second of `currentTime` progress within three seconds after the user play action, `paused=true`
  after Pause, then a slider Home/ArrowRight seek producing `currentTime` from 4 through 6 seconds.
  The fallback uses identical selectors and thresholds except duration is 29 through 31 seconds;
- state primary and fallback: exact Large radio
  `#skin-client-pref-vector-feature-custom-font-size-value-2` is checked and the root class contains
  `vector-feature-custom-font-size-clientpref-2` after reload and again before any Phase B mutation;
  Phase B then checks the Standard radio and records the Large marker as cleared.

The React selection is not a hidden retry: its availability was frozen before V2 code, attempt root
or browser execution. Attempt 9 remains immutable and V3 is a new input.

## Execution phases and budgets

Phase A launches the exact browser once, runs the five frozen tasks in table order, captures
one bounded support screenshot for each task, verifies the state preference after reload, and closes
the session. Phase B launches the same Artifact, Network Policy and Profile again, requires boot count
`1 -> 2`, verifies the persisted preference, restores the default, captures the replay screenshot,
and closes. The Host then shuts down and the temporary Profile/cache may be removed only after the
owned process tree and Windows Job are empty.

Each main navigation has 30 seconds. Document, complex-JS, graphics and form tasks have 90 seconds;
media has 120 seconds; replay has 70 seconds; each page close has 3 seconds. Elapsed monotonic time is
evidence and exceeding a budget does not pass even if late work eventually returns.

## Outcome adjudication

`Passed` requires all of the following:

- all six observations (five Phase A tasks plus Phase B replay) satisfy their frozen semantic markers
  within budget, with title/final URL and screenshot SHA-256 receipts;
- no relevant browser/context/page crash, disconnect or unbounded hang is observed;
- exact Formal-v3, Artifact, Profile and SOCKS5 bindings carry through both launches/status calls;
- the Profile preference survives reload and the clean restart without Profile damage;
- both sessions close successfully, the Host child exits, the owned process tree is empty, Windows
  Job active-process count is zero, and bounded evidence/report files are complete.

`Failed` means direct evidence of a browser/patch/Host defect (including an inherited pinned
Camoufox/Firefox product limitation), browser crash or disconnect, Profile damage or lost state after
an otherwise successful write/reload, FP2/FP3 binding regression, exceeded hard lifecycle boundary,
or dirty process/Job shutdown.

`Inconclusive` means a task cannot be adjudicated because of site outage or structure drift, consent,
region policy, CAPTCHA/challenge, rate limit, third-party media/tile failure, or another ambiguous
external cause while no direct product failure is observed. Missing selectors or a bounded task
timeout alone are Inconclusive, not proof of a Camoufox defect.

`Blocked` applies only before native execution when a required frozen input is unavailable: proxy
listener, exact browser tree, Artifact/binding input, or the currently selected primary. An available
fallback does not unblock the already-frozen attempt; selecting it requires a new availability freeze
before any attempt directory or browser process exists.

Outcome precedence is deterministic: `Blocked` stops before launch; after launch any direct `Failed`
evidence dominates `Inconclusive`, which dominates `Passed`.

A Formal-v3 site task with direct product-failure evidence triggers one pinned upstream Camoufox
control for that failed task only. If upstream passes, Formal-v3 remains `Failed` and attribution is
the VeriSilo patch/Host application layer. If upstream has the same direct failure, FP4 remains
`Failed` and attribution is an inherited Camoufox/Firefox product limitation. An unavailable or
unadjudicable control is `Inconclusive`. Independent binding, Profile or lifecycle failures remain
`Failed` without a site control. Passing and initially Inconclusive tasks receive no control.

## Evidence and explicit boundary

The immutable attempt records the exact URLs and matrix version, bounded task markers, title/final
URL, monotonic timing, relevant page errors/crash/disconnect markers, screenshot hashes, exact
Formal/Artifact/Profile/proxy bindings, both boot transitions, Host/process-tree/Job closure, aggregate
outcome and limitations. It does not save HAR, page bodies, cookies, storage contents, credentials or
user-entered sensitive data.

FP4 does not claim universal website compatibility, login/2FA/payment/CAPTCHA compatibility,
downloads, uploads, permission coverage, anti-detection, browser DNS/TLS/QUIC properties, cross-host
replay, production packaging, shipping or release. The terminal result remains `verified:false`.
After a passing FP4, `nextGate` is a clean M3-WI definition/re-freeze; FP4 does not enter M3-WI.
