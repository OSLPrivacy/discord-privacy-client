#!/usr/bin/env node

// TASK 1162: inspect the light-theme Instagram fixture in fixed-size Linux
// Chromium.  Story-edit controls are allowed only on the plain uploaded-file
// composer; camera and reel composer fixtures exist solely to prove that they
// do not acquire those controls.

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
const PNG_PATH = path.join(OUTPUT_DIR, "task-1162-linux-instagram-plain-story-controls.png");
const CONTROLS = ["text", "draw", "stickers", "music", "accessibility", "share"];

function pngFacts(buffer) {
  assert.deepEqual([...buffer.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], "screenshot is not a PNG");
  let offset = 8; let width = 0; let height = 0; let colorType = 0; const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset); const type = buffer.toString("ascii", offset + 4, offset + 8); const data = buffer.subarray(offset + 8, offset + 8 + length); offset += length + 12;
    if (type === "IHDR") { width = data.readUInt32BE(0); height = data.readUInt32BE(4); colorType = data[9]; }
    else if (type === "IDAT") idat.push(data); else if (type === "IEND") break;
  }
  const channels = colorType === 6 ? 4 : 3; assert.ok(channels === 3 || channels === 4, `unsupported PNG colour type ${colorType}`);
  const pixels = inflateSync(Buffer.concat(idat)); const row = width * channels + 1; const colors = new Set();
  for (let y = 0; y < height; y += 8) for (let x = 0; x < width; x += 8) { const at = y * row + 1 + x * channels; colors.add(`${pixels[at]},${pixels[at + 1]},${pixels[at + 2]}`); }
  return { width, height, bytes: buffer.length, colors: colors.size };
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer.address();
  if (!address || typeof address === "string") throw new Error("could not start fixture server");
  const chrome = await launchChrome(); const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, dontSetVisibleSize: true });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-1162-instagram-plain-story-fixture.html`);
    const inspected = await page.evaluate(`(() => {
      const visible = (node) => { const r = node.getBoundingClientRect(); const s = getComputedStyle(node); return s.display !== "none" && s.visibility !== "hidden" && r.width > 0 && r.height > 0; };
      const box = (node) => { const r = node.getBoundingClientRect(); return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }; };
      const composers = [...document.querySelectorAll("[data-story-composer]")].map((composer) => ({ kind: composer.dataset.storyComposer, visible: visible(composer), controls: [...composer.querySelectorAll("[data-instagram-story-control]")].filter(visible).map((control) => ({ id: control.dataset.instagramStoryControl, label: control.textContent.trim(), box: box(control) })), box: box(composer) }));
      return { theme: document.documentElement.dataset.theme, title: document.title, composers };
    })()`);
    assert.equal(inspected.theme, "light", "fixture must fix the light theme");
    const plain = inspected.composers.find((composer) => composer.kind === "plain-uploaded-file");
    assert.ok(plain?.visible, "plain uploaded-file composer is not visible");
    assert.deepEqual(plain.controls.map((control) => control.id), CONTROLS, "plain composer controls changed");
    for (const control of plain.controls) {
      assert.ok(control.box.width >= 160 && control.box.height >= 40, `${control.id} has no usable visible box`);
      assert.ok(control.box.x >= plain.box.x && control.box.y >= plain.box.y, `${control.id} is outside the plain composer`);
      console.log(`TASK1162_CONTROL composer=plain-uploaded-file id=${control.id} label="${control.label}" box=${control.box.width}x${control.box.height}`);
    }
    const nonPlain = inspected.composers.filter((composer) => composer.kind !== "plain-uploaded-file");
    assert.equal(nonPlain.length, 2, "fixture needs camera and reel non-plain composers");
    for (const composer of nonPlain) {
      assert.equal(composer.controls.length, 0, `${composer.kind} must not show plain-story controls`);
      console.log(`TASK1162_NON_PLAIN composer=${composer.kind} controls=${composer.controls.length}`);
    }
    const screenshot = await page.screenshot({ captureBeyondViewport: false }); const png = pngFacts(screenshot);
    assert.deepEqual({ width: png.width, height: png.height }, WINDOW, "fixed browser screenshot dimensions changed");
    assert.ok(png.bytes > 15_000 && png.colors >= 16, `screenshot lacks visual content bytes=${png.bytes} colors=${png.colors}`);
    writeFileSync(PNG_PATH, screenshot);
    console.log(`TASK1162_BROWSER=Linux Chromium fixed_theme=${inspected.theme}`);
    console.log(`TASK1162_WINDOW=${png.width}x${png.height}`);
    console.log(`TASK1162_SCREENSHOT=${PNG_PATH}`);
    console.log(`TASK1162_SCREENSHOT_BYTES=${png.bytes}`);
    console.log(`TASK1162_SCREENSHOT_SHA256=${createHash("sha256").update(screenshot).digest("hex")}`);
    console.log(`TASK1162_DONE visible_plain_composers=1 plain_controls=${plain.controls.length} non_plain_controls=0`);
  } finally {
    await page.close(); await chrome.close(); await new Promise((resolve, reject) => vite.httpServer.close((error) => error ? reject(error) : resolve()));
  }
}

main().catch((error) => { console.error(error.stack || error.message); process.exit(1); });
