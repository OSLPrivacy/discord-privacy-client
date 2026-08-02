#!/usr/bin/env node

// Windows can resist ordinary capture in supported cases, but it cannot tell
// OSL that somebody took a screenshot. This gate renders every Hub surface in
// Chromium and rejects wording or affirmative presentation that would suggest
// otherwise. It intentionally examines the rendered DOM, not TypeScript text.

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { launchChrome } from './lib/cdp-harness.mjs';

const SCRIPTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.dirname(SCRIPTS_DIR);
const UI_ROOT = path.join(REPO_ROOT, 'apps', 'osl-hub-ui');

const BANNED_COPY = [
  /screenshot[-\s]?proof/iu,
  /screenshot\s+detection/iu,
  /screenshots?\s+(?:are|is)\s+blocked/iu,
  /(?:we(?:'ll| will)|osl)\s+(?:will\s+)?(?:tell|notify|alert)\s+you\s+(?:if|when)\s+(?:they\s+)?screenshott?/iu,
];

const DETECTION_CLAIM = /(?:screenshot|screen\s+capture).{0,80}(?:detect(?:ion|ed|ing)?|alert|notif(?:y|ication))|(?:detect(?:ion|ed|ing)?|alert|notif(?:y|ication)).{0,80}(?:screenshot|screen\s+capture)/iu;
const AFFIRMATIVE_TONE = /\b(?:active|affirmative|ok|success|confirmed|verified)\b/iu;

function assertSafeRenderedDom(surfaces) {
  const violations = [];
  for (const surface of surfaces) {
    for (const pattern of BANNED_COPY) {
      if (pattern.test(surface.text)) violations.push(`${surface.name}: prohibited capture claim ${pattern}`);
    }
    for (const claim of surface.detectionClaims) {
      if (AFFIRMATIVE_TONE.test(claim.className) || AFFIRMATIVE_TONE.test(claim.text)) {
        violations.push(`${surface.name}: affirmative tone attached to screenshot-detection claim (${claim.text})`);
      }
    }
  }
  assert.equal(violations.length, 0, violations.join('\n'));
}

async function realSurfaceMarkup() {
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
  process.env.VITEST = 'screenshot-claim-gate';
  const vite = await createViteServer({
    root: UI_ROOT,
    configFile: false,
    appType: 'custom',
    logLevel: 'error',
    // This is a one-shot gate, not a development server. Avoid consuming a
    // watcher for every UI file when several verification lanes run at once.
    server: { middlewareMode: true, watch: { ignored: ['**/*'] } },
  });
  try {
    const { __oslHubUiTest: ui } = await vite.ssrLoadModule('/src/main.ts');
    const surfaces = [];
    const add = (name, markup) => surfaces.push({ name, markup });
    const routes = ['home', 'inbox', 'people', 'privacy', 'activity', 'connections', 'mullvad', 'osl-chat', 'osl-mail', 'osl-servers', 'signal-qa'];
    const onboarding = ['pro', 'welcome', 'create', 'import', 'unlock', 'account-recovery', 'recovery', 'mullvad', 'sending', 'defaults', 'cover', 'passwords', 'burnpass', 'privacy', 'tutorial', 'detected', 'install', 'apps', 'browser', 'decoy'];
    const settings = ['account', 'apps', 'scrub', 'cleanup', 'notifications', 'appearance', 'about'];

    for (const route of routes) {
      ui.reset({ coreReady: true, servicesChecked: true });
      add(`route:${route}`, ui.renderWorkspaceContent(route));
    }
    for (const destination of onboarding) {
      ui.reset({ coreReady: true, servicesChecked: true });
      add(`onboarding:${destination}`, ui.renderOnboardingRoute(destination));
    }
    for (const section of settings) {
      ui.reset({ coreReady: true, servicesChecked: true });
      add(`settings:${section}`, ui.renderSettingsSection(section));
    }
    ui.reset({ coreReady: true, servicesChecked: true });
    add('service:discord', ui.renderServiceHeader('discord'));
    add('protected-sheets', ui.renderProtectedSheets());
    return surfaces;
  } finally {
    await vite.close();
    if (previousVitest === undefined) delete process.env.VITEST;
    else process.env.VITEST = previousVitest;
    if (previousStorage) Object.defineProperty(globalThis, 'localStorage', previousStorage);
    else delete globalThis.localStorage;
  }
}

function startServer(surfaces) {
  const document = surfaces.map(({ name, markup }) => `<section data-surface="${name}">${markup}</section>`).join('\n');
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
    response.end(`<!doctype html><main>${document}</main>`);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function inspectRenderedDom(page) {
  return page.evaluate(`(() => [...document.querySelectorAll('[data-surface]')].map((surface) => {
    const text = surface.innerText.replace(/\\s+/g, ' ').trim();
    const detectionClaims = [...surface.querySelectorAll('*')]
      .filter((element) => ${DETECTION_CLAIM}.test(element.innerText))
      .map((element) => ({ text: element.innerText.replace(/\\s+/g, ' ').trim(), className: element.className || '' }));
    return { name: surface.dataset.surface, text, detectionClaims };
  }))()`);
}

async function renderedFixture(markup) {
  const server = await startServer([{ name: 'fixture', markup }]);
  const chrome = await launchChrome();
  const { port } = server.address();
  try {
    const page = await chrome.openPage();
    try {
      await page.navigate(`http://127.0.0.1:${port}/`);
      return await inspectRenderedDom(page);
    } finally {
      await page.close();
    }
  } finally {
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

async function run({ selfTest = false } = {}) {
  if (selfTest) {
    await assert.rejects(
      () => renderedFixture('<p>Screenshots are blocked for this chat.</p>').then(assertSafeRenderedDom),
      /prohibited capture claim/,
    );
    await assert.rejects(
      () => renderedFixture('<p class="status-tag ok">Screen capture alerts enabled</p>').then(assertSafeRenderedDom),
      /affirmative tone attached/,
    );
    console.log('check-screenshot-claims: self-test rejected prohibited copy and affirmative detection tone');
  }

  const markup = await realSurfaceMarkup();
  const server = await startServer(markup);
  const chrome = await launchChrome();
  const { port } = server.address();
  try {
    const page = await chrome.openPage();
    try {
      await page.navigate(`http://127.0.0.1:${port}/`);
      const rendered = await inspectRenderedDom(page);
      assertSafeRenderedDom(rendered);
      console.log(`check-screenshot-claims: ${rendered.length} rendered surfaces contain no screenshot-detection implication`);
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
  console.error('Usage: node scripts/check-screenshot-claims.mjs [--self-test]');
  process.exit(2);
}

run({ selfTest }).catch((error) => {
  console.error(`check-screenshot-claims: fatal error: ${error.stack || error.message}`);
  process.exit(1);
});
