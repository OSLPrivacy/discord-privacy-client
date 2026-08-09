import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0272-complete-friend-page-900x700.png");
const REPORT_PATH = path.join(EVIDENCE_DIR, "task-0272-complete-friend-page-900x700.json");
const FIXED_WINDOW = Object.freeze({ width: 900, height: 700 });
const REQUIRED = Object.freeze(["picture", "name", "tick", "Message", "account reach", "whitelist", "warning", "remove", "block"]);

function pngDimensions(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

test("TASK 0272 captures every complete friend-page action inside the fixed Linux window", async () => {
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  // This fixture imports only its two rendering modules. Do not scan the
  // worktree's unrelated app entry points while the focused visual check runs.
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, optimizeDeps: { noDiscovery: true }, logLevel: "error" });
  await vite.listen();
  const address = vite.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  const url = `http://127.0.0.1:${address.port}/screenshots/task-0272-complete-friend-page-fixture.html`;
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${FIXED_WINDOW.width},${FIXED_WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();

  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...FIXED_WINDOW, screenWidth: FIXED_WINDOW.width, screenHeight: FIXED_WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    const screen = await evaluate(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const wait = () => {
        const elements = [...document.querySelectorAll('[data-friend-page-element]')];
        if (elements.length === ${REQUIRED.length}) {
          requestAnimationFrame(() => requestAnimationFrame(() => {
            const viewport = { width: window.innerWidth, height: window.innerHeight, scrollHeight: document.documentElement.scrollHeight };
            const items = elements.map((element) => {
              const rect = element.getBoundingClientRect();
              return { name: element.dataset.friendPageElement, text: element.innerText.trim(), rect: { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height } };
            });
            const interactive = items.filter((item) => ["Message", "account reach", "whitelist", "remove", "block"].includes(item.name));
            const overlaps = [];
            for (let first = 0; first < interactive.length; first += 1) for (let second = first + 1; second < interactive.length; second += 1) {
              const a = interactive[first].rect; const b = interactive[second].rect;
              if (Math.max(a.left, b.left) < Math.min(a.right, b.right) && Math.max(a.top, b.top) < Math.min(a.bottom, b.bottom)) overlaps.push([interactive[first].name, interactive[second].name]);
            }
            resolve({ viewport, items, overlaps });
          }));
          return;
        }
        if (Date.now() > deadline) { reject(new Error('friend-page fixture did not render all named elements')); return; }
        setTimeout(wait, 25);
      };
      wait();
    })`);
    const screenshot = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, screenshot);
    writeFileSync(REPORT_PATH, JSON.stringify({ url, fixedWindow: FIXED_WINDOW, required: REQUIRED, screen }, null, 2));

    const dimensions = pngDimensions(screenshot);
    assert.deepEqual(dimensions, FIXED_WINDOW);
    assert.deepEqual(screen.items.map((item) => item.name), REQUIRED);
    assert.equal(screen.viewport.scrollHeight, FIXED_WINDOW.height, "the fixed page must not vertically clip or scroll");
    for (const item of screen.items) {
      assert.ok(item.rect.width > 0 && item.rect.height > 0, `${item.name} must paint`);
      assert.ok(item.rect.left >= 0 && item.rect.top >= 0 && item.rect.right <= FIXED_WINDOW.width && item.rect.bottom <= FIXED_WINDOW.height, `${item.name} must stay within the fixed window`);
    }
    assert.deepEqual(screen.overlaps, [], "friend-page action buttons must not overlap");

    console.log(`TASK0272_PNG=${PNG_PATH}`);
    console.log(`TASK0272_REPORT=${REPORT_PATH}`);
    console.log(`TASK0272_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0272_ACTIONS=${screen.items.map((item) => item.name).join("|")}`);
    console.log(`TASK0272_VISIBLE=${screen.items.length}/${REQUIRED.length}`);
    console.log(`TASK0272_SCROLL_HEIGHT=${screen.viewport.scrollHeight}`);
    console.log(`TASK0272_ACTION_OVERLAPS=${screen.overlaps.length}`);
    console.log(`TASK0272_PNG_SHA256=${createHash("sha256").update(screenshot).digest("hex")}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}, { timeout: 60_000 });
