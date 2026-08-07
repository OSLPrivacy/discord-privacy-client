import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { inflateSync } from "node:zlib";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const VIEWPORT = { width: 1280, height: 800 };
const OUTPUT = resolve("screenshots/no-recovery-secret-1280x800.png");
const TITLE = "No recovery secret is available";
const CONTROL = "Continue";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

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

function colorKey(decoded, x, y) {
  const at = (y * decoded.width + x) * decoded.channels;
  return `${decoded.pixels[at]},${decoded.pixels[at + 1]},${decoded.pixels[at + 2]}`;
}

function regionColors(decoded, rect) {
  const colors = new Set();
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(decoded.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(decoded.height, Math.ceil(rect.y + rect.height));
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) colors.add(colorKey(decoded, x, y));
  }
  return colors.size;
}

function fullPageFacts(bytes, rects) {
  const decoded = decodePng(bytes);
  const distinct = new Set();
  for (let y = 0; y < decoded.height; y += 8) {
    for (let x = 0; x < decoded.width; x += 8) distinct.add(colorKey(decoded, x, y));
  }
  return {
    width: decoded.width,
    height: decoded.height,
    bytes: bytes.length,
    sha256: sha256(bytes),
    sampledDistinctColors: distinct.size,
    titleRegionColors: regionColors(decoded, rects.title),
    continueRegionColors: regionColors(decoded, rects.continue),
  };
}

function axNames(tree) {
  return tree.nodes.map((node) => node.name?.value).filter((value) => typeof value === "string");
}

async function waitForVisibleRoute(page) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const ready = await page.evaluate(`(() => {
      const title = document.querySelector("#route-heading");
      const button = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent.trim() === ${JSON.stringify(CONTROL)});
      return Boolean(title && button && title.getBoundingClientRect().width > 0 && button.getBoundingClientRect().width > 0);
    })()`);
    if (ready) return;
    await new Promise((done) => setTimeout(done, 25));
  }
  throw new Error("timed out waiting for no-recovery-secret fixture");
}

test("TASK0362 captures the fixed no-recovery-secret screen at 1280x800", async (t) => {
  const server = await createServer({
    root: process.cwd(),
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "silent",
  });
  await server.listen();
  t.after(async () => { await server.close(); });
  const url = `${server.resolvedUrls.local[0]}?osl-fixture=no-recovery-secret`;

  const chrome = await launchChrome();
  t.after(async () => { await chrome.close(); });
  const page = await chrome.openPage();
  t.after(async () => { await page.close(); });

  await page.send("Emulation.setDeviceMetricsOverride", {
    width: VIEWPORT.width,
    height: VIEWPORT.height,
    deviceScaleFactor: 1,
    mobile: false,
  });
  await page.navigate(url);
  await waitForVisibleRoute(page);

  const tree = await page.send("Accessibility.getFullAXTree");
  const names = axNames(tree);
  assert.ok(names.includes(TITLE), `screen tree names: ${names.join(" | ")}`);
  assert.ok(names.includes(CONTROL), `screen tree names: ${names.join(" | ")}`);

  const rects = await page.evaluate(`(() => {
    const title = document.querySelector("#route-heading").getBoundingClientRect();
    const button = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent.trim() === ${JSON.stringify(CONTROL)}).getBoundingClientRect();
    return {
      title: { x: title.x, y: title.y, width: title.width, height: title.height },
      continue: { x: button.x, y: button.y, width: button.width, height: button.height },
      text: document.body.innerText.replace(/\\s+/g, " ").trim(),
    };
  })()`);
  assert.match(rects.text, /No recovery secret is available/u);
  assert.match(rects.text, /Continue/u);
  for (const [name, rect] of Object.entries({ title: rects.title, continue: rects.continue })) {
    assert.ok(rect.width > 20 && rect.height > 20, `${name} rect is too small: ${JSON.stringify(rect)}`);
    assert.ok(rect.x >= 0 && rect.y >= 0, `${name} rect starts outside viewport: ${JSON.stringify(rect)}`);
    assert.ok(rect.x + rect.width <= VIEWPORT.width, `${name} rect exceeds viewport width: ${JSON.stringify(rect)}`);
    assert.ok(rect.y + rect.height <= VIEWPORT.height, `${name} rect exceeds viewport height: ${JSON.stringify(rect)}`);
  }

  const bytes = await page.screenshot({
    captureBeyondViewport: false,
    fromSurface: true,
    clip: { x: 0, y: 0, width: VIEWPORT.width, height: VIEWPORT.height, scale: 1 },
  });
  const facts = fullPageFacts(bytes, rects);
  assert.equal(facts.width, VIEWPORT.width);
  assert.equal(facts.height, VIEWPORT.height);
  assert.ok(facts.bytes > 8_000, `screenshot is implausibly small: ${facts.bytes}`);
  assert.ok(facts.sampledDistinctColors > 20, `screenshot is nearly blank: ${facts.sampledDistinctColors} sampled colors`);
  assert.ok(facts.titleRegionColors > 8, `title image region lacks rendered detail: ${facts.titleRegionColors}`);
  assert.ok(facts.continueRegionColors > 8, `Continue image region lacks rendered detail: ${facts.continueRegionColors}`);

  mkdirSync(dirname(OUTPUT), { recursive: true });
  writeFileSync(OUTPUT, bytes);
  console.log(`TASK0362_PNG path=${OUTPUT} width=${facts.width} height=${facts.height} bytes=${facts.bytes} sha256=${facts.sha256} sampledDistinctColors=${facts.sampledDistinctColors} titleRegionColors=${facts.titleRegionColors} continueRegionColors=${facts.continueRegionColors}`);
  console.log(`TASK0362_AX title=${names.includes(TITLE)} continue=${names.includes(CONTROL)}`);
  console.log(`TASK0362_RECTS title=${JSON.stringify(rects.title)} continue=${JSON.stringify(rects.continue)}`);
});
