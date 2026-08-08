import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0553-timer-overlay.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0553-timer-overlay-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED = Object.freeze(["Days", "Hours", "Minutes", "Seconds"]);

async function startVite() {
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
  };
}

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (evaluated.exceptionDetails) {
    throw new Error(evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text || "page evaluation threw");
  }
  return evaluated.result.value;
}

test("TASK 0553 captures the greyed-out timer overlay with four fields and 00 days", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${WINDOW.width},${WINDOW.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: WINDOW.width,
      screenHeight: WINDOW.height,
    });
    await page.navigate(`${url}screenshots/task-0553-timer-overlay-fixture.html`, { timeoutMs: 30_000 });

    await evaluateValue(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task0553 === "ready"
          && document.querySelector(".timer-overlay")
          && document.querySelector("#timer-overlay-days")) {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) {
          reject(new Error("timer overlay did not render"));
          return;
        }
        setTimeout(tick, 25);
      };
      tick();
    })`);

    const screen = await evaluateValue(page, `(() => {
      const fields = [...document.querySelectorAll(".timer-overlay-field")].map((field) => ({
        label: field.querySelector(".timer-overlay-field-label")?.textContent?.trim() || "",
        value: field.querySelector("input")?.value || "",
      }));
      return {
        hasOverlay: Boolean(document.querySelector(".timer-overlay")),
        hasScrim: Boolean(document.querySelector(".timer-overlay-scrim")),
        fields,
      };
    })()`);

    assert.equal(screen.hasOverlay, true, "expected .timer-overlay to be present");
    assert.equal(screen.hasScrim, true, "expected the greyed-out scrim to be present");
    assert.deepEqual(screen.fields.map((field) => field.label), REQUIRED);
    assert.equal(screen.fields[0].value, "00", "expected Days to default to 00");

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-0553-timer-overlay-fixture.html`, window: WINDOW, required: REQUIRED, screen }, null, 2));

    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    const uniqueBytes = new Set(png).size;
    assert.ok(uniqueBytes > 64, `PNG nearly blank: ${uniqueBytes} unique byte values`);

    console.log(`TASK0553_PNG=${PNG_PATH}`);
    console.log(`TASK0553_TREE=${TREE_PATH}`);
    console.log(`TASK0553_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0553_PNG_BYTES=${png.length}`);
    console.log(`TASK0553_PNG_UNIQUE_BYTES=${uniqueBytes}`);
    console.log(`TASK0553_FIELDS=${screen.fields.map((field) => `${field.label}=${field.value}`).join("|")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
