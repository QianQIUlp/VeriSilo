# Store disclosure draft

**Primary purpose:** VeriSilo Companion locally verifies the browser environment created by the VeriSilo desktop application and explains browser privacy signals on pages the user explicitly scans.

The extension can also open the current site in a private window. This uses the browser's regular-versus-private website-data boundary; it does not create arbitrary account containers, and all private windows share the same temporary private context.

- It processes browser signal data locally in trusted extension storage on the same device.
- It does not transmit browsing activity, authentication information, cookies, or reports to VeriSilo servers.
- Network verification never runs automatically. After a confirmation and optional-host permission grant, it contacts `ipwho.is` for the current browser environment's visible exit IP/geolocation/ASN and Cloudflare 1.1.1.1 plus Google Public DNS for a fixed `example.com` DoH comparison. Those providers receive the user's request IP; the result is kept only in trusted session storage and can be cleared from the UI. It is Silo evidence only when the action is run inside that Silo.
- It requests website access only after the user invokes a scan or enables a site-specific feature.
- Optional privacy controls are requested only when the user explicitly enables them. They reversibly restrict WebRTC non-proxied UDP and network prediction, apply to the browser context rather than one account or tab, and are verified before the UI reports them as active.
- Reports export only after an explicit confirmation, and high-sensitivity signal values are redacted by default.
- The public-DNS comparison does not inspect the browser, proxy, operating-system, router, or ISP resolver and is not presented as DNS leak detection or proof that local DNS is free from interception. IP reputation and blacklist status are not scored in this version.
- The companion works without the desktop application. Its desktop-project button opens VeriSilo's public project page; the restricted Native Messaging handshake remains reserved for a future explicit local-status feature. Long-lived environment isolation itself is performed by the separately installed desktop application.
