import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import http from 'node:http';
import net from 'node:net';
import zlib from 'node:zlib';

const DEFAULT_URL = 'http://127.0.0.1:5173';
const VIEWPORT = { width: 1440, height: 900 };
const CHROME_CANDIDATES = [
  process.env.CHROME_BIN,
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe'
].filter(Boolean);

const targetUrl = process.env.MIDORI_VISUAL_URL ?? DEFAULT_URL;
const screenshotDir = resolve('target/browser-smoke');
const PREVIEW_STATS_HELPER = `function previewStats() {
  const text = document.querySelector('.preview-container .stats')?.textContent?.replace(/\\s+/g, ' ').trim() ?? '';
  const match = (label) => {
    const found = text.match(new RegExp(label + ':\\\\s*([0-9,]+)'));
    return found ? Number(found[1].replace(/,/g, '')) : 0;
  };
  return {
    text,
    lod: text.split(' ')[0] ?? '',
    vertices: match('Vertices'),
    triangles: match('Triangles'),
    branches: match('Branches'),
    leaves: match('Leaves'),
    scatter: match('Scatter'),
    prototypes: match('Prototypes')
  };
}`;

async function main() {
  if (typeof WebSocket === 'undefined') {
    throw new Error('This smoke test needs a Node runtime with global WebSocket support.');
  }

  await assertServerReachable(targetUrl);
  const chromePath = await findChrome();
  const port = await findFreePort();
  const userDataDir = await mkdtemp(join(tmpdir(), 'midori-browser-'));
  await mkdir(screenshotDir, { recursive: true });

  const chrome = spawn(chromePath, [
    '--headless=new',
    '--disable-gpu',
    '--disable-dev-shm-usage',
    '--disable-background-networking',
    '--disable-extensions',
    '--disable-sync',
    '--no-default-browser-check',
    '--no-first-run',
    '--enable-unsafe-swiftshader',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${userDataDir}`,
    `--window-size=${VIEWPORT.width},${VIEWPORT.height}`,
    'about:blank'
  ], {
    stdio: ['ignore', 'ignore', 'pipe']
  });

  chrome.stderr.setEncoding('utf8');
  chrome.stderr.on('data', data => {
    if (process.env.MIDORI_BROWSER_SMOKE_VERBOSE) {
      process.stderr.write(data);
    }
  });

  let tab = null;
  let pageInfo = null;

  try {
    await waitForChrome(port);
    pageInfo = await openTab(port, targetUrl);
    tab = await CdpClient.connect(pageInfo.webSocketDebuggerUrl);

    await setupPage(tab);
    await runEditorSmoke(tab);
  } finally {
    if (tab) await closeTab(tab, port, pageInfo?.id);
    if (chrome.stderr) chrome.stderr.destroy();
    await terminateProcess(chrome);
    await removeWithRetry(userDataDir);
  }
}

async function runEditorSmoke(tab) {
  await waitFor(tab, 'document.querySelector("canvas") && document.body.innerText.includes("Midori")');

  await clickButton(tab, 'Oak');
  await waitFor(tab, 'document.querySelector("#species-name")?.value === "Oak"');
  await waitFor(tab, 'previewStats().lod === "High" && previewStats().vertices > 1000 && previewStats().triangles > 1000');
  await assertCanvasLooksRendered(tab, 'oak-high');

  await clickSelector(tab, '[data-testid="lod-button-2"]');
  await waitFor(tab, 'previewStats().lod === "Low" && previewStats().vertices > 0 && previewStats().triangles > 0');

  await setChecked(tab, 'input[type="checkbox"]', label => label.includes('Scale Reference'), true);
  await waitFor(tab, 'Array.from(document.querySelectorAll("label")).some(label => label.textContent.includes("Scale Reference") && label.querySelector("input")?.checked)');

  await clickButton(tab, 'Export GLB');
  await clickSection(tab, 'Export');
  await waitFor(tab, 'document.body.innerText.includes("LOD Chain") && document.body.innerText.includes("High / Medium / Low")');
  await waitFor(tab, '!document.body.innerText.includes("Export failed")');

  await clickButton(tab, 'Joshua Prototype');
  await waitFor(tab, 'document.querySelector("#species-name")?.value === "Joshua Prototype"');
  await clickSelector(tab, '[data-testid="lod-button-0"]');
  await waitFor(tab, 'previewStats().lod === "High" && previewStats().vertices > 1000 && previewStats().triangles > 1000');
  await assertCanvasLooksRendered(tab, 'joshua-high');

  await clickButton(tab, 'Forest Floor');
  await waitFor(tab, 'document.body.innerText.includes("Temperate Forest Floor")');
  await waitFor(tab, 'previewStats().text.includes("Nature Patch") && previewStats().vertices > 1000 && previewStats().triangles > 1000 && previewStats().scatter > 0 && previewStats().prototypes >= 8');
  await assertCanvasLooksRendered(tab, 'forest-floor-nature');
  await waitFor(tab, 'document.body.innerText.includes("TERRAIN AND SOIL") && document.body.innerText.includes("Grass Density") && document.body.innerText.includes("Shrub Density") && document.body.innerText.includes("Rock Density") && document.body.innerText.includes("Log Density") && document.body.innerText.includes("PROFILES")');

  const baselineScatter = (await readPreviewStats(tab)).scatter;
  await setNumberInput(tab, 'Grass Density', 0.2);
  await waitFor(tab, `previewStats().scatter !== ${baselineScatter} && previewStats().prototypes >= 8`);
  await setNumberInput(tab, 'Grass Density', 0.13);
  await waitFor(tab, `previewStats().scatter === ${baselineScatter} && previewStats().prototypes >= 8`);
  const authoringStats = await readPreviewStats(tab);
  await setSelectValue(tab, 'Preview Profile', 'mobile');
  await waitFor(tab, `previewStats().scatter < ${authoringStats.scatter} && previewStats().text.includes("Profile: Mobile")`);
  await setSelectValue(tab, 'Prototype LOD', '2');
  await waitFor(tab, `previewStats().triangles < ${authoringStats.triangles} && previewStats().text.includes("LOD: LOD 2")`);
  await setSelectValue(tab, 'Preview Profile', 'authoring');
  await setSelectValue(tab, 'Prototype LOD', '0');
  await waitFor(tab, `previewStats().scatter === ${authoringStats.scatter} && previewStats().text.includes("Profile: Authoring") && previewStats().text.includes("LOD: LOD 0")`);
  await clickButton(tab, 'Meadow');
  await waitFor(tab, 'document.body.innerText.includes("Flowering Meadow") && previewStats().text.includes("Nature Patch") && previewStats().scatter > 0 && previewStats().prototypes >= 5');
  await assertCanvasLooksRendered(tab, 'flowering-meadow-nature');
  await clickButton(tab, 'Arid Scrub');
  await waitFor(tab, 'document.body.innerText.includes("Arid Scrub") && previewStats().text.includes("Nature Patch") && previewStats().scatter > 0 && previewStats().prototypes >= 7');
  await assertCanvasLooksRendered(tab, 'arid-scrub-nature');

  const summary = await evaluate(tab, `(() => {
    const canvas = document.querySelector('canvas');
    const lodOptions = Array.from(document.querySelectorAll('#lod-level option')).map(option => option.textContent.trim());
    return {
      title: document.title,
      canvas: canvas ? { width: canvas.width, height: canvas.height } : null,
      species: document.querySelector('#species-name')?.value,
      lodOptions,
      mode: document.body.innerText.includes('Nature Patch') ? 'nature' : 'tree',
      statsText: document.querySelector('.stats')?.textContent?.replace(/\\s+/g, ' ').trim()
    };
  })()`);

  console.log(JSON.stringify({
    ok: true,
    url: targetUrl,
    screenshots: [
      'target/browser-smoke/oak-high.png',
      'target/browser-smoke/joshua-high.png',
      'target/browser-smoke/forest-floor-nature.png',
      'target/browser-smoke/flowering-meadow-nature.png',
      'target/browser-smoke/arid-scrub-nature.png'
    ],
    summary
  }, null, 2));
}

async function setupPage(tab) {
  await tab.send('Page.enable');
  await tab.send('Runtime.enable');
  await tab.send('Log.enable');
  await tab.send('Emulation.setDeviceMetricsOverride', {
    width: VIEWPORT.width,
    height: VIEWPORT.height,
    deviceScaleFactor: 1,
    mobile: false
  });

  const errors = [];
  tab.on('Runtime.exceptionThrown', event => {
    errors.push(
      event.exceptionDetails?.exception?.description ??
      event.exceptionDetails?.text ??
      'runtime exception'
    );
  });
  tab.on('Runtime.consoleAPICalled', event => {
    if (['error', 'warning'].includes(event.type)) {
      const text = event.args?.map(arg => arg.value ?? arg.description ?? '').join(' ') ?? event.type;
      errors.push(text);
    }
  });
  tab.on('Log.entryAdded', event => {
    if (event.entry?.level === 'error') {
      errors.push(event.entry.text);
    }
  });

  await tab.send('Page.navigate', { url: targetUrl });
  await waitForEvent(tab, 'Page.loadEventFired', 15000);
  tab.errors = errors;
}

async function assertCanvasLooksRendered(tab, label) {
  await waitFor(tab, 'document.querySelector("canvas")?.width > 0 && document.querySelector("canvas")?.height > 0');
  await delay(700);

  const rect = await evaluate(tab, `(() => {
    const canvas = document.querySelector('canvas');
    const rect = canvas.getBoundingClientRect();
    return {
      x: Math.max(0, Math.floor(rect.left + rect.width * 0.2)),
      y: Math.max(0, Math.floor(rect.top + rect.height * 0.15)),
      width: Math.max(1, Math.floor(rect.width * 0.6)),
      height: Math.max(1, Math.floor(rect.height * 0.65))
    };
  })()`);

  const screenshot = await tab.send('Page.captureScreenshot', {
    format: 'png',
    fromSurface: true
  });
  const bytes = Buffer.from(screenshot.data, 'base64');
  const file = join(screenshotDir, `${label}.png`);
  await writeFile(file, bytes);

  const image = decodePng(bytes);
  const sample = sampleRegion(image, rect);
  if (sample.uniqueColors < 40 || sample.nonDarkRatio < 0.02) {
    throw new Error(`${label} canvas region looks blank: ${JSON.stringify(sample)}`);
  }
}

async function clickButton(tab, text) {
  await evaluate(tab, `(() => {
    const button = Array.from(document.querySelectorAll('button')).find(item => item.textContent.trim() === ${JSON.stringify(text)});
    if (!button) throw new Error('Missing button: ${text}');
    button.click();
    return true;
  })()`);
}

async function clickSelector(tab, selector) {
  await evaluate(tab, `(() => {
    const element = document.querySelector(${JSON.stringify(selector)});
    if (!element) throw new Error('Missing element: ${selector}');
    element.click();
    return true;
  })()`);
}

async function clickSection(tab, title) {
  await evaluate(tab, `(() => {
    const header = Array.from(document.querySelectorAll('.section-header')).find(item => item.textContent.includes(${JSON.stringify(title)}));
    if (!header) throw new Error('Missing section: ${title}');
    const section = header.closest('.section');
    if (section?.classList.contains('collapsed')) header.click();
    return true;
  })()`);
}

async function setChecked(tab, selector, labelPredicateSource, checked) {
  const predicate = labelPredicateSource.toString();
  await evaluate(tab, `(() => {
    const predicate = ${predicate};
    const input = Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(item => {
      const label = item.closest('label')?.textContent ?? '';
      return predicate(label);
    });
    if (!input) throw new Error('Missing checkbox for predicate');
    if (input.checked !== ${checked}) input.click();
    return input.checked;
  })()`);
}

async function setNumberInput(tab, labelText, value) {
  await evaluate(tab, `(() => {
    const label = Array.from(document.querySelectorAll('label')).find(item => item.textContent.trim() === ${JSON.stringify(labelText)});
    if (!label) throw new Error('Missing number label: ${labelText}');
    const wrapper = label.closest('.number-input');
    const input = wrapper?.querySelector('input[type="number"]');
    if (!input) throw new Error('Missing number input: ${labelText}');
    input.value = ${JSON.stringify(String(value))};
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`);
}

async function setSelectValue(tab, labelText, value) {
  await evaluate(tab, `(() => {
    const label = Array.from(document.querySelectorAll('label')).find(item => item.textContent.trim() === ${JSON.stringify(labelText)});
    if (!label) throw new Error('Missing select label: ${labelText}');
    const wrapper = label.closest('.select-input') ?? label.parentElement;
    const select = wrapper?.querySelector('select');
    if (!select) throw new Error('Missing select input: ${labelText}');
    select.value = ${JSON.stringify(String(value))};
    select.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  })()`);
}

async function readPreviewStats(tab) {
  return evaluate(tab, `(() => { ${PREVIEW_STATS_HELPER}; return previewStats(); })()`);
}

async function waitFor(tab, expression, timeoutMs = 15000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const result = await evaluate(tab, `(() => { ${PREVIEW_STATS_HELPER}; return Boolean(${expression}); })()`);
    if (result) return;
    await delay(100);
  }

  const body = await evaluate(tab, 'document.body.innerText');
  throw new Error(`Timed out waiting for ${expression}\nConsole errors:\n${(tab.errors ?? []).join('\n') || '--'}\nVisible text:\n${body}`);
}

async function evaluate(tab, expression) {
  const response = await tab.send('Runtime.evaluate', {
    expression,
    awaitPromise: true,
    returnByValue: true
  });
  if (response.exceptionDetails) {
    const details = response.exceptionDetails;
    const message = details.exception?.description ?? details.exception?.value ?? details.text ?? 'Runtime.evaluate failed';
    throw new Error(message);
  }
  return response.result?.value;
}

class CdpClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();

    socket.addEventListener('message', event => {
      const message = JSON.parse(event.data);
      if (message.id && this.pending.has(message.id)) {
        const { resolve, reject, timeout } = this.pending.get(message.id);
        clearTimeout(timeout);
        this.pending.delete(message.id);
        if (message.error) reject(new Error(message.error.message));
        else resolve(message.result ?? {});
        return;
      }

      const listeners = this.listeners.get(message.method) ?? [];
      for (const listener of listeners) listener(message.params ?? {});
    });

    const rejectPending = reason => {
      for (const { reject, timeout } of this.pending.values()) {
        clearTimeout(timeout);
        reject(reason);
      }
      this.pending.clear();
    };
    socket.addEventListener('close', () => rejectPending(new Error('CDP socket closed')));
    socket.addEventListener('error', () => rejectPending(new Error('CDP socket error')));
  }

  static async connect(url) {
    const socket = new WebSocket(url);
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true });
      socket.addEventListener('error', reject, { once: true });
    });
    return new CdpClient(socket);
  }

  send(method, params = {}, timeoutMs = 15000) {
    if (this.socket.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`CDP socket is not open for ${method}`));
    }

    const id = this.nextId++;
    this.socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`CDP timeout: ${method}`));
        }
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timeout });
    });
  }

  on(method, listener) {
    const listeners = this.listeners.get(method) ?? [];
    listeners.push(listener);
    this.listeners.set(method, listeners);
  }

  close() {
    for (const { reject, timeout } of this.pending.values()) {
      clearTimeout(timeout);
      reject(new Error('CDP client closed'));
    }
    this.pending.clear();

    if (this.socket.readyState === WebSocket.CLOSED) return Promise.resolve();

    return new Promise(resolve => {
      const timeout = setTimeout(resolve, 500);
      this.socket.addEventListener('close', () => {
        clearTimeout(timeout);
        resolve();
      }, { once: true });

      try {
        this.socket.close();
      } catch {
        clearTimeout(timeout);
        resolve();
      }
    });
  }
}

function decodePng(bytes) {
  if (bytes.toString('ascii', 1, 4) !== 'PNG') {
    throw new Error('Screenshot is not PNG data');
  }

  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat = [];

  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('ascii', offset + 4, offset + 8);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    const data = bytes.subarray(dataStart, dataEnd);

    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      const bitDepth = data[8];
      colorType = data[9];
      if (bitDepth !== 8 || ![2, 6].includes(colorType)) {
        throw new Error(`Unsupported PNG format bitDepth=${bitDepth} colorType=${colorType}`);
      }
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }

    offset = dataEnd + 4;
  }

  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = zlib.inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * 4);
  let source = 0;
  let previous = Buffer.alloc(stride);

  for (let y = 0; y < height; y++) {
    const filter = inflated[source++];
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    unfilter(row, previous, channels, filter);

    for (let x = 0; x < width; x++) {
      const src = x * channels;
      const dst = (y * width + x) * 4;
      pixels[dst] = row[src];
      pixels[dst + 1] = row[src + 1];
      pixels[dst + 2] = row[src + 2];
      pixels[dst + 3] = channels === 4 ? row[src + 3] : 255;
    }

    previous = row;
  }

  return { width, height, pixels };
}

function unfilter(row, previous, channels, filter) {
  for (let i = 0; i < row.length; i++) {
    const left = i >= channels ? row[i - channels] : 0;
    const up = previous[i] ?? 0;
    const upLeft = i >= channels ? previous[i - channels] : 0;

    let value = row[i];
    if (filter === 1) value += left;
    else if (filter === 2) value += up;
    else if (filter === 3) value += Math.floor((left + up) / 2);
    else if (filter === 4) value += paeth(left, up, upLeft);
    else if (filter !== 0) throw new Error(`Unsupported PNG filter ${filter}`);
    row[i] = value & 0xff;
  }
}

function paeth(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

function sampleRegion(image, rect) {
  const x0 = clamp(Math.floor(rect.x), 0, image.width - 1);
  const y0 = clamp(Math.floor(rect.y), 0, image.height - 1);
  const x1 = clamp(Math.floor(rect.x + rect.width), x0 + 1, image.width);
  const y1 = clamp(Math.floor(rect.y + rect.height), y0 + 1, image.height);
  const colors = new Set();
  let nonDark = 0;
  let total = 0;

  for (let y = y0; y < y1; y += 3) {
    for (let x = x0; x < x1; x += 3) {
      const offset = (y * image.width + x) * 4;
      const r = image.pixels[offset];
      const g = image.pixels[offset + 1];
      const b = image.pixels[offset + 2];
      colors.add(`${r >> 3},${g >> 3},${b >> 3}`);
      if (r + g + b > 90) nonDark++;
      total++;
    }
  }

  return {
    uniqueColors: colors.size,
    nonDarkRatio: total === 0 ? 0 : nonDark / total
  };
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

async function assertServerReachable(url) {
  await new Promise((resolve, reject) => {
    const request = http.get(url, response => {
      response.resume();
      if (response.statusCode && response.statusCode < 500) resolve();
      else reject(new Error(`Server returned ${response.statusCode}`));
    });
    request.on('error', reject);
    request.setTimeout(5000, () => {
      request.destroy(new Error(`Timed out reaching ${url}`));
    });
  });
}

async function findChrome() {
  const { access } = await import('node:fs/promises');
  for (const candidate of CHROME_CANDIDATES) {
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try next candidate.
    }
  }
  throw new Error('No Chrome or Edge executable found. Set CHROME_BIN to run browser smoke tests.');
}

function findFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
    server.on('error', reject);
  });
}

async function waitForChrome(port) {
  const started = Date.now();
  while (Date.now() - started < 10000) {
    try {
      await requestJson('GET', port, '/json/version');
      return;
    } catch {
      await delay(100);
    }
  }
  throw new Error('Timed out waiting for Chrome DevTools endpoint.');
}

async function openTab(port, url) {
  const encoded = encodeURIComponent(url);
  try {
    return await requestJson('PUT', port, `/json/new?${encoded}`);
  } catch {
    return await requestJson('GET', port, `/json/new?${encoded}`);
  }
}

function requestJson(method, port, path) {
  return requestText(method, port, path).then(body => JSON.parse(body));
}

function requestText(method, port, path) {
  return new Promise((resolve, reject) => {
    const request = http.request({
      method,
      hostname: '127.0.0.1',
      port,
      path
    }, response => {
      let body = '';
      response.setEncoding('utf8');
      response.on('data', chunk => {
        body += chunk;
      });
      response.on('end', () => {
        if (!response.statusCode || response.statusCode >= 400) {
          reject(new Error(`${method} ${path} returned ${response.statusCode}: ${body}`));
          return;
        }
        resolve(body);
      });
    });
    request.on('error', reject);
    request.end();
  });
}

async function closeTab(tab, port, targetId) {
  if (targetId) {
    try {
      await requestText('GET', port, `/json/close/${encodeURIComponent(targetId)}`);
    } catch {
      // Chrome may close the DevTools target before replying.
    }
  }

  await tab.close();
}

function waitForEvent(client, method, timeoutMs) {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`Timed out waiting for ${method}`)), timeoutMs);
    client.on(method, event => {
      clearTimeout(timeout);
      resolve(event);
    });
  });
}

function delay(ms) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function terminateProcess(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;

  child.kill();
  if (await waitForProcessExit(child, 2000)) return;

  child.kill('SIGKILL');
  if (await waitForProcessExit(child, 2000)) return;

  child.unref();
}

function waitForProcessExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) {
    return Promise.resolve(true);
  }

  return new Promise(resolve => {
    const timeout = setTimeout(() => resolve(false), timeoutMs);
    child.once('exit', () => {
      clearTimeout(timeout);
      resolve(true);
    });
  });
}

async function removeWithRetry(path) {
  for (let attempt = 0; attempt < 5; attempt++) {
    try {
      await rm(path, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
      return;
    } catch (error) {
      if (attempt === 4) {
        console.warn(`Warning: could not remove temporary browser profile ${path}: ${error.message}`);
        return;
      }
      await delay(250);
    }
  }
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
