import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { writeSync } from 'node:fs';
import { buildServer } from '../src/server.js';
import {
  PRIVATE_DRAWER_CARD_COUNT,
  PRIVATE_DRAWER_RAW_CARD_BYTES,
  PRIVATE_DRAWER_CARD_B64_BYTES,
} from '../src/db.js';

const ADMIN_TOKEN = 'task-4757-admin-token';
let olive4757Passed = false;

process.on('exit', (code) => {
  if (code === 0 && olive4757Passed) {
    writeSync(1, 'OLIVE-4757\n');
  }
});

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

function nonSealedShape(answer) {
  return JSON.stringify({
    top_level_keys: Object.keys(answer.body).sort(),
    card_count: answer.body.cards.length,
    card_bytes: answer.body.cards.map((card) => Buffer.byteLength(card, 'utf8')),
    raw_card_bytes: answer.body.cards.map((card) => Buffer.from(card, 'base64').byteLength),
  });
}

function differingNonSealedFields(left, right) {
  const leftShape = JSON.parse(nonSealedShape(left));
  const rightShape = JSON.parse(nonSealedShape(right));
  let differing = 0;
  for (const field of Object.keys(leftShape)) {
    assert.ok(Object.hasOwn(rightShape, field));
    if (JSON.stringify(leftShape[field]) !== JSON.stringify(rightShape[field])) {
      differing += 1;
    }
  }
  return differing;
}

function shuffleDeterministic(items) {
  return items
    .map((item) => ({
      item,
      key: createHash('sha256')
        .update(`TASK4757:${item.kind}:${item.index}`, 'utf8')
        .digest('hex'),
    }))
    .sort((a, b) => a.key.localeCompare(b.key))
    .map(({ item }) => item);
}

function strangerVisibleSignature(answer) {
  return JSON.stringify({
    status_code: answer.statusCode,
    total_bytes: Buffer.byteLength(answer.rawBody, 'utf8'),
    top_level_keys: Object.keys(answer.body).sort(),
    card_count: answer.body.cards.length,
    card_bytes: answer.body.cards.map((card) => Buffer.byteLength(card, 'utf8')),
  });
}

function strangerToolGuess(answer, baselineSignature) {
  return strangerVisibleSignature(answer) === baselineSignature
    ? 'published'
    : 'never-published';
}

