#!/usr/bin/env node

// TASK 0209: fixed Linux visual check for the four friends-tab states. Each
// screenshot is reached by pressing that tab in the browser, then verified
// against the selected tab and exactly one expected backend-style row.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const WINDOW = Object.freeze({ width: 1280, height: 800 });
const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const TABS = Object.freeze([
  { id: "online", row: "Ari Online", state: "Online" },
  { id: "all", row: "Bea Friend", state: "Friend" },
  { id: "pending", row: "Casey Pending", state: "Pending" },
  { id: "blocked", row: "Devon Blocked", state: "Blocked" },
]);

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function pngDimensions(png) {
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], "capture is not a PNG");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

async function facts(page) {
  return page.evaluate(`(() => {
    const active = [...document.querySelectorAll('.friends-tab.active')];
    const row = document.querySelector('.friend-tab-row');
    const box = (element) => { const r = element.getBoundingClientRect(); return { width: r.width, height: r.height }; };
    return {
      activeTabs: active.map((tab) => tab.dataset.friendsTab),
      activeText: active[0]?.textContent?.replace(/\\s+/g, ' ').trim() ?? '',
      rows: [...document.querySelectorAll('.friend-tab-row')].map((entry) => ({
        tab: entry.dataset.friendRow,
        name: entry.querySelector('.friend-tab-name')?.textContent?.trim(),
        state: entry.querySelector('[data-friend-state]')?.textContent?.trim(),
        visible: Boolean(entry.getClientRects().length) && box(entry).width > 200 && box(entry).height > 50,
      })),
      rowBox: row ? box(row) : null,
      allTabCount: document.querySelectorAll('[data-friends-tab]').length,
    };
  })()`);
}

async function settle(page) {
  await page.evaluate("document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer.address();
  if (!address || typeof address === "string") throw new Error("TASK0209 could not start the fixture server");
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, dontSetVisibleSize: true });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0209-friends-tabs-fixture.html`);
    await settle(page);

    for (const expected of TABS) {
      const pressed = await page.evaluate(`(() => { const button = document.querySelector('[data-friends-tab="${expected.id}"]'); if (!button) return false; button.click(); return true; })()`);
      assert.equal(pressed, true, `TASK0209 could not press ${expected.id}`);
      await settle(page);
      const screen = await facts(page);
      assert.equal(screen.allTabCount, 4, "the fixed fixture must contain four tabs");
      assert.deepEqual(screen.activeTabs, [expected.id], `selected tab for ${expected.id}`);
      assert.equal(screen.rows.length, 1, `one expected row for ${expected.id}`);
      assert.deepEqual(screen.rows[0], { tab: expected.id, name: expected.row, state: expected.state, visible: true }, `expected ${expected.id} row`);
      assert.ok(screen.rowBox && screen.rowBox.width > 200 && screen.rowBox.height > 50, `${expected.id} row has a visible box`);

      const png = await page.screenshot({ captureBeyondViewport: false });
      const dimensions = pngDimensions(png);
      assert.deepEqual(dimensions, WINDOW, `fixed ${expected.id} screenshot dimensions`);
      assert.ok(png.length > 10_000, `${expected.id} screenshot is unexpectedly empty`);
      const pngPath = path.join(OUTPUT_DIR, `task-0209-linux-friends-${expected.id}.png`);
      writeFileSync(pngPath, png);
      console.log(`TASK0209_TAB=${expected.id} selected=${screen.activeTabs[0]} row="${screen.rows[0].name}" state=${screen.rows[0].state} visible=${screen.rows[0].visible} png=${pngPath} bytes=${png.length} sha256=${sha256(png)}`);
    }
    console.log(`TASK0209_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log("TASK0209_DONE screenshots=4 selected_tabs=online,all,pending,blocked rows=Ari_Online,Bea_Friend,Casey_Pending,Devon_Blocked");
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
