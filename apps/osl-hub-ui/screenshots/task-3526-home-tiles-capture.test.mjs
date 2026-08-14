import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-3526-home-tiles.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-3526-home-tiles-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 1000 });

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

test("TASK 3526 captures the visible Home tile grid", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-3526-home-tiles-fixture.html`, { timeoutMs: 30_000 });
    await evaluate(page, `new Promise((resolve, reject) => { const deadline = Date.now() + 10000; const tick = () => document.documentElement.dataset.task3526 === "ready" && document.querySelectorAll("[data-tile-id]").length === 17 ? requestAnimationFrame(() => requestAnimationFrame(resolve)) : Date.now() > deadline ? reject(new Error("Home tiles did not render")) : setTimeout(tick, 25); tick(); })`);
    await page.send("Accessibility.enable");
    const screen = await evaluate(page, `(() => ({ title: document.querySelector("#route-heading")?.textContent?.trim(), controls: [...document.querySelectorAll("[data-tile-id] button")].map((button) => button.getAttribute("aria-label")), text: document.body.innerText.replace(/\\s+/gu, " ").trim() }))()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const expected = ["OSL Mail", "OSL Notes", "OSL Chat", "Scrub", "Discord", "Telegram", "Signal", "WhatsApp", "Gmail", "Outlook", "Proton Mail", "Yahoo Mail", "AOL Mail", "GMX Mail", "Mail.com", "iCloud Mail", "Tuta Mail"];
    assert.equal(screen.title, "Home");
    assert.equal(screen.controls.length, expected.length);
    for (const name of expected) assert.ok(screen.controls.some((control) => control?.startsWith(`${name},`)), `missing Home tile control: ${name}`);
    assert.match(screen.controls.find((control) => control?.startsWith("OSL Mail,")), /Not started/u);
    assert.match(screen.controls.find((control) => control?.startsWith("OSL Notes,")), /Not started/u);
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-3526-home-tiles-fixture.html`, window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    const dimensions = pngDimensions(png);
    const uniqueBytes = new Set(png).size;
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    assert.ok(uniqueBytes > 64, `PNG nearly blank: ${uniqueBytes} unique byte values`);
    console.log(`TASK3526_PNG=${PNG_PATH}`);
    console.log(`TASK3526_TREE=${TREE_PATH}`);
    console.log(`TASK3526_HOME_TITLE=${screen.title}`);
    console.log(`TASK3526_HOME_TILE_CONTROLS=${screen.controls.length}`);
    console.log(`TASK3526_HOME_TILE_NAMES=${expected.join("|")}`);
    console.log(`TASK3526_PNG_BYTES=${png.length}`);
    console.log(`TASK3526_PNG_UNIQUE_BYTES=${uniqueBytes}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
