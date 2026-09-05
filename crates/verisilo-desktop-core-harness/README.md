# Desktop core Rust harness

This development/test-only crate compiles the production desktop core modules
directly from `apps/desktop/src-tauri/src`. It intentionally has no Tauri,
GTK, WebKit, or application bundling dependency.

It also compiles `application/`, the shared operations called by Tauri commands
and the local HTTP API. Run its focused tests with `cargo test --offline --locked
--manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --lib application::`.
The two-root test checks independent Vault sessions and reopening without an app
window. See [parallel development](../../docs/development-worktrees.md) for ownership
and isolated development commands.

Run all gates with Rust 1.88:

```bash
cargo fmt --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml -- --check
cargo check --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml
cargo test --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml
cargo clippy --offline --locked --manifest-path crates/verisilo-desktop-core-harness/Cargo.toml --all-targets -- -D warnings
```

`Cargo.lock` belongs only to this isolated harness and makes CI's `--locked`
resolution reproducible. Dependency requirements mirror the desktop manifest;
the production desktop manifest and lockfile remain authoritative for shipped
artifacts.

Linux runs the platform-independent domain, engine, environment model/backend,
launcher, Mihomo, relay, Native Host model, and Vault tests. Windows CI runs the
same harness natively so `cfg(target_os = "windows")` code is compiled and its
Windows-only tests can run. Passing this harness is core Rust evidence only: it
does not prove a Tauri build, GTK/WebKit integration, Windows packaging, real
browser behavior, or OS-level isolation.
