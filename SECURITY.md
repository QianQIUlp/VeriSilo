# Security policy

Please do not file public issues for vulnerabilities involving the vault, native host, extension message handling, browser profile paths, or proxy configuration. Report them privately to the maintainers once a security contact is published.

Until then, include a minimal reproduction, impact, affected version, and any safe mitigation. Do not include cookies, credentials, personal browsing data, or proxy credentials in reports.

## Security invariants

- The default browser profile is never selected, copied, or modified.
- Native host messages are schema validated, size limited, and origin checked.
- Content-script messages are treated as attacker controlled.
- Long-lived vault keys are never sent to page or MAIN-world code.
- The project does not silently install extensions or apply enterprise policy.
