# Capability model

Every control has a static tier and a runtime state.

| Tier          | Meaning                                                                                                  |
| ------------- | -------------------------------------------------------------------------------------------------------- |
| `reliable`    | VeriSilo can control it through a supported API or launch configuration and can collect direct evidence. |
| `best_effort` | It can be attempted, but page, browser, version, or context limits can defeat or reveal it.              |
| `unsupported` | It is outside a normal Chrome/Edge extension and launcher boundary.                                      |

Runtime states are `not_requested`, `permission_missing`, `not_controllable`, `configured`, `applied`, `verified`, and `verification_failed`.

`reliable` never implies `verified` by itself. Proxy configuration, for example, remains unverified until the user explicitly runs an exit test against a disclosed endpoint.

Network launch evidence uses a parallel staged vocabulary: `configured`, `reachable`, `applied`, `verified`, `failed`, `not_requested`, `not_applicable`, and `unavailable`. A reachable proxy port is not an authenticated exit; an applied browser route is not an observed public IP. See [network identity providers](network-identity-providers.md).

Environment backends use four deliberately non-interchangeable positive/boundary
labels in the desktop UI:

| Environment state | Meaning                                                                                                             |
| ----------------- | ------------------------------------------------------------------------------------------------------------------- |
| `configured`      | The controller wrote a deterministic descriptor, selection, or policy. No guest behavior is inferred.               |
| `guest_observed`  | A fixed guest agent returned a bounded, identity-bound observation; this alone may still lack a required dimension. |
| `verified`        | The current UUID/runtime/profile/hash/time binding and every dimension required for the claim passed.               |
| `unavailable`     | The backend has no reliable channel or mechanism for that claim. It is never treated as pass.                       |

`missing` and `unknown` remain explicit prerequisite states. In particular,
Sandbox host-process health is not guest health, and Hyper-V control-plane
health is not browser/network readiness.

`unsupported` is always scoped to the active implementation layer. For example, a stock Chrome/Edge Profile cannot change real hardware or its TLS stack; this does not prevent VeriSilo from scheduling a controlled engine, VM, or remote backend. The UI must name that future layer and version without presenting it as currently available. See [the environment roadmap](environment-roadmap.md).

The currently implemented experimental WebRTC control uses Chrome/Edge's `webRTCIPHandlingPolicy` setting to request `disable_non_proxied_udp`. It follows `observe → apply → verify → restore` and only reports `verified` after the browser returns the expected setting and `controllable_by_this_extension`. It remains `best_effort`: it is neither a packet-level proof nor a guarantee about every WebRTC implementation path.

## Labs experiment state

V0.5 high-risk controls use a separate versioned state machine: `disabled` (the default), `permission_missing`, `applying`, `best_effort`, `verified`, `failed`, `leak_detected`, `restored`, and `unsupported`. `best_effort` is explicit because an internal self-test can pass while a coverage prerequisite remains unproven. `verified` requires direct evidence for every declared prerequisite; the current user-triggered MAIN-world Worker injection cannot prove document-start ordering and is therefore never promoted to `verified`.

The UI keeps five evidence meanings distinct:

| Meaning     | Labs representation                                                            |
| ----------- | ------------------------------------------------------------------------------ |
| Available   | A selectable definition exists, but it is still `disabled` by default.         |
| Configured  | Current Silo/site authorization and optional host permission are both present. |
| Applied     | State is `applying` and the original constructor has a restorable baseline.    |
| Verified    | All declared coverage and ordering prerequisites have direct evidence.         |
| Unsupported | The card is unselectable and names a stronger-layer alternative.               |

The narrow Dedicated Worker experiment currently finishes at `best_effort`: it verifies its new same-origin/blob classic Worker handshake and same-origin iframe probe, while recording that injection order, existing/module/cross-origin Workers, SharedWorker and Service Worker memory are not covered. Cookie repository virtualization and comprehensive Set-Cookie interception are `unsupported`, not placeholder controls. Page-visible Cookie canary observation is only a stop signal and never modifies Cookie state.
