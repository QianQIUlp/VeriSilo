# Product site deployment

The VeriSilo product site is a static Astro application in `apps/site`. It is designed for Cloudflare Pages and does not require an Astro Cloudflare adapter, Pages Functions, or runtime secrets.

## Local verification

From the repository root:

```bash
pnpm install
pnpm site:check
pnpm site:build
```

The production output is written to `apps/site/dist`.

## Cloudflare Pages build configuration

Connect the `QianQIUlp/VeriSilo` GitHub repository and use these values:

| Setting                | Value                                    |
| ---------------------- | ---------------------------------------- |
| Production branch      | `main`                                   |
| Framework preset       | `Astro` or `None`                        |
| Root directory         | Leave blank (repository root)            |
| Build command          | `pnpm --filter @verisilo/site run build` |
| Build output directory | `apps/site/dist`                         |
| Build system version   | `3`                                      |

Set the following variables for both Production and Preview:

| Variable       | Value     |
| -------------- | --------- |
| `PNPM_VERSION` | `11.17.0` |
| `NODE_VERSION` | `22.16.0` |

Keeping the repository root as the build root is intentional: the shared `pnpm-lock.yaml`, `pnpm-workspace.yaml`, `.npmrc`, and package-manager declaration all live there. Do not use the root `pnpm build` command in Pages; it also builds the desktop and extension applications.

Cloudflare documents the relevant settings in [Build configuration](https://developers.cloudflare.com/pages/configuration/build-configuration/), [Monorepos](https://developers.cloudflare.com/pages/configuration/monorepos/), and [Astro on Pages](https://developers.cloudflare.com/pages/framework-guides/deploy-an-astro-site/).

## Git integration

1. Open Cloudflare Dashboard → **Workers & Pages** → **Create application** → **Pages**.
2. Choose **Import an existing Git repository** (some dashboard versions call this **Connect to Git**).
3. Authorize the Cloudflare GitHub App for the VeriSilo repository.
4. Apply the build settings and variables above.
5. Select **Save and Deploy**.
6. Confirm that both `/` and `/zh/` render on the generated `*.pages.dev` address.

The production branch updates the production URL. Other branches create preview deployments by default, and pull requests from the same repository receive preview links. Fork pull requests do not automatically receive previews. See Cloudflare's [Git integration](https://developers.cloudflare.com/pages/get-started/git-integration/) and [Preview deployments](https://developers.cloudflare.com/pages/configuration/preview-deployments/) documentation.

## Build watch paths

In **Settings → Build → Build watch paths**, use these include paths:

```text
apps/site/*
pnpm-lock.yaml
pnpm-workspace.yaml
package.json
.npmrc
```

Leave excludes empty. These paths avoid rebuilding the website for desktop-only or extension-only changes while still rebuilding it when workspace installation metadata changes. See [Build watch paths](https://developers.cloudflare.com/pages/configuration/build-watch-paths/).

## Custom domain

For `verisilo.qiu.works`:

1. Open the Pages project → **Custom domains** → **Set up a domain**.
2. Enter `verisilo.qiu.works`.
3. If `qiu.works` is already managed by the same Cloudflare account, allow Cloudflare to create the DNS record.
4. If DNS is managed elsewhere, first finish the Pages domain association, then create a CNAME:
   - Name: `verisilo`
   - Target: the generated `<project>.pages.dev` hostname
5. Wait for the Pages domain status to become **Active**, then verify HTTPS.

Associate the domain from the Pages project before adding a manual CNAME. Cloudflare warns that creating only the DNS record can result in a `522` response. See [Custom domains](https://developers.cloudflare.com/pages/configuration/custom-domains/).

## Post-deployment checks

Verify:

- `/` returns the English page.
- `/zh/` returns the Chinese page.
- The language links switch between the two routes.
- GitHub and documentation links open the intended repository pages.
- `https://verisilo.qiu.works/sitemap.xml` and `/robots.txt` are reachable.
- Response headers include the policies defined in `apps/site/public/_headers`.
- A pull request branch receives a preview URL and does not replace production.

Cloudflare dashboard labels change occasionally. Locate the equivalent **Build configuration**, **Variables and Secrets**, **Build watch paths**, **Branch control**, and **Custom domains** sections if the wording differs.
