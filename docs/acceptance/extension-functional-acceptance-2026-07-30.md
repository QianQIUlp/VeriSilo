# Extension functional acceptance record (2026-07-30)

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> **Historical acceptance record.** This document applies only to the date, browser and environment named in the title. It does not decide the current
> product stage, the current RC or the next task. For current routing, see the [Camoufox status page](../camoufox-program-status.md).

## Conclusion

This round started from `e5eaeb8` on `main`. It operated the extension item by item in a real Chromium and fixed the problems it found within the same change set. The conclusion has two layers:

- **Core functional regression passed**: On Chrome for Testing 151.0.7922.34 in a Linux/X11 environment, the extension entry point, scan, report, history, export, network check, lightweight isolation, permissions, Labs, privacy windows and the degraded behavior when the desktop is unavailable all have real-browser evidence.
- **Extension release acceptance has not passed**: Chrome Stable and Edge Stable on Windows 10/11, the official extension IDs, real Native Host/desktop integration, and browser upgrade/extension update/crash recovery have not been run. This record cannot substitute for them.

"Passed" in this record applies only to the listed browsers and scenarios. Automated tests, static source-code gates and real Windows results are recorded separately and do not substitute for one another.

## Acceptance environment and evidence boundary

| Item             | This baseline                                                                                                    |
| ---------------- | ---------------------------------------------------------------------------------------------------------------- |
| Starting code    | `main` / `e5eaeb8`, plus the worktree with this round's acceptance fixes                                         |
| Browser          | Chrome for Testing `151.0.7922.34`                                                                               |
| Operating system | Linux + Xvfb; **not Windows or Edge evidence**                                                                   |
| Extension form   | `apps/extension/dist` unpacked MV3; real action, Side Panel, extension permission prompt and incognito window    |
| Test site        | In-repository loopback fixture `127.0.0.1:4173`; `localhost:4173` is also used to verify cross-origin navigation |
| Desktop          | Native Host deliberately unavailable, to verify standalone operation and explicit degradation messages           |
| Automation       | Extension TypeScript, 43 Vitest tests, 46 contracts tests, production build, bundle remote-code/manifest audit   |

No actual exit IP, Cookie, token, canary or unredacted report content was recorded.

## Real-browser results

| Scope                          | Result               | Verified behavior                                                                                                                                                                                                                                           |
| ------------------------------ | -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Install and entry point        | Passed               | The unpacked extension loaded; clicking the action opened the real Side Panel; the four pages were reachable; no blocking console errors.                                                                                                                   |
| Current-site permission        | Passed               | Site authorization is triggered by a user click; after the browser permission prompt allowed it, scan/Labs worked; after revocation `permissions.contains` was false.                                                                                       |
| Site scan                      | Passed               | The scan stayed busy until the report arrived; the fixture produced 16 signals; a MAIN-world failure still kept the isolated-world result; the timeout is bounded.                                                                                          |
| Report and tab switching       | Passed               | The empty state and the result are mutually exclusive; switching to a tab without a report clears the result, and switching back restores the matching report without showing the old tab's content.                                                        |
| History                        | Passed               | A normal window saved redacted history; the cancel branch of the clear action deleted nothing and the confirm branch deleted; the current session's report could still be exported.                                                                         |
| JSON/HTML export               | Passed               | Both downloads succeeded; the high-sensitivity WebGL renderer sentinel did not appear in the files; the HTML summary came from the redacted report.                                                                                                         |
| Privacy window                 | Passed               | It clearly states that Chrome incognito / Edge InPrivate is one shared temporary space; normal and incognito localStorage and Cookie are separate; temporary state was cleared after all incognito windows were closed.                                     |
| Incognito persistence boundary | Passed               | An incognito scan did not write to the normal `storage.local` report history; the local receipt count did not change before and after enabling/stopping Labs in incognito, and the state existed only in the session.                                       |
| Optional privacy control       | Passed               | The `privacy` permission was triggered by a user click; the WebRTC non-proxied UDP and network prediction settings were applied and rechecked; after restoring the original values, the permission was revoked.                                             |
| Network check                  | Passed               | After the user explicitly authorized three endpoints, the exit observation and the public DoH comparison completed with no errors; when the desktop was unavailable it was labeled “仅本地” ("local only"); clearing also revoked the endpoint permissions. |
| Desktop entry / degradation    | Passed               | When the Native Host was missing, an explicit error was shown instead of a false success; “打开桌面端” ("Open desktop") and the project page are two separate entry points.                                                                                 |
| Labs default state and scope   | Passed               | Off by default; Cookie virtualization and Set-Cookie interception stay unselectable; without the desktop it is explicitly labeled as at most 2 minutes, local temporary, not a Silo.                                                                        |
| Labs enable / explicit stop    | Passed               | The MAIN-world `Worker` was indeed wrapped; the same-origin/blob classic Worker and the about:blank iframe passed the self-check and the UI labeled them “有限覆盖” ("limited coverage"); after stop, the main world restored the native `Worker`.          |
| Labs automatic recovery        | Passed               | A page ErrorEvent, a same-document `history.pushState` and a cross-origin navigation all stopped the run; in the same-document scenario the state object disappeared and `Worker` was measured to be the native constructor again.                          |
| Labs receipts                  | Passed               | Shows the Observe/Apply/Verify/Restore phases, evidence codes, coverage, stop reason and restore result; shows only the latest 8; canceling clear changed nothing, and after confirming it went 14→0.                                                       |
| Labs delayed-leak monitoring   | Passed in automation | The 5-second non-overlapping canary monitor can restore and stop on a delayed leak, and a stopped or replaced run is not processed again by an old scan; no long-running stress test against a real malicious site has been done.                           |

