import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

import { launchChrome } from '../../../scripts/lib/cdp-harness.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appRoot = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appRoot, '..', '..');
const evidenceDir = path.join(repoRoot, 'evidence', 'task-0375');
const screenshotPath = path.join(evidenceDir, 'browser-account-finder.png');
const fixturePath = '/screenshots/fixtures/task-0375-browser-account-finder.html';
const viewport = { width: 720, height: 820 };
const requiredText = [
  'Find saved browser accounts',
  'Check selected',
  'Delete area',
  'Not now',
  'Back',
];

async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const { port } = server.address();
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function startVite(t) {
  const port = await freePort();
  const viteBin = path.join(appRoot, 'node_modules', '.bin', 'vite');
  const child = spawn(viteBin, ['--host', '127.0.0.1', '--port', String(port), '--strictPort'], {
    cwd: appRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  child.stdout.on('data', (chunk) => { output += chunk.toString('utf8'); });
  child.stderr.on('data', (chunk) => { output += chunk.toString('utf8'); });
  t.after(() => {
    if (child.exitCode === null) child.kill('SIGTERM');
  });
  const url = `http://127.0.0.1:${port}${fixturePath}`;
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Vite exited before serving fixture:\n${output}`);
    try {
      const response = await fetch(url);
      if (response.ok) return { url, output };
    } catch {
      // Keep polling until Vite binds the port.
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for Vite fixture:\n${output}`);
}

async function waitForFixture(page) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const ready = await page.evaluate('Boolean(globalThis.__TASK0375_READY)');
    if (ready) return;
    await delay(50);
  }
  throw new Error('timed out waiting for TASK0375 fixture');
}

function pngFacts(file) {
  const result = spawnSync('python3', [path.join(repoRoot, 'scripts', 'vmqa', 'png-facts.py'), file], {
    cwd: repoRoot,
    encoding: 'utf8',
  });
  if (result.status !== 0) throw new Error(result.stderr || result.stdout || 'png-facts.py failed');
  return JSON.parse(result.stdout);
}

function axNames(nodes) {
  return nodes
    .map((node) => typeof node.name?.value === 'string' ? node.name.value.trim() : '')
    .filter(Boolean);
}

test('TASK 0375 captures the fixed browser account finder screen', async (t) => {
  const vite = await startVite(t);
  const chrome = await launchChrome();
  t.after(() => chrome.close());
  const page = await chrome.openPage();
  t.after(() => page.close());

  await page.send('Emulation.setDeviceMetricsOverride', {
    width: viewport.width,
    height: viewport.height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await page.navigate(vite.url);
  await waitForFixture(page);

  const domFacts = await page.evaluate('globalThis.__TASK0375_FACTS');
  if (domFacts.title !== 'Find saved browser accounts') {
    const body = await page.evaluate('document.body.innerText');
    throw new Error(`TASK0375 fixture did not paint account finder. facts=${JSON.stringify(domFacts)} body=${JSON.stringify(body.slice(0, 500))}`);
  }
  assert.deepEqual(domFacts, {
    title: 'Find saved browser accounts',
    checked: true,
    checkSelected: true,
    deleteArea: true,
    notNow: true,
    back: true,
  });

  const axTree = await page.send('Accessibility.getFullAXTree');
  const names = axNames(axTree.nodes);
  const visibleText = await page.evaluate('document.body.innerText');
  for (const text of requiredText) {
    assert.ok(names.includes(text), `accessibility tree contains ${text}`);
    assert.match(visibleText, new RegExp(text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'u'));
  }

  mkdirSync(evidenceDir, { recursive: true });
  const png = await page.screenshot({
    clip: { x: 0, y: 0, width: viewport.width, height: viewport.height, scale: 1 },
  });
  writeFileSync(screenshotPath, png);
  const facts = pngFacts(screenshotPath);
  assert.equal(facts.width, viewport.width);
  assert.equal(facts.height, viewport.height);
  assert.ok(facts.distinctColors > 24, `screenshot is nearly blank: ${facts.distinctColors} sampled colors`);

  console.log(`TASK0375_SCREENSHOT_PATH=${path.relative(repoRoot, screenshotPath)}`);
  console.log(`TASK0375_WINDOW=${viewport.width}x${viewport.height}`);
  console.log(`TASK0375_TITLE=${domFacts.title}`);
  console.log(`TASK0375_CONTROL_TICK=chrome:Default checked=${domFacts.checked}`);
  console.log(`TASK0375_CHECK_SELECTED=Check selected`);
  console.log(`TASK0375_DELETE_AREA=Delete area`);
  console.log(`TASK0375_NOT_NOW=Not now`);
  console.log(`TASK0375_BACK=Back`);
  console.log(`TASK0375_AX_NAMES=${requiredText.map((text) => `${text}:${names.includes(text)}`).join(',')}`);
  console.log(`TASK0375_PNG_FACTS width=${facts.width} height=${facts.height} distinctColors=${facts.distinctColors}`);
}, 90_000);
