import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = path.join(APP_ROOT, "screenshots", "evidence", "task-0566-view-once-display");
const PNG_PATH = path.join(OUTPUT_DIR, "view-once-display-linux.png");
const REPORT_PATH = path.join(OUTPUT_DIR, "report.json");
const FIXTURE_PATH = "/screenshots/fixtures/task-0566-view-once-display.html";
const WINDOW = Object.freeze({ width: 1440, height: 900 });

function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}${FIXTURE_PATH}` };
}

test("TASK 0566 captures an opened view-once display over OSL Chat", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, "(async () => { await document.fonts.ready; await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))); if (document.body.dataset.fixtureReady !== 'task-0566') throw new Error('fixture did not render'); document.querySelector('[data-open-view-once]').click(); await new Promise((resolve) => requestAnimationFrame(resolve)); return document.body.dataset.viewOnceOpened; })()");
    const screen = await evaluate(page, `(() => {
      const rect = (selector) => { const node = document.querySelector(selector); const box = node?.getBoundingClientRect(); return box ? { x: box.x, y: box.y, width: box.width, height: box.height, right: box.right, bottom: box.bottom } : null; };
      return {
        viewport: { width: innerWidth, height: innerHeight }, opened: document.body.dataset.viewOnceOpened,
        appBehind: Boolean(document.querySelector('.osl-chats-view')), appText: document.querySelector('.osl-chats-view')?.textContent?.replace(/\\s+/gu, ' ').trim() || '',
        display: rect('[data-view-once-card]'), content: rect('[data-view-once-content]'), close: rect('[data-view-once-close]'),
        contentText: document.querySelector('[data-view-once-content]')?.textContent?.trim() || '', closeText: document.querySelector('[data-view-once-close]')?.textContent?.trim() || '', closeLabel: document.querySelector('[data-view-once-close]')?.getAttribute('aria-label') || '',
      };
    })()`);
    assert.deepEqual(screen.viewport, WINDOW);
    assert.equal(screen.opened, "true", "the capture must open the view-once message");
    assert.equal(screen.appBehind, true, "OSL Chat must remain behind the display");
    assert.match(screen.appText, /Mina/u);
    assert.match(screen.contentText, /shown only for this view/u);
    assert.equal(screen.closeText, "×");
    assert.equal(screen.closeLabel, "Close view-once display");
    for (const [name, rect] of Object.entries({ display: screen.display, content: screen.content, close: screen.close })) {
      assert.ok(rect && rect.width > 0 && rect.height > 0 && rect.x >= 0 && rect.y >= 0 && rect.right <= WINDOW.width && rect.bottom <= WINDOW.height, `${name} is not visibly inside the fixed Linux window`);
    }
    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const facts = imageFacts(png, { display: screen.display, content: screen.content, close: screen.close });
    assert.deepEqual({ width: facts.width, height: facts.height }, WINDOW);
    assert.ok(facts.distinctColors >= 30, "fixed Linux screen is blank or nearly blank");
    assert.ok(facts.crops.display.distinctColors >= 8, "view-once display did not paint");
    assert.ok(facts.crops.content.distinctColors >= 4, "view-once content did not paint");
    assert.ok(facts.crops.close.distinctColors >= 3, "X close control did not paint");
    const report = { schema: "task-0566-view-once-display/v1", window: WINDOW, fixture: FIXTURE_PATH, screen, image: { path: path.basename(PNG_PATH), bytes: png.length, sha256: sha256(png), ...facts } };
    writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK0566_SCREEN window=${facts.width}x${facts.height} opened=${screen.opened} app_behind=${screen.appBehind} content=${JSON.stringify(screen.contentText)} close=${JSON.stringify(screen.closeText)} png_bytes=${png.length} distinct_colors=${facts.distinctColors}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
