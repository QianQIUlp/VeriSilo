# Linux desktop

The RC5 desktop source has one Windows/Linux implementation. The Linux build
targets native x86_64 Linux, starting with Ubuntu 22.04's glibc and WebKitGTK 4.1.
Windows keeps its existing RC5 NSIS build. Linux produces Debian and AppImage
packages. The Linux Managed build uses the same pinned Firefox/Camoufox source
and RC5 downstream patches, compiled for Linux; an upstream prebuilt Linux
browser or the Windows engine package cannot substitute for that build.

## Build

On native Linux, install Node 22+, pnpm 11.17.0, Rust 1.88.0, Python 3.12, uv,
OpenSSL, and the native Tauri dependencies:

```sh
sudo apt-get install build-essential libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf openssl
pnpm install --frozen-lockfile
```

For the bundled Managed Identity engine, the browser compiler additionally
needs the Firefox development packages and bootstrap toolchain. The complete
package list and reproducible commands are in
[the native Linux workflow](../.github/workflows/linux-desktop.yml). Allow at
least 30 GiB free space and 8 GiB combined RAM/swap; the CI recipe uses two build
workers and additional swap for the Firefox linker. This is a native Firefox
build and can take several hours.

```sh
python3 apps/camoufox-host/build/linux/build.py \
  --work-root /absolute/path/to/new-engine-work \
  --out artifacts/linux-engine-build
uv sync --project apps/camoufox-host --frozen --group build
apps/camoufox-host/.venv/bin/python scripts/build-camoufox-host-package.py \
  --linux --sign --out artifacts/linux-engine-package \
  --browser-root artifacts/linux-engine-build/browser \
  --browser-tree-manifest artifacts/linux-engine-build/browser-tree-manifest.json \
  --build-result artifacts/linux-engine-build/linux-build-result.json \
  --pem-certificate /external/path/to/code-signing-cert.pem \
  --pem-private-key /external/path/to/private-key.pem
export VERISILO_ENGINE_SIGNER_SHA256="$(openssl x509 -in /external/path/to/code-signing-cert.pem -outform DER | sha256sum | cut -d' ' -f1)"
pnpm desktop:package:linux --engine-package artifacts/linux-engine-package \
  --python apps/camoufox-host/.venv/bin/python
```

The signing certificate must currently be valid and explicitly declare the
code-signing EKU. Keep private keys outside the repository. An encrypted PEM
key's password is read from the environment variable named by `--password-env`.
The desktop embeds the public certificate pin and the newly compiled Linux
browser's exact asset hashes. Packaging verifies detached SHA-256 CMS, the
signer pin and the complete package tree before producing the installers.
Explicit engine install/update/rollback uses the production CMS verifier.
Bundled-engine activation retains RC5's installed-package trust model: it
checks fixed layout and asset bindings without rehashing the complete installed
tree on each health query or launch. Runtime evidence records that distinction;
matched identity requires observations from the actual launched browser.

The Linux workflow uses an ephemeral evaluation certificate. Its artifacts are
CI candidates; the workflow does not publish a GitHub release or reuse the
Windows release signer. An unchanged compiled browser artifact can be restored
with `build.py --restore-build <directory>`; its archive, recipe, patch inputs and
tree must still match. Desktop/Host fixes do not require recompiling unchanged
browser inputs.

Main pushes and pull requests compile/test Linux and build and exercise the
explicit Standard Debian and AppImage packages. The complete Managed build is
manual because compiling Firefox can take several hours:

```sh
gh workflow run linux-desktop.yml --ref main -f buildManagedEngine=true
```

To rebuild the current desktop and Host with an unchanged browser, set
`ENGINE_ARTIFACT_ID` to the Actions artifact ID of a saved
`verisilo-linux-engine-build-...` artifact, then run:

```sh
gh workflow run linux-desktop.yml --ref main -f engineBuildArtifactId="$ENGINE_ARTIFACT_ID"
```

Both manual paths install and exercise the complete Managed Debian and AppImage
packages, including Standard browser lifecycle. The separate Standard job is
skipped on those runs. Browser artifact reuse verifies the frozen recipe,
source lock, patch inputs, archive and browser tree before rebuilding the Host
and desktop; an expired artifact must be replaced with a new native build.

Managed font verification requires genuine system fonts distinct from the
browser's bundled identity fonts. The Managed Debian package installs
`fontconfig`, `fonts-unfonts-core` and `fonts-unfonts-extra` as dependencies.
AppImage users must install those system packages before the first Managed
launch. Missing native font controls fail closed; bundled fonts are not reported
as native host evidence.

For an explicit Standard-only development build:

```sh
pnpm desktop:package:linux --standard
```

The default Linux packaging command requires a signed Managed engine package;
it never silently omits Managed Identity. Windows remains available through
`pnpm desktop:package:windows` and its existing release script options.

## Runtime and data

The Linux desktop uses the existing React UI, Vault encryption, Silo identity,
network policy and runtime evidence contracts. Browser discovery recognizes
native Chrome/Chromium and Edge paths; the CLI is `verisilo-cli`. Data defaults
to `$XDG_DATA_HOME/VeriSilo` or `~/.local/share/VeriSilo`. Named Vaults remain below
that root and use an exclusive per-Vault process lock.

Native Clash Verge uses its current-user Unix socket through the existing
`pipe://verge-mihomo/` controller alias. A loopback HTTP controller is also
supported. Pinned nodes run in a Silo-specific Mihomo process using a validated
copy of the active configuration; generic controllers cannot substitute for
that isolated provider.

Managed Host descendants use Linux process groups and an independent owner
supervisor. Closing or crashing the owner must reclaim its browser tree without
affecting unrelated browsers. Installed browser payloads are immutable; caches,
Profiles and evidence remain in the user's data directory.

Cold backup remains limited to the same machine, operating-system user, Vault
and Silo. Windows retains DPAPI. Linux wraps the random archive factor with
authenticated encryption bound to the secret seed in the unlocked encrypted
Vault, `/etc/machine-id` and the effective UID. The backup passphrase is also
required. Copying an archive to another machine, user or Vault does not make it
restorable. This is not a Windows/Linux Profile migration format.

Linux exposes native desktop execution; Windows-only WSL, Sandbox and Hyper-V
providers keep their unavailable status outside Windows. Native OS window
enumeration is distinct from Playwright page control and must not be represented
as verified when unavailable.

## Verification

The native workflow compiles/tests the Linux backend, builds both package
formats and exercises the actual desktop API and WebView, Standard browser
lifecycle and Vault persistence. The Managed job additionally builds the patched
engine and tests native Host ownership, signatures, matched identity/page control,
screenshots, two concurrent Managed Silos with isolated same-site storage, and
cold backup/restore while the other Silo remains running. Results are
written under `artifacts/linux-desktop-smoke/` and attached to the Actions run.

Windows runtime evidence remains Windows-specific. Linux unit tests or a
successful package build do not establish Linux Managed runtime acceptance;
the native Managed smoke must pass on the built candidate. Distribution-wide
compatibility, native Wayland behavior and ARM builds require their own evidence.
