import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { buildServer } from '../src/server.js';
import {
  PRIVATE_DRAWER_CARD_COUNT,
  PRIVATE_DRAWER_RAW_CARD_BYTES,
  PRIVATE_DRAWER_CARD_B64_BYTES,
} from '../src/db.js';

const ADMIN_TOKEN = 'task-4753-admin-token';

function bearer(token) {
  return { authorization: `Bearer ${token}` };
}

function fixedCard(seed) {
  return createHash('sha512').update(seed, 'utf8').digest('base64');
}

async function inject(server, opts) {
  const res = await server.inject(opts);
  let body = res.body;
  try {
    body = JSON.parse(res.body);
  } catch {
    /* leave as string */
  }
  return {
    statusCode: res.statusCode,
    rawBody: res.body,
    body,
  };
}

function forbiddenLogPattern(publishedRows) {
  const sensitive = publishedRows.flatMap((row) => [row.handle, row.account]);
  const escaped = sensitive.map((value) =>
    value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
  );
  return new RegExp(
    `(?:${escaped.join('|')})|\\b(?:yes|no|true|false|match|matched|missing|found)\\b`,
    'i',
  );
}

test('TASK4753 fixed-size private drawer answers are padded and log-silent', async () => {
  const logLines = [];
  const s = await buildServer({
    dbFile: ':memory:',
    adminToken: ADMIN_TOKEN,
    logger: {
      level: 'info',
      stream: {
        write(line) {
          logLines.push(line.trimEnd());
        },
      },
    },
  });

  const drawers = [
    { name: 'task4753-drawer-empty', realCount: 0 },
    { name: 'task4753-drawer-one', realCount: 1 },
    { name: 'task4753-drawer-seven', realCount: 7 },
    { name: 'task4753-drawer-forty', realCount: 40 },
  ];
  const publishedByDrawer = new Map();
  const publishedRows = [];
  let overflowSplitDrawers = 0;
  let overflowSplitExtraChars = 0;
  let overflowRecoveredCards = 0;

  try {
    for (const drawer of drawers) {
      const published = [];
      for (let i = 0; i < drawer.realCount; i += 1) {
        const row = {
          drawer_name: drawer.name,
          handle: `handle4753_${drawer.realCount}_${i}`,
          account: `account4753_${drawer.realCount}_${i}`,
          card_b64: fixedCard(`TASK4753:${drawer.name}:${i}`),
        };
        published.push(row.card_b64);
        publishedRows.push(row);
        const upload = await inject(s, {
          method: 'POST',
          url: '/v1/private-drawer/cards',
          headers: bearer(ADMIN_TOKEN),
          payload: row,
        });
        assert.equal(upload.statusCode, 201);
        assert.equal(upload.body.assigned_drawer_name, drawer.name);
      }
      publishedByDrawer.set(drawer.name, new Set(published));
    }

    const overflowRoot = 'task4753-overflow-root';
    const overflowPublished = [];
    const overflowAssignedDrawers = new Map();
    for (let i = 0; i < PRIVATE_DRAWER_CARD_COUNT + 1; i += 1) {
      const row = {
        drawer_name: overflowRoot,
        handle: `handle4753_overflow_${i}`,
        account: `account4753_overflow_${i}`,
        card_b64: fixedCard(`TASK4753:${overflowRoot}:${i}`),
      };
      publishedRows.push(row);
      overflowPublished.push(row.card_b64);
      const upload = await inject(s, {
        method: 'POST',
        url: '/v1/private-drawer/cards',
        headers: bearer(ADMIN_TOKEN),
        payload: row,
      });
      assert.equal(upload.statusCode, 201);
    }
    for (const card of overflowPublished) {
      const assigned = `${overflowRoot}:${card.slice(0, 1)}`;
      const cards = overflowAssignedDrawers.get(assigned) ?? new Set();
      cards.add(card);
      overflowAssignedDrawers.set(assigned, cards);
    }
    overflowSplitDrawers = overflowAssignedDrawers.size;
    const overflowExtraCharCounts = new Set(
      [...overflowAssignedDrawers.keys()].map(
        (drawerName) => drawerName.slice(`${overflowRoot}:`.length).length,
      ),
    );
    assert.deepEqual(overflowExtraCharCounts, new Set([1]));
    overflowSplitExtraChars = [...overflowExtraCharCounts][0];

    const recoveredOverflow = new Set();
    for (const [assignedDrawer, expectedCards] of overflowAssignedDrawers) {
      const answer = await inject(s, {
        method: 'POST',
        url: '/v1/private-drawer/question',
        payload: { drawer_name: assignedDrawer },
      });
      assert.equal(answer.statusCode, 200);
      assert.equal(answer.body.cards.length, PRIVATE_DRAWER_CARD_COUNT);
      const returnedCards = new Set(answer.body.cards);
      for (const card of expectedCards) {
        if (returnedCards.has(card)) recoveredOverflow.add(card);
      }
    }
    assert.equal(recoveredOverflow.size, overflowPublished.length);
    overflowRecoveredCards = recoveredOverflow.size;

    logLines.length = 0;

    const distinctCardCounts = new Set();
    const distinctTotalByteLengths = new Set();
    const allPublishedCards = new Set(
      [...publishedByDrawer.values()].flatMap((cards) => [...cards]),
    );
    let droppedRealCards = 0;
    let questionCount = 0;
    let fakeCard = null;
    let realCard = null;

    for (const drawer of drawers) {
      const expectedRealCards = publishedByDrawer.get(drawer.name);
      for (let i = 0; i < 50; i += 1) {
        const answer = await inject(s, {
          method: 'POST',
          url: '/v1/private-drawer/question',
          payload: { drawer_name: drawer.name },
        });
        assert.equal(answer.statusCode, 200);
        assert.ok(Array.isArray(answer.body.cards));
        distinctCardCounts.add(answer.body.cards.length);
        distinctTotalByteLengths.add(Buffer.byteLength(answer.rawBody, 'utf8'));
        assert.equal(answer.body.cards.length, PRIVATE_DRAWER_CARD_COUNT);

        const returnedCards = new Set(answer.body.cards);
        for (const card of answer.body.cards) {
          assert.equal(Buffer.byteLength(card, 'utf8'), PRIVATE_DRAWER_CARD_B64_BYTES);
          assert.equal(Buffer.from(card, 'base64').byteLength, PRIVATE_DRAWER_RAW_CARD_BYTES);
        }
        for (const card of expectedRealCards) {
          if (!returnedCards.has(card)) droppedRealCards += 1;
        }

        if (!fakeCard) {
          fakeCard = answer.body.cards.find((card) => !allPublishedCards.has(card)) ?? null;
        }
        if (!realCard) {
          realCard = answer.body.cards.find((card) => allPublishedCards.has(card)) ?? null;
        }
        questionCount += 1;
      }
    }

    assert.equal(questionCount, 200);
    assert.equal(distinctCardCounts.size, 1);
    assert.equal(distinctTotalByteLengths.size, 1);
    assert.equal(droppedRealCards, 0);
    assert.ok(fakeCard);
    assert.ok(realCard);
    assert.equal(Buffer.byteLength(fakeCard, 'utf8'), Buffer.byteLength(realCard, 'utf8'));
    assert.equal(Buffer.from(fakeCard, 'base64').byteLength, Buffer.from(realCard, 'base64').byteLength);

    const forbidden = forbiddenLogPattern(publishedRows);
    const forbiddenLogLines = logLines.filter((line) => forbidden.test(line));
    assert.equal(forbiddenLogLines.length, 0, forbiddenLogLines.join('\n'));

    console.log(`TASK4753 question_count=${questionCount}`);
    console.log(
      `TASK4753 distinct_card_counts=${distinctCardCounts.size} values=${[...distinctCardCounts].join(',')}`,
    );
    console.log(
      `TASK4753 distinct_total_byte_lengths=${distinctTotalByteLengths.size} values=${[...distinctTotalByteLengths].join(',')}`,
    );
    console.log(`TASK4753 dropped_real_cards=${droppedRealCards}`);
    console.log(
      `TASK4753 real_card_utf8_bytes=${Buffer.byteLength(realCard, 'utf8')} fake_card_utf8_bytes=${Buffer.byteLength(fakeCard, 'utf8')}`,
    );
    console.log(
      `TASK4753 real_card_raw_bytes=${Buffer.from(realCard, 'base64').byteLength} fake_card_raw_bytes=${Buffer.from(fakeCard, 'base64').byteLength}`,
    );
    console.log(
      `TASK4753 overflow_split_drawers=${overflowSplitDrawers} overflow_split_extra_chars=${overflowSplitExtraChars} overflow_recovered_cards=${overflowRecoveredCards}`,
    );
    console.log(
      'TASK4753 forbidden_log_search=published handles/accounts or yes/no/true/false/match/matched/missing/found',
    );
    console.log(`TASK4753 captured_question_log_lines=${logLines.length}`);
    console.log(`TASK4753 forbidden_log_lines=${forbiddenLogLines.length}`);
  } finally {
    await s.close();
  }
});
