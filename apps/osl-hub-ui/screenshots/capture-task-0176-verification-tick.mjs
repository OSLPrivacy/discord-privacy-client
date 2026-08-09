#!/usr/bin/env node

/**
 * TASK 0176 - render gate 0175's paired verification-tick fixtures at one
 * Linux window size and save the two visual receipts.
 */

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { transform } from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = process.env.TASK0176_APP_ROOT;
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const WINDOW = { width: 1280, height: 800 };
const FIXTURES = [
  { name: "two-way", query: "", expectedTicks: 2 },
  { name: "one-way", query: "?fixture=one-way", expectedTicks: 0 },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function pngDimensions(bytes) {
  if (!bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    throw new Error("capture is not a PNG");
  }
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

async function main() {
  if (!APP_ROOT) throw new Error("TASK0176_APP_ROOT must name an OSL Hub UI checkout containing TASK 0175");
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const sourceDir = path.join(APP_ROOT, "src");
  const fixtureDir = path.join(APP_ROOT, "screenshots", "fixtures");
  const moduleSource = readFileSync(path.join(sourceDir, "osl-chats-view.ts"), "utf8");
  const renderedModule = await transform(moduleSource, { loader: "ts", format: "esm", target: "es2022" });
  const { oslChatsViewMarkup } = await import(`data:text/javascript;base64,${Buffer.from(renderedModule.code).toString("base64")}`);
  const stylesheet = readFileSync(path.join(sourceDir, "styles.css"), "utf8");
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    for (const fixture of FIXTURES) {
      const fixtureFile = fixture.name === "one-way" ? "task-0175-verification-tick-one-way.json" : "task-0175-verification-tick-two-way.json";
      const friend = JSON.parse(readFileSync(path.join(fixtureDir, fixtureFile), "utf8"));
      const markup = oslChatsViewMarkup({ friends: [friend], activePersonId: friend.personId, messages: [], draft: "", busy: false });
      await page.navigate("about:blank", { timeoutMs: 10_000 });
      const frameTree = await page.send("Page.getFrameTree");
      await page.send("Page.setDocumentContent", { frameId: frameTree.frameTree.frame.id, html: `<!doctype html><html data-task0175="ready"><head><meta charset="utf-8"><title>Verification tick</title><style>${stylesheet}</style></head><body><div id="app">${markup}</div></body></html>` });
      const facts = await evaluate(page, `(() => {
          const ticks = [...document.querySelectorAll('[data-osl-verification-tick="visible"]')];
          return {
            title: document.title,
            ready: document.documentElement.dataset.task0175,
            tickCount: ticks.length,
            tickTexts: ticks.map((tick) => tick.textContent.trim()),
            tickTitles: ticks.map((tick) => tick.getAttribute("title")),
            tickRects: ticks.map((tick) => { const r = tick.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; }),
            hasTickMarkup: document.documentElement.outerHTML.includes("data-osl-verification-tick"),
            visibleText: document.body.innerText,
          };
        })()`);
      await evaluate(page, "document.fonts.ready.then(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
      const png = await page.screenshot({ captureBeyondViewport: false });
      const pngPath = path.join(OUTPUT_DIR, `task-0176-linux-verification-tick-${fixture.name}-1280x800.png`);
      writeFileSync(pngPath, png);
      const dimensions = pngDimensions(readFileSync(pngPath));
      const visibleRects = facts.tickRects.filter((rect) => rect.width > 0 && rect.height > 0).length;
      console.log(`TASK0176 fixture=${fixture.name} window=${WINDOW.width}x${WINDOW.height} tick_count=${facts.tickCount} visible_tick_rects=${visibleRects} png=${pngPath} png_size=${dimensions.width}x${dimensions.height} sha256=${sha256(png)}`);
      if (facts.ready !== "ready") throw new Error(`${fixture.name}: fixture did not report ready`);
      if (facts.tickCount !== fixture.expectedTicks) throw new Error(`${fixture.name}: tick_count=${facts.tickCount}, expected ${fixture.expectedTicks}`);
      if (dimensions.width !== WINDOW.width || dimensions.height !== WINDOW.height) throw new Error(`${fixture.name}: PNG size ${dimensions.width}x${dimensions.height}, expected ${WINDOW.width}x${WINDOW.height}`);
      if (fixture.expectedTicks > 0 && (visibleRects !== fixture.expectedTicks || !facts.tickTitles.every((title) => title === "Verified"))) {
        throw new Error(`${fixture.name}: required visible verification ticks were not rendered`);
      }
      if (fixture.expectedTicks === 0 && facts.hasTickMarkup) throw new Error(`${fixture.name}: verification tick appeared`);
    }
  } finally {
    await chrome.close();
  }
}

main().catch((error) => { console.error(error.stack || error); process.exitCode = 1; });
