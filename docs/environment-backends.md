# V0.8 EnvironmentBackend execution boundary

V0.8 begins with one lifecycle contract and three capability-negotiated
providers. A backend must describe `create`, `start`, `stop`, `pause`,
`snapshot`, `destroy`, `configureNetwork`, `health`, and `logs` exactly once.
An unsupported operation returns `unavailable` with a reason; it is never a
successful no-op.

The executable Rust slice lives in
`apps/desktop/src-tauri/src/environment_backend.rs` and is exposed as
`environment::backend`. `EnvironmentManager` and the narrow Tauri commands are
wired in `apps/desktop/src-tauri/src/environment.rs` and `lib.rs`. The matching
strict UI/native schemas live in
`packages/contracts/src/environment.ts`.

## Evidence rule

Environment exit, DNS, proxy, and guest-health claims can be promoted only from
the fixed guest-agent response protocol. The contract contains only the
`guest_agent` evidence source; it deliberately has no `host` alternative. A
desktop WebView request can describe the controller's path but cannot verify a
WSL, Sandbox, or Hyper-V guest.

For every `proxyRequired: true` request, start fails closed unless the same
environment UUID and controller runtime UUID have fresh guest evidence with the
loopback SOCKS5 path, exit, and **proxy DNS** all `verified`. Proxy DNS and the
guest OS resolver are separate fields. The current WSL agent can verify a
user-configured self-hosted proxy-DNS probe, but deliberately leaves the guest
OS resolver `unavailable`; a successful hostname request is never relabeled as
proof that public DNS is clean.
`proxyRequired: false` still does not authorize a fallback to DIRECT: a selected
fixed proxy must pass both guest exit probes or Chromium is not launched.
Configured host switches, `.wsb` networking, reachable proxy ports, and
successful host HTTP requests do not satisfy the required-proxy rule. WSL
evidence is accepted only when its `observedAt`/`validUntil` window is no more
than two minutes (with at most 30 seconds of future clock skew), its evidence
and runtime UUIDs are non-zero, and its environment UUID, deterministic profile
path, loopback proxy port, and guest-agent hash all match.

## WSL Chromium

- The provider accepts only an exact distribution name present in the latest
  `wsl.exe --list --quiet` discovery result.
- Host launch is always `wsl.exe -d <discovered-name> --user root --exec
/opt/verisilo/bin/verisilo-guest-agent <fixed-operation> --silo-id <uuid>`.
  No `sh -c`, command string, caller executable, or caller argument tail exists.
- The profile path is derived from the Silo UUID in the guest. Pause, snapshot,
  distribution creation/import, and distribution deletion are unavailable.
  Explicitly confirmed `destroy` maps only to the fixed guest `detach`
  operation: it requires Chromium to be stopped, removes that UUID's profile
  and ready receipt plus the matching desktop binding, and never invokes
  `wsl.exe --unregister`.
- V0.8 writes one persistent guest binding and permits only that Silo UUID in a
  selected distribution. A separate active-process binding recovers stale PIDs
  but refuses concurrent multi-Silo Chromium.
- Status distinguishes Windows/WSL availability, discovered distribution,
  fixed guest-agent presence, and WSLg/GUI availability.
- `scripts/verisilo-wsl-guest-agent.sh` implements the current guest operation
  core. It accepts strict, UUID-bound `configure-network`, `start`, `stop`,
  explicitly confirmed `detach`, `health`, `logs`, and `identity` operations;
  it is not a general command runner.

### Fixed guest installation and probe configuration

The agent must be installed as a non-symlink at
`/opt/verisilo/bin/verisilo-guest-agent`. Selection probes run it as root and
accept it only when `identity` reports schema `1`, agent version `0.8.0`, the
fixed path, owner UID `0`, exact mode `0755`, and the fixed non-login
`verisilo-browser` account with an unprivileged UID. The desktop embeds the
source guest-agent bytes at build time, first requires the bundled resource to
match those bytes exactly, then requires the guest file to report the same
SHA-256. Every guest operation rechecks its executing path, owner, mode, and
hash. Chromium is exec'd as the dedicated account with no-new-privileges and
all capability sets empty; the root agent does not launch a root browser. This
is a source-identity check, not a substitute for installer authenticity,
package signatures, or a measured guest boot chain.

