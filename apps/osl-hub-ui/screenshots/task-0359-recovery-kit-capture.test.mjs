import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUT = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG = path.join(OUT, "task-0359-recovery-kit.png");
const TREE = path.join(OUT, "task-0359-recovery-kit-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = Object.freeze(["Save your recovery kit", "Account details", "Copy", "Saved my recovery kit", "Continue", "Retype the requested words"]);

async function server() { const s = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } }); await s.listen(); const a = s.httpServer.address(); return { s, url: `http://127.0.0.1:${a.port}/` }; }
function treeText(nodes) { return nodes.flatMap((n) => [n.role?.value, n.name?.value, n.value?.value, n.description?.value]).filter((v) => typeof v === "string" && v.trim()).join("\n"); }
function pngDimensions(b) { assert.equal(b.subarray(0, 8).toString("hex"), "89504e470d0a1a0a"); return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) }; }

test("TASK 0359 captures the fixed Save your recovery kit screen", async () => {
  const { s, url } = await server(); const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] }); const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`${url}screenshots/task-0359-recovery-kit-fixture.html`, { timeoutMs: 30000 }); await page.send("Accessibility.enable");
    const screen = await page.send("Runtime.evaluate", { expression: `(() => ({ title: document.querySelector("h1")?.textContent || "", text: document.body.innerText, controls: [...document.querySelectorAll("button,input")].map((e) => ({ tag: e.tagName, text: e.innerText || "", aria: e.getAttribute("aria-label") || "", checked: e.checked ?? null })) }))()`, returnByValue: true });
    const ax = await page.send("Accessibility.getFullAXTree"); const joined = `${screen.result.value.title}\n${screen.result.value.text}\n${treeText(ax.nodes ?? [])}`.toLowerCase();
    for (const required of REQUIRED) assert.match(joined, new RegExp(required.toLowerCase().replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    mkdirSync(OUT, { recursive: true }); const png = await page.screenshot({ fromSurface: true }); writeFileSync(PNG, png); writeFileSync(TREE, JSON.stringify({ window: WINDOW, required: REQUIRED, screen: screen.result.value, axNodes: ax.nodes }, null, 2));
    const dimensions = pngDimensions(png); assert.deepEqual(dimensions, WINDOW); assert.ok(png.length > 10000, `PNG too small: ${png.length}`); assert.ok(new Set(png).size > 64, "PNG nearly blank");
    console.log(`TASK0359_PNG=${PNG}`); console.log(`TASK0359_TREE=${TREE}`); console.log(`TASK0359_WINDOW=${dimensions.width}x${dimensions.height}`); console.log(`TASK0359_PNG_BYTES=${png.length}`); console.log(`TASK0359_PNG_UNIQUE_BYTES=${new Set(png).size}`); console.log(`TASK0359_REQUIRED=${REQUIRED.join("|")}`);
  } finally { await page.close(); await chrome.close(); await s.close(); }
}, { timeout: 60000 });
