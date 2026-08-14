import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0357-restore-account.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0357-restore-account-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = Object.freeze(["Restore your account", "password", "Restore", "Back"]);

function treeText(nodes) {
  return nodes.flatMap((node) => [node.role?.value, node.name?.value, node.value?.value, node.description?.value])
    .filter((value) => typeof value === "string" && value.trim()).join("\n");
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.text || "page evaluation threw");
  return result.result.value;
}

test("TASK 0357 captures the fixed Restore your account screen", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-0357-restore-account-fixture.html`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, `new Promise((resolve, reject) => { const deadline = Date.now() + 10000; const tick = () => { if (document.querySelector("#route-heading")) { requestAnimationFrame(() => requestAnimationFrame(resolve)); return; } if (Date.now() > deadline) { reject(new Error("Restore account form did not render")); return; } setTimeout(tick, 25); }; tick(); })`);
    const screen = await evaluate(page, `(() => { const text = document.body.innerText.replace(/\\s+/gu, " ").trim(); const controls = [...document.querySelectorAll("button, input, textarea")].map((element) => ({ tag: element.tagName.toLowerCase(), id: element.id, type: element.getAttribute("type") || "", text: element.innerText.replace(/\\s+/gu, " ").trim(), label: element.id ? (document.querySelector('label[for="' + CSS.escape(element.id) + '"]')?.innerText || "").replace(/\\s+/gu, " ").trim() : "", ariaLabel: element.getAttribute("aria-label") || "" })); return { title: document.querySelector("#route-heading")?.textContent || "", text, controls }; })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const combined = `${screen.title}\n${screen.text}\n${screen.controls.map((c) => `${c.label} ${c.ariaLabel} ${c.text}`).join("\n")}\n${treeText(ax.nodes ?? [])}`.toLowerCase();
    for (const required of REQUIRED) assert.match(combined, new RegExp(required.toLowerCase().replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-0357-restore-account-fixture.html`, window: WINDOW, required: REQUIRED, screen, axNodes: ax.nodes }, null, 2));
    assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
    assert.deepEqual({ width: png.readUInt32BE(16), height: png.readUInt32BE(20) }, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    const uniqueBytes = new Set(png).size;
    assert.ok(uniqueBytes > 64, `PNG nearly blank: ${uniqueBytes} unique byte values`);
    console.log(`TASK0357_PNG=${PNG_PATH}`); console.log(`TASK0357_TREE=${TREE_PATH}`); console.log(`TASK0357_WINDOW=${WINDOW.width}x${WINDOW.height}`); console.log(`TASK0357_PNG_BYTES=${png.length}`); console.log(`TASK0357_PNG_UNIQUE_BYTES=${uniqueBytes}`); console.log(`TASK0357_REQUIRED=${REQUIRED.join("|")}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60_000 });
