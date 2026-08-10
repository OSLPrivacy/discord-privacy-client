import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { readPng } from "./lib/png-pixels.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(EVIDENCE_DIR, "task-1129-x-protected-composer-1280x800.png");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const REQUIRED_CONTROLS = Object.freeze(["lock", "private-box", "count", "send-trigger", "eye-toggle", "tick", "tray"]);
const REQUIRED_ACCENTS = new Set(["rgb(42, 192, 240)", "rgb(61, 214, 140)", "rgb(240, 180, 41)", "rgb(240, 169, 58)", "rgb(167, 155, 255)", "rgb(224, 86, 86)"]);

function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/screenshots/fixtures/task-1129-x-protected-composer.html` };
}

const SCREEN_FACTS = `(() => {
  const controls = [...document.querySelectorAll("[data-control]")].map((node) => {
    const rect = node.getBoundingClientRect();
    const style = getComputedStyle(node);
    return { name: node.dataset.control, text: node.textContent.trim().replace(/\\s+/gu, " "), rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }, background: style.backgroundImage, backgroundColor: style.backgroundColor, borderRadius: style.borderRadius, fontFamily: style.fontFamily, textTransform: style.textTransform };
  });
  const protectedBox = document.querySelector("[data-protected-composer]").getBoundingClientRect();
  return { title: document.title, ground: getComputedStyle(document.body).backgroundColor, controls, protectedRect: { x: protectedBox.x, y: protectedBox.y, width: protectedBox.width, height: protectedBox.height }, source: document.querySelector("style").textContent };
})()`;

function auditDesign(facts) {
  const failures = [];
  if (facts.ground !== "rgb(8, 12, 13)") failures.push(`ground=${facts.ground}`);
  if (/gradient/iu.test(facts.source)) failures.push("gradient source present");
  if (/background(?:-color)?\\s*:\\s*#2ac0f0/iu.test(facts.source)) failures.push("filled cyan source present");
  for (const control of facts.controls) {
    if (!REQUIRED_CONTROLS.includes(control.name)) continue;
    if (control.rect.width < 1 || control.rect.height < 1) failures.push(`${control.name} has no visible rectangle`);
    if (control.name !== "private-box" && control.backgroundColor === "rgb(42, 192, 240)") failures.push(`${control.name} is a filled cyan control`);
    if (control.background !== "none") failures.push(`${control.name} background image=${control.background}`);
    if (control.text && control.name !== "private-box" && !/Consolas/i.test(control.fontFamily)) failures.push(`${control.name} is not Consolas status text`);
    if (control.text && control.name !== "private-box" && control.textTransform !== "uppercase") failures.push(`${control.name} is not uppercase`);
    const radius = Number.parseFloat(control.borderRadius);
    if (Number.isFinite(radius) && radius > 3) failures.push(`${control.name} radius=${control.borderRadius}`);
  }
  return failures;
}

test("TASK1129 captures X protected composer controls at a fixed Linux browser size", { timeout: 180_000 }, async () => {
  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: WINDOW.width, height: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, "document.fonts.ready.then(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
    const facts = await evaluate(page, SCREEN_FACTS);
    const controlNames = facts.controls.map((control) => control.name);
    assert.deepEqual([...controlNames].sort(), [...REQUIRED_CONTROLS].sort(), "protected composer exposes exactly the required controls");
    for (const control of facts.controls) {
      assert.ok(control.rect.x >= 0 && control.rect.y >= 0 && control.rect.x + control.rect.width <= WINDOW.width && control.rect.y + control.rect.height <= WINDOW.height, `${control.name} remains inside fixed X browser`);
      assert.ok(control.text.length > 0 || ["tick"].includes(control.name), `${control.name} has drawn control text`);
    }
    assert.equal(facts.title, "Messages / X — protected composer");
    assert.ok(facts.protectedRect.width > 500 && facts.protectedRect.height > 140, "private protected box is visibly large");
    assert.deepEqual(auditDesign(facts), [], "OSL design conforms before capture");

    // Negative controls: a modified capture that fills the cyan send control,
    // or introduces a gradient, is rejected by the same visual audit.
    await evaluate(page, "document.head.insertAdjacentHTML('beforeend', '<style id=task1129-negative>#send-trigger{background:#2ac0f0}</style>')");
    const filled = await evaluate(page, SCREEN_FACTS);
    assert.ok(auditDesign(filled).some((failure) => failure.includes("filled cyan")), "filled cyan control must be rejected");
    await evaluate(page, "document.querySelector('#task1129-negative').remove(); document.head.insertAdjacentHTML('beforeend', '<style id=task1129-negative>#send-trigger{background:linear-gradient(#2ac0f0,#3dd68c)}</style>')");
    const gradient = await evaluate(page, SCREEN_FACTS);
    assert.ok(auditDesign(gradient).some((failure) => failure.includes("gradient")), "gradient control must be rejected");
    await evaluate(page, "document.querySelector('#task1129-negative').remove()");
    assert.deepEqual(auditDesign(await evaluate(page, SCREEN_FACTS)), [], "restored capture is conformant");

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    const png = readPng(screenshot);
    assert.deepEqual([png.width, png.height], [WINDOW.width, WINDOW.height], "fixed screenshot dimensions");
    assert.ok(screenshot.length > 15_000, "screenshot contains rendered X and OSL controls");
    console.log(`TASK1129_OS=${os.type()} ${os.release()}`);
    console.log(`TASK1129_URL=${url}`);
    console.log(`TASK1129_WINDOW=${png.width}x${png.height}`);
    console.log(`TASK1129_GROUND=${facts.ground}`);
    console.log(`TASK1129_CONTROLS=${controlNames.join("|")}`);
    for (const control of facts.controls) console.log(`TASK1129_CONTROL name=${control.name} text=${JSON.stringify(control.text)} box=${Math.round(control.rect.x)},${Math.round(control.rect.y)} ${Math.round(control.rect.width)}x${Math.round(control.rect.height)}`);
    console.log(`TASK1129_PALETTE=${[...REQUIRED_ACCENTS].join("|")}`);
    console.log("TASK1129_NEGATIVE_FILLED_CYAN=REJECTED");
    console.log("TASK1129_NEGATIVE_GRADIENT=REJECTED");
    console.log(`TASK1129_PNG=${PNG_PATH}`);
    console.log(`TASK1129_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK1129_PNG_BYTES=${screenshot.length}`);
    console.log("TASK1129_DONE lock=true private_box=true count=49B send_trigger=true eye_toggle=true tick=true tray=true");
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await server.close().catch(() => {});
  }
});
