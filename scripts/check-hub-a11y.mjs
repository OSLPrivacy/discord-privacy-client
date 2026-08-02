#!/usr/bin/env node

// Route-wide accessibility audit for the shipping Hub UI.  The public-site
// checker owns the actual DOM assertions; this gate supplies every Hub route
// and every onboarding step as real renderer output under the built CSS/CSP.

import assert from 'node:assert/strict';
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { auditPage } from './check-a11y.mjs';
import { launchChrome } from './lib/cdp-harness.mjs';
import { shippedHubCspHeaders } from './lib/csp-mirror.mjs';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const UI_ROOT = path.join(REPO_ROOT, 'apps', 'osl-hub-ui');
const DIST_DIR = path.join(UI_ROOT, 'dist');
const WIDTHS = [320, 390, 768, 1280];
const ZOOMS = [100, 200];
const DESTINATIONS = ['home', 'inbox', 'people', 'privacy', 'activity', 'connections', 'settings'];
const ONBOARDING_STEPS = [
  'pro', 'welcome', 'create', 'import', 'unlock', 'account-recovery', 'recovery',
  'mullvad', 'sending', 'defaults', 'cover', 'passwords', 'burnpass', 'privacy',
  'tutorial', 'detected', 'install', 'apps', 'browser', 'decoy',
];

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
  const relative = decoded.replace(/^\/+/, '');
  const candidate = path.resolve(DIST_DIR, relative);
  return candidate.startsWith(`${DIST_DIR}${path.sep}`) && existsSync(candidate) && statSync(candidate).isFile()
    ? candidate
    : null;
}

function builtMainStylesheet() {
  if (!existsSync(path.join(DIST_DIR, 'index.html'))) {
    throw new Error('missing apps/osl-hub-ui/dist/index.html; run npm run build in apps/osl-hub-ui first');
  }
  const stylesheets = readdirSync(path.join(DIST_DIR, 'assets')).filter((name) => /^main-.*\.css$/.test(name));
  assert.equal(stylesheets.length, 1, 'built dist must contain exactly one main stylesheet');
  return `/assets/${stylesheets[0]}`;
}

async function routeMarkup() {
  const requireFromUi = createRequire(path.join(UI_ROOT, 'package.json'));
  const { createServer: createViteServer } = requireFromUi('vite');
  const previousVitest = process.env.VITEST;
  const previousStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  const values = new Map();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: (key) => values.get(key) ?? null, setItem: (key, value) => values.set(key, String(value)), removeItem: (key) => values.delete(key), clear: () => values.clear() },
  });
  process.env.VITEST = 'hub-a11y-gate';
  const vite = await createViteServer({
    root: UI_ROOT,
    configFile: false,
    appType: 'custom',
    logLevel: 'error',
    // SSR loading is one-shot. Avoid a repository watcher just to render the
    // route fixtures; the shared CI host may already have many Vite lanes.
    server: { middlewareMode: true, watch: { ignored: ['**/*'] } },
  });
  try {
    const { __oslHubUiTest } = await vite.ssrLoadModule('/src/main.ts');
    const pages = [];
    for (const destination of DESTINATIONS) {
      __oslHubUiTest.reset({ route: destination });
      pages.push({ name: destination, markup: __oslHubUiTest.renderRouteShell(destination) });
    }
    for (const step of ONBOARDING_STEPS) {
      __oslHubUiTest.reset({ route: 'onboarding', onboardingRoute: step });
      pages.push({ name: `onboarding/${step}`, markup: __oslHubUiTest.renderRouteShell('onboarding') });
    }
    return pages;
  } finally {
    await vite.close();
    if (previousVitest === undefined) delete process.env.VITEST;
    else process.env.VITEST = previousVitest;
    if (previousStorage) Object.defineProperty(globalThis, 'localStorage', previousStorage);
    else delete globalThis.localStorage;
  }
}