The guest exit probe has one fixed configuration file:

```json
{
  "ipEchoUrl": "https://probe.example.invalid/ip",
  "dnsEchoUrl": "https://controlled-answer.example.invalid/dns",
  "dnsProbeHostname": "controlled-answer.example.invalid",
  "expectedDnsAnswer": "192.0.2.10"
}
```

It must be a regular, non-symlink file at
`/etc/verisilo/guest-agent.json`, owned by root with exact mode `0600`. The
object contains exactly `ipEchoUrl`, or that field plus the complete optional
`dnsEchoUrl`/`dnsProbeHostname`/`expectedDnsAnswer` triple. VeriSilo ships no
default public service. The IP endpoint returns only a plain IPv4/IPv6 address;
the DNS endpoint accepts the agent's fixed `hostname` query and returns only
the expected plain address. URL credentials, HTTP, fragments, partial DNS
configuration, unknown fields, oversized responses, and non-IP bodies are
rejected.

The shared contract can represent HTTP, HTTPS, and SOCKS5, but the WSL evidence
provider accepts only credential-free `socks5` at `127.0.0.1:<Silo port>`. Its
JSON field is exactly `proxyRequired`; the snake-case `proxy_required` spelling
and unknown fields are rejected. Every IP and DNS probe explicitly uses
`socks5h`, clears curl's no-proxy bypass, remains HTTPS-only, has fixed time and
4 KiB response ceilings, and has no DIRECT retry.

### What fail-closed and DNS evidence mean here

For every fixed proxy, the guest must obtain a valid plain-IP response through
the configured proxy during network configuration and again immediately before
launch. Chromium is then started with the fixed proxy, no implicit loopback
bypass, direct hostname resolution mapped to failure except for the proxy host,
QUIC disabled, and non-proxied WebRTC UDP disabled. Failure removes the ready
receipt or prevents start; it never changes the Silo to direct mode.
The desktop and guest both invalidate the previous authorization at the start
of a valid stopped-Silo reconfiguration. A failed required-proxy attempt
therefore cannot reuse an older DIRECT binding or ready receipt on the next
Start.

This is **browser-launch-level fail-closed behavior**, not an OS firewall, TUN,
network namespace, or proof that every guest process is unable to use the host
route. A health-probe failure also reports failure but does not kill an already
running browser. If the proxy host is a hostname, resolving that proxy hostname
is an explicit resolver exception; a literal proxy IP avoids that particular
lookup.

With the optional controlled DNS service, the guest response emits
`proxyDns: verified` only when the expected answer returns through the bound
SOCKS5H port. It always emits `guestResolver: unavailable`: the agent does not
issue a direct `getent`/resolver request, so it cannot identify the guest's
recursive resolver, test DNSSEC, or prove all OS traffic leak-free. Without the
DNS triple, optional proxy mode can report proxy DNS unavailable; required mode
fails before launch.

Evidence expiry, a wrong answer, or a crashed proxy removes the ready receipt
and prevents a new launch. It does **not** automatically terminate a running
Chromium process or damage its profile. The fixed proxy remains in Chromium's
arguments, so loss of that listener fails as a proxy connection error rather
than falling back to DIRECT. Only an explicit user `stop` sends SIGTERM to the
exact UID/executable/profile-bound process and never force-kills it.

## Windows Sandbox

Generated `.wsb` files disable vGPU, clipboard, audio input, video input, and
printer redirection, enable Protected Client, XML-escape all values, and reject
every writable host mapping. The bootstrap mapping is read-only. Launch is
only `WindowsSandbox.exe <generated-absolute-wsb-path>` as an argument array.

Sandbox is disposable: pause and snapshot remain unavailable. A signed fixed
host controller now starts `WindowsSandbox.exe` with the deterministic
descriptor and persists a bounded receipt containing the PID, process start
ticks, exact System32 executable, descriptor path, and descriptor SHA-256.
Health and logs describe only that exact host process. Explicit stop uses
`CloseMainWindow` and a bounded wait; it never calls `Stop-Process`, `taskkill`,
or targets an unowned same-name process. Destroy first requires the tracked
process to be exited; a missing receipt plus any untracked Sandbox process
blocks cleanup. Only then are the descriptor, receipts, and read-only bootstrap
copy removed.

