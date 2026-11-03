// The playground's service worker: the app shell, the wasm and every bundle once fetched stay in the
// browser, so the page and its analysis keep working when the server is gone. build.py fills
// VERSION; play/offline.json lists what the page may fetch in the background. Also sets the headers
// that make the page crossOriginIsolated (SharedArrayBuffer), as coi-serviceworker does.

const VERSION = '__VERSION__';
const DEV = VERSION === 'dev'; // the served scripts and styles change under their names: always ask first
const SHELL = `satex-shell-${VERSION}`, ASSETS = 'satex-assets';
const HASHED = /-[0-9a-f]{12}\.[\w.]+$/;
const root = new URL('../', self.registration.scope);
const waitFor = (ms) => new Promise((r) => setTimeout(r, ms));

/** Same document or worker, plus the headers that isolate it. `credentialless` lets other origins' no-cors fetches through. */
function isolated(response, request) {
  if (!response || response.status === 0 || !['document', 'iframe', 'worker', 'sharedworker'].includes(request.destination)) return response;
  const headers = new Headers(response.headers);
  headers.set('Cross-Origin-Opener-Policy', 'same-origin');
  headers.set('Cross-Origin-Embedder-Policy', 'credentialless');
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

/** The page and the files it needs to start: fetched once at install, the hashed ones kept across versions. */
async function precache() {
  const list = await (await fetch(new URL('play/offline.json', root), { cache: 'no-cache' })).json();
  const [shell, assets] = await Promise.all([caches.open(SHELL), caches.open(ASSETS)]);
  const have = new Set((await assets.keys()).map((r) => r.url));
  const urls = [self.registration.scope, ...list.core.map((p) => new URL(p, root).href)];
  await Promise.all(urls.map(async (url) => {
    const keep = HASHED.test(url) ? assets : shell;
    if (keep === assets && have.has(url)) return;
    const response = await fetch(url, { cache: 'reload' });
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    await keep.put(url, response);
  }));
  // what the new version no longer lists goes
  const wanted = new Set([...list.core, ...list.more.map((m) => m[0])].map((p) => new URL(p, root).href));
  await Promise.all([...have].filter((u) => !wanted.has(u)).map((u) => assets.delete(u)));
}

self.addEventListener('install', (e) => { e.waitUntil(precache().then(() => self.skipWaiting())); });
self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    for (const name of await caches.keys()) if (name.startsWith('satex-shell-') && name !== SHELL) await caches.delete(name);
    await self.clients.claim();
  })());
});

/** The network, unless it is gone or slow: then what was kept. */
async function networkFirst(request) {
  const shell = await caches.open(SHELL);
  try {
    const response = await Promise.race([fetch(request), waitFor(8000).then(() => { throw new Error('slow'); })]);
    if (response.ok && request.method === 'GET') shell.put(request, response.clone()).catch(() => {});
    return response;
  } catch (e) {
    const kept = await shell.match(request, { ignoreSearch: request.mode === 'navigate' }) ?? await caches.match(request);
    if (kept) return kept;
    throw e;
  }
}

/** A file whose name carries its hash never changes: kept, then read from the keeping. */
async function cacheFirst(request) {
  const assets = await caches.open(ASSETS);
  const kept = await assets.match(request);
  if (kept) return kept;
  const response = await fetch(request);
  if (response.ok && response.status === 200) assets.put(request, response.clone()).catch(() => {});
  return response;
}

self.addEventListener('fetch', (e) => {
  const { request } = e, url = new URL(request.url);
  // the server's own endpoints (compile, quit) are never served from the keeping
  if (request.method !== 'GET' || url.origin !== location.origin || url.pathname.includes('/__') || request.headers.has('range')) return;
  const stable = HASHED.test(url.pathname) && !(DEV && /\.(js|css|html)$/.test(url.pathname));
  e.respondWith((stable ? cacheFirst(request) : networkFirst(request)).then((r) => isolated(r, request)));
});
