import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractPath = new URL("./transport-frames.md", import.meta.url);
const source = readFileSync(contractPath, "utf8");
const vectorMatch = source.match(/## 5\. T1-T50 frame vectors[\s\S]*?```json\s+([\s\S]*?)\s+```/u);

assert.ok(vectorMatch, "frame vectors must be present");
const vectors = JSON.parse(vectorMatch[1]);

const RAW_BYTES = 1536;
const HEADER_BYTES = 64;
const TICK_SLOTS = 92;
const RESPONSE_SLOTS = 46;

function u64(value) {
  const encoded = Buffer.alloc(8);
  encoded.writeBigUInt64BE(BigInt(value));
  return encoded;
}

function encodeFrame({ direction, sessionId, frameId, ackId, entries }) {
  const slotBytes = direction === 0 ? 16 : 32;
  const maxEntries = direction === 0 ? TICK_SLOTS : RESPONSE_SLOTS;
  assert.ok(entries.length <= maxEntries, "entry count exceeds fixed slot capacity");

  const frame = Buffer.alloc(RAW_BYTES);
  frame.write("OSLF", 0, "ascii");
  frame[4] = 1;
  frame[5] = direction;
  Buffer.from(sessionId, "hex").copy(frame, 8);
  u64(frameId).copy(frame, 24);
  u64(ackId).copy(frame, 32);
  frame[40] = entries.length;

  entries.forEach((entry, index) => {
    const offset = HEADER_BYTES + index * slotBytes;
    Buffer.from(entry.deliveryTag, "hex").copy(frame, offset);
    if (direction === 1) Buffer.from(entry.blobId, "hex").copy(frame, offset + 16);
  });

  return frame;
}

function sha256(frame) {
  return createHash("sha256").update(frame).digest("hex");
}

test("T1-T50 tick vector has fixed size, canonical slots, and header order", () => {
  assert.equal(vectors.rawBytes, RAW_BYTES);
  assert.equal(vectors.textChars, 2048);
  assert.equal(vectors.tickSlots, TICK_SLOTS);

  const frame = encodeFrame({
    direction: 0,
    sessionId: vectors.tick.sessionId,
    frameId: vectors.tick.frameId,
    ackId: vectors.tick.ackId,
    entries: vectors.tick.subscriptions.map((deliveryTag) => ({ deliveryTag })),
  });

  assert.equal(frame.toString("base64url").length, vectors.textChars);
  assert.equal(sha256(frame), vectors.tick.sha256);
  assert.deepEqual(frame.subarray(64 + vectors.tick.subscriptions.length * 16), Buffer.alloc(1536 - 96));
});

test("T1-T50 response vector preserves wakeup-only slots and capacity", () => {
  assert.equal(vectors.responseSlots, RESPONSE_SLOTS);
  const frame = encodeFrame({
    direction: 1,
    sessionId: vectors.response.sessionId,
    frameId: vectors.response.frameId,
    ackId: vectors.response.ackId,
    entries: vectors.response.wakeups,
  });

  assert.equal(frame.toString("base64url").length, vectors.textChars);
  assert.equal(sha256(frame), vectors.response.sha256);
  assert.deepEqual(frame.subarray(64 + vectors.response.wakeups.length * 32), Buffer.alloc(1536 - 96));
  assert.throws(
    () => encodeFrame({ direction: 1, sessionId: vectors.response.sessionId, frameId: 2, ackId: 1, entries: Array(47).fill(vectors.response.wakeups[0]) }),
    /entry count exceeds fixed slot capacity/,
  );
});
