# Investigation report: extension scan of the current page returns no result on https sites

> Translated from the original Chinese on 2026-11-10. The original text is in Git history at commit 1cf0f29. For historical evidence, the original wording governs if the two differ.

> Status: fixed in a follow-up implementation after VeriSilo Companion 0.1.2. This document keeps the reproduction, root cause and acceptance basis from that time. The Manifest snippet in it records the 0.1.0 baseline and does not represent the current permission configuration.

Reproduction environment: Edge, VeriSilo Companion v0.1.0, page https://dash.cloudflare.com/.... Investigation only. No code was changed.

## Symptom

On an https Cloudflare page, the user opens the side panel and clicks 「扫描当前页面」 ("Scan current page"). 「当前结果」 ("Current result") always shows 「尚未扫描」 ("Not scanned yet"). The English message VeriSilo only scans regular HTTP(S) pages after you request it appears at the top (circled in red by the user). The page is clearly https.

## Root cause

The real trigger of that English error is not "the page is not http(s)" but "tab.url cannot be read". The code is in scanCurrentTab at apps/extension/src/background.ts:128:

```ts
if (tab.url === undefined || !/^https?:/u.test(tab.url)) {
  throw new Error(
    "VeriSilo only scans regular HTTP(S) pages after you request it.",
  );
}
```

In MV3, when the extension has neither the tabs permission nor the host permission for that origin, the tab.url returned by chrome.tabs.query is stripped to undefined.

The manifest (apps/extension/manifest.json):

```json
{
  "permissions": [
    "storage",
    "sidePanel",
    "activeTab",
    "nativeMessaging",
    "scripting"
  ],
  "optional_permissions": ["privacy", "proxy", "declarativeNetRequest"],
  "optional_host_permissions": ["http://*/*", "https://*/*"]
}
```

Three key facts:

1. There is only activeTab. It is a temporary permission. A side-panel button click does not count as a new activeTab gesture. In the service worker, tabs.query takes the normal path, and without the tabs permission url is undefined.
2. optional_host_permissions declares all of http/https, but no code in the whole repository calls chrome.permissions.request({ origins: [...] }) to actually request them.
3. request_optional_privacy_permission requests only privacy. It has nothing to do with the host permission the scan needs.

So the scan throws early because tab.url is undefined, before content.js is injected. It never enters collection, and the result is forever 「尚未扫描」 ("Not scanned yet"). The error text also disguises "cannot read the URL" as "the page is not http(s)".

## Secondary problems on the same path

getCurrentReport also checks tab.url for undefined first and returns report:null directly. Even if the session holds an old report, it cannot be read, which amplifies the "Not scanned yet" impression.
Injecting main-world.js with scripting world:MAIN has a race. This is a known best-effort limitation and not the main cause here.
Scan-completion feedback relies on storage.onChanged and has no explicit completion event. When url cannot be read, the whole chain never runs.

## Fix suggestions (by priority)

P0: make the scan really able to read the tab and inject. Recommended option A (fits the product boundary of "authorize on demand"): in scanCurrentTab, after getting the tab, if tab.url cannot be read, call chrome.permissions.request({ origins: [current origin /*] }) to request the host permission for that site, then inject. Note that permissions.request must be called inside a user gesture. The safe way is to first add a 「为当前站点授权」 ("Authorize current site") button, request the grant inside the side-panel page, and send scan_current_tab only after granted.

Alternative option B: add tabs directly to permissions, and url becomes readable immediately. The cost is that the tabs permission is heavy and conflicts with least privilege and store disclosure, so it is not recommended.

P1: fix the misleading text. When url cannot be read, show 「无法读取当前页地址，请先为该站点授权或重新点击扩展图标」 ("Cannot read the current page address. Authorize this site first or click the extension icon again."). Show 「只扫描普通 http(s) 页面」 ("Only regular http(s) pages are scanned") only when a url is really obtained but is not http(s) (chrome, edge, file and so on).

P1: add an explicit 「为当前站点授权」 ("Authorize current site") entry that matches the declared optional_host_permissions. It calls chrome.permissions.request({ origins: [current origin /*] }). After the grant it takes effect only for that origin and not for all origins.

P2: make scan-completion feedback real. After the content script finishes collecting, it sends scan_completed explicitly (or a phase flag is added on verisilo_observation). The side panel then shows 「采集中/采集完成」 ("Collecting / Collection complete") so that the user does not think it is stuck.

## Verification after the fix

1. Load the extension in a clean Edge and go to https://dash.cloudflare.com/.
2. Click 「为当前站点授权」 ("Authorize current site") → confirm in the popup → returns granted.
3. Click 「扫描当前页面」 ("Scan current page"). The only scans regular HTTP(S) error no longer appears.
4. Within a few seconds, 「当前结果」 ("Current result") shows signals such as navigator/canvas/webgl.
5. In the Edge extension details, confirm the host permission takes effect only for dash.cloudflare.com and not for all origins.
6. Scanning a page such as chrome settings should give the (non-misleading) hint 「只扫描普通 http(s)」 ("Only regular http(s) pages are scanned").

## Out of scope

At the time, 「连接桌面端」 ("Connect to desktop") showing not connected was expected (the allowlist was not registered) and is unrelated to this bug. Since 0.2.3, the primary button opens the unified project page. The Native Messaging handshake is kept for later desktop-status capabilities.
Desktop Silo isolation, the Vault and other main-thread capabilities work normally and are not affected.
The best-effort boundary of MAIN world and Worker coverage is kept per the frozen product claims.
