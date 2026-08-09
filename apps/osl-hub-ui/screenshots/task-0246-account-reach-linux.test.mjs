import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = path.join(APP_ROOT, "screenshots", "evidence", "task-0246-account-reach-linux");
const PNG_PATH = path.join(OUTPUT_DIR, "friend-account-reach-linux.png");
const REPORT_PATH = path.join(OUTPUT_DIR, "report.json");
const WINDOW = Object.freeze({ width: 1024, height: 700 });
const FIXTURE_PATH = "/screenshots/fixtures/task-0246-account-reach-linux.html";
const EXPECTED = Object.freeze([
  { accountId: "gmail-primary", label: "Gmail (primary)", checked: true },
  { accountId: "discord-alt", label: "Discord (alt)", checked: false },
  { accountId: "signal-personal", label: "Signal (personal)", checked: true },
  { accountId: "telegram-work", label: "Telegram (work)", checked: false },
]);

function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}${FIXTURE_PATH}` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

const READ_SCREEN = `(() => {
  const rect = (node) => { const box = node.getBoundingClientRect(); return { x: box.x, y: box.y, width: box.width, height: box.height, right: box.right, bottom: box.bottom }; };
  const list = document.querySelector("[data-account-reach-list]");
  if (!list) throw new Error("TASK0246 account reach list missing");
  return {
    viewport: { width: innerWidth, height: innerHeight },
    title: document.querySelector("h1")?.textContent?.trim(),
    list: rect(list),
    rows: [...document.querySelectorAll("[data-account-reach]")].map((box) => ({ accountId: box.dataset.accountReach, label: box.closest("label")?.querySelector("span")?.textContent?.trim(), checked: box.checked, box: rect(box), row: rect(box.closest("label")) })),
  };
})()`;

function inViewport(rect, description) {
  assert.ok(rect.x >= 0 && rect.y >= 0 && rect.right <= WINDOW.width && rect.bottom <= WINDOW.height, `${description} is clipped on the fixed Linux screen`);
}

test("TASK 0246 captures four readable account names with clear mixed tick states", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, "(async () => { await document.fonts.ready; await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))); if (document.documentElement.dataset.fixtureReady !== 'task-0246') throw new Error('fixture did not render'); return 'painted'; })()");
    const screen = await evaluate(page, READ_SCREEN);
    assert.deepEqual(screen.viewport, WINDOW);
    assert.equal(screen.title, "Vera");
    assert.equal(screen.rows.length, 4, "the fixture must show exactly four owned accounts");
    assert.deepEqual(screen.rows.map(({ accountId, label, checked }) => ({ accountId, label, checked })), EXPECTED);
    inViewport(screen.list, "account reach list");
    for (const row of screen.rows) {
      assert.ok(row.row.height >= 55, `${row.label} row is too short to read or use`);
      assert.ok(row.box.width >= 20 && row.box.height >= 20, `${row.label} tick is not visually clear (${row.box.width}x${row.box.height})`);
      inViewport(row.row, `${row.label} row`);
      inViewport(row.box, `${row.label} tick`);
    }
    assert.equal(screen.rows.filter((row) => row.checked).length, 2, "the fixture must show two ticked accounts");
    assert.equal(screen.rows.filter((row) => !row.checked).length, 2, "the fixture must show two unticked accounts");
    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const facts = imageFacts(png, { accountReachList: screen.list, ...Object.fromEntries(screen.rows.map((row) => [row.accountId, row.row])) });
    assert.deepEqual({ width: facts.width, height: facts.height }, WINDOW);
    assert.ok(facts.distinctColors >= 40, "fixed Linux capture is blank or nearly blank");
    assert.ok(facts.crops.accountReachList.distinctColors >= 10, "account list did not paint distinctly");
    for (const row of screen.rows) assert.ok(facts.crops[row.accountId].distinctColors >= 4, `${row.label} row did not paint`);
    const report = { schema: "task-0246-account-reach-linux/v1", window: WINDOW, fixture: FIXTURE_PATH, screen, image: { path: path.basename(PNG_PATH), bytes: png.length, sha256: sha256(png), ...facts } };
    writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK0246_SCREEN window=${facts.width}x${facts.height} accounts=${screen.rows.length} names=${screen.rows.map((row) => JSON.stringify(row.label)).join(",")} ticked=${screen.rows.filter((row) => row.checked).map((row) => row.accountId).join(",")} unticked=${screen.rows.filter((row) => !row.checked).map((row) => row.accountId).join(",")} png_bytes=${png.length}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
