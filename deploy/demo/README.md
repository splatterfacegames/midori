# Midori demo site

Private demo of the browser workbench at **https://midori.splatterfacegames.com**.
It is hosted the same way as squads.jethachan.net.

- `.github/workflows/demo.yml` builds the workbench (`npm run build` → `dist/`) on every push to `master` and deploys it to the Cloudflare Worker `midori-demo`.
  The Worker's custom domain is midori.splatterfacegames.com. `workflow_dispatch` redeploys manually.
- The engine is the committed wasm bundle in `apps/desktop/src/wasm`, so deployment needs no Rust toolchain.
  Rebuild it with `npm run wasm` after engine changes. CI fails if the committed bundle is stale.
- Cloudflare Access restricts the hostname to the owner's sign-in. `worker.mjs` also checks the Access JWT before serving any file: signature, issuer, `ACCESS_AUD` audience, expiry and allowed email.
  A wrong or missing audience therefore fails closed with 403. Public workers.dev and preview URLs are disabled.
  This repository does not provision the Access application or its policy.
- Repository secrets: `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` and `JETHAFORGE_TOKEN` (the last one is already used by CI).
  `ACCESS_ISSUER` and `ACCESS_AUD` in `wrangler.jsonc` are public identifiers, not credentials.
- The Content-Security-Policy allows `'wasm-unsafe-eval'` for the engine and `blob:` for generated GLB/PNG previews.
  `scripts/browser-smoke.mjs` passes under this policy.

Manual deploy: `npm ci && npm run build && npx wrangler@4.162.0 deploy --config deploy/demo/wrangler.jsonc`, with Cloudflare credentials in the environment.
