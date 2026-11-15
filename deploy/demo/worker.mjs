// Midori demo: the browser build of the workbench, served only to the owner.
// Cloudflare Access gates the hostname at the edge; this Worker re-checks the
// Access JWT (signature, issuer, audience, expiry, allowed email) before any
// static asset is served, so a misconfigured Access policy fails closed.
const HOST = 'midori.splatterfacegames.com';
const allowed = new Set(['jethachan@gmail.com']);
let keyCache;

function decode(s) { return Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0)); }

async function authenticate(request, env) {
  const token = request.headers.get('Cf-Access-Jwt-Assertion');
  if (!token || !env.ACCESS_ISSUER || !env.ACCESS_AUD) return false;
  try {
    const [h, p, s, ...extra] = token.split('.');
    if (extra.length) return false;
    const header = JSON.parse(new TextDecoder().decode(decode(h)));
    const claims = JSON.parse(new TextDecoder().decode(decode(p)));
    const now = Date.now() / 1000;
    if (header.alg !== 'RS256' || claims.iss !== env.ACCESS_ISSUER || !Array.isArray(claims.aud) || !claims.aud.includes(env.ACCESS_AUD)
      || !(claims.exp > now) || claims.nbf > now + 30 || !allowed.has(String(claims.email).toLowerCase())) return false;
    if (!keyCache || keyCache.until < Date.now() || !keyCache.keys.some((k) => k.kid === header.kid)) {
      const response = await fetch(env.ACCESS_ISSUER + '/cdn-cgi/access/certs');
      if (!response.ok) return false;
      keyCache = { keys: (await response.json()).keys, until: Date.now() + 300000 };
    }
    const jwk = keyCache.keys.find((k) => k.kid === header.kid);
    if (!jwk) return false;
    const key = await crypto.subtle.importKey('jwk', jwk, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']);
    return await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, decode(s), new TextEncoder().encode(h + '.' + p));
  } catch {
    return false;
  }
}

// The workbench needs WebAssembly compilation for the engine and blob: URLs for
// generated GLB/PNG previews; everything else stays same-origin.
const CSP = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self'; connect-src 'self' data: blob:; worker-src 'self' blob:; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

function secure(response) {
  const r = new Response(response.body, { status: response.status, statusText: response.statusText, headers: response.headers, encodeBody: response.headers.has('Content-Encoding') ? 'manual' : 'automatic' });
  r.headers.set('Cache-Control', 'private, no-store');
  r.headers.set('X-Content-Type-Options', 'nosniff');
  r.headers.set('X-Robots-Tag', 'noindex, nofollow, noarchive');
  r.headers.set('Referrer-Policy', 'same-origin');
  r.headers.set('Content-Security-Policy', CSP);
  return r;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.hostname !== HOST || !await authenticate(request, env)) return secure(new Response('Sign in through Cloudflare Access.', { status: 403 }));
    if (!['GET', 'HEAD'].includes(request.method)) return secure(new Response('Method not allowed', { status: 405 }));
    return secure(await env.ASSETS.fetch(request));
  },
};
