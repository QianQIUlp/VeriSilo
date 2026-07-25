# Compatibility matrix

| Target                    | Status                    | Notes                                                                             |
| ------------------------- | ------------------------- | --------------------------------------------------------------------------------- |
| Windows 10 x64            | planned validation target | Desktop installer and browser discovery require Windows end-to-end verification.  |
| Windows 11 x64            | planned validation target | Desktop installer and browser discovery require Windows end-to-end verification.  |
| Chrome Stable             | supported target          | Uses Chromium command-line profile isolation and Manifest V3 companion APIs.      |
| Edge Stable               | supported target          | Uses the same MV3 source with its own Store ID and Native Messaging registration. |
| macOS/Linux               | not yet supported         | Deferred until Windows release behavior and installer safety are validated.       |
| Multiple concurrent Silos | not yet supported         | Intentionally blocked by the runtime manager in V0.1.                             |
