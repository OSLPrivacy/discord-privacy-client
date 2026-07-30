#!/usr/bin/env node

// Accessibility and 200%-zoom audit for the public crawl surface.
//
// The public-surface manifest decides which pages are public. This gate loads
// every manifest page in a real browser, at four widths and at 100%/200% zoom,
// then fails on concrete accessibility regressions: missing image alt text,
// unnamed controls, horizontal overflow, or a crawl that did not exercise real
// visible controls.

import { createServer as createHttpServer } from 'node:http';
import { createReadStream, existsSync, globSync, readFileSync, statSync } from 'node:fs';
import { createServer as createTcpServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const WIDTHS = [320, 390, 768, 1280];
const ZOOMS = [100, 200];
const MIN_PUBLIC_CLAIM_CHANNELS = 9;
const MIN_VISIBLE_CONTROLS = 8;

function routeFromHtmlPath(htmlPath) {
  if (htmlPath.endsWith('/index.html')) return `/${htmlPath.slice(0, -'index.html'.length)}`;
  if (htmlPath === 'index.html') return '/';
  return `/${htmlPath.slice(0, -'.html'.length)}`;
}

function loadManifestPages() {
  const manifest = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
  if (manifest.schema_version !== 1 || manifest.manifest_id !== 'osl-public-surface') {
    throw new Error('public surface manifest must use schema_version 1 and manifest_id osl-public-surface');
  }
  if (!Array.isArray(manifest.claim_channels) || manifest.claim_channels.length < MIN_PUBLIC_CLAIM_CHANNELS) {
    throw new Error(`public surface manifest must declare at least ${MIN_PUBLIC_CLAIM_CHANNELS} claim channels`);
  }
  if (!Array.isArray(manifest.html) || manifest.html.length === 0) {
    throw new Error('public surface manifest must declare public HTML pages');
  }

  const seen = new Set();
  return manifest.html.map((htmlPath) => {
    if (typeof htmlPath !== 'string' || !htmlPath.endsWith('.html')) {
      throw new Error(`invalid public HTML manifest entry: ${String(htmlPath)}`);
    }
    const file = path.join(REPO_ROOT, htmlPath);
    if (!existsSync(file)) throw new Error(`public HTML manifest entry is missing on disk: ${htmlPath}`);
    const route = routeFromHtmlPath(htmlPath);
    if (seen.has(route)) throw new Error(`multiple public HTML entries map to ${route}`);
    seen.add(route);
    return { htmlPath, route };
  });
}

function contentType(file) {
  const ext = path.extname(file).toLowerCase();
  if (ext === '.html') return 'text/html; charset=utf-8';
  if (ext === '.css') return 'text/css; charset=utf-8';
  if (ext === '.js' || ext === '.mjs' || ext === '.ts') return 'text/javascript; charset=utf-8';
  if (ext === '.svg') return 'image/svg+xml';
  if (ext === '.png') return 'image/png';
  if (ext === '.jpg' || ext === '.jpeg') return 'image/jpeg';
  if (ext === '.webp') return 'image/webp';
  return 'application/octet-stream';
}

function fileForRequest(urlPath) {
  const decoded = decodeURIComponent(urlPath.split('?')[0]);
  const relative = decoded === '/' ? 'index.html' : decoded.replace(/^\/+/, '');
  const candidates = [
    path.join(REPO_ROOT, relative),
    path.join(REPO_ROOT, `${relative}.html`),
    path.join(REPO_ROOT, relative, 'index.html'),
  ];
  for (const candidate of candidates) {
    const normalized = path.normalize(candidate);
    if (!normalized.startsWith(REPO_ROOT + path.sep)) continue;
    if (existsSync(normalized) && statSync(normalized).isFile()) return normalized;
  }
  return null;
}

function startStaticServer(port) {
  const server = createHttpServer((request, response) => {
    const file = fileForRequest(request.url || '/');
    if (!file) {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end('not found');
      return;
    }
    response.writeHead(200, {
      'content-type': contentType(file),
      'cache-control': 'no-store',
    });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', () => resolve(server));
  });
}

function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = createTcpServer();
    server.unref();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function locateChrome() {
  const envPath = process.env.OSL_CHROME;
  if (envPath && existsSync(envPath)) return envPath;
  const home = os.homedir();
  const patterns = [
    `${home}/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell`,
    `${home}/.cache/ms-playwright/chromium_headless_shell-*/chrome-linux/headless_shell`,
    `${home}/.cache/ms-playwright/chromium-*/chrome-linux64/chrome`,
    `${home}/.cache/ms-playwright/chromium-*/chrome-linux/chrome`,
  ];
  for (const pattern of patterns) {
    const matches = globSync(pattern).sort();
    if (matches.length > 0) return matches[matches.length - 1];
  }
  throw new Error('no Chrome/Chromium binary found; set OSL_CHROME to a local browser binary');
}

class CDPClient {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    ws.addEventListener('message', (event) => this.onMessage(event));
  }

  onMessage(event) {
    const message = JSON.parse(event.data);
    if (message.id !== undefined) {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
      return;
    }
    if (!message.method) return;
    const listeners = this.listeners.get(message.method);
    if (listeners) for (const listener of [...listeners]) listener(message.params, message.sessionId);
  }

  send(method, params = {}, sessionId) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const payload = { id, method, params };
      if (sessionId) payload.sessionId = sessionId;
      this.ws.send(JSON.stringify(payload));
    });
  }

  on(method, listener) {
    if (!this.listeners.has(method)) this.listeners.set(method, new Set());
    this.listeners.get(method).add(listener);
    return () => this.listeners.get(method)?.delete(listener);
  }

  once(method, predicate = () => true) {
    return new Promise((resolve) => {
      const off = this.on(method, (params, sessionId) => {
        if (!predicate(params, sessionId)) return;
        off();
        resolve(params);
      });
    });
  }
}

