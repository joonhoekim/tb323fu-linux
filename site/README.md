# Project website

The website for this repository: a [Next.js](https://nextjs.org/) (App Router, TypeScript) site exported as static files
for GitHub Pages. The site code is MIT-licensed; the page text is CC BY-SA 4.0 (see `NOTICE`).

**The Markdown in the repository is the only source.** Pages are rendered at build time; nothing is copied into `site/`:

| Source | Route |
|---|---|
| `README.md` | `/docs/` (overview) |
| `docs/<name>.md` — every file, picked up automatically | `/docs/<name>/` |
| `kernel/README.md`, `kernel/initramfs/README.md`, `firmware/README.md`, `userspace/platform/README.md`, `helper/README.md`, `android/README.md` | `/docs/repo/<directory>/` |
| `docs/distros.md` (first table), `docs/hardware-status.md` (status columns), README status note | facts on the landing page `/` |

Internal checklists (`docs/photo-shotlist.md`) are not rendered; the list is `HIDDEN` in [`lib/docs.ts`](lib/docs.ts),
which also holds the navigation order and the directory READMEs. Relative links between rendered files become site
routes (anchors kept, GitHub-style heading ids); links to any other repository file go to its GitHub page. Images that
Markdown references by relative path are copied to `public/_repo/` by `scripts/copy-doc-assets.mjs` before each build.

## Develop

Node.js 20.9 or newer (the workflow uses the current LTS).

```sh
cd site
corepack enable    # once: provides the pnpm version pinned in package.json
pnpm install
pnpm run dev        # http://localhost:3000 — edits to the Markdown show on reload
pnpm run typecheck
```

## Build

```sh
pnpm run build      # static site in site/out/
pnpm run serve      # optional: serve out/ locally
```

For a project page under `https://<user>.github.io/<repo>/` set the base path at build time:

```sh
PAGES_BASE_PATH=/tb323fu-linux pnpm run build
```

Other build-time variables: `REPO_URL` (default `https://github.com/joonhoekim/tb323fu-linux`) and `REPO_BRANCH`
(default `main`) for links to files, `REPO_ROOT` (default: the parent of `site/`).

## Deploy

[`.github/workflows/pages.yml`](../.github/workflows/pages.yml) builds `site/` and publishes `site/out` with the official
Pages actions (`configure-pages` supplies the base path) on every push to `main`, or by hand (Actions → "Deploy
website" → Run workflow). It needs Settings → Pages → Source: **GitHub Actions**.
