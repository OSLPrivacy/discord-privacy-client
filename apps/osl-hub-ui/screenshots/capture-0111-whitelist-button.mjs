#!/usr/bin/env node

// TASK 0111: render the committed two-state whitelist fixture in Linux
// Chromium at one fixed window size.  This is deliberately a browser capture,
// not a markup-only assertion: both buttons must have an on-screen box beside
// their conversation control before the PNG is retained.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const WINDOW = Object.freeze({ width: 1280, height: 800 });
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-0111-linux-whitelist-buttons.png");

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function pngFacts(buffer) {
  assert.deepEqual([...buffer.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], "screenshot is not a PNG");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  let bitDepth = 0;
  const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") break;
  }
  assert.equal(bitDepth, 8, "screenshot is not 8-bit PNG");
  assert.ok(colorType === 2 || colorType === 6, `unsupported PNG colour type ${colorType}`);
  const channels = colorType === 6 ? 4 : 3;
  const rowLength = width * channels + 1;
  const decoded = inflateSync(Buffer.concat(idat));
  const colors = new Set();
  for (let y = 0; y < height; y += 8) {
    for (let x = 0; x < width; x += 8) {
      const pixel = y * rowLength + 1 + x * channels;
      colors.add(`${decoded[pixel]},${decoded[pixel + 1]},${decoded[pixel + 2]}`);
    }
  }
  return { width, height, bytes: buffer.length, distinctColors: colors.size };
}

async function waitForFixture(page) {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const count = await page.evaluate('document.querySelectorAll("[data-whitelist-button=\\"single-place\\"]").length');
    if (count === 2) return;
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  throw new Error("timed out waiting for the two whitelist buttons");
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer.address();
  if (!address || typeof address === "string") throw new Error("could not start fixture server");
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, dontSetVisibleSize: true });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0111-whitelist-button-fixture.html`);
    await waitForFixture(page);
    const facts = await page.evaluate(`(() => {
      const box = (element) => {
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      return [...document.querySelectorAll('[data-whitelist-button="single-place"]')].map((button) => {
        const control = button.previousElementSibling;
        return {
          state: button.dataset.whitelistState,
          label: button.querySelector('.discord-qa-whitelist-label')?.textContent?.trim(),
          pressed: button.getAttribute('aria-pressed'),
          visible: Boolean(button.getClientRects().length) && box(button).width >= 40 && box(button).height >= 24,
          buttonBox: box(button),
          conversationControl: control?.textContent?.trim() ?? null,
          controlBox: control ? box(control) : null,
        };
      });
    })()`);
    assert.equal(facts.length, 2, `whitelist buttons found=${facts.length}, expected 2`);
    const states = new Map(facts.map((fact) => [fact.state, fact]));
    for (const expected of [
      { state: "off-list", label: "Off list", pressed: "false", control: "Proof" },
      { state: "on-list", label: "On list", pressed: "true", control: "Lock" },
    ]) {
      const fact = states.get(expected.state);
      assert.ok(fact, `missing ${expected.state} button`);
      assert.equal(fact.label, expected.label, `${expected.state} label`);
      assert.equal(fact.pressed, expected.pressed, `${expected.state} aria-pressed`);
      assert.equal(fact.conversationControl, expected.control, `${expected.state} conversation control`);
      assert.ok(fact.visible, `${expected.state} button has no visible box`);
      assert.ok(fact.controlBox && fact.buttonBox.x >= fact.controlBox.x + fact.controlBox.width, `${expected.state} button is not beside ${expected.control}`);
      console.log(`TASK0111_BUTTON state=${fact.state} label="${fact.label}" aria_pressed=${fact.pressed} control=${fact.conversationControl} visible=${fact.visible} box=${Math.round(fact.buttonBox.width)}x${Math.round(fact.buttonBox.height)}`);
    }
    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    const png = pngFacts(screenshot);
    assert.equal(png.width, WINDOW.width, `PNG width is ${png.width}, expected ${WINDOW.width}`);
    assert.equal(png.height, WINDOW.height, `PNG height is ${png.height}, expected ${WINDOW.height}`);
    assert.ok(png.distinctColors >= 8, `PNG is too flat: ${png.distinctColors} sampled colours`);
    writeFileSync(PNG_PATH, screenshot);
    console.log(`TASK0111_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0111_PNG_PATH=${PNG_PATH}`);
    console.log(`TASK0111_PNG_SIZE=${png.width}x${png.height}`);
    console.log(`TASK0111_PNG_BYTES=${png.bytes}`);
    console.log(`TASK0111_PNG_DISTINCT_COLORS=${png.distinctColors}`);
    console.log(`TASK0111_PNG_SHA256=${sha256(screenshot)}`);
    console.log("TASK0111_DONE states=on-list,off-list beside_conversation_controls=2");
  } finally {
    await page.close();
    await chrome.close();
    await new Promise((resolve, reject) => vite.httpServer.close((error) => error ? reject(error) : resolve()));
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
