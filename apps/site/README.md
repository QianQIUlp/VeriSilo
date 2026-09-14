# Public site and interactive demo

Run `pnpm site:dev` from the workspace root. Validate with `pnpm site:check`,
`pnpm site:build`, and the desktop focused tests. Static output is `apps/site/dist`.

The landing pages (`/`, `/zh/`) load a lazy same-origin `/demo/` iframe. This is the
actual desktop React UI, bundled by Astro from `apps/desktop/src/preview/public.tsx`.
It uses the in-memory preview API; no Rust, desktop bridge, or local service runs.
The website build aliases the network check client to an explicit simulated result,
so clicking network checks does not probe the visitor's IP or DNS. Unsupported
desktop operations fail with a demo-specific message. Refreshing resets demo data.
Only the optional `verisilo.motion` preference persists in local storage.

The iframe remains mounted when expanded or collapsed to preserve the visitor's
session. It announces readiness and Escape via origin/source-checked messages.
The public CSP and frame policy allow embedding only by the same origin; production
styles remain external. Test the production output with these response headers,
since Vite development mode does not apply the Pages `_headers` file.

Deployment remains the existing Cloudflare Pages project and build command.
In addition to the watch paths in `docs/site-deployment.md`, changes to
`apps/desktop/src/*`, `apps/desktop/package.json`, and `packages/contracts/*` now
affect this bundle and should trigger site rebuilds. No desktop production build
or installer is needed. Public download copy deliberately identifies rc3 while
the demo is labelled as the current development UI.