// Runs inside the audited page.
function auditPage() {
  function visible(el) {
    const style = getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') return false;
    if (Number.parseFloat(style.opacity) === 0) return false;
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  }

  function accessibleName(el) {
    const aria = (el.getAttribute('aria-label') || '').trim();
    if (aria) return aria;
    const labelledBy = el.getAttribute('aria-labelledby');
    if (labelledBy) {
      const text = labelledBy.split(/\s+/)
        .map((id) => document.getElementById(id)?.textContent?.trim() || '')
        .filter(Boolean)
        .join(' ');
      if (text) return text;
    }
    const title = (el.getAttribute('title') || '').trim();
    if (title) return title;
    const text = (el.textContent || '').replace(/\s+/g, ' ').trim();
    if (text) return text;
    const image = el.querySelector('img[alt]');
    if (image && image.getAttribute('alt').trim()) return image.getAttribute('alt').trim();
    const svgTitle = el.querySelector('svg > title');
    if (svgTitle && svgTitle.textContent.trim()) return svgTitle.textContent.trim();
    return '';
  }

  function describe(el) {
    const id = el.id ? `#${el.id}` : '';
    const cls = typeof el.className === 'string' && el.className.trim()
      ? `.${el.className.trim().split(/\s+/).slice(0, 2).join('.')}`
      : '';
    return `${el.tagName.toLowerCase()}${id}${cls}`.slice(0, 90);
  }

  const imagesMissingAlt = [];
  for (const image of document.querySelectorAll('img')) {
    if (visible(image) && image.getAttribute('alt') === null) imagesMissingAlt.push(describe(image));
  }

  const controlsMissingName = [];
  let visibleControlCount = 0;
  const controls = document.querySelectorAll('a[href], button, input, select, textarea, [role="button"], [role="link"]');
  for (const control of controls) {
    if (!visible(control)) continue;
    if (control.getAttribute('aria-hidden') === 'true') continue;
    if (control.tabIndex < 0) continue;
    if (control.tagName === 'INPUT' && control.type === 'hidden') continue;
    visibleControlCount += 1;
    if (control.tagName === 'INPUT') {
      const id = control.getAttribute('id');
      if ((id && document.querySelector(`label[for="${CSS.escape(id)}"]`)) || control.closest('label')) continue;
    }
    if (!accessibleName(control)) controlsMissingName.push(describe(control));
  }

  const doc = document.documentElement;
  const horizontalOverflow = Math.max(0, doc.scrollWidth - doc.clientWidth);
  const overflowingElements = [];
  if (horizontalOverflow > 1) {
    for (const el of document.querySelectorAll('body *')) {
      if (!visible(el)) continue;
      const rect = el.getBoundingClientRect();
      if (rect.right <= doc.clientWidth + 1 || rect.width <= 0) continue;
      const style = getComputedStyle(el);
      if (style.overflowX === 'auto' || style.overflowX === 'scroll') continue;
      overflowingElements.push(`${describe(el)} right=${Math.round(rect.right)}`);
    }
  }

  const structuralFindings = [];
  if (!document.title.trim()) structuralFindings.push('missing document title');
  const viewport = document.querySelector('meta[name="viewport"]')?.getAttribute('content') || '';
  if (!/width\s*=\s*device-width/i.test(viewport)) structuralFindings.push('viewport is not device-width');
  const bodyText = (document.body?.innerText || '').replace(/\s+/g, ' ').trim();
  if (bodyText.length < 20) structuralFindings.push('page rendered too little visible text');

  return {
    imagesMissingAlt,
    controlsMissingName,
    horizontalOverflow,
    overflowingElements: overflowingElements.slice(0, 8),
    structuralFindings,
    visibleControlCount,
    visibleTextLength: bodyText.length,
  };
}

