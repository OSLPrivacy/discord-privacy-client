#!/usr/bin/env node
import { pathToFileURL } from 'node:url';

import { launchChrome } from './lib/cdp-harness.mjs';

const SURFACES = new Set(['discord', 'x']);
const DEFAULT_LIMIT = 10;

function usage() {
  return [
    'usage: node scripts/probe-dm-row-publishers.mjs --surface discord|x --url URL [--limit 10] [--read-date YYYY-MM-DD]',
    '       node scripts/probe-dm-row-publishers.mjs --surface discord|x --file HTML [--limit 10] [--read-date YYYY-MM-DD]',
  ].join('\n');
}

function parseArgs(argv) {
  const args = {
    limit: DEFAULT_LIMIT,
    readDate: new Date().toISOString().slice(0, 10),
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];
    if (arg === '--surface' && next) {
      args.surface = next;
      index += 1;
    } else if (arg === '--url' && next) {
      args.url = next;
      index += 1;
    } else if (arg === '--file' && next) {
      args.url = pathToFileURL(next).href;
      index += 1;
    } else if (arg === '--limit' && next) {
      args.limit = Number(next);
      index += 1;
    } else if (arg === '--read-date' && next) {
      args.readDate = next;
      index += 1;
    } else if (arg === '--help' || arg === '-h') {
      args.help = true;
    } else {
      throw new Error(`unknown or incomplete argument: ${arg}`);
    }
  }
  if (args.help) return args;
  if (!SURFACES.has(args.surface)) throw new Error('missing --surface discord|x');
  if (!args.url) throw new Error('missing --url or --file');
  if (!Number.isInteger(args.limit) || args.limit <= 0 || args.limit > 50) {
    throw new Error('--limit must be an integer from 1 to 50');
  }
  if (!/^\d{4}-\d{2}-\d{2}$/u.test(args.readDate)) {
    throw new Error('--read-date must be YYYY-MM-DD');
  }
  return args;
}

function jsString(value) {
  return JSON.stringify(value);
}

function serializeField(value) {
  if (value === null || value === undefined) return 'none';
  if (Array.isArray(value)) {
    if (value.length === 0) return 'none';
    return value.map(serializeField).join(',');
  }
  if (typeof value === 'object') {
    const text = Object.entries(value)
      .filter(([, item]) => item !== null && item !== undefined && item !== '')
      .map(([key, item]) => `${key}:${serializeField(item)}`)
      .join('+');
    return text || 'none';
  }
  const text = String(value).replace(/\s+/gu, ' ').trim();
  return text.length > 0 ? text : 'none';
}

function formatRow(surface, row) {
  const prefix = `${surface}_row ${row.index}`;
  return [
    prefix,
    `test=${serializeField(row.testsAndIdentifiers)}`,
    `who=${serializeField(row.whoWroteIt)}`,
    `roles_states=${serializeField(row.rolesAndStates)}`,
    `parents=${serializeField(row.parentContainers)}`,
    `boxes_beside=${serializeField(row.boxesBeside)}`,
    `picture=${serializeField(row.pictureAddresses)}`,
    `account_links=${serializeField(row.accountLinks)}`,
    `screen_reader=${serializeField(row.screenReaderText)}`,
  ].join(' ');
}

