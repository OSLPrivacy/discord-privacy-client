import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const OUTPUT_DIR = path.join(APP_ROOT, "screenshots", "evidence", "task-0215-name-request-linux");
const BEFORE_PNG = path.join(OUTPUT_DIR, "before-submission-linux.png");
const AFTER_PNG = path.join(OUTPUT_DIR, "after-submission-linux.png");
const REPORT_PATH = path.join(OUTPUT_DIR, "report.json");
const WINDOW = Object.freeze({ width: 1024, height: 700 });
const FIXTURE_PATH = "/screenshots/fixtures/task-0215-name-request-linux.html";
const KNOWN_NAME = "maple_0213";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}${FIXTURE_PATH}` };
}

const READ_SCREEN = `(() => {
  const box = (selector) => {
    const rect = document.querySelector(selector)?.getBoundingClientRect();
    return rect ? { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom } : null;
  };
  const text = (selector) => document.querySelector(selector)?.textContent?.replace(/\\s+/gu, " ").trim() || "";
  const input = document.querySelector("[data-add-friend-name]");
  if (!input) throw new Error("TASK0215 name input missing");
  return {
    viewport: { width: window.innerWidth, height: window.innerHeight },
    input: { value: input.value, placeholder: input.placeholder, ariaLabel: input.getAttribute("aria-label"), box: box("[data-add-friend-name]") },
    label: text("label[for=add-friend-name-input]"),
    button: text("[data-send-name-request]"),
    status: text("[data-name-request-status]"),
    pending: text(".pending-name-request-result"),
    pendingCount: document.querySelector("[data-pending-count]")?.getAttribute("data-pending-count") || "",
    pendingEntry: text("[data-pending-name-request]"),
    panelBox: box(".add-friend-name-request"),
    inputBox: box("[data-add-friend-name]"),
    pendingBox: box(".pending-name-request-result"),
  };
})()`;

function fitsViewport(rect, name) {
  assert.ok(rect, `${name} is absent`);
  assert.ok(rect.x >= 0 && rect.y >= 0 && rect.right <= WINDOW.width && rect.bottom <= WINDOW.height, `${name} is clipped on the fixed screen`);
}

test("TASK 0215 captures the Linux name-request fixture before and after submission", async () => {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, "(async () => { await document.fonts.ready; await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))); return 'painted'; })()");

    const before = await evaluate(page, READ_SCREEN);
    assert.deepEqual(before.viewport, WINDOW);
    assert.equal(before.label, "Their exact OSL name");
    assert.equal(before.input.value, "", "the before-submission name field must be clear");
    assert.equal(before.input.placeholder, "OSL name");
    assert.equal(before.button, "Send Request");
    assert.equal(before.pendingCount, "0");
    assert.match(before.pending, /Pending requests \(0\).*No pending name requests\./u);
    assert.ok(before.inputBox.width >= 500 && before.inputBox.height >= 44, "the name field is not clearly usable");
    fitsViewport(before.panelBox, "request panel");
    fitsViewport(before.inputBox, "name field");
    fitsViewport(before.pendingBox, "initial Pending result");

    const beforePng = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(BEFORE_PNG, beforePng);
    const beforeImage = imageFacts(beforePng, { nameField: before.inputBox, pending: before.pendingBox });
    assert.deepEqual({ width: beforeImage.width, height: beforeImage.height }, WINDOW);
    assert.ok(beforeImage.distinctColors >= 40, "before-submission capture is nearly blank");
    assert.ok(beforeImage.crops.nameField.distinctColors >= 3, "the clear name field did not paint");

    const after = await evaluate(page, `(async () => {
      const input = document.querySelector("[data-add-friend-name]");
      input.value = ${JSON.stringify(KNOWN_NAME)};
      input.dispatchEvent(new Event("input", { bubbles: true }));
      document.querySelector("[data-send-name-request]").click();
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return ${READ_SCREEN};
    })()`);
    assert.equal(after.input.value, "", "the submitted name field must clear after a successful request");
    assert.equal(after.pendingCount, "1");
    assert.match(after.status, new RegExp(`Request sent to ${KNOWN_NAME}\\. It is pending until they accept\\.`));
    assert.equal(after.pendingEntry, `Request to ${KNOWN_NAME} is pending until they accept.`);
    assert.match(after.pending, new RegExp(`Pending requests \\(1\\).*${KNOWN_NAME}`));
    fitsViewport(after.inputBox, "cleared name field after submission");
    fitsViewport(after.pendingBox, "Pending result after submission");

    const afterPng = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(AFTER_PNG, afterPng);
    const afterImage = imageFacts(afterPng, { nameField: after.inputBox, pending: after.pendingBox });
    assert.deepEqual({ width: afterImage.width, height: afterImage.height }, WINDOW);
    assert.ok(afterImage.distinctColors >= 40, "after-submission capture is nearly blank");
    assert.ok(afterImage.crops.pending.distinctColors >= 3, "the Pending result did not paint");

    const report = {
      schema: "task-0215-name-request-linux/v1",
      window: WINDOW,
      fixture: FIXTURE_PATH,
      before,
      after,
      images: {
        before: { path: path.basename(BEFORE_PNG), bytes: beforePng.length, sha256: sha256(beforePng), ...beforeImage },
        after: { path: path.basename(AFTER_PNG), bytes: afterPng.length, sha256: sha256(afterPng), ...afterImage },
      },
    };
    writeFileSync(REPORT_PATH, `${JSON.stringify(report, null, 2)}\n`);
    console.log(`TASK0215_BEFORE window=${beforeImage.width}x${beforeImage.height} clear_name_field=${JSON.stringify(before.input.value)} label=${JSON.stringify(before.label)} pending_count=${before.pendingCount} png_bytes=${beforePng.length}`);
    console.log(`TASK0215_AFTER submitted_name=${KNOWN_NAME} clear_name_field=${JSON.stringify(after.input.value)} pending_count=${after.pendingCount} pending_entry=${JSON.stringify(after.pendingEntry)} png_bytes=${afterPng.length}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