async function connectToChrome(chromeChild) {
  let buffer = '';
  const wsUrl = await new Promise((resolve, reject) => {
    chromeChild.stderr.on('data', (chunk) => {
      buffer += chunk.toString('utf8');
      const match = buffer.match(/DevTools listening on (ws:\/\/\S+)/);
      if (match) resolve(match[1]);
    });
    chromeChild.once('exit', (code) => reject(new Error(`Chrome exited before CDP was ready (${code})`)));
    delay(15000).then(() => reject(new Error('timed out waiting for Chrome CDP')));
  });
  const cdpPort = new URL(wsUrl).port;
  const versionInfo = await fetch(`http://127.0.0.1:${cdpPort}/json/version`).then((response) => response.json());
  const ws = new WebSocket(versionInfo.webSocketDebuggerUrl || wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', () => resolve());
    ws.addEventListener('error', () => reject(new Error('WebSocket connection failed')));
  });
  return { cdp: new CDPClient(ws), ws, versionInfo };
}

async function run() {
  const pages = loadManifestPages();
  const expectedCombinations = pages.length * WIDTHS.length * ZOOMS.length;
  const port = await getFreePort();
  const server = await startStaticServer(port);
  const chromeChild = spawn(locateChrome(), [
    '--headless=new',
    '--remote-debugging-port=0',
    '--no-sandbox',
    '--disable-gpu',
    '--force-device-scale-factor=1',
    'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });

  let ws;
  try {
    const connection = await connectToChrome(chromeChild);
    const cdp = connection.cdp;
    ws = connection.ws;
    const results = [];
    let totalVisibleControls = 0;

    for (const page of pages) {
      const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
      const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
      try {
        await cdp.send('Page.enable', {}, sessionId);
        await cdp.send('Runtime.enable', {}, sessionId);
        for (const width of WIDTHS) {
          for (const zoom of ZOOMS) {
            const layoutWidth = zoom === 200 ? Math.round(width / 2) : width;
            await cdp.send('Emulation.setDeviceMetricsOverride', {
              width: layoutWidth,
              height: 900,
              deviceScaleFactor: 1,
              mobile: false,
            }, sessionId);
            const loaded = cdp.once('Page.loadEventFired', (_params, sid) => sid === sessionId);
            await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}${page.route}` }, sessionId);
            await loaded;
            await delay(300);
            const evaluated = await cdp.send('Runtime.evaluate', {
              expression: `(${auditPage.toString()})()`,
              returnByValue: true,
            }, sessionId);
            if (evaluated.exceptionDetails) {
              throw new Error(evaluated.exceptionDetails.text || 'page audit threw');
            }
            const audit = evaluated.result.value;
            totalVisibleControls += audit.visibleControlCount;
            results.push({ page: page.route, width, zoom, layout_width: layoutWidth, ...audit });
          }
        }
      } finally {
        await cdp.send('Target.closeTarget', { targetId }).catch(() => {});
      }
    }

    const sum = (key) => results.reduce((total, result) => total + result[key].length, 0);
    const overflowRows = results.filter((result) => result.horizontalOverflow > 1);
    const unique = (key) => [...new Set(results.flatMap((result) => result[key]))];

    console.log('\ncheck-a11y summary');
    console.log(`  pages x widths x zoom : ${results.length} combinations`);
    console.log(`  images missing alt    : ${sum('imagesMissingAlt')} (${unique('imagesMissingAlt').length} distinct)`);
    console.log(`  controls without name : ${sum('controlsMissingName')} (${unique('controlsMissingName').length} distinct)`);
    console.log(`  horizontal overflow   : ${overflowRows.length} combinations`);
    console.log(`  structural findings   : ${sum('structuralFindings')} (${unique('structuralFindings').length} distinct)`);
    console.log(`  visible controls      : ${totalVisibleControls}`);

    for (const name of unique('controlsMissingName')) console.log(`    [name] ${name}`);
    for (const image of unique('imagesMissingAlt')) console.log(`    [alt] ${image}`);
    for (const row of overflowRows.slice(0, 12)) {
      console.log(`    [overflow] ${row.page} ${row.width}px @${row.zoom}% by ${row.horizontalOverflow}px :: ${row.overflowingElements.join(' | ')}`);
    }
    for (const finding of unique('structuralFindings')) console.log(`    [structure] ${finding}`);

    const blockingFindings = sum('imagesMissingAlt')
      + sum('controlsMissingName')
      + overflowRows.length
      + sum('structuralFindings');
    const floorFailed = results.length !== expectedCombinations || totalVisibleControls < MIN_VISIBLE_CONTROLS;
    if (results.length !== expectedCombinations) {
      console.error(`check-a11y floor: expected ${expectedCombinations} combinations, audited ${results.length}`);
    }
    if (totalVisibleControls < MIN_VISIBLE_CONTROLS) {
      console.error(`check-a11y floor: expected at least ${MIN_VISIBLE_CONTROLS} visible controls, found ${totalVisibleControls}`);
    }
    console.log(`\ncheck-a11y: ${results.length} combinations, ${blockingFindings} blocking findings.`);
    process.exit(blockingFindings > 0 || floorFailed ? 1 : 0);
  } finally {
    if (ws) {
      try { ws.close(); } catch { /* already closed */ }
    }
    chromeChild.kill('SIGTERM');
    server.close();
  }
}

run().catch((error) => {
  console.error(`check-a11y: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
