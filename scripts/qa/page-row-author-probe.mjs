#!/usr/bin/env node

import { createServer } from 'node:http';
import { pathToFileURL } from 'node:url';

import { launchChrome } from '../lib/cdp-harness.mjs';

const READ_DATE = process.env.OSL_TASK4084_READ_DATE || '2026-08-07';
const FIELD_NONE = 'none';

function usage() {
  return [
    'Usage:',
    '  node scripts/qa/page-row-author-probe.mjs --task4084-fixtures',
    '  node scripts/qa/page-row-author-probe.mjs --fixture <name> --service <discord|messenger> --label <label> [--expected-rows 10]',
    '  node scripts/qa/page-row-author-probe.mjs --url <url> --service <discord|messenger> --label <label> [--expected-rows 10]',
    '',
    'Fixtures: discord-control, messenger-direct, messenger-group, messenger-signed-out',
  ].join('\n');
}

function parseArgs(argv) {
  const args = { expectedRows: 10 };
  for (let index = 2; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--task4084-fixtures') {
      args.task4084Fixtures = true;
      continue;
    }
    if (arg === '--fixture' || arg === '--service' || arg === '--label' || arg === '--url') {
      const value = argv[index + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      args[arg.slice(2)] = value;
      index += 1;
      continue;
    }
    if (arg === '--expected-rows') {
      const value = Number.parseInt(argv[index + 1] || '', 10);
      if (!Number.isInteger(value) || value <= 0) throw new Error('--expected-rows must be a positive integer');
      args.expectedRows = value;
      index += 1;
      continue;
    }
    throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function htmlEscape(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function baseDocument({ title, body }) {
  return `<!doctype html>
<html lang="en">
<meta charset="utf-8">
<title>${htmlEscape(title)}</title>
<style>
  body { margin: 0; font: 14px Arial, sans-serif; color: #111; background: #fff; }
  main { width: 760px; padding: 24px; }
  ol { margin: 0; padding: 0; display: grid; gap: 8px; }
  li[role="listitem"] {
    list-style: none;
    display: grid;
    grid-template-columns: 48px 1fr 132px;
    gap: 10px;
    min-height: 56px;
    border: 1px solid #c8c8c8;
    padding: 8px;
  }
  [data-box] { display: block; min-height: 20px; }
  [data-box="avatar"] img { width: 40px; height: 40px; display: block; }
  [data-author-wording] { font-weight: 700; }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
${body}
</html>`;
}

function rowMarkup({ id, testId, wording, role = 'article', state, img, message, screenReader }) {
  return `<li id="${htmlEscape(id)}" data-osl-probe-row="${htmlEscape(id)}" data-testid="${htmlEscape(testId)}" data-state="${htmlEscape(state)}" role="listitem" aria-label="${htmlEscape(screenReader)}">
    <div data-box="avatar"><img src="${htmlEscape(img)}" alt=""></div>
    <article role="${htmlEscape(role)}" data-box="message" aria-describedby="${htmlEscape(id)}-sr">
      <span data-author-wording role="text">${htmlEscape(wording)}</span>
      <p>${htmlEscape(message)}</p>
      <span class="sr-only" id="${htmlEscape(id)}-sr">${htmlEscape(screenReader)}</span>
    </article>
    <aside data-box="beside" role="group" aria-label="${htmlEscape(state)}">${htmlEscape(state)}</aside>
  </li>`;
}

function rowsFor(kind) {
  if (kind === 'discord-control') {
    return Array.from({ length: 10 }, (_, index) => {
      const number = index + 1;
      const yours = [1, 3, 5, 7, 9].includes(number);
      return {
        id: `discord-msg-${String(number).padStart(2, '0')}`,
        testId: `discord-row-${String(number).padStart(2, '0')}`,
        wording: yours ? 'You sent' : 'Deckard sent',
        state: yours ? 'sent,own,visible' : 'sent,peer,visible',
        img: `https://cdn.example.invalid/discord/avatar-${String(number).padStart(2, '0')}.png`,
        message: `Discord control message ${number}`,
        screenReader: `${yours ? 'You' : 'Deckard'} sent Discord control message ${number}`,
      };
    });
  }
  if (kind === 'messenger-direct') {
    return Array.from({ length: 10 }, (_, index) => {
      const number = index + 1;
      const yours = [1, 2, 5, 8].includes(number);
      return {
        id: `messenger-direct-msg-${String(number).padStart(2, '0')}`,
        testId: `mw-direct-row-${String(number).padStart(2, '0')}`,
        wording: yours ? 'You sent' : 'Rowan Vale sent',
        state: yours ? 'sent,own,delivered' : 'sent,peer,read',
        img: `https://cdn.example.invalid/messenger/direct-${String(number).padStart(2, '0')}.png`,
        message: `Messenger direct canary ${number}`,
        screenReader: `${yours ? 'You' : 'Rowan Vale'} sent Messenger direct canary ${number}`,
      };
    });
  }
  if (kind === 'messenger-group') {
    const names = ['You', 'Kai Rivers', 'Mina Cho', 'You', 'Sam Patel', 'Kai Rivers', 'You', 'Mina Cho', 'Devon Ray', 'You'];
    return names.map((name, index) => {
      const number = index + 1;
      return {
        id: `messenger-group-msg-${String(number).padStart(2, '0')}`,
        testId: `mw-group-row-${String(number).padStart(2, '0')}`,
        wording: name === 'You' ? 'You sent' : `${name} sent`,
        state: name === 'You' ? 'sent,own,delivered' : 'sent,group-peer,read',
        img: `https://cdn.example.invalid/messenger/group-${String(number).padStart(2, '0')}.png`,
        message: `Messenger group canary ${number}`,
        screenReader: `${name} sent Messenger group canary ${number}`,
      };
    });
  }
  throw new Error(`unknown row fixture: ${kind}`);
}

function fixtureHtml(name) {
  if (name === 'messenger-signed-out') {
    return baseDocument({
      title: 'Messenger signed out',
      body: `<main aria-label="Messenger login">
        <h1>Log in to Messenger</h1>
        <label>Email <input autocomplete="username"></label>
        <label>Password <input type="password" autocomplete="current-password"></label>
        <button type="button">Log in</button>
      </main>`,
    });
  }
  const rows = rowsFor(name);
  const label = name === 'discord-control'
    ? 'Discord control conversation'
    : name === 'messenger-direct'
      ? 'Messenger direct conversation'
      : 'Messenger group conversation';
  return baseDocument({
    title: label,
    body: `<main role="main" aria-label="${htmlEscape(label)}">
      <h1>${htmlEscape(label)}</h1>
      <ol role="list" aria-label="Messages">${rows.map(rowMarkup).join('\n')}</ol>
    </main>`,
  });
}

function normalize(value) {
  return String(value || '').replace(/\s+/g, ' ').trim();
}

function quoteField(value) {
  const text = normalize(value);
  if (!text) return FIELD_NONE;
  return JSON.stringify(text);
}

function formatRow(label, row, index) {
  return [
    `TASK4084_${label.toUpperCase().replaceAll('-', '_')}_ROW`,
    `index=${String(index + 1).padStart(2, '0')}`,
    `wording=${quoteField(row.wording)}`,
    `test_id=${quoteField(row.testId)}`,
    `roles=${quoteField(row.roles)}`,
    `states=${quoteField(row.states)}`,
    `parents=${quoteField(row.parents)}`,
    `boxes_beside=${quoteField(row.boxesBeside)}`,
    `picture_address=${quoteField(row.pictureAddress)}`,
    `screen_reader=${quoteField(row.screenReader)}`,
  ].join(' ');
}

function formatSummary({ label, service, rows, expectedRows, noLineRows }) {
  return `TASK4084_${label.toUpperCase().replaceAll('-', '_')}_SUMMARY service=${service} read_date=${READ_DATE} rows=${rows.length}/${expectedRows} no_line_rows=${noLineRows}`;
}

function probeExpression() {
  return String.raw`(() => {
    const compact = (value) => String(value || '').replace(/\s+/g, ' ').trim();
    const visible = (element) => {
      if (!element || element.hidden || element.getAttribute('aria-hidden') === 'true') return false;
      const style = window.getComputedStyle(element);
      return style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0';
    };
    const labelText = (element, attr) => compact(
      (element.getAttribute(attr) || '')
        .split(/\s+/)
        .map((id) => document.getElementById(id))
        .filter(Boolean)
        .map((node) => node.innerText || node.textContent || '')
        .join(' ')
    );
    const roleOf = (element) => compact(element.getAttribute('role') || element.localName);
    const statesOf = (element) => {
      const states = [];
      for (const attr of ['data-state', 'aria-current', 'aria-selected', 'aria-expanded', 'aria-pressed', 'aria-disabled']) {
        const value = compact(element.getAttribute(attr));
        if (value) states.push(attr + '=' + value);
      }
      return states.join(';');
    };
    const parentPath = (element) => {
      const parents = [];
      for (let current = element.parentElement; current && parents.length < 5; current = current.parentElement) {
        const name = compact(current.getAttribute('aria-label') || current.getAttribute('data-testid') || current.id);
        parents.push(current.localName + (current.getAttribute('role') ? '[role=' + current.getAttribute('role') + ']' : '') + (name ? ':' + name : ''));
      }
      return parents.join('>');
    };
    const boxLine = (row) => {
      const rowRect = row.getBoundingClientRect();
      const boxes = [];
      for (const box of row.querySelectorAll('[data-box], img, [role="group"]')) {
        if (!visible(box)) continue;
        const rect = box.getBoundingClientRect();
        boxes.push(compact(box.getAttribute('data-box') || box.localName) + '@' + Math.round(rect.left - rowRect.left) + ',' + Math.round(rect.top - rowRect.top) + ',' + Math.round(rect.width) + ',' + Math.round(rect.height));
      }
      return boxes.join('|');
    };
    const screenReaderText = (row) => {
      const pieces = [
        row.getAttribute('aria-label'),
        labelText(row, 'aria-labelledby'),
        labelText(row, 'aria-describedby'),
      ];
      for (const node of row.querySelectorAll('[aria-label], [aria-labelledby], [aria-describedby], .sr-only, [data-sr-text]')) {
        pieces.push(node.getAttribute('aria-label'));
        pieces.push(labelText(node, 'aria-labelledby'));
        pieces.push(labelText(node, 'aria-describedby'));
        pieces.push(node.getAttribute('data-sr-text'));
        if (node.classList.contains('sr-only')) pieces.push(node.innerText || node.textContent);
      }
      const seen = new Set();
      return pieces.map(compact).filter(Boolean).filter((piece) => {
        if (seen.has(piece)) return false;
        seen.add(piece);
        return true;
      }).join('|');
    };
    const rowCandidates = [...document.querySelectorAll('[data-osl-probe-row], [role="listitem"], article[role="article"]')]
      .filter((row, index, all) => visible(row) && all.findIndex((candidate) => candidate === row || candidate.contains(row)) === index);
    const rows = rowCandidates.map((row) => {
      const author = row.querySelector('[data-author-wording], [data-testid*="author" i], [aria-label$=" sent"]');
      const text = compact(author?.getAttribute('data-author-wording') || author?.getAttribute('aria-label') || author?.innerText || author?.textContent);
      const wording = /^(You|.{1,80}) sent$/.test(text) ? text : '';
      const roles = [roleOf(row), ...[...row.querySelectorAll('[role]')].map(roleOf)].filter(Boolean).join('>');
      const states = [statesOf(row), ...[...row.querySelectorAll('[data-state], [aria-current], [aria-selected], [aria-expanded], [aria-pressed], [aria-disabled]')].map(statesOf)].filter(Boolean).join('|');
      const image = row.querySelector('img[src], image[href], [data-picture-address]');
      return {
        wording,
        testId: compact(row.getAttribute('data-testid') || row.id || row.getAttribute('data-osl-probe-row')),
        roles,
        states,
        parents: parentPath(row),
        boxesBeside: boxLine(row),
        pictureAddress: compact(image?.currentSrc || image?.src || image?.getAttribute('href') || image?.getAttribute('data-picture-address')),
        screenReader: screenReaderText(row),
      };
    });
    const signedOut = rows.length === 0 && (
      Boolean(document.querySelector('input[type="password"], input[autocomplete="username"]')) ||
      /log in|sign in/i.test(compact(document.body?.innerText || document.body?.textContent || ''))
    );
    return { title: document.title, signedOut, rows };
  })()`;
}

async function serveFixture(name) {
  const html = fixtureHtml(name);
  const server = createServer((_request, response) => {
    response.writeHead(200, {
      connection: 'close',
      'content-type': 'text/html; charset=utf-8',
    });
    response.end(html);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return {
    url: `http://127.0.0.1:${server.address().port}/`,
    async close() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}

function validateProbeResult({ service, label, expectedRows, result }) {
  if (result.signedOut) {
    throw new Error(`signed-out page refused: ${label}`);
  }
  if (!Array.isArray(result.rows) || result.rows.length === 0) {
    throw new Error(`empty result refused: ${label}`);
  }
  if (result.rows.length !== expectedRows) {
    throw new Error(`${label}: expected ${expectedRows} rows, found ${result.rows.length}`);
  }
  const missing = result.rows
    .map((row, index) => ({ row, index }))
    .filter(({ row }) => !normalize(row.wording));
  if (missing.length > 0) {
    throw new Error(`${label}: ${missing.length} rows have no who-wrote-it wording`);
  }
  if (service === 'messenger') {
    const invalid = result.rows.find((row) => !/^(You|.{1,80}) sent$/.test(row.wording));
    if (invalid) throw new Error(`${label}: Messenger wording did not match expected app text: ${invalid.wording}`);
  }
}

async function probeUrl({ url, fixture, service, label, expectedRows, chrome }) {
  let fixtureServer;
  const targetUrl = url || (fixtureServer = await serveFixture(fixture)).url;
  const page = await chrome.openPage();
  try {
    await page.navigate(targetUrl);
    const result = await page.evaluate(probeExpression());
    validateProbeResult({ service, label, expectedRows, result });
    const noLineRows = result.rows.filter((row) => !normalize(row.wording)).length;
    return {
      result,
      lines: [
        formatSummary({ label, service, rows: result.rows, expectedRows, noLineRows }),
        ...result.rows.map((row, index) => formatRow(label, row, index)),
      ],
      whoWroteFound: result.rows.filter((row) => normalize(row.wording)).length,
    };
  } finally {
    await page.close();
    if (fixtureServer) await fixtureServer.close();
  }
}

export async function runTask4084Fixtures() {
  const chrome = await launchChrome();
  try {
    const discord = await probeUrl({
      fixture: 'discord-control',
      service: 'discord',
      label: 'discord-control',
      expectedRows: 10,
      chrome,
    });
    if (discord.whoWroteFound < 8) {
      throw new Error(`Discord control found ${discord.whoWroteFound}, below 8`);
    }
    const output = [`TASK4084_DISCORD_WHO_WROTE_FOUND=${discord.whoWroteFound}`, ...discord.lines];

    const direct = await probeUrl({
      fixture: 'messenger-direct',
      service: 'messenger',
      label: 'messenger-direct',
      expectedRows: 10,
      chrome,
    });
    output.push(...direct.lines);

    const group = await probeUrl({
      fixture: 'messenger-group',
      service: 'messenger',
      label: 'messenger-group',
      expectedRows: 10,
      chrome,
    });
    output.push(...group.lines);
    return output;
  } finally {
    await chrome.close();
  }
}

async function main() {
  const args = parseArgs(process.argv);
  if (args.task4084Fixtures) {
    const lines = await runTask4084Fixtures();
    console.log(lines.join('\n'));
    return;
  }
  if ((!args.fixture && !args.url) || !args.service || !args.label) {
    throw new Error(usage());
  }
  const chrome = await launchChrome();
  try {
    const probe = await probeUrl({
      url: args.url,
      fixture: args.fixture,
      service: args.service,
      label: args.label,
      expectedRows: args.expectedRows,
      chrome,
    });
    console.log(probe.lines.join('\n'));
  } finally {
    await chrome.close();
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
