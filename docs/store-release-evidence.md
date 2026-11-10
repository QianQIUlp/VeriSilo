# VeriSilo Companion Store RC 0.2.11 evidence

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

- Status: **frozen evidence page**
- Freeze date: 2026-08-15
- Purpose: records the single RC package of Store Release Preparation, the verification results and the known boundaries. Both stores (Chrome Web Store, Edge Add-ons) upload only the one ZIP recorded on this page.

## Frozen object

| Item                | Value                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------ |
| Source baseline     | `main` @ `2f62e815700273f24e8cce429f251219e821efeb` (0.2.11 locale fix)                                |
| Build environment   | fresh git worktree + `pnpm install --frozen-lockfile`; pnpm 11.17.0, Node 24.19.0                      |
| `SOURCE_DATE_EPOCH` | `1786781541` (= commit time of `2f62e81`, 2026-08-15T08:12:21Z)                                        |
| Archive file name   | `VeriSilo-Companion-0.2.11-chrome-edge.zip`                                                            |
| Archive SHA-256     | `3ab22f31eb5f37bcef9a37eb512ed273df90f4778ecc18b5c6d1c7e54408c76e`                                     |
| Archive size, bytes | `1018149`                                                                                              |
| File count          | 21                                                                                                     |
| Format              | ZIP32 stored, byte-sorted, DOS timestamps taken from `SOURCE_DATE_EPOCH`                               |
| Content manifest    | `artifacts/store-rc/extension-zip-manifest.json` (schema `urn:verisilo:deterministic-extension-zip:1`) |
| Artifact location   | `artifacts/store-rc/` (gitignored, like the repository's other evidence artifacts, not in git)         |

The only change in 0.2.11 relative to 0.2.10: the hard-coded Chinese error texts in the background script were added to the bilingual dictionary (8 `ENGLISH_TEXT` entries + the dynamic pattern `IP 出口` ("IP exit")), so the EN UI no longer shows Chinese error notices. The version number, the bundle gate version and the item-by-item pairing check were updated in step.

## Superseded old hashes

| Source           | Value                                                              | Disposition                         |
| ---------------- | ------------------------------------------------------------------ | ----------------------------------- |
| 0.2.10 frozen RC | `29e5485d9c7abb2c3da92b03a7ce97771d87f653a1354c269a1eb80ef6c5471b` | **Superseded**, replaced by 0.2.11  |
| PR #14 body      | `01e18a35f641c928d92a98e46e94d643074ad54f128719e034021cabf2cbab7a` | **Superseded**, input not traceable |
| User record      | `74eeae8f72b80bc85fb91f4e43c163419d1b0a21653d8dae9f17b9f87805fbd3` | **Superseded**, input not traceable |

Release and the Native Host configuration recognize only the SHA-256 of 0.2.11.

## Rebuild and verification record

Run in a fresh worktree (`main@2f62e81`, frozen lockfile):

```text
pnpm check                                    -> passed (extension / desktop / site, 0 errors)
pnpm test                                     -> extension 12 files / 54 tests passed;
                                                 desktop 12 files / 53 tests passed;
                                                 session-fixture self-test passed
pnpm extension:build                          -> passed (icons check + esbuild bundle)
pnpm extension:verify                         -> manifest / remote-code / disclosure /
                                                 bilingual error text pairing gate passed
pnpm extension:package:self-test              -> deterministic ZIP self-check passed

$env:SOURCE_DATE_EPOCH='1786781541'
node scripts/package-extension-zip.mjs --input apps/extension/dist \
  --out artifacts/store-rc/VeriSilo-Companion-0.2.11-chrome-edge.zip \
  --manifest artifacts/store-rc/extension-zip-manifest.json
node scripts/package-extension-zip.mjs --input apps/extension/dist \
  --out artifacts/store-rc/VeriSilo-Companion-0.2.11-chrome-edge.zip \
  --manifest artifacts/store-rc/extension-zip-manifest.json --check
```

After repackaging, `--check` matches the written bytes byte for byte. This shows that the same input and the same epoch reproduce the same archive.

## Sideload smoke test (load after unpacking the archive, directly verifying the store ZIP content)

- Browsers and versions: Edge Stable `151.0.4129.86` (`--load-extension`); Chrome Stable `151.0.7922.139` (the branded build removed `--load-extension`, so CDP `Extensions.loadUnpacked` was used instead).
- Method: unpack the archive into a temporary directory and load it, driven by CDP automation, with a separate temporary Profile (the default Profile was not touched).
- Result: **27/27 checks passed in each of the two browsers**.

| Check category                    | Result                                                                                                                                                                                                                                              |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| manifest                          | MV3, version 0.2.11, exact permissions set `storage/sidePanel/activeTab/nativeMessaging/scripting`, optional `privacy`, optional host `http(s)://*/*`, no permanent host permissions, `side_panel.default_path=sidepanel.html`, `default_locale=en` |
| side panel UI                     | EN copy renders; the language selector contains `en`+`zh-CN`; switching to `zh-CN` renders Chinese; tabs overview/isolation/labs/raw; the scan button exists; the isolation panel exists                                                            |
| dismiss behavior                  | The notice is initially hidden; the local pill is initially visible (Local only); both can be dismissed; the pill is restored after the side panel is reopened                                                                                      |
| scan without an activeTab gesture | Graceful degradation: a dismissible permission notice is shown, no crash                                                                                                                                                                            |
| **EN error text fix**             | With the EN language, the scan permission notice shows "The browser has not granted one-time access to the current page…", and Chinese no longer appears                                                                                            |
| service worker                    | Alive; manifest consistent; the `storage.local.setAccessLevel` API is available (when Chrome does not expose the SW debug target, verification used a message round trip instead)                                                                   |
| console                           | No error/warning on the side panel page                                                                                                                                                                                                             |

The smoke test used the temporary ID `egnejcalcdjkfhdcjfdpohdjanpcjibh` (derived from the unpacked path, **not** any store ID). The smoke scripts and run logs are kept in `artifacts/store-rc/` (`chrome-smoke.log` / `edge-smoke.log`).

## Store screenshots (accepted by the account holder)

The store screenshots were recaptured from the unpacked RC by `scripts/capture-store-screenshots.mjs`. The left side is an explicitly labeled `example.test` example web page composition, and the right side is the Companion panel actually loaded in the RC. The report images use a synthetic, redacted `example.test` fixture and contain no real user data.

The final assets are:

- `assets/store/store-screenshot-1280x800-en-scan.png`: English scan entry;
- `assets/store/store-screenshot-1280x800-zh-report.png`: Simplified Chinese scan report;
- `assets/store/store-screenshot-1280x800-en-private-space.png`: English temporary private space;
- the corresponding `assets/store/store-screenshot-640x400-*.png`: the same compositions at the size compatible with Chrome.

Screenshot acceptance: the six files are exactly `1280×800` / `640×400`. A visual check confirmed that the example web page, panel title, buttons, language and report content are readable, and that no scrollbar, blank error state or truncated card is used as the main visual. The private space image keeps only the complete first-screen capability boundary. The assets do not claim anti-detection, controlled fingerprint or network isolation capability.

Screenshot byte acceptance (SHA-256):

| File                                             |  Bytes | SHA-256                                                            |
| ------------------------------------------------ | -----: | ------------------------------------------------------------------ |
| `store-screenshot-1280x800-en-scan.png`          | 296577 | `08a95877e3af143df35c9bd59268197d22de2fa405ef98346dae9b03c9176000` |
| `store-screenshot-1280x800-zh-report.png`        | 315382 | `52b12b595afb36087bbe68bd35c73d4047888b9ede75782c715a5179c40a9c2a` |
| `store-screenshot-1280x800-en-private-space.png` | 324554 | `e347a4f940cd6e0a94587bdd48854960d0e4ad1fa95e527320bd774eb71f9bce` |
| `store-screenshot-640x400-en-scan.png`           | 109469 | `690676df42f7b4876fdacc0fefb6d128caccc2d33ca707cef97e1beb91b667ff` |
| `store-screenshot-640x400-zh-report.png`         | 114054 | `0d7e22b5a50540be1b221bb5634909aeb36d89df729d409821c99bfead004685` |
| `store-screenshot-640x400-en-private-space.png`  | 120914 | `8befc804810e09c4a0f98bfbaf76302351be7d9bb44894af1af3030ffdb77b18` |

## Items unavailable on this machine (not passed off as verified)

| Item                   | Status and substitute                                                                                                                           |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Native Messaging smoke | This machine has no built `verisilo-native-host.exe` / desktop app: `unavailable`. To be verified per the runbook once P6 obtains the store IDs |

## Known defects

The hard-coded Chinese error text defect recorded for 0.2.10 was fixed in 0.2.11 and verified by the smoke test. There is currently no known blocking defect.

## Your manual follow-up verification checklist (on a machine with the desktop app, or in the P6 stage)

1. On a real web page, click the toolbar icon to open the side panel → run a scan → confirm that a report is generated (automation on this machine cannot simulate the toolbar gesture).
2. After granting the optional host permission, run the Network Check, and confirm the ipwho.is / DoH results and the local degradation path (when ipwho.is is unavailable, fall back to api.ipify.org / api.ip.sb).
3. After installing the desktop app + Native Host, confirm that the `Local only` state changes to the Silo-bound state (P6 runbook).

## Update rules

Any change to code, manifest, locale or packaging inputs produces a new SHA. The freeze process must be run again and the hashes on this page replaced. Superseded hashes stay in the table for traceability.
