# Midori demo site

Public demo of the browser workbench at **https://midori.splatterfacegames.com**.

- `.github/workflows/demo.yml` builds the workbench (`npm run build` → `dist/`) on every push to `master` and deploys it to the Cloudflare Worker `midori-demo`.
  The Worker's custom domain is `midori.splatterfacegames.com`. `workflow_dispatch` redeploys manually.
- The engine is the committed wasm bundle in `apps/desktop/src/wasm`, so deployment needs no Rust toolchain.
  Rebuild it with `npm run wasm` after engine changes. CI fails if the committed bundle is stale.
- Public `workers.dev` and preview URLs remain disabled; requests are served publicly on `midori.splatterfacegames.com` with hashed `/assets/*` bundles cached immutably and HTML revalidated cleanly.
- Repository secrets: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` and `JETHAFORGE_TOKEN` (the last one is already used by CI).
- The Content-Security-Policy allows `'wasm-unsafe-eval'` for the engine and `blob:` for generated GLB/PNG previews.
  `scripts/browser-smoke.mjs` passes under this policy.

Manual deploy: `npm ci && npm run build && npx wrangler@4.162.0 deploy --config deploy/demo/wrangler.jsonc`, with Cloudflare credentials in the environment.
