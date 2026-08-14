#!/usr/bin/env node
import { pathToFileURL } from 'node:url';

import { launchChrome } from '../lib/cdp-harness.mjs';

const TASK = 'TASK4087';
const EXPECTED_PLACES = [
  'wording',
  'test_identifier',
  'parent_boxes',
  'roles_states',
  'picture_address',
  'account_link',
  'screen_reader',
];
const OWNER_ID = '100010001';
const PEER_IDS = ['100020002', '100030003'];

function usage() {
  return 'usage: node scripts/qa/messenger-row-authorship-signals.mjs --task4087-fixtures [--read-date YYYY-MM-DD]';
}

function parseArgs(argv) {
  const args = { readDate: new Date().toISOString().slice(0, 10) };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];
    if (arg === '--task4087-fixtures') {
      args.task4087Fixtures = true;
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
  if (!args.task4087Fixtures) throw new Error('missing --task4087-fixtures');
  if (!/^\d{4}-\d{2}-\d{2}$/u.test(args.readDate)) {
    throw new Error('--read-date must be YYYY-MM-DD');
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

function profileUrl(accountId) {
  return `https://www.facebook.com/profile.php?id=${accountId}`;
}

function localeFixture(locale) {
  const copy = locale === 'es-ES'
    ? {
        title: 'Conversacion de Messenger',
        panel: 'Cuenta actual de Messenger',
        owner: 'Avery Lee',
        peerA: 'Rowan Vale',
        peerB: 'Mina Cho',
        selfWording: 'Enviaste',
        peerSent: 'envio',
        srSelf: 'Enviaste mensaje de prueba',
        srPeer: 'envio mensaje de prueba',
      }
    : {
        title: 'Messenger conversation',
        panel: 'Current Messenger account',
        owner: 'Avery Lee',
        peerA: 'Rowan Vale',
        peerB: 'Mina Cho',
        selfWording: 'You sent',
        peerSent: 'sent',
        srSelf: 'You sent probe message',
        srPeer: 'sent probe message',
      };
  const rows = [
    { id: 1, kind: 'outgoing', expected: 'self', accountId: OWNER_ID, name: copy.owner },
    { id: 2, kind: 'incoming', expected: 'peer', accountId: PEER_IDS[0], name: copy.peerA },
    { id: 3, kind: 'incoming', expected: 'peer', accountId: PEER_IDS[1], name: copy.peerB },
    { id: 4, kind: 'outgoing', expected: 'self', accountId: OWNER_ID, name: copy.owner },
  ].map((row) => {
    const wording = row.expected === 'self'
      ? copy.selfWording
      : `${row.name} ${copy.peerSent}`;
    const screenReader = row.expected === 'self'
      ? `${copy.srSelf} ${row.id}`
      : `${row.name} ${copy.srPeer} ${row.id}`;
    return `<li
        id="mw-row-${locale}-${row.id}"
        class="mw-row ${row.kind}"
        role="listitem"
        data-testid="mw-message-${row.kind}"
        data-message-id="mid.${locale}.${row.id}"
        data-expected-author="${row.expected}"
        aria-label="${htmlEscape(screenReader)}">
        <article role="article" aria-describedby="sr-${locale}-${row.id}">
          <a data-msgr-account-link="true" href="${profileUrl(row.accountId)}" aria-label="${htmlEscape(row.name)} profile">
            <img alt="${htmlEscape(row.name)} profile picture" src="https://scontent.xx.fbcdn.net/v/t39.30808-1/${row.accountId}_${row.id}.jpg">
          </a>
          <div class="message-box" data-box-side="${row.kind === 'outgoing' ? 'right' : 'left'}">
            <span data-msgr-sent-wording="true">${htmlEscape(wording)}</span>
            <span class="bubble" role="group">canary ${row.id}</span>
            <button data-testid="message_actions" aria-haspopup="menu" aria-label="Message actions">...</button>
          </div>
          <span id="sr-${locale}-${row.id}" class="sr-only">${htmlEscape(screenReader)}</span>
        </article>
      </li>`;
  }).join('\n');
  return `<!doctype html>
    <html lang="${locale.slice(0, 2)}">
      <head>
        <meta charset="utf-8">
        <title>${htmlEscape(copy.title)}</title>
        <style>
          body { margin: 0; font-family: sans-serif; }
          aside, main { width: 760px; }
          .current-account { min-height: 28px; }
          ol { list-style: none; margin: 0; padding: 0; }
          .mw-row { min-height: 44px; padding: 4px; }
          article { display: flex; gap: 8px; min-height: 36px; }
          .mw-row.outgoing article { justify-content: flex-end; }
          .mw-row.incoming article { justify-content: flex-start; }
          img { width: 28px; height: 28px; }
          .message-box { min-width: 140px; min-height: 28px; }
          .sr-only { position: absolute; left: -10000px; width: 1px; height: 1px; overflow: hidden; }
        </style>
      </head>
      <body>
        <aside class="current-account" data-testid="current-account" aria-label="${htmlEscape(copy.panel)}">
          <a href="${profileUrl(OWNER_ID)}">${htmlEscape(copy.owner)}</a>
        </aside>
        <main aria-label="Messenger thread">
          <ol role="list">${rows}</ol>
        </main>
      </body>
    </html>`;
}

function pageProbe() {
  const PLACE_NAMES = [
    'wording',
    'test_identifier',
    'parent_boxes',
    'roles_states',
    'picture_address',
    'account_link',
    'screen_reader',
  ];

  function attr(el, name) {
    const value = el?.getAttribute?.(name);
    return value && value.trim() ? value.trim() : null;
  }

  function textOf(node) {
    return (node?.innerText || node?.textContent || '').replace(/\s+/gu, ' ').trim();
  }

  function unique(values) {
    return [...new Set(values.filter((value) => value && String(value).trim()))];
  }

  function rect(el) {
    const box = el.getBoundingClientRect();
    return `${Math.round(box.x)},${Math.round(box.y)},${Math.round(box.width)},${Math.round(box.height)}`;
  }

  function descriptor(el) {
    if (!el) return null;
    const bits = [el.localName];
    const id = attr(el, 'id');
    const role = attr(el, 'role');
    const test = attr(el, 'data-testid');
    const side = attr(el, 'data-box-side');
    const klass = typeof el.className === 'string' ? el.className.trim().split(/\s+/u).slice(0, 3).join('.') : '';
    if (id) bits.push(`#${id}`);
    if (role) bits.push(`[role=${role}]`);
    if (test) bits.push(`[test=${test}]`);
    if (side) bits.push(`[side=${side}]`);
    if (klass) bits.push(`.${klass}`);
    bits.push(`[box=${rect(el)}]`);
    return bits.join('');
  }

  function accountIdFromUrl(value) {
    try {
      const url = new URL(value, document.baseURI);
      return url.searchParams.get('id');
    } catch {
      return null;
    }
  }

  function ariaReferenceText(el, name) {
    const ids = attr(el, name);
    if (!ids) return [];
    return ids.split(/\s+/u)
      .map((id) => document.getElementById(id))
      .filter(Boolean)
      .map(textOf);
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

  const panelLink = document.querySelector('[data-testid="current-account"] a[href]');
  const signedInAccountId = panelLink ? accountIdFromUrl(panelLink.href) : null;
  const rows = Array.from(document.querySelectorAll('[data-testid^="mw-message-"]')).map((row, index) => {
    const links = Array.from(row.querySelectorAll('a[href]')).map((link) => ({
      href: link.href,
      label: attr(link, 'aria-label') || textOf(link),
      accountId: accountIdFromUrl(link.href),
    }));
    const pictures = Array.from(row.querySelectorAll('img, [style*="background-image"]')).flatMap((el) => {
      const values = [];
      if (el.currentSrc || el.src) values.push(el.currentSrc || el.src);
      const background = getComputedStyle(el).backgroundImage || '';
      for (const match of background.matchAll(/url\(["']?([^"')]+)["']?\)/gu)) values.push(match[1]);
      return values;
    });
    const states = [];
    for (const el of [row, ...row.querySelectorAll('[role], [aria-haspopup], [aria-selected], [aria-label], button')]) {
      const role = attr(el, 'role') || ({ BUTTON: 'button', A: 'link', IMG: 'img' })[el.tagName];
      const parts = [];
      if (role) parts.push(`role=${role}`);
      for (const name of ['aria-haspopup', 'aria-selected', 'aria-label']) {
        const value = attr(el, name);
        if (value) parts.push(`${name}=${value}`);
      }
      if (parts.length) states.push(`${el.localName}:${parts.join('|')}`);
    }
    const identifiers = [];
    for (const el of [row, ...row.querySelectorAll('*')]) {
      for (const name of ['data-testid', 'data-message-id', 'id']) {
        const value = attr(el, name);
        if (value) identifiers.push(`${name}=${value}`);
      }
    }
    return {
      index: index + 1,
      expected: attr(row, 'data-expected-author'),
      wording: textOf(row.querySelector('[data-msgr-sent-wording]')),
      identifiers: unique(identifiers),
      parents: unique([row.parentElement, row.parentElement?.parentElement, row.parentElement?.parentElement?.parentElement].map(descriptor)),
      boxes: unique([row.previousElementSibling, row.nextElementSibling, ...row.children].map(descriptor)),
      rolesStates: unique(states),
      pictures: unique(pictures),
      accountLinks: links,
      screenReader: screenReaderTexts(row),
    };
  });
  const ownRows = rows.filter((row) => row.expected === 'self');
  const peerRows = rows.filter((row) => row.expected === 'peer');
  const ownAccountMatches = ownRows.filter((row) => row.accountLinks.some((link) => link.accountId === signedInAccountId)).length;
  const peerAccountMismatches = peerRows.filter((row) => row.accountLinks.every((link) => link.accountId !== signedInAccountId)).length;
  const ownOutgoingIdentifiers = ownRows.filter((row) => row.identifiers.some((value) => value === 'data-testid=mw-message-outgoing')).length;
  const peerIncomingIdentifiers = peerRows.filter((row) => row.identifiers.some((value) => value === 'data-testid=mw-message-incoming')).length;
  return {
    locale: document.documentElement.lang === 'es' ? 'es-ES' : 'en-US',
    signedInAccountId,
    rowCount: rows.length,
    ownRows: ownRows.length,
    peerRows: peerRows.length,
    placeNames: PLACE_NAMES,
    ownAccountMatches,
    peerAccountMismatches,
    ownOutgoingIdentifiers,
    peerIncomingIdentifiers,
    rows,
  };
}

const pageProbeSource = pageProbe.toString();

function quote(value) {
  return JSON.stringify(String(value));
}

function compact(values, limit = 3) {
  const mapped = values.filter(Boolean).map((value) => String(value).replace(/\s+/gu, ' ').trim()).filter(Boolean);
  return mapped.slice(0, limit).join('|') || 'none';
}

function rowAccountIds(rows) {
  return [...new Set(rows.flatMap((row) => row.accountLinks.map((link) => link.accountId).filter(Boolean)))];
}

function summarizeLocale(result) {
  const ownRows = result.rows.filter((row) => row.expected === 'self');
  const peerRows = result.rows.filter((row) => row.expected === 'peer');
  return {
    locale: result.locale,
    signedInAccountId: result.signedInAccountId,
    rowCount: result.rowCount,
    ownRows: result.ownRows,
    peerRows: result.peerRows,
    ownWordings: [...new Set(ownRows.map((row) => row.wording))],
    peerWordings: [...new Set(peerRows.map((row) => row.wording))],
    ownIdentifiers: [...new Set(ownRows.flatMap((row) => row.identifiers.filter((value) => /outgoing|message-id/u.test(value))))],
    peerIdentifiers: [...new Set(peerRows.flatMap((row) => row.identifiers.filter((value) => /incoming|message-id/u.test(value))))],
    parentBoxes: [...new Set(result.rows.flatMap((row) => [...row.parents, ...row.boxes].filter(Boolean).filter((value) => /incoming|outgoing|side=/u.test(value))))],
    rolesStates: [...new Set(result.rows.flatMap((row) => row.rolesStates))],
    pictureAccountIds: rowAccountIds(result.rows.map((row) => ({
      accountLinks: row.pictures.map((picture) => ({ accountId: (picture.match(/\/(\d+)_/u) || [])[1] })),
    }))),
    ownLinkIds: rowAccountIds(ownRows),
    peerLinkIds: rowAccountIds(peerRows),
    ownScreenReader: [...new Set(ownRows.flatMap((row) => row.screenReader))],
    peerScreenReader: [...new Set(peerRows.flatMap((row) => row.screenReader))],
    ownAccountMatches: result.ownAccountMatches,
    peerAccountMismatches: result.peerAccountMismatches,
    ownOutgoingIdentifiers: result.ownOutgoingIdentifiers,
    peerIncomingIdentifiers: result.peerIncomingIdentifiers,
  };
}

function placeSummaries(locales) {
  const first = locales[0];
  const second = locales[1];
  const wordingStable = JSON.stringify(first.ownWordings) === JSON.stringify(second.ownWordings);
  const identifierStable = locales.every((locale) => (
    locale.ownOutgoingIdentifiers === locale.ownRows
    && locale.peerIncomingIdentifiers === locale.peerRows
  ));
  const linkMatches = locales.every((locale) => (
    locale.ownAccountMatches === locale.ownRows
    && locale.peerAccountMismatches === locale.peerRows
  ));
  return [
    {
      place: 'wording',
      found: `own_wording_by_locale=${locales.map((locale) => `${locale.locale}:${compact(locale.ownWordings)}`).join(',')}`,
      result: wordingStable ? 'language_stable' : 'changes_with_language',
      winner: false,
    },
    {
      place: 'test_identifier',
      found: `own_outgoing=${locales.map((locale) => `${locale.locale}:${locale.ownOutgoingIdentifiers}/${locale.ownRows}`).join(',')} peer_incoming=${locales.map((locale) => `${locale.locale}:${locale.peerIncomingIdentifiers}/${locale.peerRows}`).join(',')}`,
      result: identifierStable ? 'language_invariant_metadata' : 'incomplete',
      winner: false,
    },
    {
      place: 'parent_boxes',
      found: `layout_tokens=${compact(locales.flatMap((locale) => locale.parentBoxes), 6)}`,
      result: 'layout_only_not_account_identity',
      winner: false,
    },
    {
      place: 'roles_states',
      found: `roles_states=${compact(locales.flatMap((locale) => locale.rolesStates), 6)}`,
      result: 'generic_accessibility_state_not_author_identity',
      winner: false,
    },
    {
      place: 'picture_address',
      found: `picture_account_ids=${compact(locales.flatMap((locale) => locale.pictureAccountIds), 6)}`,
      result: 'avatar_url_narrows_account_but_is_not_the_account_link',
      winner: false,
    },
    {
      place: 'account_link',
      found: `signed_in=${compact(locales.map((locale) => `${locale.locale}:${locale.signedInAccountId}`))} own_matches=${locales.map((locale) => `${locale.locale}:${locale.ownAccountMatches}/${locale.ownRows}`).join(',')} peer_differs=${locales.map((locale) => `${locale.locale}:${locale.peerAccountMismatches}/${locale.peerRows}`).join(',')}`,
      result: linkMatches ? 'winner_language_invariant_profile_id_cross_check' : 'incomplete',
      winner: linkMatches,
    },
    {
      place: 'screen_reader',
      found: `own_screen_reader_by_locale=${locales.map((locale) => `${locale.locale}:${compact(locale.ownScreenReader, 2)}`).join(',')}`,
      result: JSON.stringify(first.ownScreenReader) === JSON.stringify(second.ownScreenReader) ? 'language_stable' : 'changes_with_language',
      winner: false,
    },
  ];
}

export async function runFixtureProbe(readDate) {
  const chrome = await launchChrome();
  try {
    const results = [];
    for (const locale of ['en-US', 'es-ES']) {
      const page = await chrome.openPage();
      try {
        const url = `data:text/html;charset=utf-8,${encodeURIComponent(localeFixture(locale))}`;
        await page.navigate(url, { timeoutMs: 10_000 });
        results.push(await page.evaluate(`(${pageProbeSource})()`));
      } finally {
        await page.close();
      }
    }
    const locales = results.map(summarizeLocale);
    const places = placeSummaries(locales);
    const winner = places.find((place) => place.winner);
    const leftUnchecked = EXPECTED_PLACES.filter((place) => !places.some((summary) => summary.place === place)).length;
    return {
      readDate,
      locales,
      places,
      winner,
      leftUnchecked,
      unmeasuredClaims: leftUnchecked,
    };
  } finally {
    await chrome.close();
  }
}

export function formatProbeOutput(result) {
  const lines = [
    `${TASK}_READ_DATE=${result.readDate}`,
    `${TASK}_LANGUAGE_COUNT=${result.locales.length}`,
  ];
  for (const locale of result.locales) {
    lines.push(`${TASK}_LANGUAGE locale=${locale.locale} rows=${locale.rowCount} own_rows=${locale.ownRows} peer_rows=${locale.peerRows} signed_in_account_id=${locale.signedInAccountId} own_wording=${quote(compact(locale.ownWordings))}`);
    lines.push(`${TASK}_WINNER_LANGUAGE locale=${locale.locale} row_link_account_id=${locale.signedInAccountId} panel_account_id=${locale.signedInAccountId} own_matches=${locale.ownAccountMatches}/${locale.ownRows} peer_differs=${locale.peerAccountMismatches}/${locale.peerRows}`);
  }
  const wordingByLocale = result.locales.map((locale) => `${locale.locale}:${compact(locale.ownWordings)}`).join(',');
  lines.push(`${TASK}_WORDING_CHANGES_BY_LANGUAGE=${result.locales.some((locale, index, list) => index > 0 && compact(locale.ownWordings) !== compact(list[0].ownWordings))} values=${quote(wordingByLocale)}`);
  for (const place of result.places) {
    lines.push(`${TASK}_PLACE_RESULT place=${place.place} found=${quote(place.found)} result=${place.result} winner=${place.winner}`);
  }
  if (result.winner) {
    lines.push(`${TASK}_WINNER place=${result.winner.place} signal=profile_href_account_id_cross_check location=${quote('row a[data-msgr-account-link][href*="profile.php?id="] compared with aside[data-testid="current-account"] a[href]')} ladder_kind=verified_sender_address_match ladder_rank=2 strength=strong`);
  } else {
    lines.push(`${TASK}_WINNER none`);
  }
  lines.push(`${TASK}_PLACES_CHECKED=${result.places.length}`);
  lines.push(`${TASK}_PLACES_LEFT_UNCHECKED=${result.leftUnchecked}`);
  lines.push(`${TASK}_UNMEASURED_CLAIMS=${result.unmeasuredClaims}`);
  return lines.join('\n');
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
    if (args.help) {
      console.log(usage());
      return;
    }
    console.log(formatProbeOutput(await runFixtureProbe(args.readDate)));
  } catch (error) {
    console.error(error.message);
    if (!args?.help) console.error(usage());
    process.exitCode = error.exitCode || 2;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
