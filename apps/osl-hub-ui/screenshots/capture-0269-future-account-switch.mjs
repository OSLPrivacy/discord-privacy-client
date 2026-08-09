#!/usr/bin/env node

// TASK 0269: capture the connected future-account switch in both states at
// one fixed Linux window size. The off screen is reached by operating the
// fixture's real checkbox, so the paired PNGs reflect the visible switch flip.

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
const STATES = Object.freeze([
  {
    state: "on",
    checked: true,
    detail: "A new account this friend adds is approved for the chats you already share.",
    file: "task-0269-linux-future-account-on.png",
  },
  {
    state: "off",
    checked: false,
    detail: "A new account this friend adds stays unapproved until you approve it.",
    file: "task-0269-linux-future-account-off.png",
  },
]);

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

function pngDimensions(png) {
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10], "capture is not a PNG");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

async function settle(page) {
  await page.evaluate("document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
}

async function switchFacts(page) {
  return page.evaluate(`(() => {
    const input = document.querySelector('[data-future-account-toggle]');
    const row = document.querySelector('[data-future-account-switch]');
    const box = (element) => { const r = element.getBoundingClientRect(); return { width: r.width, height: r.height }; };
    return {
      fixtureState: document.querySelector('[data-task-0269-friend-page]')?.getAttribute('data-task-0269-state') ?? '',
      state: row?.dataset.futureAccountSwitchState ?? '',
      checked: input instanceof HTMLInputElement && input.checked,
      role: input?.getAttribute('role') ?? '',
      label: row?.querySelector('strong')?.textContent?.trim() ?? '',
      detail: row?.querySelector('small')?.textContent?.trim() ?? '',
      rowVisible: Boolean(row?.getClientRects().length) && box(row).width > 300 && box(row).height >= 40,
      inputVisible: Boolean(input?.getClientRects().length) && box(input).width >= 18 && box(input).height >= 18,
      inputBox: input ? box(input) : null,
    };
  })()`);
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer.address();
  if (!address || typeof address === "string") throw new Error("TASK0269 could not start the fixture server");
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  const captures = [];
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, dontSetVisibleSize: true });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0269-future-account-switch-fixture.html`);
    await settle(page);

    for (const [index, expected] of STATES.entries()) {
      if (index > 0) {
        const changed = await page.evaluate(`(() => { const input = document.querySelector('[data-future-account-toggle]'); if (!(input instanceof HTMLInputElement)) return false; input.click(); return true; })()`);
        assert.equal(changed, true, "TASK0269 could not operate the future-account switch");
        await settle(page);
      }
      const facts = await switchFacts(page);
      assert.deepEqual(facts, {
        fixtureState: expected.state,
        state: expected.state,
        checked: expected.checked,
        role: "switch",
        label: "Auto-whitelist new accounts",
        detail: expected.detail,
        rowVisible: true,
        inputVisible: true,
        inputBox: { width: 18, height: 18 },
      }, `TASK0269 ${expected.state} switch facts`);

      const png = await page.screenshot({ captureBeyondViewport: false });
      assert.deepEqual(pngDimensions(png), WINDOW, `fixed ${expected.state} screenshot dimensions`);
      assert.ok(png.length > 10_000, `${expected.state} screenshot is unexpectedly empty`);
      const pngPath = path.join(OUTPUT_DIR, expected.file);
      writeFileSync(pngPath, png);
      captures.push({ state: expected.state, png, pngPath });
      console.log(`TASK0269_SWITCH state=${facts.state} checked=${facts.checked} label="${facts.label}" visible=${facts.rowVisible} switch_box=${facts.inputBox.width}x${facts.inputBox.height} png=${pngPath} bytes=${png.length} sha256=${sha256(png)}`);
    }
    assert.notEqual(sha256(captures[0].png), sha256(captures[1].png), "on and off screenshots must visibly differ");
    console.log(`TASK0269_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log("TASK0269_DONE screenshots=2 states=on,off switched_by_checkbox=true");
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
