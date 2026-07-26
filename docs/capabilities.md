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

`unsupported` is always scoped to the active implementation layer. For example, a stock Chrome/Edge Profile cannot change real hardware or its TLS stack; this does not prevent VeriSilo from scheduling a controlled engine, VM, or remote backend. The UI must name that future layer and version without presenting it as currently available. See [the environment roadmap](environment-roadmap.md).

The currently implemented experimental WebRTC control uses Chrome/Edge's `webRTCIPHandlingPolicy` setting to request `disable_non_proxied_udp`. It follows `observe → apply → verify → restore` and only reports `verified` after the browser returns the expected setting and `controllable_by_this_extension`. It remains `best_effort`: it is neither a packet-level proof nor a guarantee about every WebRTC implementation path.
