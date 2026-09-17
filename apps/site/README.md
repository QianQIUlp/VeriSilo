# Public site and interactive demo

Run `pnpm site:dev` from the workspace root. Validate with `pnpm site:check`,
`pnpm site:build`, and the desktop focused tests. Static output is `apps/site/dist`.

The English landing page (`/`) loads `/demo/en/`; the Chinese page (`/zh/`) loads
`/demo/`. The fallback and standalone links use the same locale. These are lazy,
same-origin iframes. This is the
actual desktop React UI, bundled by Astro from `apps/desktop/src/preview/public.tsx`.
It uses the in-memory preview API; no Rust, desktop bridge, or local service runs.
The website build aliases the network check client to an explicit simulated result,
so clicking network checks does not probe the visitor's IP or DNS. Unsupported
desktop operations fail with a demo-specific message. Refreshing resets demo data.
Only the optional `verisilo.motion` preference persists in local storage.

Demo language is set by the route's document language, so scene switches and
reloads retain it. `src/demo/localize.mjs` is a site-only build transform: it maps
authored desktop UI copy through `src/demo/en.json` before React compiles it.
Runtime names, URLs, commands, credentials, and observed values are not translated.
Mock fixture names are authored demo content and do have English equivalents.
The installed desktop bundle never uses this transform. New authored Chinese
copy without a catalog entry fails the site build instead of silently mixing
languages. Keep template placeholders intact when updating translations.
Run `node --test apps/site/src/demo/localize.test.mjs` for the focused language checks.

The iframe remains mounted when expanded or collapsed to preserve the visitor's
session. It announces readiness and Escape via origin/source-checked messages.
The public CSP and frame policy allow embedding only by the same origin; production
styles remain external. Test the production output with these response headers,
since Vite development mode does not apply the Pages `_headers` file.

Deployment remains the existing Cloudflare Pages project and build command.
In addition to the watch paths in `docs/site-deployment.md`, changes to
`apps/desktop/src/*`, `apps/desktop/package.json`, and `packages/contracts/*` now
affect this bundle and should trigger site rebuilds. No desktop production build
or installer is needed. Public download copy identifies the current public
prerelease while the demo remains labelled as the current development UI.