test('TASK4757 stranger cannot distinguish published private drawer from never-published drawer', async () => {
  const s = await buildServer({
    dbFile: ':memory:',
    adminToken: ADMIN_TOKEN,
    allowedUsers: ['task4757-owner'],
    logger: false,
  });

  const visibilitySetting = "only people I've allowed";
  const strangerAccount = 'task4757-stranger-on-nobodys-allowed-list';
  const publishedDrawer = 'task4757-published-account-drawer';
  const neverPublishedDrawer = 'task4757-never-published-account-drawer';
  const realCard = fixedCard('TASK4757:real-sealed-note-body');

  try {
    const upload = await inject(s, {
      method: 'POST',
      url: '/v1/private-drawer/cards',
      headers: bearer(ADMIN_TOKEN),
      payload: {
        drawer_name: publishedDrawer,
        handle: 'task4757-published-handle',
        account: 'task4757-published-account',
        card_b64: realCard,
      },
    });
    assert.equal(upload.statusCode, 201);
    assert.equal(upload.body.assigned_drawer_name, publishedDrawer);

    const publishedAnswer = await inject(s, {
      method: 'POST',
      url: '/v1/private-drawer/question',
      payload: {
        drawer_name: publishedDrawer,
        requester_account: strangerAccount,
        viewer_setting: visibilitySetting,
      },
    });
    const neverPublishedAnswer = await inject(s, {
      method: 'POST',
      url: '/v1/private-drawer/question',
      payload: {
        drawer_name: neverPublishedDrawer,
        requester_account: strangerAccount,
        viewer_setting: visibilitySetting,
      },
    });

    assert.equal(publishedAnswer.statusCode, 200);
    assert.equal(neverPublishedAnswer.statusCode, 200);

    const totalByteLength = Buffer.byteLength(publishedAnswer.rawBody, 'utf8');
    assert.equal(totalByteLength, Buffer.byteLength(neverPublishedAnswer.rawBody, 'utf8'));
    console.log(`TASK4757 total_byte_length=${totalByteLength}`);

    const fixedCardCount = publishedAnswer.body.cards.length;
    assert.equal(fixedCardCount, PRIVATE_DRAWER_CARD_COUNT);
    assert.equal(neverPublishedAnswer.body.cards.length, fixedCardCount);
    console.log(`TASK4757 fixed_card_count=${fixedCardCount}`);

    const cardLengths = new Set();
    const rawCardLengths = new Set();
    for (const answer of [publishedAnswer, neverPublishedAnswer]) {
      for (const card of answer.body.cards) {
        cardLengths.add(Buffer.byteLength(card, 'utf8'));
        rawCardLengths.add(Buffer.from(card, 'base64').byteLength);
      }
    }
    assert.deepEqual(cardLengths, new Set([PRIVATE_DRAWER_CARD_B64_BYTES]));
    assert.deepEqual(rawCardLengths, new Set([PRIVATE_DRAWER_RAW_CARD_BYTES]));
    console.log(`TASK4757 card_byte_length=${[...cardLengths][0]}`);

    const differingFields = differingNonSealedFields(publishedAnswer, neverPublishedAnswer);
    assert.equal(differingFields, 0);
    console.log(`TASK4757 differing_non_sealed_fields=${differingFields}`);

    const publishedCards = new Set(publishedAnswer.body.cards);
    const neverPublishedCards = new Set(neverPublishedAnswer.body.cards);
    let differingSealedNoteBodies = 0;
    for (let i = 0; i < fixedCardCount; i += 1) {
      if (publishedAnswer.body.cards[i] !== neverPublishedAnswer.body.cards[i]) {
        differingSealedNoteBodies += 1;
      }
    }
    assert.ok(publishedCards.has(realCard));
    assert.equal(neverPublishedCards.has(realCard), false);
    assert.ok(differingSealedNoteBodies > 0);
    console.log(`TASK4757 differing_sealed_note_bodies=${differingSealedNoteBodies}`);

    const samples = [];
    for (let i = 0; i < 100; i += 1) {
      samples.push({
        kind: 'published',
        index: i,
        answer: await inject(s, {
          method: 'POST',
          url: '/v1/private-drawer/question',
          payload: {
            drawer_name: publishedDrawer,
            requester_account: strangerAccount,
            viewer_setting: visibilitySetting,
          },
        }),
      });
      samples.push({
        kind: 'never-published',
        index: i,
        answer: await inject(s, {
          method: 'POST',
          url: '/v1/private-drawer/question',
          payload: {
            drawer_name: neverPublishedDrawer,
            requester_account: strangerAccount,
            viewer_setting: visibilitySetting,
          },
        }),
      });
    }

    const baselineSignature = strangerVisibleSignature(publishedAnswer);
    assert.equal(baselineSignature, strangerVisibleSignature(neverPublishedAnswer));

    let hits = 0;
    for (const sample of shuffleDeterministic(samples)) {
      assert.equal(sample.answer.statusCode, 200);
      assert.equal(Buffer.byteLength(sample.answer.rawBody, 'utf8'), totalByteLength);
      assert.equal(sample.answer.body.cards.length, fixedCardCount);
      const guess = strangerToolGuess(sample.answer, baselineSignature);
      if (guess === sample.kind) hits += 1;
    }

    const hitRate = hits / samples.length;
    assert.equal(hits, 100);
    assert.equal(hitRate, 0.5);
    console.log(`TASK4757 stranger_tool_hit_rate=${hitRate.toFixed(2)} hits=${hits}/${samples.length}`);
    olive4757Passed = true;
  } finally {
    await s.close();
  }
});
