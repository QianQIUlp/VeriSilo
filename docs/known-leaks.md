# Known leaks and limits

- Independent `user-data-dir` values isolate browser persistence, not the operating system, hardware, network stack, installed fonts, behavior, payment data, or account graph.
- Chromium proxy configuration can still have browser/network edge cases. A successful configuration write or TCP preflight is not an exit-IP guarantee.
- The companion's MAIN-world code can be observed by a page and may inject after page code has run.
- Browser features and permissions vary by Chrome/Edge version and enterprise policy. Runtime state must expose control conflicts and verification failures.
- Local vault encryption protects VeriSilo metadata at rest from casual file inspection. It does not protect against an attacker controlling the browser process, operating system, or signed extension updates.