function pageProbe(input) {
  const limit = input.limit;
  const surface = input.surface;
  const signerHints = [
    'log in', 'login', 'sign in', 'signin', 'join discord', 'unlock more from x',
    'phone, email, or username', 'password', 'create account',
  ];

  function textOf(node) {
    return (node && (node.innerText || node.textContent) || '').replace(/\s+/gu, ' ').trim();
  }

  function attr(el, name) {
    const value = el.getAttribute && el.getAttribute(name);
    return value && value.trim() ? value.trim() : null;
  }

  function rect(el) {
    const box = el.getBoundingClientRect();
    return {
      x: Math.round(box.x),
      y: Math.round(box.y),
      w: Math.round(box.width),
      h: Math.round(box.height),
    };
  }

  function shortClass(el) {
    const value = typeof el.className === 'string' ? el.className : '';
    return value.split(/\s+/u).filter(Boolean).slice(0, 3).join('.');
  }

  function descriptor(el) {
    if (!el) return null;
    const bits = [el.localName || 'node'];
    const id = attr(el, 'id');
    const test = attr(el, 'data-testid') || attr(el, 'data-test-id') || attr(el, 'data-test');
    const role = attr(el, 'role');
    const label = attr(el, 'aria-label');
    const klass = shortClass(el);
    if (id) bits.push(`#${id}`);
    if (test) bits.push(`[test=${test}]`);
    if (role) bits.push(`[role=${role}]`);
    if (klass) bits.push(`.${klass}`);
    if (label) bits.push(`[aria=${label.slice(0, 80)}]`);
    const box = rect(el);
    bits.push(`[box=${box.x},${box.y},${box.w},${box.h}]`);
    return bits.join('');
  }

  function unique(values) {
    return [...new Set(values.filter((value) => value && String(value).trim()))];
  }

  function ariaReferenceText(el, attrName) {
    const ids = attr(el, attrName);
    if (!ids) return [];
    return ids.split(/\s+/u)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map(textOf)
      .filter(Boolean);
  }

  function screenReaderTexts(row) {
    const values = [];
    for (const el of [row, ...row.querySelectorAll('*')]) {
      values.push(attr(el, 'aria-label'));
      values.push(attr(el, 'alt'));
      values.push(attr(el, 'title'));
      values.push(...ariaReferenceText(el, 'aria-labelledby'));
      values.push(...ariaReferenceText(el, 'aria-describedby'));
      const klass = typeof el.className === 'string' ? el.className : '';
      if (/\b(sr-only|visually-hidden|screen-reader|a11y)\b/iu.test(klass)) values.push(textOf(el));
    }
    return unique(values);
  }

  function testsAndIds(row) {
    const values = [];
    for (const el of [row, ...row.querySelectorAll('*')]) {
      for (const name of ['data-testid', 'data-test-id', 'data-test', 'data-list-item-id', 'data-item-id', 'data-message-id', 'id']) {
        const value = attr(el, name);
        if (value) values.push(`${name}=${value}`);
      }
    }
    return unique(values).slice(0, 30);
  }

  function rolesAndStates(row) {
    const values = [];
    const stateAttrs = [
      'aria-busy', 'aria-checked', 'aria-current', 'aria-disabled', 'aria-expanded',
      'aria-haspopup', 'aria-hidden', 'aria-invalid', 'aria-live', 'aria-modal',
      'aria-pressed', 'aria-readonly', 'aria-selected',
    ];
    for (const el of [row, ...row.querySelectorAll('[role], [aria-busy], [aria-checked], [aria-current], [aria-disabled], [aria-expanded], [aria-haspopup], [aria-hidden], [aria-invalid], [aria-live], [aria-modal], [aria-pressed], [aria-readonly], [aria-selected], button, input, textarea')]) {
      const parts = [];
      const role = attr(el, 'role') || ({ A: 'link', BUTTON: 'button', IMG: 'img', INPUT: 'input', TEXTAREA: 'textbox' })[el.tagName];
      if (role) parts.push(`role=${role}`);
      for (const name of stateAttrs) {
        const value = attr(el, name);
        if (value !== null) parts.push(`${name}=${value}`);
      }
      if (el.disabled) parts.push('disabled=true');
      if (el.checked) parts.push('checked=true');
      if (parts.length) values.push(`${el.localName}:${parts.join('|')}`);
    }
    return unique(values).slice(0, 30);
  }

  function parentContainers(row) {
    const values = [];
    let current = row.parentElement;
    for (let depth = 0; current && depth < 5; depth += 1, current = current.parentElement) {
      values.push(descriptor(current));
    }
    return unique(values);
  }

  function boxesBeside(row) {
    const siblings = [row.previousElementSibling, row.nextElementSibling]
      .filter(Boolean)
      .map((el) => `sibling:${descriptor(el)}`);
    const childBoxes = Array.from(row.children)
      .filter((el) => {
        const box = el.getBoundingClientRect();
        return box.width > 0 && box.height > 0;
      })
      .slice(0, 8)
      .map((el) => `child:${descriptor(el)}`);
    return unique([...siblings, ...childBoxes]);
  }

  function pictureAddresses(row) {
    const values = [];
    for (const img of row.querySelectorAll('img, [style*="background-image"]')) {
      const src = img.currentSrc || img.src || '';
      if (src) values.push(src);
      const background = getComputedStyle(img).backgroundImage || '';
      for (const match of background.matchAll(/url\(["']?([^"')]+)["']?\)/gu)) values.push(match[1]);
    }
    return unique(values);
  }

  function accountLinks(row) {
    const values = [];
    for (const link of row.querySelectorAll('a[href]')) {
      const href = link.href || attr(link, 'href') || '';
      const label = attr(link, 'aria-label') || textOf(link);
      const handleOrNumber = /(?:^|[/?#&])@?[A-Za-z0-9_]{2,32}(?:$|[/?#&])|\b\d{5,}\b|\/(?:i\/user|users)\/\d+/u.test(href);
      if (handleOrNumber) values.push(label ? `${href} (${label})` : href);
    }
    return unique(values);
  }

  function whoWroteIt(row) {
    const values = [];
    for (const name of ['data-author-id', 'data-author-username', 'data-user-id', 'data-sender-id', 'data-sender', 'data-testid']) {
      const value = attr(row, name);
      if (value && !/message|cellinnerdiv|row/iu.test(value)) values.push(`${name}=${value}`);
    }
    for (const el of row.querySelectorAll('[data-author-id], [data-author-username], [data-user-id], [data-sender-id], [data-sender], [aria-label], img[alt], a[href]')) {
      for (const name of ['data-author-id', 'data-author-username', 'data-user-id', 'data-sender-id', 'data-sender']) {
        const value = attr(el, name);
        if (value) values.push(`${name}=${value}`);
      }
      const label = attr(el, 'aria-label') || attr(el, 'alt');
      if (label && /(?:from|sent by|message by|avatar|profile|@)/iu.test(label)) values.push(label);
      if (el.matches('a[href]')) {
        const href = el.href || attr(el, 'href') || '';
        if (/\/(?:users\/\d+|i\/user\/\d+|[A-Za-z0-9_]{2,32})(?:$|[/?#])/u.test(href)) {
          values.push(href);
        }
      }
    }
    const heading = row.querySelector('h2, h3, [class*="username"], [class*="author"], [data-testid*="User"], [data-testid*="user"]');
    if (heading) values.push(textOf(heading));
    return unique(values).slice(0, 12);
  }

  function isVisibleRow(el) {
    const box = el.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0) return false;
    const style = getComputedStyle(el);
    return style.visibility !== 'hidden' && style.display !== 'none';
  }

  function candidateRows() {
    const selectors = surface === 'discord'
      ? [
          '[data-author-id]',
          '[data-list-item-id*="chat-messages"]',
          '[id^="chat-messages-"]',
          '[class*="messageListItem"]',
          '[role="article"]',
          'li[class*="message"]',
        ]
      : [
          '[data-testid="conversationMessage"]',
          '[data-testid="messageEntry"]',
          '[data-testid="messageGroup"]',
          '[data-testid="cellInnerDiv"] [data-testid*="message"]',
          '[data-testid="cellInnerDiv"]',
          '[role="listitem"]',
          '[role="article"]',
        ];
    const rows = [];
    for (const selector of selectors) {
      for (const el of document.querySelectorAll(selector)) {
        if (!isVisibleRow(el)) continue;
        const text = textOf(el);
        const hasMessageSignals = testsAndIds(el).length > 0
          || screenReaderTexts(el).length > 0
          || pictureAddresses(el).length > 0
          || accountLinks(el).length > 0
          || text.length > 0;
        if (hasMessageSignals) rows.push(el);
      }
      if (rows.length >= limit) break;
    }
    return uniqueElements(rows).slice(0, limit);
  }

  function uniqueElements(elements) {
    const seen = new Set();
    const kept = [];
    for (const el of elements) {
      if (seen.has(el)) continue;
      if (kept.some((other) => other.contains(el))) continue;
      for (let index = kept.length - 1; index >= 0; index -= 1) {
        if (el.contains(kept[index])) kept.splice(index, 1);
      }
      seen.add(el);
      kept.push(el);
    }
    return kept;
  }

  function signedOutReason() {
    const body = textOf(document.body).toLowerCase();
    if (document.querySelector('input[type="password"], input[name="session[username_or_email]"], a[href*="/login"], a[href*="/i/flow/login"]')) {
      return 'signed-out login controls present';
    }
    const hit = signerHints.find((hint) => body.includes(hint));
    return hit ? `signed-out text present: ${hit}` : null;
  }

  const rows = candidateRows();
  const signout = signedOutReason();
  if (signout && rows.length === 0) {
    return { ok: false, reason: signout, title: document.title, rowCount: 0 };
  }
  if (rows.length === 0) {
    return { ok: false, reason: 'no readable direct-message rows found', title: document.title, rowCount: 0 };
  }
  const mapped = rows.map((row, index) => ({
    index: index + 1,
    testsAndIdentifiers: testsAndIds(row),
    whoWroteIt: whoWroteIt(row),
    rolesAndStates: rolesAndStates(row),
    parentContainers: parentContainers(row),
    boxesBeside: boxesBeside(row),
    pictureAddresses: pictureAddresses(row),
    accountLinks: accountLinks(row),
    screenReaderText: screenReaderTexts(row),
  }));
  return {
    ok: true,
    title: document.title,
    rowCount: mapped.length,
    rows: mapped,
    whoWroteRows: mapped.filter((row) => row.whoWroteIt.length > 0).length,
  };
}

const pageProbeSource = pageProbe.toString();

export async function runProbe(args) {
  const chrome = await launchChrome();
  try {
    const page = await chrome.openPage();
    try {
      await page.navigate(args.url, { timeoutMs: 20_000 });
      const result = await page.evaluate(`(${pageProbeSource})(${jsString({
        surface: args.surface,
        limit: args.limit,
      })})`);
      if (!result || result.ok !== true) {
        const reason = result?.reason || 'probe failed';
        const rowCount = Number.isInteger(result?.rowCount) ? result.rowCount : 'unknown';
        throw Object.assign(new Error(`${reason}; rows=${rowCount}; title=${result?.title || 'none'}`), {
          exitCode: 1,
        });
      }
      return result;
    } finally {
      await page.close();
    }
  } finally {
    await chrome.close();
  }
}

export function formatProbeOutput(args, result) {
  if (args.surface === 'discord') {
    return [
      `${result.whoWroteRows} discord_who_wrote_it_rows_of_${args.limit}`,
      `discord_rows_with_no_line ${Math.max(0, args.limit - result.rows.length)}`,
      ...result.rows.map((row) => formatRow('discord', row)),
    ].join('\n');
  }
  return [
    `x_page_read_date ${args.readDate}`,
    ...result.rows.map((row) => formatRow('x', row)),
    `x_rows_with_no_line ${Math.max(0, args.limit - result.rows.length)}`,
  ].join('\n');
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
    if (args.help) {
      console.log(usage());
      return;
    }
    const result = await runProbe(args);
    console.log(formatProbeOutput(args, result));
  } catch (error) {
    console.error(error.message);
    if (!args?.help) console.error(usage());
    process.exitCode = error.exitCode || 2;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
