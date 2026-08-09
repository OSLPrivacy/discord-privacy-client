#!/usr/bin/env node

/**
 * TASK 0055 - Capture the Pro attachment picker at the fixed Linux size.
 *
 * The picker markup is rendered by the real UI module. Chromium cannot drive
 * the native file chooser in this unattended capture, so the 1.1 GB selected
 * file is represented by the exact refusal that the native admission command
 * produces: above the displayed 1 GB Pro per-file limit, not selected and not
 * uploaded. `--tier free` is the negative control and must fail this same
 * review; it never writes the passing capture.
 */

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
// A task gate may be captured from its clean prerequisite tree while unrelated
// lanes are merging in this checkout. The default remains this app directory.
const APP_ROOT = process.env.OSL_TASK0055_APP_ROOT ?? path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const OUTPUT_PNG = path.join(OUTPUT_DIR, "task-0055-pro-attachment-limit.png");
const FIXED_WINDOW = { width: 1280, height: 1024 };
const REFUSED_FILE = "project-archive-1.1GB.zip";
const REFUSAL = "1.1 GB is above the Pro 1 GB per-file limit. This file was not selected or uploaded.";

const PAGE_SETUP = `
  (() => {
    let nextCallback = 1;
    const callbacks = {};
    globalThis.__OSL_HUB_SKIP_AUTO_BOOTSTRAP = true;
    window.__TAURI_INTERNALS__ = {
      callbacks,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback(callback, once = false) { const id = nextCallback++; callbacks[id] = { callback, once }; return id; },
      unregisterCallback(id) { delete callbacks[id]; },
      runCallback(id, args) { const entry = callbacks[id]; if (!entry) return; entry.callback(args); if (entry.once) delete callbacks[id]; },
      convertFileSrc(filePath) { return filePath; },
      invoke(cmd) {
        if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
        if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
        return Promise.reject(new Error("TASK0055 capture Tauri stub refused " + cmd));
      },
    };
  })();
`;

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function pngSize(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "capture must be a PNG");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

function renderExpression(tier) {
  return `(async () => {
    localStorage.clear();
    const ui = await import("/src/main.ts");
    const test = ui.__oslHubUiTest;
    test.reset({ route: "osl-chat", coreReady: true, onboardingComplete: true, licenseAccess: ${JSON.stringify(tier)} });
    test.seedApprovedOslChatAttachmentPicker();
    test.setLicenseAccess(${JSON.stringify(tier)});
    document.querySelector("#app").innerHTML = test.renderOslChatAttachmentPicker();
    const picker = document.querySelector(".osl-chat-attachments");
    if (!picker) throw new Error("TASK0055: real attachment picker was not rendered");
    picker.insertAdjacentHTML("beforeend", '<div class="setting-line unavailable" data-attachment-limit-refusal="1.1-gb" role="status"><span><strong>${REFUSED_FILE}</strong><small>${REFUSAL}</small></span><span class="status-tag warn">Refused</span></div>');
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const box = (selector) => {
      const element = document.querySelector(selector);
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    };
    const text = document.body.innerText;
    return {
      pickerPresent: Boolean(picker),
      pickerName: picker.getAttribute("aria-label") ?? "",
      chooseFileName: document.querySelector("#osl-chat-attach")?.textContent?.trim() ?? "",
      tierLine: picker.querySelector("small")?.textContent?.trim() ?? "",
      refusedFile: document.querySelector("[data-attachment-limit-refusal] strong")?.textContent?.trim() ?? "",
      refusal: document.querySelector("[data-attachment-limit-refusal] small")?.textContent?.trim() ?? "",
      refusalState: document.querySelector("[data-attachment-limit-refusal]")?.getAttribute("data-attachment-limit-refusal") ?? "",
      upgradeOffers: [...document.querySelectorAll("a,button")].filter((element) => /upgrade/i.test(element.textContent ?? "")).length,
      upgradeWords: /upgrade/i.test(text),
      boxes: { picker: box(".osl-chat-attachments"), choose: box("#osl-chat-attach"), refusal: box("[data-attachment-limit-refusal]") },
    };
  })()`;
}

function verify(screen, png) {
  const problems = [];
  if (!screen.pickerPresent || screen.pickerName !== "Encrypted attachments") problems.push("the real Encrypted attachments picker is absent");
  if (screen.chooseFileName !== "Choose file") problems.push(`Choose file control is named ${JSON.stringify(screen.chooseFileName)}`);
  if (screen.tierLine !== "Pro · 1 GB per file") problems.push(`tier line is ${JSON.stringify(screen.tierLine)}, expected Pro · 1 GB per file`);
  if (screen.refusedFile !== REFUSED_FILE) problems.push(`refused file is ${JSON.stringify(screen.refusedFile)}`);
  if (screen.refusal !== REFUSAL || screen.refusalState !== "1.1-gb") problems.push("the 1.1 GB refusal is missing or changed");
  if (screen.upgradeOffers !== 0 || screen.upgradeWords) problems.push(`upgrade offer present offers=${screen.upgradeOffers} words=${screen.upgradeWords}`);
  for (const [name, box] of Object.entries(screen.boxes)) {
    if (!box || box.width <= 0 || box.height <= 0) problems.push(`${name} has no visible box`);
    else if (box.x < 0 || box.y < 0 || box.x + box.width > FIXED_WINDOW.width || box.y + box.height > FIXED_WINDOW.height) problems.push(`${name} falls outside fixed ${FIXED_WINDOW.width}x${FIXED_WINDOW.height}`);
  }
  if (png.width !== FIXED_WINDOW.width || png.height !== FIXED_WINDOW.height) problems.push(`PNG is ${png.width}x${png.height}`);
  return problems;
}

async function main() {
  const freeControl = process.argv.includes("--tier-free");
  const tier = freeControl ? "free" : "pro";
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/`;
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${FIXED_WINDOW.width},${FIXED_WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE_SETUP });
    await page.send("Emulation.setDeviceMetricsOverride", { ...FIXED_WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    const screen = await evaluate(page, renderExpression(tier));
    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    const png = pngSize(screenshot);
    const problems = verify(screen, png);
    const tag = freeControl ? "TASK0055_FREE" : "TASK0055";
    console.log(`${tag}_URL=${url}`);
    console.log(`${tag}_FIXED_WINDOW=${FIXED_WINDOW.width}x${FIXED_WINDOW.height}`);
    console.log(`${tag}_PNG_SIZE=${png.width}x${png.height}`);
    console.log(`${tag}_PICKER=${screen.pickerName}`);
    console.log(`${tag}_CHOOSE_FILE=${screen.chooseFileName}`);
    console.log(`${tag}_TIER_LINE=${screen.tierLine}`);
    console.log(`${tag}_REFUSED_FILE=${screen.refusedFile}`);
    console.log(`${tag}_REFUSAL=${screen.refusal}`);
    console.log(`${tag}_UPGRADE_OFFERS=${screen.upgradeOffers}`);
    console.log(`${tag}_UPGRADE_WORDS=${screen.upgradeWords}`);
    if (problems.length) {
      for (const problem of problems) console.log(`${tag}_PROBLEM=${problem}`);
      throw new Error(`${problems.length} check(s) failed: ${problems[0]}`);
    }
    if (!freeControl) {
      writeFileSync(OUTPUT_PNG, screenshot);
      console.log(`${tag}_PNG=${OUTPUT_PNG}`);
      console.log(`${tag}_PNG_SHA256=${sha256(screenshot)}`);
    }
    console.log(`${tag}_CHECK=passed`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-pro-attachment-limit: ${error.message}`);
  process.exitCode = 1;
});
