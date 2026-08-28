# Third-party notices

VeriSilo does not copy code from Donut Browser or any AGPL-licensed project.

Before any source code is incorporated from another project, the pull request must record the upstream URL, immutable version, exact license, attribution text, and whether the change is a derivative work. FingerprintJS and CreepJS concepts may be studied under their upstream MIT licenses; BrowserForge concepts may be studied under Apache-2.0. No upstream code is included in the initial repository.

## Dependency inventory is not a license conclusion

VeriSilo's JavaScript and Rust dependency inventory is generated from
`pnpm-lock.yaml` and the locked Rust graphs rooted at
`apps/desktop/src-tauri/Cargo.lock` and
`crates/verisilo-remote-backend/Cargo.lock`. The desktop graph includes the
path-linked Remote Agent library; its standalone lock is also retained as a
release-provenance input. The generated CycloneDX
and SPDX documents intentionally use `NOASSERTION` when the lockfile itself does
not prove a license. Before a public binary release, maintainers must review the
upstream license text for the exact target-specific dependency set and add any
required copyright, attribution, source-offer, or notice text here.

The inventory contains development, build, optional, target-specific, and
transitive packages. Presence in the lockfile inventory does not by itself mean a
component is linked into or distributed with a Windows artifact.

## Bundled UI icons

- **Heroicons v2.2.0:** the browser extension bundles six unmodified 24px
  outline SVG icons from
  <https://github.com/tailwindlabs/heroicons/tree/0435d4ca364a608cc75e2f8683d374e55abbae26/optimized/24/outline>.
  Upstream tag `v2.2.0` resolves to commit
  `0435d4ca364a608cc75e2f8683d374e55abbae26`. Heroicons is MIT licensed,
  Copyright (c) Tailwind Labs, Inc. The distributed license text is retained at
  `apps/extension/icons/ui/HEROICONS-LICENSE.txt`. These files are unmodified
  upstream assets, not a derivative icon set.

Windows release candidates also include `dependency-licenses.json`. It
cross-checks the lockfiles against `pnpm licenses list` and `cargo metadata` for
the Windows target, strips runner-local paths, and leaves unmatched lock entries
visible. Every entry remains marked `requiresHumanReview`; this evidence report
does not replace review of the exact shipped dependency graph or required
license texts and notices.

## Components discussed but not bundled

- **Mihomo / Clash-compatible cores:** VeriSilo currently connects to a user's
  separately installed, user-controlled local endpoint/controller. No Mihomo or
  Clash binary, subscription, node list, configuration, or source code is bundled.
  Bundling one later requires a separate immutable-version and license review.
- **Controlled Chromium and Camoufox:** the V0.7 source contains adapter IDs,
  package verification/update/rollback state and launch protocol support, but no
  Chromium/Camoufox executable, patch set, browser distribution or upstream source
  is bundled. Enabling either requires a separately reviewed, signed, hash-locked
  package and its own license/SBOM evidence.
- **Hyper-V base image:** no VHD/VHDX, Windows installation, browser image, or
  image license is stored in this repository. Release plumbing can consume one
  exact same-repository Actions artifact only after a redistribution-rights
  acknowledgement, but that acknowledgement is not legal approval. A public
  candidate still requires independent review of the exact image source,
  license, notices, redistribution scope, and any browser/OS terms.
- **Remote browser Provider and guest image:** the V0.9 Agent contains only a
  fixed-hash typed Provider bridge and an honest unavailable-provider example. No
  VM/container image, Linux distribution, browser, media server, automation
  runtime, proxy node or remote Provider binary is bundled.
- **Chromium, Google Chrome, and Microsoft Edge:** the stock provider discovers a
  browser already installed by the user. VeriSilo does not redistribute those
  browser binaries. The Windows installer may use Microsoft's WebView2 bootstrap
  mechanism required by Tauri; Microsoft distribution terms apply separately.
- **FingerprintJS, CreepJS, and BrowserForge:** they are research references only
  at this stage. Their repositories are not vendored and their source code is not
  copied into the release.

## VeriSilo licensing boundary

VeriSilo source code is offered under MPL-2.0 as stated in `LICENSE`.
Documentation licensing and third-party components remain governed by their own
applicable terms; this notice does not relicense them.