Windows Sandbox still has no reliable guest return channel here. `guestHealth`,
proxy, exit, proxy DNS, guest resolver, and browser readiness therefore remain
`unavailable`; host-process health is not promoted into guest health. All fixed
proxies, including optional ones, fail closed at create/configure and never
become DIRECT. No writable host mapping, clipboard, device, audio/video,
printer, or vGPU redirection is added.

## Hyper-V

Release builds do not accept a runner-local image path. Both Windows candidate
workflows require a same-repository Actions artifact ID, one strict lowercase
VHDX leaf filename, a lowercase SHA-256, and the exact redistribution
acknowledgement. `stage-hyperv-image.ps1` queries only the current repository
API, rejects expired artifacts and ZIP traversal, links, or multiple entries,
verifies the VHDX before copying it under the ignored Tauri release staging
tree, and writes the versioned `urn:verisilo:hyperv-image-source:1` manifest.
`verify-hyperv-image.mjs`, release policy, Tauri resource-map self-test,
`SHA256SUMS`, and provenance independently cover the staged manifest and image.
The same filename and SHA-256 are passed to `build.rs` as
`VERISILO_HYPERV_IMAGE_FILE` / `VERISILO_HYPERV_IMAGE_SHA256` for the compile-time
provider boundary. Missing, stale, extra, traversing, or hash-mismatched input
stops the candidate before bundling.

This plumbing does not supply an image and does not make the acknowledgement a
license. A lawfully obtained external Windows/browser image, its retained
license/source evidence, explicit human redistribution approval, and real
signed Hyper-V lifecycle acceptance remain blockers.

The Rust provider writes a new JSON request under the UUID-derived environment
directory and invokes only `scripts/verisilo-hyperv.ps1` with `-File`,
`-RequestPath`, `-StateRoot`, and `-ApprovedImageRoot` arguments. It never builds
a PowerShell command string. A missing manifest image makes `create`
unavailable, but does not remove the signed control path for a previously bound
VM; stop, health, logs, and explicitly confirmed cleanup are not coupled to the
continued presence of the base image.

The script:

- rejects unknown/missing fields, oversized request files, invalid UUIDs,
  unknown actions, traversal, and image paths that are not leaf names under the
  approved root;
- requires elevation and installed Hyper-V cmdlets;
- refuses a create request unless the compile-time manifest schema, leaf image
  filename, and SHA-256 are present, the fixed host-probe and Hyper-V scripts
  have valid timestamped Authenticode signatures from the same signer, and the
  approved base VHDX hash matches. A caller-supplied `imageVerified` assertion
  is not part of the request schema;
- derives the full VM name, differencing-disk path, and full-UUID internal
  switch name from the UUID, then persists and rechecks matching host binding
  metadata and Hyper-V notes;
- persists a bounded provider receipt with exact VM GUID/name, generation 2,
  switch/disk paths, base-image filename/hash, and explicit null guest-agent
  version/hash plus unavailable profile/network/browser evidence; every later
  action revalidates that receipt and the VM GUID;
- uses `New-VMSwitch`, `New-VHD -Differencing`, `New-VM`, `Start-VM`,
  `Stop-VM`, `Save-VM`, `Checkpoint-VM`, and explicitly confirmed `Remove-VM`;
- makes create/start/stop/save/checkpoint/remove retries deterministic, rejects
  foreign same-name resources, disables the Guest Service Interface, and
  rejects DVD, assignable-device, GPU-partition, Fibre Channel, named-pipe COM,
  or external-switch drift; it creates no clipboard, device-redirection, or
  writable-host mapping;
- requires the exact VM to be Off or Saved before destroy, verifies it is gone
  and the private switch has no active adapters before deleting disks/switch,
  and retains bounded success/failure status receipts;
- labels exported status as Hyper-V control-plane data and reports guest-agent
  version/hash, guest profile, health, proxy, exit, proxy DNS, guest resolver,
  and browser readiness as unavailable.

Uninstall has no VM removal path. A VM delete must arrive as its own strict
request with `confirmDestroy: true`; the script removes only the selected
UUID-derived VM, disk, and switch.

