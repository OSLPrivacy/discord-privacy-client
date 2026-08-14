import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0358-device-key-lost.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0358-device-key-lost-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED_TITLE = "Device key lost";
const REQUIRED_CONTROL = "Restore with recovery phrase";

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.text || "evaluation failed");
  return result.result.value;
}

test("TASK 0358 captures the fixed Device key lost screen", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-0358-device-key-lost-fixture.html`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    const screen = await evaluate(page, `(() => { const h = document.querySelector("#route-heading"); if (!h) throw new Error("screen did not render: " + document.body.innerText + " / " + document.body.innerHTML.slice(0, 500)); const controls = [...document.querySelectorAll("button, input")].map(e => ({ role: e.tagName.toLowerCase() === "button" ? "button" : "textbox", name: e.innerText || e.getAttribute("aria-label") || e.value || "" })); return { title: h.textContent.trim(), controls, text: document.body.innerText }; })()`);
    assert.equal(screen.title, REQUIRED_TITLE);
    assert.equal(screen.controls.filter((control) => control.name.trim() === REQUIRED_CONTROL).length, 1);
    const ax = await page.send("Accessibility.getFullAXTree");
    const names = (ax.nodes ?? []).map((node) => node.name?.value).filter(Boolean);
    assert.ok(names.includes(REQUIRED_TITLE));
    assert.ok(names.includes(REQUIRED_CONTROL));
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ fixture: "task-0358-device-key-lost-fixed", window: WINDOW, title: screen.title, controls: screen.controls, axNames: names, image: { path: PNG_PATH, bytes: png.length, width: png.readUInt32BE(16), height: png.readUInt32BE(20) } }, null, 2));
    assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
    assert.equal(png.readUInt32BE(16), WINDOW.width);
    assert.equal(png.readUInt32BE(20), WINDOW.height);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    assert.ok(new Set(png).size > 64, "PNG nearly blank");
    console.log(`TASK0358_TITLE=${screen.title}`);
    console.log(`TASK0358_CONTROL_COUNT=${screen.controls.filter((control) => control.name.trim() === REQUIRED_CONTROL).length}`);
    console.log(`TASK0358_PNG=${PNG_PATH}`);
    console.log(`TASK0358_TREE=${TREE_PATH}`);
    console.log(`TASK0358_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0358_PNG_BYTES=${png.length}`);
    console.log(`TASK0358_PNG_UNIQUE_BYTES=${new Set(png).size}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60_000 });
