# Store disclosure draft

**Primary purpose:** VeriSilo Companion locally verifies the browser environment created by the VeriSilo desktop application and explains browser privacy signals on pages the user explicitly scans.

- It processes browser signal data locally in trusted extension storage on the same device.
- It does not transmit browsing activity, authentication information, cookies, or reports to VeriSilo servers.
- It requests website access only after the user invokes a scan or enables a site-specific feature.
- Optional privacy controls are requested only when the user explicitly enables them.
- Reports export only after an explicit confirmation, and high-sensitivity signal values are redacted by default.
- The companion requires the separately installed VeriSilo desktop application only to perform its optional local Native Messaging handshake; environment isolation itself is performed by the desktop application.
