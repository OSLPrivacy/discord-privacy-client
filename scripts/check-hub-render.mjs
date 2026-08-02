#!/usr/bin/env node

// Render the production Hub bundle in Chromium. This is deliberately a
// computed-style gate: a selector existing in a source stylesheet says nothing
// about whether the generated dist/ serves it or whether it matches the real
// markup returned by the application.

import assert from 'node:assert/strict';
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { launchChrome } from './lib/cdp-harness.mjs';
import { shippedHubCspHeaders } from './lib/csp-mirror.mjs';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const UI_ROOT = path.join(REPO_ROOT, 'apps', 'osl-hub-ui');
const DIST_DIR = path.join(REPO_ROOT, 'apps', 'osl-hub-ui', 'dist');
const SIDEBAR_SELECTOR = '.primary-sidebar';
const EXPECTED_SIDEBAR_WIDTH = 232;

function contentType(file) {
  const extension = path.extname(file).toLowerCase();
  if (extension === '.html') return 'text/html; charset=utf-8';
  if (extension === '.css') return 'text/css; charset=utf-8';
  if (extension === '.js') return 'text/javascript; charset=utf-8';
  if (extension === '.svg') return 'image/svg+xml';
  if (extension === '.png') return 'image/png';
  if (extension === '.woff2') return 'font/woff2';
  return 'application/octet-stream';
}

function fileForRequest(urlPath) {
  const decoded = decodeURIComponent(urlPath.split('?')[0]);
  const relative = decoded === '/' ? 'index.html' : decoded.replace(/^\/+/, '');
  const candidate = path.resolve(DIST_DIR, relative);
  return candidate.startsWith(`${DIST_DIR}${path.sep}`) && existsSync(candidate) && statSync(candidate).isFile()
    ? candidate
    : null;
}

function builtMainStylesheet() {
  if (!existsSync(path.join(DIST_DIR, 'index.html'))) {
    throw new Error('missing apps/osl-hub-ui/dist/index.html; run npm run build in apps/osl-hub-ui first');
  }
  const stylesheets = readdirSync(path.join(DIST_DIR, 'assets'))
    .filter((name) => /^main-.*\.css$/.test(name));
  assert.equal(stylesheets.length, 1, 'built dist must contain exactly one main stylesheet');
  return `/assets/${stylesheets[0]}`;
}

async function realSidebarMarkup() {
  // Load the same exported renderer the UI uses, but prevent its normal native
  // bootstrap. The browser below receives only markup it returns and CSS from
  // dist/, so this cannot accidentally pass on Vite's development stylesheet.
  const requireFromUi = createRequire(path.join(UI_ROOT, 'package.json'));
  const { createServer: createViteServer } = requireFromUi('vite');
  const previousVitest = process.env.VITEST;
  const previousStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  const values = new Map();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (key) => values.get(key) ?? null,
      setItem: (key, value) => values.set(key, String(value)),
      removeItem: (key) => values.delete(key),
      clear: () => values.clear(),
    },
  });
  process.env.VITEST = 'render-gate';
  const vite = await createViteServer({
    root: UI_ROOT,
    configFile: false,
    appType: 'custom',
    logLevel: 'error',
    server: { middlewareMode: true },
  });
  try {
    const ui = await vite.ssrLoadModule('/src/main.ts');
    assert.equal(typeof ui.primarySidebarMarkup, 'function', 'Hub must export primarySidebarMarkup');
    return ui.primarySidebarMarkup();
  } finally {
    await vite.close();
    if (previousVitest === undefined) delete process.env.VITEST;
    else process.env.VITEST = previousVitest;
    if (previousStorage) Object.defineProperty(globalThis, 'localStorage', previousStorage);
    else delete globalThis.localStorage;
  }
}

function startDistServer(sidebarMarkup) {
  const stylesheet = builtMainStylesheet();
  const cspHeaders = shippedHubCspHeaders();
  const server = createServer((request, response) => {
    if ((request.url || '').startsWith('/__render-gate-sidebar')) {
      response.writeHead(200, { ...cspHeaders, 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      response.end(`<!doctype html><link rel="stylesheet" href="${stylesheet}"><main>${sidebarMarkup}</main>`);
      return;
    }
    if ((request.url || '').startsWith('/__render-gate-unstyled')) {
      response.writeHead(200, { ...cspHeaders, 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      response.end('<!doctype html><main><aside class="primary-sidebar">unstyled fixture</aside></main>');
      return;
    }
    const file = fileForRequest(request.url || '/');
    if (!file) {
      response.writeHead(404, { ...cspHeaders, 'content-type': 'text/plain; charset=utf-8' });
      response.end('not found');
      return;
    }
    response.writeHead(200, { ...cspHeaders, 'content-type': contentType(file), 'cache-control': 'no-store' });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function sidebarMetrics(page) {
  return page.evaluate(`(() => {
    const sidebar = document.querySelector(${JSON.stringify(SIDEBAR_SELECTOR)});
    if (!sidebar) return null;
    const style = getComputedStyle(sidebar);
    const rect = sidebar.getBoundingClientRect();
    return { computedWidth: style.width, rectWidth: rect.width, rectHeight: rect.height };
  })()`);
}

function assertSidebarMetrics(metrics) {
  assert.ok(metrics, `rendered app did not mount ${SIDEBAR_SELECTOR}`);
  assert.equal(metrics.computedWidth, `${EXPECTED_SIDEBAR_WIDTH}px`, 'sidebar computed width');
  assert.equal(metrics.rectWidth, EXPECTED_SIDEBAR_WIDTH, 'sidebar rendered width');
  assert.ok(metrics.rectHeight > 0, 'sidebar must have a visible rendered height');
}

async function waitForSidebar(page) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const metrics = await sidebarMetrics(page);
    if (metrics) return metrics;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return null;
}

async function run({ selfTest = false } = {}) {
  const sidebarMarkup = await realSidebarMarkup();
  const server = await startDistServer(sidebarMarkup);
  const chrome = await launchChrome();
  const { port } = server.address();
  try {
    const page = await chrome.openPage();
    try {
      await page.navigate(`http://127.0.0.1:${port}/__render-gate-sidebar`);
      const metrics = await waitForSidebar(page);
      assertSidebarMetrics(metrics);
      console.log(`check-hub-render: sidebar ${metrics.computedWidth}, rendered ${metrics.rectWidth}px`);

      if (selfTest) {
        await page.navigate(`http://127.0.0.1:${port}/__render-gate-unstyled`);
        await assert.rejects(
          async () => assertSidebarMetrics(await sidebarMetrics(page)),
          /sidebar computed width/,
          'an unstyled route must fail this computed-style gate',
        );
        console.log('check-hub-render: self-test confirmed an unstyled route is rejected');
      }
    } finally {
      await page.close();
    }
  } finally {
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

const selfTest = process.argv.slice(2).join(' ') === '--self-test';
if (process.argv.length > 2 && !selfTest) {
  console.error('Usage: node scripts/check-hub-render.mjs [--self-test]');
  process.exit(2);
}

run({ selfTest }).catch((error) => {
  console.error(`check-hub-render: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
