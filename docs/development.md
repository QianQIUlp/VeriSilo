# Development

## Prerequisites

- Node.js 22 or newer and pnpm 11 or newer.
- Rust 1.88 or newer, Cargo, and the Tauri v2 Windows prerequisites for the desktop build.
- Chrome and/or Edge for manual Windows validation.

The current development environment may build and test the TypeScript workspace without Rust. Desktop compilation is intentionally not faked when the Rust toolchain is unavailable.

## Commands

```bash
pnpm install
pnpm check
pnpm test
pnpm extension:build
pnpm desktop:dev
```

## Native Messaging development

The production installer must write user-level host manifests that contain only the published Chrome Web Store and Edge Add-ons extension IDs. The development registration script accepts IDs explicitly and is never a release-installation mechanism.
