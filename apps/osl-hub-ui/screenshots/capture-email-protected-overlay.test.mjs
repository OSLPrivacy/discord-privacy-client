import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { inflateSync } from "node:zlib";

import { FIXED_VIEWPORT, REQUIRED_NAMES, captureEmailProtectedOverlay } from "./capture-email-protected-overlay.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const EVIDENCE_DIR = path.join(SCRIPT_DIR, "evidence");

function paeth(left, up, upLeft) {
  const p = left + up - upLeft;
  const pa = Math.abs(p - left);
  const pb = Math.abs(p - up);
  const pc = Math.abs(p - upLeft);
  if (pa <= pb && pa <= pc) return left;
  return pb <= pc ? up : upLeft;
}

function decodePng(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.subarray(offset + 4, offset + 8).toString("ascii");
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      assert.equal(data[8], 8, "screenshot PNG must be 8-bit");
      colorType = data[9];
    }
    if (type === "IDAT") idat.push(data);
    if (type === "IEND") break;
  }
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  assert.ok(channels > 0, `unsupported PNG color type ${colorType}`);
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * channels);
  let input = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[input];
    input += 1;
    const row = inflated.subarray(input, input + stride);
    input += stride;
    const out = y * stride;
    const prev = y === 0 ? -1 : out - stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? pixels[out + x - channels] : 0;
      const up = prev >= 0 ? pixels[prev + x] : 0;
      const upLeft = prev >= 0 && x >= channels ? pixels[prev + x - channels] : 0;
      const raw = row[x];
      pixels[out + x] = filter === 0 ? raw
        : filter === 1 ? (raw + left) & 255
        : filter === 2 ? (raw + up) & 255
        : filter === 3 ? (raw + Math.floor((left + up) / 2)) & 255
        : filter === 4 ? (raw + paeth(left, up, upLeft)) & 255
        : assert.fail(`unsupported PNG filter ${filter}`);
    }
  }
  return { width, height, channels, pixels };
}

function countDifferingPixels(a, b) {
  assert.equal(a.width, b.width, "compared screenshots must share a width");
  assert.equal(a.height, b.height, "compared screenshots must share a height");
  let differing = 0;
  const perPixelA = a.channels;
  const perPixelB = b.channels;
  const compareChannels = Math.min(perPixelA, perPixelB, 3);
  for (let pixel = 0; pixel < a.width * a.height; pixel += 1) {
    const atA = pixel * perPixelA;
    const atB = pixel * perPixelB;
    let same = true;
    for (let channel = 0; channel < compareChannels; channel += 1) {
      if (a.pixels[atA + channel] !== b.pixels[atB + channel]) {
        same = false;
        break;
      }
    }
    if (!same) differing += 1;
  }
  return differing;
}

test("TASK1228 the shared email overlay fixture shows both named overlays and differs from the empty state", async () => {
  const built = await captureEmailProtectedOverlay({
    state: "built",
    output: path.join(EVIDENCE_DIR, "task-1228-email-protected-overlay-1200x820.png"),
  });
  const empty = await captureEmailProtectedOverlay({
    state: "empty",
    output: path.join(EVIDENCE_DIR, "task-1228-email-protected-overlay-empty-state-1200x820.png"),
  });

  // Element 1 and 2: draft overlay and reading overlay, each exactly once,
  // named in both the visible text and the accessibility tree.
  assert.equal(built.draftOverlayCount, 1);
  assert.equal(built.readingOverlayCount, 1);
  for (const name of REQUIRED_NAMES) {
    assert.equal(built.visibleTextRequiredNames[name], 1, `'${name}' missing from visible text`);
    assert.ok(built.screenTreeRequiredNames[name] >= 1, `'${name}' missing from the screen tree`);
  }

  // The empty-state fixture must genuinely be empty -- otherwise "differs
  // from the empty-state capture" could pass for the wrong reason.
  assert.equal(empty.draftOverlayCount, 0);
  assert.equal(empty.readingOverlayCount, 0);
  for (const name of REQUIRED_NAMES) {
    assert.equal(empty.visibleTextRequiredNames[name], 0, `empty state names '${name}'`);
    assert.equal(empty.screenTreeRequiredNames[name], 0, `empty state's screen tree names '${name}'`);
  }

  assert.equal(built.png.width, FIXED_VIEWPORT.width);
  assert.equal(built.png.height, FIXED_VIEWPORT.height);
  assert.ok(built.png.bytes > 10_000, `built screenshot is implausibly small: ${built.png.bytes}`);
  assert.ok(built.png.distinctColors > 50, `built screenshot is nearly blank: ${built.png.distinctColors} colors`);

  assert.notEqual(built.png.sha256, empty.png.sha256, "built screenshot is byte-identical to the empty-state capture");

  const decodedBuilt = decodePng(built.pngBytes);
  const decodedEmpty = decodePng(empty.pngBytes);
  const differingPixels = countDifferingPixels(decodedBuilt, decodedEmpty);
  const totalPixels = decodedBuilt.width * decodedBuilt.height;
  assert.ok(differingPixels > 0, "built screenshot has zero pixels different from the empty-state capture");

  mkdirSync(EVIDENCE_DIR, { recursive: true });
  writeFileSync(built.screenshot, built.pngBytes);
  writeFileSync(empty.screenshot, empty.pngBytes);

  console.log(`TASK1228_BUILT_PNG ${built.screenshot}`);
  console.log(`TASK1228_BUILT_SHA256 ${built.png.sha256}`);
  console.log(`TASK1228_EMPTY_PNG ${empty.screenshot}`);
  console.log(`TASK1228_EMPTY_SHA256 ${empty.png.sha256}`);
  console.log(`TASK1228_WINDOW ${built.png.width}x${built.png.height}`);
  console.log(
    `TASK1228_SCREEN_TREE_NAMED draft_overlay=${built.screenTreeRequiredNames["draft overlay"]} reading_overlay=${built.screenTreeRequiredNames["reading overlay"]}`,
  );
  console.log(
    `TASK1228_VISIBLE_TEXT_NAMED draft_overlay=${built.visibleTextRequiredNames["draft overlay"]} reading_overlay=${built.visibleTextRequiredNames["reading overlay"]}`,
  );
  console.log(`TASK1228_DIFF_VS_EMPTY_STATE ${differingPixels} pixels of ${totalPixels}`);
});
