#!/usr/bin/env node

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { launchChrome } from '../lib/cdp-harness.mjs';

const BOX_KEY_PRESSES = 100;
const SHORTCUT_REPEATS = 20;

function usage() {
  return 'Usage: node scripts/qa/osl-rapid-key-check.mjs [--self-test]';
}

function uniqueRunMark() {
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  return `TASK3547-${stamp}-${process.pid}`;
}

function exactMark(runMark, ordinal) {
  const prefix = `${runMark}-BOX${String(ordinal).padStart(2, '0')}-`;
  return (prefix + 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789'.repeat(3))
    .slice(0, BOX_KEY_PRESSES);
}

function shortcutDefinitions() {
  return [
    { id: 'save', label: 'Ctrl+S save', key: 's', code: 'KeyS', ctrlKey: true, enabled: true },
    { id: 'palette', label: 'Ctrl+K palette', key: 'k', code: 'KeyK', ctrlKey: true, enabled: true },
    { id: 'blocked-admin', label: 'Ctrl+B blocked admin', key: 'b', code: 'KeyB', ctrlKey: true, enabled: false },
  ];
}

function fixedScreenHtml({ empty = false } = {}) {
  const boxes = empty ? '' : `
    <label>Single line <input data-rapid-box="title" aria-label="Title box" autocomplete="off" spellcheck="false"></label>
    <label>Search <input data-rapid-box="search" type="search" aria-label="Search box" autocomplete="off" spellcheck="false"></label>
    <label>Message <textarea data-rapid-box="message" aria-label="Message box" spellcheck="false"></textarea></label>
    <div data-rapid-box="rich" role="textbox" aria-label="Rich text box" contenteditable="true"></div>
  `;
  const shortcuts = shortcutDefinitions().map((shortcut) => (
    `<button type="button" data-shortcut="${shortcut.id}" data-shortcut-enabled="${shortcut.enabled ? 'true' : 'false'}">${shortcut.label}</button>`
  )).join('');
  return `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>OSL fixed rapid key screen</title>
<style>
  body { margin: 0; font: 16px system-ui, sans-serif; background: #101418; color: #f6f7f8; }
  main { padding: 24px; display: grid; gap: 16px; max-width: 760px; }
  label { display: grid; gap: 6px; }
  input, textarea, [contenteditable] { box-sizing: border-box; min-height: 44px; width: 100%; border: 1px solid #6b7683; border-radius: 6px; background: #171d23; color: white; padding: 10px 12px; font: inherit; }
  textarea { min-height: 92px; resize: none; }
  [contenteditable] { white-space: pre-wrap; }
  .shortcuts { display: flex; gap: 8px; flex-wrap: wrap; }
  button { border: 1px solid #607084; border-radius: 6px; background: #263445; color: white; padding: 8px 10px; font: inherit; }
</style>
<main>
  <h1>OSL fixed rapid key screen</h1>
  <section id="boxes">${boxes}</section>
  <section class="shortcuts" aria-label="Shortcuts">${shortcuts}</section>
  <output id="rapid-key-report" aria-live="off"></output>
</main>
<script>
  const actions = Object.fromEntries([...document.querySelectorAll('[data-shortcut]')].map((button) => [button.dataset.shortcut, 0]));
  const fired = new Set();
  const shortcuts = ${JSON.stringify(shortcutDefinitions())};
  function matches(event, shortcut) {
    return event.key.toLowerCase() === shortcut.key
      && Boolean(event.ctrlKey) === Boolean(shortcut.ctrlKey)
      && Boolean(event.metaKey) === Boolean(shortcut.metaKey)
      && Boolean(event.altKey) === Boolean(shortcut.altKey)
      && Boolean(event.shiftKey) === Boolean(shortcut.shiftKey);
  }
  window.addEventListener('keydown', (event) => {
    for (const shortcut of shortcuts) {
      if (!matches(event, shortcut)) continue;
      event.preventDefault();
      if (!shortcut.enabled || fired.has(shortcut.id)) return;
      fired.add(shortcut.id);
      actions[shortcut.id] += 1;
      document.querySelector('[data-shortcut="' + shortcut.id + '"]').dataset.fired = 'true';
      return;
    }
  });
  window.__rapidKeyScreen = {
    boxes: () => [...document.querySelectorAll('[data-rapid-box]')].map((element) => ({
      id: element.dataset.rapidBox,
      label: element.getAttribute('aria-label') || element.dataset.rapidBox,
      tag: element.tagName.toLowerCase(),
      value: element.isContentEditable ? element.textContent : element.value,
    })),
    shortcuts: () => shortcuts.map((shortcut) => ({
      id: shortcut.id,
      label: shortcut.label,
      repeats: ${SHORTCUT_REPEATS},
      enabled: shortcut.enabled,
      count: actions[shortcut.id] || 0,
    })),
  };
</script>
</html>`;
}

function startFixedScreenServer(options = {}) {
  const html = fixedScreenHtml(options);
  const server = createServer((request, response) => {
    if ((request.url || '').split('?')[0] !== '/') {
      response.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
      response.end('not found');
      return;
    }
    response.writeHead(200, { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' });
    response.end(html);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

async function waitForScreen(page) {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    if (await page.evaluate('Boolean(window.__rapidKeyScreen)')) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error('fixed rapid key screen did not initialize');
}

async function keyStroke(page, { key, code, text = '', ctrlKey = false, metaKey = false, altKey = false, shiftKey = false }) {
  const modifiers = (altKey ? 1 : 0) | (ctrlKey ? 2 : 0) | (metaKey ? 4 : 0) | (shiftKey ? 8 : 0);
  const base = { key, code, windowsVirtualKeyCode: key.toUpperCase().charCodeAt(0), nativeVirtualKeyCode: key.toUpperCase().charCodeAt(0), modifiers };
  await page.send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', text, unmodifiedText: text, ...base });
  await page.send('Input.dispatchKeyEvent', { type: 'keyUp', ...base });
}

async function typeMark(page, selector, mark) {
  await page.send('Runtime.evaluate', { expression: `document.querySelector(${JSON.stringify(selector)}).focus()` });
  for (const char of mark) {
    await keyStroke(page, { key: char, code: `Key${char.toUpperCase()}`, text: char });
  }
}

async function runShortcutRepeats(page, shortcut) {
  await page.send('Runtime.evaluate', { expression: 'document.body.focus()' });
  for (let repeat = 0; repeat < SHORTCUT_REPEATS; repeat += 1) {
    await keyStroke(page, shortcut);
  }
}

async function collect(page) {
  return page.evaluate(`(() => ({
    boxes: window.__rapidKeyScreen.boxes(),
    shortcuts: window.__rapidKeyScreen.shortcuts(),
  }))()`);
}

export function validateReport(report) {
  assert.ok(report.runMark, 'report must include the unique run mark');
  assert.equal(report.keyPressesPerBox, BOX_KEY_PRESSES, 'box key press count');
  assert.equal(report.shortcutRepeats, SHORTCUT_REPEATS, 'shortcut repeat count');
  assert.ok(report.boxCount > 0, 'box count must be above 0');
  assert.equal(report.exactBoxMatchCount, report.boxCount, 'all boxes must hold their exact mark');
  assert.equal(report.unexpectedProcessStops, 0, 'no process may stop before cleanup');
  assert.ok(report.shortcuts.length > 0, 'shortcut count must be above 0');
  assert.equal(report.shortcutCount, report.shortcuts.length, 'shortcut count must match listed shortcuts');
  assert.ok(report.shortcuts.every((shortcut) => shortcut.count === 0 || shortcut.count === 1), 'every action count must be 0 or 1');
  assert.ok(report.shortcuts.some((shortcut) => shortcut.count === 1), 'at least one action count must be exactly 1');
  return report;
}

async function runRapidKeyCheck(options = {}) {
  const runMark = uniqueRunMark();
  const server = await startFixedScreenServer(options);
  const chrome = await launchChrome();
  const { port } = server.address();
  const page = await chrome.openPage();
  try {
    await page.navigate(`http://127.0.0.1:${port}/`);
    await waitForScreen(page);

    const initial = await collect(page);
    const expectedById = new Map();
    for (const [index, box] of initial.boxes.entries()) {
      const mark = exactMark(runMark, index + 1);
      expectedById.set(box.id, mark);
      await typeMark(page, `[data-rapid-box="${box.id}"]`, mark);
    }
    for (const shortcut of shortcutDefinitions()) {
      await runShortcutRepeats(page, shortcut);
    }

    const observed = await collect(page);
    const boxes = observed.boxes.map((box) => ({
      ...box,
      expected: expectedById.get(box.id) || '',
      exact: box.value === expectedById.get(box.id),
    }));
    const shortcuts = observed.shortcuts;
    const unexpectedProcessStops = chrome.child.exitCode === null && chrome.child.signalCode === null ? 0 : 1;
    const report = {
      schema: 'osl-rapid-key-check-v1',
      runMark,
      keyPressesPerBox: BOX_KEY_PRESSES,
      shortcutRepeats: SHORTCUT_REPEATS,
      boxCount: boxes.length,
      exactBoxMatchCount: boxes.filter((box) => box.exact).length,
      shortcutCount: shortcuts.length,
      unexpectedProcessStops,
      boxes,
      shortcuts,
    };
    return validateReport(report);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

function printReport(report) {
  console.log(`TASK3547_RUN mark=${report.runMark} key_presses_per_box=${report.keyPressesPerBox} shortcut_repeats=${report.shortcutRepeats}`);
  console.log(`TASK3547_COUNTS boxes=${report.boxCount} exact_box_matches=${report.exactBoxMatchCount} shortcuts=${report.shortcutCount} unexpected_process_stops=${report.unexpectedProcessStops}`);
  for (const box of report.boxes) {
    console.log(`TASK3547_BOX id=${box.id} tag=${box.tag} expected=${JSON.stringify(box.expected)} actual=${JSON.stringify(box.value)} exact=${box.exact ? 1 : 0}`);
  }
  for (const shortcut of report.shortcuts) {
    console.log(`TASK3547_SHORTCUT id=${shortcut.id} label=${JSON.stringify(shortcut.label)} repeats=${shortcut.repeats} count=${shortcut.count}`);
  }
  console.log(`TASK3547_FINISH boxes_above_zero=${report.boxCount > 0 ? 1 : 0} unexpected_process_stops=${report.unexpectedProcessStops} exact_matches_equal_box_count=${report.exactBoxMatchCount === report.boxCount ? 1 : 0} action_counts_0_or_1=${report.shortcuts.every((shortcut) => shortcut.count === 0 || shortcut.count === 1) ? 1 : 0} at_least_one_action_count_1=${report.shortcuts.some((shortcut) => shortcut.count === 1) ? 1 : 0}`);
}

async function runSelfTest() {
  try {
    await runRapidKeyCheck({ empty: true });
    throw new Error('empty fixed screen unexpectedly passed');
  } catch (error) {
    if (!/box count must be above 0/.test(error.message)) throw error;
    console.log(`TASK3547_SELFTEST_EMPTY status=red reason=${JSON.stringify(error.message)}`);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const args = process.argv.slice(2);
  if (args.length > 1 || (args[0] && args[0] !== '--self-test')) {
    console.error(usage());
    process.exit(2);
  }
  try {
    if (args[0] === '--self-test') await runSelfTest();
    const report = await runRapidKeyCheck();
    printReport(report);
  } catch (error) {
    console.error(`TASK3547_ERROR ${error.stack || error.message}`);
    process.exit(1);
  }
}
