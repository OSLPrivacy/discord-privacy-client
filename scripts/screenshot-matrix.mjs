#!/usr/bin/env node

// Responsive screenshot matrix for the public crawl surface.
//
// Captures every public HTML page with JavaScript on, JavaScript off and
// reduced-motion media enabled. The script fails if any manifest page, viewport
// width, browser mode, PNG dimensions or capture file is missing.

import { createServer as createHttpServer } from 'node:http';
import { createReadStream, existsSync, globSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createServer as createTcpServer } from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const MANIFEST_PATH = path.join(REPO_ROOT, 'data', 'public-surface-manifest.json');
const EVIDENCE_DIR = path.join(REPO_ROOT, 'docs', 'evidence', 'website-matrix');
const SCREENSHOT_DIR = path.join(EVIDENCE_DIR, 'screenshots');
const MATRIX_PATH = path.join(EVIDENCE_DIR, 'matrix.json');
const WIDTHS = [320, 390, 768, 1280];
const VIEWPORT_HEIGHT = 900;
const MODES = [
  { id: 'js-on', javascript: true, reducedMotion: false },
  { id: 'js-off', javascript: false, reducedMotion: false },
  { id: 'reduced-motion', javascript: true, reducedMotion: true },
];

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

function slug(value) {
  return value.replace(/^\/+|\/+$/g, '').replace(/[^a-z0-9]+/gi, '-').replace(/^-|-$/g, '') || 'root';
}

function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

function pngFacts(buffer) {
  const signature = '89504e470d0a1a0a';
  if (buffer.subarray(0, 8).toString('hex') !== signature) throw new Error('PNG signature is missing');
  if (buffer.subarray(12, 16).toString('ascii') !== 'IHDR') throw new Error('PNG IHDR is missing');
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
    bytes: buffer.length,
    sha256: sha256(buffer),
  };
}

function pageMetricsExpression() {
  return `(() => {
    const text = (document.body?.innerText || '').replace(/\\s+/g, ' ').trim();
    return {
      title: document.title,
      visibleTextLength: text.length,
      bodyChildCount: document.body?.children.length || 0,
      scrollWidth: document.documentElement.scrollWidth,
      scrollHeight: document.documentElement.scrollHeight
    };
  })()`;
}

async function captureCombo({ cdp, port, page, width, mode }) {
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  try {
    await cdp.send('Page.enable', {}, sessionId);
    await cdp.send('Runtime.enable', {}, sessionId);
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width,
      height: VIEWPORT_HEIGHT,
      deviceScaleFactor: 1,
      mobile: width < 700,
    }, sessionId);
    await cdp.send('Emulation.setScriptExecutionDisabled', { value: !mode.javascript }, sessionId);
    await cdp.send('Emulation.setEmulatedMedia', {
      features: mode.reducedMotion
        ? [{ name: 'prefers-reduced-motion', value: 'reduce' }]
        : [{ name: 'prefers-reduced-motion', value: 'no-preference' }],
    }, sessionId);

    const loaded = cdp.once('Page.loadEventFired', (_params, sid) => sid === sessionId);
    await cdp.send('Page.navigate', { url: `http://127.0.0.1:${port}${page.route}` }, sessionId);
    await loaded;
    await delay(mode.javascript ? 650 : 250);

    const metricsResult = await cdp.send('Runtime.evaluate', {
      expression: pageMetricsExpression(),
      returnByValue: true,
    }, sessionId);
    if (metricsResult.exceptionDetails) {
      throw new Error(metricsResult.exceptionDetails.text || 'page metrics threw');
    }
    const pageMetrics = metricsResult.result.value;
    if (!pageMetrics.title.trim()) throw new Error(`${page.route} ${mode.id} ${width}px is missing a document title`);
    if (pageMetrics.bodyChildCount < 1) throw new Error(`${page.route} ${mode.id} ${width}px rendered no body children`);
    if (mode.javascript && pageMetrics.visibleTextLength < 20) {
      throw new Error(`${page.route} ${mode.id} ${width}px rendered too little visible text`);
    }

    const layoutMetrics = await cdp.send('Page.getLayoutMetrics', {}, sessionId);
    const contentSize = layoutMetrics.cssContentSize || { width, height: VIEWPORT_HEIGHT };
    const clipWidth = Math.max(width, Math.ceil(contentSize.width));
    const clipHeight = Math.max(VIEWPORT_HEIGHT, Math.min(5000, Math.ceil(contentSize.height)));
    const screenshot = await cdp.send('Page.captureScreenshot', {
      format: 'png',
      captureBeyondViewport: true,
      fromSurface: true,
      clip: { x: 0, y: 0, width: clipWidth, height: clipHeight, scale: 1 },
    }, sessionId);
    const bytes = Buffer.from(screenshot.data, 'base64');
    const facts = pngFacts(bytes);
    if (facts.width < width || facts.height < VIEWPORT_HEIGHT) {
      throw new Error(`${page.route} ${mode.id} ${width}px captured ${facts.width}x${facts.height}`);
    }
    if (facts.bytes < 500) throw new Error(`${page.route} ${mode.id} ${width}px screenshot is implausibly small`);

    const relativePath = path.join(
      'docs',
      'evidence',
      'website-matrix',
      'screenshots',
      `${slug(page.route)}-${mode.id}-${width}.png`,
    );
    writeFileSync(path.join(REPO_ROOT, relativePath), bytes);
    return {
      page: page.route,
      html: page.htmlPath,
      mode: mode.id,
      javascript: mode.javascript,
      reduced_motion: mode.reducedMotion,
      viewport_width: width,
      viewport_height: VIEWPORT_HEIGHT,
      screenshot: relativePath.replaceAll(path.sep, '/'),
      screenshot_sha256: facts.sha256,
      screenshot_width: facts.width,
      screenshot_height: facts.height,
      screenshot_bytes: facts.bytes,
      visible_text_length: pageMetrics.visibleTextLength,
      scroll_width: pageMetrics.scrollWidth,
      scroll_height: pageMetrics.scrollHeight,
    };
  } finally {
    await cdp.send('Target.closeTarget', { targetId }).catch(() => {});
  }
}

