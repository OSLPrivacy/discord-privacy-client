import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0379-whitelisting-setup.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0379-whitelisting-setup-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = Object.freeze(["Whitelisting setup", "conversation ticks", "Select all", "Clear all", "search", "rule", "Continue", "Back"]);
const escapeRegExp = (value) => value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

function treeText(nodes) { return nodes.flatMap((node) => [node.role?.value, node.name?.value, node.value?.value, node.description?.value]).filter((value) => typeof value === "string" && value.trim()).join("\n"); }
function dimensions(buffer) { assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a"); return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) }; }
async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation threw");
  return result.result.value;
}

test("TASK 0379 captures the fixed Whitelisting setup screen", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-0379-whitelisting-setup-fixture.html`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, `new Promise((resolve, reject) => { const end = Date.now() + 10000; const tick = () => { if (document.documentElement.dataset.task0379 === "ready" && document.querySelector("#route-heading")) { requestAnimationFrame(() => requestAnimationFrame(resolve)); return; } if (Date.now() > end) reject(new Error("Whitelisting setup did not render")); else setTimeout(tick, 25); }; tick(); })`);
    const screen = await evaluate(page, `(() => { const text = document.body.innerText.replace(/\\s+/gu, " ").trim(); const controls = [...document.querySelectorAll("button, input, fieldset")].map((element) => ({ tag: element.tagName.toLowerCase(), id: element.id, type: element.getAttribute("type") || "", name: element.getAttribute("name") || "", text: element.innerText.replace(/\\s+/gu, " ").trim(), ariaLabel: element.getAttribute("aria-label") || "", value: element.getAttribute("value") || "" })); return { title: document.querySelector("#route-heading")?.textContent || "", text, controls }; })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const normalized = [screen.title, screen.text, screen.controls.map((control) => `${control.text} ${control.ariaLabel} ${control.value}`).join("\n"), treeText(ax.nodes ?? [])].join("\n").toLowerCase();
    for (const required of REQUIRED) assert.match(normalized, new RegExp(escapeRegExp(required.toLowerCase()), "u"));
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-0379-whitelisting-setup-fixture.html`, window: WINDOW, required: REQUIRED, screen, axNodes: ax.nodes }, null, 2));
    const size = dimensions(png); assert.deepEqual(size, WINDOW); assert.ok(png.length > 10_000, `PNG too small: ${png.length}`); const uniqueBytes = new Set(png).size; assert.ok(uniqueBytes > 64, `PNG nearly blank: ${uniqueBytes} unique byte values`);
    console.log(`TASK0379_PNG=${PNG_PATH}`); console.log(`TASK0379_TREE=${TREE_PATH}`); console.log(`TASK0379_WINDOW=${size.width}x${size.height}`); console.log(`TASK0379_PNG_BYTES=${png.length}`); console.log(`TASK0379_PNG_UNIQUE_BYTES=${uniqueBytes}`); console.log(`TASK0379_REQUIRED=${REQUIRED.join("|")}`); console.log(`TASK0379_CONTROLS=${screen.controls.map((control) => control.text || control.ariaLabel || control.value).filter(Boolean).join("|")}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60_000 });
