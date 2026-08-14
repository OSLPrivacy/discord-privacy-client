import assert from "node:assert/strict";
import zlib from "node:zlib";
import test from "node:test";
import { FIXED_VIEWPORT, REQUIRED_NAMES, pngFacts } from "./capture-connection-choice.mjs";

function pngChunk(kind, body) {
  const crc = new Uint32Array(1);
  const payload = Buffer.concat([Buffer.from(kind, "ascii"), body]);
  let value = 0xffffffff;
  for (const byte of payload) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit += 1) value = (value >>> 1) ^ (value & 1 ? 0xedb88320 : 0);
  }
  crc[0] = (value ^ 0xffffffff) >>> 0;
  const out = Buffer.alloc(12 + body.length);
  out.writeUInt32BE(body.length, 0);
  out.write(kind, 4, 4, "ascii");
  body.copy(out, 8);
  out.writeUInt32BE(crc[0], 8 + body.length);
  return out;
}

function blankPng() {
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(2, 0);
  ihdr.writeUInt32BE(2, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const rows = Buffer.from([
    0, 0, 0, 0, 255, 0, 0, 0, 255,
    0, 0, 0, 0, 255, 0, 0, 0, 255,
  ]);
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(rows)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

test("TASK0371 pins the fixed connection choice viewport and screen tree names", () => {
  assert.deepEqual(FIXED_VIEWPORT, { width: 800, height: 620 });
  assert.deepEqual(REQUIRED_NAMES, ["Connection choice", "Tor", "Direct", "Mullvad", "You can use both. Neither replaces the other.", "Continue", "Back"]);
});

test("TASK0371 rejects a blank PNG", () => {
  const facts = pngFacts(blankPng());
  assert.equal(facts.width, 2);
  assert.equal(facts.height, 2);
  assert.equal(facts.distinctColors, 1);
});
