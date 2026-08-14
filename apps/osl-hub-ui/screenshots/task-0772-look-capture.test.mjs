import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0772-look.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0772-look-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED_TEXT = ["Look", "Light", "Dark", "Computer", "Named looks", "Midnight", "Paper", "Signal", "Accent", "Corners", "GLOW", "TEXT", "SPACING", "Reset look"];

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

test("TASK 0772 captures every Look control and its on-screen explanation", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0772-look-fixture.html`, { timeoutMs: 30000 });
    await page.send("Accessibility.enable");
    await page.evaluate(`new Promise((resolve, reject) => { const deadline = Date.now() + 10000; const tick = () => { if (document.querySelector("[data-look-screen]")) return requestAnimationFrame(() => requestAnimationFrame(resolve)); if (Date.now() > deadline) return reject(new Error("Look fixture did not render")); setTimeout(tick, 25); }; tick(); })`);
    const screen = JSON.parse(await page.evaluate(`JSON.stringify((() => ({ title: document.querySelector("#look-title")?.textContent.trim() || "", text: document.body.innerText.replace(/\\s+/gu, " ").trim(), controls: [...document.querySelectorAll("button")].map((button) => button.innerText.trim()) }))())`));
    assert.equal(screen.title, "Look");
    for (const text of REQUIRED_TEXT) assert.ok(screen.text.includes(text), `screen text includes ${text}`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = JSON.stringify(ax.nodes ?? []);
    for (const text of REQUIRED_TEXT) assert.ok(treeText.includes(text), `accessibility tree includes ${text}`);
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000);
    assert.ok(new Set(png).size > 64);
    console.log(`TASK0772_PNG=${PNG_PATH}`);
    console.log(`TASK0772_TREE=${TREE_PATH}`);
    console.log(`TASK0772_TITLE=${screen.title}`);
    console.log(`TASK0772_CONTROLS=${REQUIRED_TEXT.join("|")}`);
    console.log(`TASK0772_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0772_PNG_BYTES=${png.length}`);
    console.log(`TASK0772_PNG_UNIQUE_BYTES=${new Set(png).size}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60000 });