function validateCaptures({ pages, captures }) {
  const expected = new Set();
  for (const page of pages) {
    for (const mode of MODES) {
      for (const width of WIDTHS) expected.add(`${page.route}|${mode.id}|${width}`);
    }
  }
  const seen = new Set();
  for (const capture of captures) {
    const key = `${capture.page}|${capture.mode}|${capture.viewport_width}`;
    if (!expected.has(key)) throw new Error(`unexpected screenshot matrix cell ${key}`);
    if (seen.has(key)) throw new Error(`duplicate screenshot matrix cell ${key}`);
    seen.add(key);
    const file = path.join(REPO_ROOT, capture.screenshot);
    if (!existsSync(file)) throw new Error(`capture file is missing: ${capture.screenshot}`);
    const facts = pngFacts(readFileSync(file));
    if (facts.sha256 !== capture.screenshot_sha256) throw new Error(`capture hash mismatch: ${capture.screenshot}`);
    if (facts.width !== capture.screenshot_width || facts.height !== capture.screenshot_height) {
      throw new Error(`capture dimensions mismatch: ${capture.screenshot}`);
    }
  }
  const missing = [...expected].filter((key) => !seen.has(key));
  if (missing.length > 0) throw new Error(`screenshot matrix missing ${missing.length} cells: ${missing.slice(0, 5).join(', ')}`);
}

async function run() {
  const pages = loadManifestPages();
  mkdirSync(SCREENSHOT_DIR, { recursive: true });
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
    const captures = [];
    for (const page of pages) {
      for (const mode of MODES) {
        for (const width of WIDTHS) {
          captures.push(await captureCombo({ cdp, port, page, width, mode }));
        }
      }
    }
    validateCaptures({ pages, captures });
    const matrix = {
      schema_version: 1,
      matrix_id: 'osl-public-website-responsive-screenshots',
      source_manifest: 'data/public-surface-manifest.json',
      modes: MODES.map((mode) => mode.id),
      widths: WIDTHS,
      captures,
    };
    writeFileSync(MATRIX_PATH, `${JSON.stringify(matrix, null, 2)}\n`);

    console.log('\nscreenshot-matrix summary');
    console.log(`  pages       : ${pages.length}`);
    console.log(`  modes       : ${MODES.map((mode) => mode.id).join(', ')}`);
    console.log(`  widths      : ${WIDTHS.join(', ')}`);
    console.log(`  captures    : ${captures.length}`);
    console.log(`  matrix      : ${path.relative(REPO_ROOT, MATRIX_PATH)}`);
    console.log('\nscreenshot-matrix: complete.');
  } finally {
    if (ws) {
      try { ws.close(); } catch { /* already closed */ }
    }
    chromeChild.kill('SIGTERM');
    server.close();
  }
}

run().catch((error) => {
  console.error(`screenshot-matrix: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