## Desktop integration boundary

`AppState` now owns an `EnvironmentManager` behind a dedicated mutex. The
desktop exposes status listing, exact discovered-WSL-distribution selection,
and one strict typed execution command. Execution first requires an existing,
unlocked Silo and refuses to run while the stock-browser runtime has an active
Silo. The manager dispatches only the nine typed lifecycle operations; there is
still no WebView-accessible executable, shell text, arbitrary argument list,
PowerShell fragment, or filesystem-path command.

Stock launch, local provider execution, identity/network/engine updates,
archive, and permanent delete share one outer lifecycle reservation. The fixed
lock order is reservation, Vault, Runtime, then EnvironmentManager. Provider
authorization is checked while holding that reservation, and the reservation
is retained until the slow provider returns even though the inner Vault and
Runtime locks are released. This prevents launch and edit/destructive actions
from interleaving with create, configure, start, or destroy.

Each UUID now has a bounded strict `binding.json` under the fixed application
state root. It persistently binds that UUID to its backend and provider key
(the exact WSL distribution, Sandbox provider generation, or full Hyper-V VM
name). Sandbox descriptors and Hyper-V resources are reconstructed from this
binding after a desktop restart, and mismatched bindings fail closed. The
selected WSL distribution remains session state and fresh guest network
evidence is deliberately not promoted across restart; WSL must be selected and
network evidence refreshed before launch. This is durable provider binding,
not a general multi-Silo registry.

Archive and permanent delete inspect all three UUID-derived local provider
namespaces. A binding, descriptor, request residue, empty partial-create
directory, or other artifact blocks the Vault operation until the provider's
explicit destroy/detach completes. Interrupted cleanup remains visible and
fail-closed rather than orphaning a guest.

On Windows, WSL, Windows Sandbox, PowerShell, and Certutil are resolved through
the trusted System32 path resolver before a `CommandSpec` is created. The host
probe is not run unless the build supplies an exact 64-character lowercase
`VERISILO_AUTHENTICODE_SIGNER_SHA256`; that compile-time value is passed as the
fixed `-ExpectedSignerCertificateSha256` argument. A missing or malformed pin
leaves `releaseScriptsTrusted` false.

## Real Windows release gates

The non-Windows tests prove only schema strictness, fail-closed decisions,
argument-array construction, XML escaping/default denial, path rejection, and
strict request generation. Release still requires real Windows evidence for:

- Windows 10/11 supported editions, including explicit Home/Sandbox/Hyper-V
  unavailable states, administrator vs non-administrator tokens, firmware
  virtualization, optional-feature state, and pending reboot;
- WSL 2 distribution discovery, fixed-agent installation/upgrade/signature,
  root-owned probe configuration, WSLg GUI, Chromium process ownership, proxy
  outage, and guest-origin exit probes per Silo UUID; plus separate evidence
  for resolver identity/DNS leakage and OS-level enforcement if those stronger
  claims are ever exposed;
- Sandbox feature presence, generated policy behavior, read-only mapping,
  clipboard/audio/video/printer/GPU denial, one-instance/disposable behavior,
  and bootstrap signature enforcement;
- signed base-image manifest, VHDX hash, differencing disk, internal switch,
  create/start/stop/save/checkpoint/remove recovery, cross-Silo disk/network
  isolation, and confirmation/uninstall non-deletion guarantees;
- Authenticode for the PowerShell/bootstrap assets and integration tests proving
  that `-ExecutionPolicy AllSigned` succeeds only with the shipped signatures.

Until those gates pass, statuses must remain `missing`, `unknown`, or
`unavailable`; generated plans are not runtime verification.

`tests/windows/Invoke-VeriSiloEnvironmentAcceptance.ps1 -SelfTest` is the
executable acceptance entry point. Its default path runs bounded source
self-tests only. WSL identity, Sandbox exact-process lifecycle, and the destructive Hyper-V
lifecycle each require a separate explicit switch; Hyper-V additionally
requires same-signer timestamped providers, an exact leaf image/hash pair, and
`-ConfirmHyperVDestroy`. The harness emits a JSON result artifact and never
turns an omitted runtime test into a pass. The Node static checks likewise say
explicitly that they exercised no Windows virtualization runtime.