function startServer(pages) {
  const stylesheet = builtMainStylesheet();
  const cspHeaders = shippedHubCspHeaders();
  const markupByRoute = new Map(pages.map((page) => [`/${page.name}`, page.markup]));
  const server = createServer((request, response) => {
    const requestPath = (request.url || '/').split('?')[0];
    const markup = markupByRoute.get(requestPath);
    if (markup !== undefined) {
      response.writeHead(200, { ...cspHeaders, 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      response.end(`<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>OSL Privacy</title><link rel="stylesheet" href="${stylesheet}"></head><body>${markup}</body></html>`);
      return;
    }
    const file = fileForRequest(requestPath);
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

function assertRouteAudit(name, audit) {
  assert.equal(audit.imagesMissingAlt.length, 0, `${name}: images missing alt text`);
  assert.equal(audit.controlsMissingName.length, 0, `${name}: controls without accessible names`);
  assert.ok(audit.horizontalOverflow <= 1, `${name}: horizontal overflow of ${audit.horizontalOverflow}px`);
  assert.equal(audit.structuralFindings.length, 0, `${name}: ${audit.structuralFindings.join(', ')}`);
  const headings = audit.routeHeadings || [];
  assert.equal(headings.length, 1, `${name}: expected exactly one #route-heading`);
  assert.ok(headings[0].name, `${name}: route heading must have an accessible name`);
}

async function auditRoute(page, url, name) {
  await page.navigate(url);
  return assertRouteAudit(name, await pageAudit(page));
}

async function pageAudit(page) {
  const audit = await page.evaluate(`(() => {
    const result = (${auditPage.toString()})()
    const headings = [...document.querySelectorAll('#route-heading')].map((heading) => ({
      name: (heading.getAttribute('aria-label') || heading.textContent || '').replace(/\\s+/g, ' ').trim(),
    }));
    return { ...result, routeHeadings: headings };
  })()`);
  return audit;
}

async function run({ selfTest = false } = {}) {
  const pages = await routeMarkup();
  assert.equal(pages.length, DESTINATIONS.length + ONBOARDING_STEPS.length, 'route coverage must include every destination and onboarding step');
  const server = await startServer(pages);
  const chrome = await launchChrome();
  const { port } = server.address();
  try {
    const page = await chrome.openPage();
    try {
      if (selfTest) {
        await page.navigate(`http://127.0.0.1:${port}/home`);
        await page.evaluate(`document.body.innerHTML = '<main aria-labelledby="route-heading"><h1 id="route-heading">Fixture accessibility route</h1><p>This fixture has enough visible text for the shared audit.</p><button type="button">Continue</button></main>'`);
        assertRouteAudit('named-control self-test', await pageAudit(page));
        await page.evaluate(`document.querySelector('button')?.replaceChildren()`);
        await assert.rejects(
          async () => assertRouteAudit('unnamed-control self-test', await pageAudit(page)),
          /controls without accessible names/,
        );
        await page.evaluate(`document.querySelector('button')?.replaceChildren('Continue')`);
        assertRouteAudit('restored-control self-test', await pageAudit(page));
        console.log('check-hub-a11y: self-test confirmed an unnamed control is rejected and its restored name passes');
      }
      for (const route of pages) {
        for (const width of WIDTHS) {
          for (const zoom of ZOOMS) {
            await page.send('Emulation.setDeviceMetricsOverride', { width: zoom === 200 ? Math.round(width / 2) : width, height: 900, deviceScaleFactor: 1, mobile: false });
            await auditRoute(page, `http://127.0.0.1:${port}/${route.name}`, `${route.name} ${width}px @${zoom}%`);
          }
        }
      }
      console.log(`check-hub-a11y: ${pages.length} routes x ${WIDTHS.length} widths x ${ZOOMS.length} zooms passed`);
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
  console.error('Usage: node scripts/check-hub-a11y.mjs [--self-test]');
  process.exit(2);
}

run({ selfTest }).catch((error) => {
  console.error(`check-hub-a11y: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