## Fix list for this round

| Severity | Problem                                                                                                                        | Closure                                                                                                                                                                                                                               |
| -------- | ------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P1       | `.result-stack { display:grid }` overrode `hidden`, so the empty state and the report appeared together                        | A global `[hidden]` rule restores reliable hiding.                                                                                                                                                                                    |
| P1       | The old report/Labs state was still shown after a tab switch                                                                   | Added activate/update listeners and a version guard.                                                                                                                                                                                  |
| P1       | The Labs about:blank iframe was misjudged as `origin=null`, so the self-check always failed                                    | Confirm the site realm only along an accessible about:blank/srcdoc owner/parent chain that really inherits.                                                                                                                           |
| P1       | The HTML export summary used the unredacted report and could leak high-sensitivity values                                      | HTML and JSON are both generated only from the redacted report; added a sentinel regression.                                                                                                                                          |
| P1       | Incognito reports and Labs receipts could enter the shared local history                                                       | Only an explicitly non-incognito tab may persist; an unknown tab state is treated as not persisting.                                                                                                                                  |
| P1       | Labs scanned the canary only at enable time, so a delayed leak went undetected                                                 | Added a bounded, non-overlapping, cancelable runtime monitor.                                                                                                                                                                         |
| P1       | A permission request left the user click stack, and real Chrome rejected it                                                    | Site, Labs and privacy permissions are all requested directly in the click callback.                                                                                                                                                  |
| P1       | The scan message returned "started" first, so the UI ended busy too early and read an old report                               | Clear the old report, isolate by scan generation, add a content-collection timeout, and make the UI poll with a bound until the result arrives.                                                                                       |
| P1       | Labs navigation stop treated same-document `loading` as the old realm being destroyed, so the receipt falsely reported restore | Navigation always probes/restores the current realm first; the old realm is treated as destroyed only when it becomes inaccessible because of a cross-origin navigation; a real `pushState` regression confirmed the native `Worker`. |
| P2       | The site-permission target cache could be overwritten by a slower query from an old tab                                        | Clear synchronously on switch, guard async results with a version, and listen for window focus changes.                                                                                                                               |
| P2       | A desktop message could wait forever, and the entry point existed but was unreachable in the UI                                | Native Messaging uses a uniform 10-second timeout; the desktop entry point and error message were completed.                                                                                                                          |
| P2       | The three endpoint permissions were kept after the network result was cleared                                                  | Clearing the result also revokes the permissions and updates the copy.                                                                                                                                                                |
| P2       | Labs receipts showed only a total, with insufficient restore/coverage information; a clean navigation was labeled as a failure | Expanded to the full phases and coverage; a successful lifecycle stop is uniformly labeled `restored`.                                                                                                                                |

## Extension gaps not yet closed out

The following are not failures of this round's Chrome/Linux core regression, but they must keep being tracked until the extension stage closes:

1. **Windows Chrome/Edge compatibility**: Real Chrome Stable, Edge Stable, incognito/InPrivate, Side Panel, permission UI, download policy and the `addHostAccessRequest` compatibility branch all await testing on Windows 10/11.
2. **Real desktop integration**: Only the degradation for a missing Native Host has been verified. The full report is not passed to the desktop, and the current cross-device bridge submits only bounded network evidence.
3. **Desktop state sync while running**: The network-submission badge refreshes only by expiry time. A Vault lock or Silo stop is not actively pushed to an already-open Side Panel. After Labs starts, the Silo/Vault binding is also not continuously revalidated.
4. **Lifecycle stress**: Browser upgrade, extension update, Service Worker eviction, extension reload, browser crash/recovery and fast multi-window switching need long-term regression.
5. **Proof of download completion**: The current UI can trigger a download, but it has no `downloads` permission, so it cannot tell a browser-policy block from a real write to disk. The release matrix must confirm the file externally.
6. **Production packaging**: The extension already uses the full icon set consistent with the website. The store ID, Edge ID, Native Host allowlist and the official package are still not bound.
7. **Labs authorization record governance**: The `labs-authorization:*` records of the desktop Silo mode have no separate UI or cleanup policy. The current local-temporary and incognito modes do not write these records.

## Next acceptance threshold

The next step for the extension is not signed release. It is to run on real Windows machines:

- Windows 10 x64 × Chrome Stable;
- Windows 10 x64 × Edge Stable;
- Windows 11 x64 × Chrome Stable;
- Windows 11 x64 × Edge Stable;

All four cells must repeat the real-browser table in this file and add the official Native Host/desktop integration. Any `SKIP`, `BLOCKED` or verbal result without saved evidence cannot mark the extension stage as finally passed.

For step-by-step operations, PowerShell commands, pass conditions and the evidence template, see the
[Manual acceptance runbook for Chrome, Edge and the Windows desktop](manual-windows-acceptance-runbook.md).
