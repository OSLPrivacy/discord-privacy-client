import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0118-group-dropdown-linux.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0118-group-dropdown-linux-screen-tree.json");
const FIXTURE = "screenshots/task-0118-group-dropdown-fixture.html";
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const VISIBLE_NAMES = Object.freeze([
  "Ada Lovelace", "Grace Hopper", "Katherine Johnson", "Dorothy Vaughan",
  "Mary Jackson", "Radia Perlman", "Annie Easley", "Margaret Hamilton",
]);

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

test("TASK 0118 captures the open group whitelist dropdown on Linux", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, screenWidth: WINDOW.width, screenHeight: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
    const screen = await evaluate(page, `(() => {
      const dropdown = document.querySelector('#whitelist-roster-dropdown');
      const rows = [...document.querySelectorAll('[data-whitelist-dropdown-row]')].map((row) => {
        const name = row.querySelector('strong')?.textContent?.trim() || '';
        const checkbox = row.querySelector('input[type=checkbox]');
        const rect = row.getBoundingClientRect();
        const nameRect = row.querySelector('strong')?.getBoundingClientRect();
        return { name, checked: Boolean(checkbox?.checked), row: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }, nameRect: nameRect && { x: nameRect.x, y: nameRect.y, width: nameRect.width, height: nameRect.height } };
      });
      const rect = dropdown?.getBoundingClientRect();
      return {
        fixtureOs: document.documentElement.dataset.fixtureOs,
        expanded: document.querySelector('#discord-qa-whitelist-roster')?.getAttribute('aria-expanded'),
        dropdown: rect && { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        rows,
        visibleRows: rows.filter(({ row }) => row.y >= rect.y && row.y + row.height <= rect.y + rect.height).map(({ name }) => name),
      };
    })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const png = await page.screenshot({ fromSurface: true });
    const dimensions = pngDimensions(png);
    const checked = screen.rows.filter((row) => row.checked).length;
    const unchecked = screen.rows.length - checked;

    assert.equal(screen.fixtureOs, "linux");
    assert.equal(screen.expanded, "true");
    assert.deepEqual(dimensions, WINDOW);
    assert.equal(screen.rows.length, 12);
    assert.equal(checked, 7);
    assert.equal(unchecked, 5);
    assert.ok(screen.dropdown.width >= 320 && screen.dropdown.height >= 500, `dropdown too small: ${JSON.stringify(screen.dropdown)}`);
    assert.ok(screen.visibleRows.length >= 8, `fewer than eight readable rows: ${screen.visibleRows.join(", ")}`);
    for (const name of VISIBLE_NAMES) assert.ok(screen.visibleRows.includes(name), `name is not visibly shown: ${name}`);
    for (const row of screen.rows.filter((row) => screen.visibleRows.includes(row.name))) {
      assert.ok(row.nameRect.width >= 100 && row.nameRect.height >= 14, `name did not lay out legibly: ${row.name}`);
    }
    assert.ok(png.length > 20_000, `PNG too small: ${png.length}`);
    assert.ok(new Set(png).size > 96, "PNG is nearly blank");

    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ fixture: FIXTURE, window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    console.log(`TASK0118_PNG=${PNG_PATH}`);
    console.log(`TASK0118_TREE=${TREE_PATH}`);
    console.log(`TASK0118_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0118_DROPDOWN_OPEN=${screen.expanded}`);
    console.log(`TASK0118_ROWS=${screen.rows.length}`);
    console.log(`TASK0118_CHECKBOXES=${screen.rows.length}`);
    console.log(`TASK0118_CHECKED=${checked}`);
    console.log(`TASK0118_UNCHECKED=${unchecked}`);
    console.log(`TASK0118_VISIBLE_NAMES=${screen.visibleRows.join("|")}`);
    console.log(`TASK0118_PNG_BYTES=${png.length}`);
    console.log(`TASK0118_SHA256=${createHash("sha256").update(png).digest("hex")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
