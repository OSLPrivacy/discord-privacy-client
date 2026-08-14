import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { assertComparablePngDimensions, countDistinctRgb, readPng } from "./lib/png-pixels.mjs";
import { blankRgbaPng } from "./lib/png-test-fixtures.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0357-restore-account.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0357-restore-account-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const DISTINCT_RGB_FLOOR = 32;
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

function assertDecodedDistinctRgb(png, label) {
  const decodedDistinctRgb = countDistinctRgb(readPng(png));
  assert.ok(decodedDistinctRgb >= DISTINCT_RGB_FLOOR,
    `${label} has too few decoded distinct RGB colours: ${decodedDistinctRgb} (floor ${DISTINCT_RGB_FLOOR})`);
  return decodedDistinctRgb;
}

test("TASK 0357 captures the fixed Restore your account screen", async () => {
  const blankPng = blankRgbaPng(WINDOW.width, WINDOW.height);
  const blankDecodedDistinctRgb = countDistinctRgb(readPng(blankPng));
  assert.throws(() => assertDecodedDistinctRgb(blankPng, "blank PNG"), /too few decoded distinct RGB colours: 1/u);
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
    assertComparablePngDimensions(png, WINDOW, "capture PNG");
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    const decodedDistinctRgb = assertDecodedDistinctRgb(png, "capture PNG");
    console.log(`TASK0357_PNG=${PNG_PATH}`); console.log(`TASK0357_TREE=${TREE_PATH}`); console.log(`TASK0357_WINDOW=${WINDOW.width}x${WINDOW.height}`); console.log(`TASK0357_PNG_BYTES=${png.length}`); console.log(`TASK0357_BLANK_DECODED_DISTINCT_RGB=${blankDecodedDistinctRgb} floor=${DISTINCT_RGB_FLOOR} rejected=true`); console.log(`TASK0357_DECODED_DISTINCT_RGB=${decodedDistinctRgb} floor=${DISTINCT_RGB_FLOOR}`); console.log(`TASK0357_REQUIRED=${REQUIRED.join("|")}`);
  } finally { await page.close(); await chrome.close(); await server.close(); }
}, { timeout: 60_000 });
