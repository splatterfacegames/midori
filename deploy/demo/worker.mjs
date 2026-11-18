// Midori demo: public browser build of the workbench at midori.splatterfacegames.com.
const HOST = 'midori.splatterfacegames.com';

// The workbench needs WebAssembly compilation for the engine and blob: URLs for
// generated GLB/PNG previews; everything else stays same-origin.
const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self'; connect-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

function secure(response, immutableAsset = false) {
  const r = new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
    encodeBody: response.headers.has('Content-Encoding') ? 'manual' : 'automatic',
  });
  if (response.status >= 400) {
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
    const immutableAsset = url.pathname.startsWith('/assets/');
    return secure(await env.ASSETS.fetch(request), immutableAsset);
  },
};
