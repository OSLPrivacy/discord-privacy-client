import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0555-timer-overlay.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0555-timer-overlay-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const response = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (response.exceptionDetails) throw new Error(response.exceptionDetails.exception?.description || response.exceptionDetails.text);
  return response.result.value;
}

test("TASK 0555 opens the timer picker over a greyed-out chat with default 00 Days on Linux", async () => {
  assert.equal(process.platform, "linux", "TASK 0555 capture must run on Linux");
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  assert.ok(address && typeof address !== "string", "Vite did not expose a TCP address");
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0555-timer-overlay-fixture.html`, { timeoutMs: 30_000 });
    await evaluate(page, 'document.querySelector("#open-timer-picker").click()');
    const screen = await evaluate(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        const overlay = document.querySelector(".timer-overlay");
        if (document.documentElement.dataset.task0555 === "timer-picker-open" && overlay) {
          requestAnimationFrame(() => requestAnimationFrame(() => resolve({
            chat: document.querySelector(".timer-chat")?.getAttribute("aria-label"),
            scrim: getComputedStyle(document.querySelector(".timer-overlay-scrim")).backgroundColor,
            fields: [...document.querySelectorAll(".timer-overlay-field")].map((field) => ({ label: field.querySelector(".timer-overlay-field-label")?.textContent.trim(), value: field.querySelector("input")?.value })),
          })));
          return;
        }
        if (Date.now() > deadline) return reject(new Error("timer picker did not open over chat"));
        setTimeout(tick, 25);
      };
      tick();
    })`);
    assert.equal(screen.chat, "OSL chat with Asha");
    assert.equal(screen.scrim, "rgba(10, 10, 10, 0.72)");
    assert.deepEqual(screen.fields, [{ label: "Days", value: "00" }, { label: "Hours", value: "00" }, { label: "Minutes", value: "00" }, { label: "Seconds", value: "00" }]);
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const dimensions = pngDimensions(readFileSync(PNG_PATH));
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    writeFileSync(TREE_PATH, JSON.stringify({ platform: process.platform, window: WINDOW, screenshot: path.basename(PNG_PATH), screen }, null, 2));
    console.log(`TASK0555_PLATFORM=${process.platform}`);
    console.log(`TASK0555_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0555_CHAT=${screen.chat}`);
    console.log(`TASK0555_GREY_SCRIM=${screen.scrim}`);
    console.log(`TASK0555_DAYS=${screen.fields[0].value} ${screen.fields[0].label}`);
    console.log(`TASK0555_PNG=${PNG_PATH}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
