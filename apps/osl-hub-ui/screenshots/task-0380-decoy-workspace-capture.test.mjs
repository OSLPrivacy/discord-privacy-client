import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0380-decoy-workspace.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0380-decoy-workspace-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

test("TASK 0380 captures the fixed Decoy workspace", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer.address();
  const url = `http://127.0.0.1:${address.port}/`;
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-0380-decoy-workspace-fixture.html`, { timeoutMs: 30000 });
    await page.send("Accessibility.enable");
    await page.evaluate(`new Promise((resolve, reject) => { const deadline = Date.now() + 10000; const tick = () => { if (document.documentElement.dataset.task0380 === "ready" && document.querySelector("#route-heading")?.textContent.trim()) return requestAnimationFrame(() => requestAnimationFrame(resolve)); if (Date.now() > deadline) return reject(new Error("decoy fixture did not render")); setTimeout(tick, 25); }; tick(); })`);
    const screen = JSON.parse(await page.evaluate(`JSON.stringify((() => { const heading = document.querySelector("#route-heading"); const controls = [...document.querySelectorAll("button")].map((e) => ({ text: e.innerText.trim(), ariaLabel: e.getAttribute("aria-label") || "" })); return { title: heading?.textContent.trim() || "", text: document.body.innerText.replace(/\\s+/gu, " ").trim(), controls }; })())`));
    assert.equal(screen.title, "Workspace");
    assert.deepEqual(screen.controls.map((c) => c.text || c.ariaLabel), ["Close"]);
    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = JSON.stringify(ax.nodes ?? []);
    assert.match(treeText, /Workspace/u);
    assert.match(treeText, /Close/u);
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-0380-decoy-workspace-fixture.html`, window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10000);
    const uniqueBytes = new Set(png).size;
    assert.ok(uniqueBytes > 64);
    console.log(`TASK0380_PNG=${PNG_PATH}`);
    console.log(`TASK0380_TREE=${TREE_PATH}`);
    console.log(`TASK0380_TITLE=${screen.title}`);
    console.log(`TASK0380_CONTROLS=${screen.controls.map((c) => c.text || c.ariaLabel).join("|")}`);
    console.log(`TASK0380_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0380_PNG_BYTES=${png.length}`);
    console.log(`TASK0380_PNG_UNIQUE_BYTES=${uniqueBytes}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60000 });
