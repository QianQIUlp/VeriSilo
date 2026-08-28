# Camoufox FP4 ordinary-site compatibility contract

## Question and scope

FP4 asks one bounded product question: can the exact Formal-v3 browser and Artifact v6 that passed
FP2/FP3 complete two frozen anonymous HTTPS user tasks on native Windows, through the required
SOCKS5 route, and then close cleanly?

This is Runtime Evidence for the Camoufox Engine binding. It does not merge Profile, Artifact,
Engine, Network Policy or Evidence lifecycles. The route remains configured by Network Policy and
applied through the existing Host option; FP4 only carries that binding forward while observing
ordinary-site behavior.

## Frozen input

- exact Formal-v3 browser tree already qualified by FP2 and FP3;
- exact FP3 Artifact v6 copy and raw SHA binding;
- one fresh Profile and one Host/browser session on native Windows;
- required unauthenticated SOCKS5 endpoint `127.0.0.1:7897`;
- no retries, alternate sites, fallback engine or sample rotation;
- 30 seconds per navigation, 70 seconds per complete task and 3 seconds to close each task page.

The runner verifies the existing Formal-v3/Artifact bindings and adds evidence-only observation. It
does not add a Host v1 navigation command or rerun FP3 exit, Geo, Geolocation or ICE checks.

## Required tasks

| Task | Frozen actions | Required observation |
| --- | --- | --- |
| Document navigation | Load `https://example.com/`, require main response 2xx and exact `h1` text `Example Domain`, then click its first HTTPS anchor | Final host is `iana.org` or `www.iana.org`, final main response is 2xx, and exact `h1` text is `Example Domains` |
| Interactive form | Load `https://en.wikipedia.org/wiki/Main_Page`, require main response 2xx, fill `input[name="search"]` with `Web browser`, and submit with Enter | Final host is `en.wikipedia.org`, final path is `/wiki/Web_browser`, final main response is 2xx, and `#firstHeading` is exactly `Web browser` |

Both task pages are opened separately inside the same fresh persistent context and are closed even
after a task failure. Evidence records only bounded semantic markers, response status and final URL;
it does not persist page bodies, cookies, storage values or credentials.

## Binary adjudication

An immutable attempt passes only when all of the following are directly present:

- the exact Artifact, browser executable and required proxy binding are echoed by the running Host;
- both frozen tasks complete within their deadlines with every required marker;
- Host status remains running and the browser boot count is exactly `0 -> 1`;
- close and shutdown succeed, the Host child exits, the owned process tree is empty and the Windows
  Job active-process count is zero.

A required task timeout, main-navigation failure, unexpected final host/path, missing or wrong
semantic marker, browser/context crash, or dirty lifecycle makes that attempt `Failed`. Generic
console noise is diagnostic only. A failed attempt is preserved and is not retried with unchanged
inputs or replaced by another site.

The attempt preserves native evidence and `run-report.json` with SHA-256 sidecars. A terminal main-
brain result may reference that immutable evidence, but remains `verified:false`.

## Explicit boundary

FP4 does not claim universal website compatibility, login/2FA/payment/CAPTCHA compatibility,
downloads, uploads, permission coverage, anti-detection, browser DNS/TLS/QUIC properties,
cross-host replay, production packaging, shipping or release. Site fallback remains unavailable.
The next Gate after a passing FP4 is a clean M3-WI definition/re-freeze; FP4 does not enter it.
