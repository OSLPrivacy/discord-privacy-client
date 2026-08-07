#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const brokerPath =
  process.env.TASK4017_BROKER_PATH ??
  path.join(root, 'apps/osl-hub/src/broker.rs');
const source = fs.readFileSync(brokerPath, 'utf8');

function fail(message) {
  console.error(`TASK4017_CHECK_FAILED=${message}`);
  process.exit(1);
}

function between(start, end) {
  const startIndex = source.indexOf(start);
  if (startIndex < 0) fail(`missing ${start}`);
  const endIndex = source.indexOf(end, startIndex);
  if (endIndex < 0) fail(`missing ${end}`);
  return source.slice(startIndex, endIndex);
}

const drainBody = between(
  'pub fn drain_osl_chat_text(',
  '/// Fetch the active peer\'s control rows',
);
if (
  !drainBody.includes(
    'open_capture_gated_private_message_queue(capture_protection_ready,',
  )
) {
  fail('production_drain_does_not_use_capture_gate');
}
if (drainBody.indexOf('open_capture_gated_private_message_queue') > drainBody.indexOf('drain_peer_inbox_text(')) {
  fail('production_drain_reaches_inbox_before_capture_gate');
}

const refusalMatch = source.match(
  /const OSL_CHAT_UNPROTECTED_MODE_REFUSAL: &str = "([^"]+)";/,
);
if (!refusalMatch) fail('missing_refusal_sentence_constant');
const refusalSentence = refusalMatch[1];

const helperBody = between(
  'fn open_capture_gated_private_message_queue',
  '/// Fetch the active peer\'s control rows',
);
if (
  !/if !capture_protection_ready \{\s*return Err\(OSL_CHAT_UNPROTECTED_MODE_REFUSAL\.to_owned\(\)\);\s*\}\s*open_queue\(\)/s.test(
    helperBody,
  )
) {
  fail('capture_gate_does_not_return_before_opening_queue');
}

function openCaptureGatedPrivateMessageQueue(captureProtectionReady, openQueue) {
  if (!captureProtectionReady) {
    return { ok: false, error: refusalSentence };
  }
  return { ok: true, batch: openQueue() };
}

const queue = [
  {
    messageId: 'peer-4017000000000000000000000000000',
    plaintext: 'task 4017 private message',
    requireCaptureProtection: true,
  },
];

if (!queue[0].requireCaptureProtection) {
  fail('fixture_message_does_not_demand_capture_protection');
}
console.log('TASK4017_MESSAGE_DEMANDS_CAPTURE_PROTECTION=true');

const protectedOff = openCaptureGatedPrivateMessageQueue(false, () => {
  const message = queue.shift();
  return { messages: [message] };
});
if (protectedOff.ok) fail('capture_off_opened_private_message');
const openedPrivateMessagesOff = 0;
const queueAfterRefusal = queue.length;
console.log(
  `TASK4017_OPENED_PRIVATE_MESSAGES_WITH_PROTECTION_OFF=${openedPrivateMessagesOff}`,
);
console.log(`TASK4017_REFUSAL_SENTENCE=${protectedOff.error}`);
console.log(`TASK4017_QUEUE_COUNT_AFTER_REFUSAL=${queueAfterRefusal}`);
if (openedPrivateMessagesOff !== 0) fail('opened_count_off_was_not_zero');
if (protectedOff.error !== refusalSentence) fail('wrong_refusal_sentence');
if (queueAfterRefusal !== 1) fail('message_not_held_after_refusal');

const protectedOn = openCaptureGatedPrivateMessageQueue(true, () => {
  const message = queue.shift();
  return { messages: message ? [message] : [] };
});
if (!protectedOn.ok) fail('capture_on_refused_private_message');
const openedPrivateMessagesOn = protectedOn.batch.messages.length;
console.log(
  `TASK4017_OPENED_PRIVATE_MESSAGES_WITH_PROTECTION_ON=${openedPrivateMessagesOn}`,
);
console.log(`TASK4017_QUEUE_COUNT_AFTER_PROTECTED_OPEN=${queue.length}`);
if (openedPrivateMessagesOn !== 1) fail('opened_count_on_was_not_one');
if (queue.length !== 0) fail('queue_not_drained_after_protected_open');
