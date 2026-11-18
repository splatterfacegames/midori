// Midori public site + interactive browser workbench at midori.splatterfacegames.com.
// `/` serves the product overview page; `/demo/` serves the WebAssembly workbench.
const HOST = 'midori.splatterfacegames.com';

// Allows WebAssembly compilation for the engine, blob: URLs for generated GLB/PNG
// downloads, self-hosted fonts, and same-origin embedding of `/demo/` on `/`.
const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data: blob:; media-src 'self'; connect-src 'self' data: blob:; worker-src 'self' blob:; frame-src 'self'; frame-ancestors 'self'; base-uri 'none'; form-action 'none'";

function secure(response, immutableAsset = false) {
  const r = new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
    encodeBody: response.headers.has('Content-Encoding') ? 'manual' : 'automatic',
  });
  if (response.status >= 300 && response.status < 400) {
    // Keep redirect headers intact.
  } else if (response.status >= 400) {
    r.headers.set('Cache-Control', 'no-store');
  } else if (immutableAsset) {
    r.headers.set('Cache-Control', 'public, max-age=31536000, immutable');
  } else {
    r.headers.set('Cache-Control', 'public, max-age=60, must-revalidate');
  }
  r.headers.set('X-Content-Type-Options', 'nosniff');
  r.headers.set('Referrer-Policy', 'same-origin');
  r.headers.set('Content-Security-Policy', CSP);
  return r;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname !== HOST) {
      return secure(new Response('Not found', { status: 404 }));
    }
    if (!['GET', 'HEAD'].includes(request.method)) {
      return secure(new Response('Method not allowed', { status: 405 }));
    }
    // Canonicalize `/demo` -> `/demo/` so relative `./assets/*` URLs resolve under `/demo/`.
    if (url.pathname === '/demo') {
      url.pathname = '/demo/';
      return secure(Response.redirect(url.toString(), 301));
    }
    const immutableAsset =
      url.pathname.startsWith('/demo/assets/') ||
      url.pathname.startsWith('/assets/') ||
      url.pathname.startsWith('/fonts/');
    return secure(await env.ASSETS.fetch(request), immutableAsset);
  },
};
