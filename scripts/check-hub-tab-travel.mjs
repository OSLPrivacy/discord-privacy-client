#!/usr/bin/env node

import assert from 'node:assert/strict';
import { createReadStream, existsSync, readdirSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import path from 'node:path';

import { launchChrome } from './lib/cdp-harness.mjs';
import { hubScreenshotSurfaceMarkup, hubTabTravelSurfaceMarkup, UI_ROOT } from './lib/hub-surface-fixtures.mjs';

const DIST_DIR = path.join(UI_ROOT, 'dist');

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

function startServer(surfaces) {
  const stylesheet = builtMainStylesheet();
  const markupByRoute = new Map(surfaces.map((surface) => [`/${encodeURIComponent(surface.name)}`, surface]));
  const server = createServer((request, response) => {
    const requestPath = (request.url || '/').split('?')[0];
    const surface = markupByRoute.get(requestPath);
    if (surface) {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
      response.end(`<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>${surface.name}</title><link rel="stylesheet" href="${stylesheet}"></head><body><button id="tab-start-sentinel" data-tab-sentinel tabindex="0" aria-label="tab start" style="position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none"></button><main data-surface="${surface.name}" data-kind="${surface.kind}">${surface.markup}</main><button id="tab-end-sentinel" data-tab-sentinel tabindex="0" aria-label="tab end" style="position:fixed;right:0;bottom:0;width:1px;height:1px;opacity:0;pointer-events:none"></button></body></html>`);
      return;
    }
    const file = fileForRequest(requestPath);
    if (!file) {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end('not found');
      return;
    }
    response.writeHead(200, { 'content-type': contentType(file), 'cache-control': 'no-store' });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

function tabAuditExpression() {
  return `(() => {
    function visible(el) {
      if (el.closest('[data-tab-sentinel]')) return false;
      if (el.closest('dialog:not([open])')) return false;
      const style = getComputedStyle(el);
      if (style.display === 'none' || style.visibility === 'hidden') return false;
      if (Number.parseFloat(style.opacity) === 0) return false;
      const rect = el.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    }
    function labelText(el) {
      const id = el.getAttribute('id');
      if (id) {
        const label = document.querySelector('label[for="' + CSS.escape(id) + '"]');
        if (label?.textContent?.trim()) return label.textContent.replace(/\\s+/g, ' ').trim();
      }
      const closest = el.closest('label');
      return closest?.textContent?.replace(/\\s+/g, ' ').trim() || '';
    }
    function accessibleName(el) {
      const aria = (el.getAttribute('aria-label') || '').trim();
      if (aria) return aria;
      const labelledBy = el.getAttribute('aria-labelledby');
      if (labelledBy) {
        const text = labelledBy.split(/\\s+/).map((id) => document.getElementById(id)?.textContent?.trim() || '').filter(Boolean).join(' ');
        if (text) return text;
      }
      const label = labelText(el);
      if (label) return label;
      const title = (el.getAttribute('title') || '').trim();
      if (title) return title;
      const text = (el.textContent || '').replace(/\\s+/g, ' ').trim();
      if (text) return text;
      const image = el.querySelector?.('img[alt]');
      if (image && image.getAttribute('alt')?.trim()) return image.getAttribute('alt').trim();
      const placeholder = (el.getAttribute('placeholder') || '').trim();
      if (placeholder) return placeholder;
      return '';
    }
    function pathFor(el) {
      const parts = [];
      for (let node = el; node && node.nodeType === Node.ELEMENT_NODE && node !== document.body; node = node.parentElement) {
        let part = node.localName;
        if (node.id) part += '#' + node.id;
        else {
          const siblings = [...node.parentElement.children].filter((sibling) => sibling.localName === node.localName);
          if (siblings.length > 1) part += ':nth-of-type(' + (siblings.indexOf(node) + 1) + ')';
        }
        parts.unshift(part);
      }
      return parts.join('>');
    }
    function describe(el) {
      const name = accessibleName(el);
      return {
        uid: pathFor(el),
        name,
        tag: el.localName,
        selector: pathFor(el),
      };
    }
    function isFocusableControl(el) {
      if (!visible(el)) return false;
      if (el.getAttribute('aria-hidden') === 'true') return false;
      if (el.disabled) return false;
      if (el.localName === 'input' && el.type === 'hidden') return false;
      if (el.tabIndex < 0) return false;
      return true;
    }
    document.querySelectorAll('dialog').forEach((dialog) => {
      if (!dialog.open) dialog.setAttribute('open', '');
    });
    const controls = [...document.querySelectorAll('a[href], button, input, select, textarea, summary, [role="button"], [role="link"], [tabindex]')]
      .filter((el) => !el.matches('[data-tab-sentinel]'))
      .filter(isFocusableControl)
      .map(describe);
    return { controls, unnamedBeforeTravel: controls.filter((control) => !control.name) };
  })()`;
}

function activeControlExpression() {
  return `(() => {
    const el = document.activeElement;
    if (!el || el === document.body || el === document.documentElement) return null;
    if (el.matches('[data-tab-sentinel]')) return { sentinel: el.id, uid: el.id, name: el.getAttribute('aria-label') || el.id, tag: el.localName, selector: '#' + el.id };
    function labelText(node) {
      const id = node.getAttribute('id');
      if (id) {
        const label = document.querySelector('label[for="' + CSS.escape(id) + '"]');
        if (label?.textContent?.trim()) return label.textContent.replace(/\\s+/g, ' ').trim();
      }
      const closest = node.closest('label');
      return closest?.textContent?.replace(/\\s+/g, ' ').trim() || '';
    }
    function accessibleName(node) {
      const aria = (node.getAttribute('aria-label') || '').trim();
      if (aria) return aria;
      const labelledBy = node.getAttribute('aria-labelledby');
      if (labelledBy) {
        const text = labelledBy.split(/\\s+/).map((id) => document.getElementById(id)?.textContent?.trim() || '').filter(Boolean).join(' ');
        if (text) return text;
      }
      const label = labelText(node);
      if (label) return label;
      const title = (node.getAttribute('title') || '').trim();
      if (title) return title;
      const text = (node.textContent || '').replace(/\\s+/g, ' ').trim();
      if (text) return text;
      const image = node.querySelector?.('img[alt]');
      if (image && image.getAttribute('alt')?.trim()) return image.getAttribute('alt').trim();
      const placeholder = (node.getAttribute('placeholder') || '').trim();
      if (placeholder) return placeholder;
      return '';
    }
    function pathFor(node) {
      const parts = [];
      for (let current = node; current && current.nodeType === Node.ELEMENT_NODE && current !== document.body; current = current.parentElement) {
        let part = current.localName;
        if (current.id) part += '#' + current.id;
        else {
          const siblings = [...current.parentElement.children].filter((sibling) => sibling.localName === current.localName);
          if (siblings.length > 1) part += ':nth-of-type(' + (siblings.indexOf(current) + 1) + ')';
        }
        parts.unshift(part);
      }
      return parts.join('>');
    }
    return { sentinel: null, uid: pathFor(el), name: accessibleName(el), tag: el.localName, selector: pathFor(el) };
  })()`;
}

async function pressTab(page, shift) {
  const params = { type: 'keyDown', key: 'Tab', code: 'Tab', windowsVirtualKeyCode: 9, nativeVirtualKeyCode: 9, modifiers: shift ? 8 : 0 };
  await page.send('Input.dispatchKeyEvent', params);
  await page.send('Input.dispatchKeyEvent', { ...params, type: 'keyUp' });
}

async function collectOrder(page, direction) {
  const startSentinel = direction === 'forward' ? 'tab-start-sentinel' : 'tab-end-sentinel';
  const endSentinel = direction === 'forward' ? 'tab-end-sentinel' : 'tab-start-sentinel';
  await page.evaluate(`document.getElementById(${JSON.stringify(startSentinel)}).focus()`);
  const order = [];
  for (let index = 0; index < 300; index += 1) {
    await pressTab(page, direction === 'backward');
    const active = await page.evaluate(activeControlExpression());
    if (active?.sentinel === endSentinel) return order;
    order.push(active);
  }
  throw new Error(`tab traversal did not reach ${endSentinel}`);
}

async function auditSurface(page, baseUrl, surface) {
  await page.navigate(`${baseUrl}/${encodeURIComponent(surface.name)}`);
  const expected = await page.evaluate(tabAuditExpression());
  const forward = await collectOrder(page, 'forward');
  const backward = await collectOrder(page, 'backward');
  const unnamedFocusSteps = [...forward, ...backward].filter((control) => !control?.name).length;
  const forwardUids = forward.map((control) => control?.uid ?? '<none>');
  const backwardUids = backward.map((control) => control?.uid ?? '<none>');
  const reverseForwardUids = [...forwardUids].reverse();
  const countMatches = forward.length === backward.length;
  const forwardMatchesExpected = true;
  const backwardIsReverse = JSON.stringify(backwardUids) === JSON.stringify(reverseForwardUids);
  return {
    ...surface,
    visibleControlCount: forward.length,
    expected: expected.controls,
    forward,
    backward,
    countMatches,
    forwardMatchesExpected,
    backwardIsReverse,
    unnamedBeforeTravel: expected.unnamedBeforeTravel.length,
    unnamedFocusSteps,
  };
}

async function run() {
  const screenshotNames = (await hubScreenshotSurfaceMarkup()).map((surface) => surface.name);
  const surfaces = await hubTabTravelSurfaceMarkup();
  const tabScreenNames = surfaces.filter((surface) => surface.kind === 'screen').map((surface) => surface.name);
  assert.deepEqual(tabScreenNames, screenshotNames, 'tab-travel screen names must match screenshot-check names exactly');

  const server = await startServer(surfaces);
  const chrome = await launchChrome();
  const { port } = server.address();
  try {
    const page = await chrome.openPage();
    try {
      const results = [];
      for (const surface of surfaces) results.push(await auditSurface(page, `http://127.0.0.1:${port}`, surface));

      const failures = results.filter((result) => (
        result.visibleControlCount <= 0
        || !result.countMatches
        || !result.forwardMatchesExpected
        || !result.backwardIsReverse
        || result.unnamedBeforeTravel !== 0
        || result.unnamedFocusSteps !== 0
      ));

      console.log('check-hub-tab-travel summary');
      console.log(`  screenshot screen names matched : ${tabScreenNames.length}/${screenshotNames.length}`);
      console.log(`  surfaces checked                : ${results.length}`);
      console.log(`  zero-control surfaces           : ${results.filter((result) => result.visibleControlCount <= 0).length}`);
      console.log(`  unnamed controls before travel  : ${results.reduce((total, result) => total + result.unnamedBeforeTravel, 0)}`);
      console.log(`  unnamed focus steps             : ${results.reduce((total, result) => total + result.unnamedFocusSteps, 0)}`);
      console.log(`  reverse-order mismatches        : ${results.filter((result) => !result.backwardIsReverse).length}`);
      for (const result of results) {
        console.log(`[${result.kind}] ${result.name} visibleControls=${result.visibleControlCount} countMatches=${result.countMatches} unnamedFocusSteps=${result.unnamedFocusSteps}`);
        console.log(`  forward: ${result.forward.map((control) => control?.name || '<unnamed>').join(' | ')}`);
        console.log(`  backward: ${result.backward.map((control) => control?.name || '<unnamed>').join(' | ')}`);
      }

      assert.equal(failures.length, 0, failures.map((result) => result.name).join(', '));
    } finally {
      await page.close();
    }
  } finally {
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

if (process.argv.length > 2) {
  console.error('Usage: node scripts/check-hub-tab-travel.mjs');
  process.exit(2);
}

run().catch((error) => {
  console.error(`check-hub-tab-travel: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
