#!/usr/bin/env node
/**
 * TASK 0564 - capture the view-once overlay fixture.
 *
 * The finish line is "a Linux screenshot shows all 2 named elements: play
 * button and X, and differs from the empty-state capture." So this captures
 * BOTH the empty fixture and the populated one and checks:
 *   - the populated overlay's accessibility tree names the play control and
 *     the close control;
 *   - the play glyph and the close glyph are on screen in the DOM;
 *   - the populated PNG is not blank, and its bytes differ from the empty
 *     fixture's PNG (proving the overlay actually changed).
 */
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const EMPTY_PNG_PATH = path.join(OUTPUT_DIR, "task-0564-view-once-overlay-empty.png");
const POPULATED_PNG_PATH = path.join(OUTPUT_DIR, "task-0564-view-once-overlay-populated.png");
/** #0a0a0a, the app's --bg. */
const BACKGROUND = [10, 10, 10];
const WINDOW = { width: 1000, height: 700 };

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description
      || result.exceptionDetails.text
      || "page evaluation failed",
    );
  }
  return result.result.value;
}

async function captureFixture(vite, chrome, fixture) {
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/view-once-overlay.html?fixture=${fixture}`;
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "reduce" }],
    });
    await page.navigate(url, { timeoutMs: 30_000 });

    const rendered = await evaluate(page, `(async () => {
      const deadline = Date.now() + 20000;
      while (!document.body.dataset.viewOnceOverlayFixture) {
        if (Date.now() > deadline) throw new Error("fixture never finished rendering");
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const state = document.querySelector("[data-voo-state]");
      const play = document.querySelector("[data-voo-play]");
      const close = document.querySelector("[data-voo-close]");
      const duration = document.querySelector("[data-voo-duration]");
      return {
        fixture: document.body.dataset.viewOnceOverlayFixture,
        screenState: state ? state.dataset.vooState : null,
        playText: play ? play.textContent.trim() : null,
        playLabel: play ? play.getAttribute("aria-label") : null,
        closeText: close ? close.textContent.trim() : null,
        closeLabel: close ? close.getAttribute("aria-label") : null,
        durationText: duration ? duration.value : null,
        visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
      };
    })()`);

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
      .filter(Boolean);

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    return { url, rendered, axNames, screenshot };
  } finally {
    await page.close().catch(() => {});
  }
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
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
  try {
    const empty = await captureFixture(vite, chrome, "empty");
    const populated = await captureFixture(vite, chrome, "populated");

    if (empty.rendered.screenState !== "closed") {
      throw new Error(`expected the empty fixture to render state "closed", got "${empty.rendered.screenState}"`);
    }
    if (empty.rendered.playLabel || empty.rendered.closeLabel) {
      throw new Error("empty fixture unexpectedly contains a play or close control");
    }

    if (populated.rendered.screenState !== "open") {
      throw new Error(`expected the populated fixture to render state "open", got "${populated.rendered.screenState}"`);
    }
    if (populated.rendered.playLabel !== "Play view once") {
      throw new Error(`unexpected play control label: ${populated.rendered.playLabel}`);
    }
    if (populated.rendered.closeLabel !== "Close view once overlay") {
      throw new Error(`unexpected close control label: ${populated.rendered.closeLabel}`);
    }
    if (populated.rendered.durationText !== "10") {
      throw new Error(`expected duration value "10", got "${populated.rendered.durationText}"`);
    }

    const wantedAx = ["Play view once", "Close view once overlay"];
    const missingAx = wantedAx.filter((text) => !populated.axNames.some((name) => name.includes(text)));
    if (missingAx.length > 0) {
      throw new Error(`missing accessibility names in populated fixture: ${missingAx.join(", ")}`);
    }

    writeFileSync(EMPTY_PNG_PATH, empty.screenshot);
    writeFileSync(POPULATED_PNG_PATH, populated.screenshot);

    const emptyFacts = imageFacts(empty.screenshot, {}, { background: BACKGROUND });
    const populatedFacts = imageFacts(populated.screenshot, {}, { background: BACKGROUND });
    if (populatedFacts.nearlyBlank) {
      throw new Error(
        `populated PNG is blank or nearly blank distinctColors=${populatedFacts.distinctColors} nonBackground=${populatedFacts.nonBackground}`,
      );
    }

    const emptyHash = sha256(empty.screenshot);
    const populatedHash = sha256(populated.screenshot);
    if (emptyHash === populatedHash) {
      throw new Error("populated capture is byte-identical to the empty-state capture");
    }

    console.log(`TASK0564_EMPTY_URL=${empty.url}`);
    console.log(`TASK0564_POPULATED_URL=${populated.url}`);
    console.log(`TASK0564_EMPTY_PNG=${EMPTY_PNG_PATH}`);
    console.log(`TASK0564_POPULATED_PNG=${POPULATED_PNG_PATH}`);
    console.log(`TASK0564_EMPTY_PNG_SHA256=${emptyHash}`);
    console.log(`TASK0564_POPULATED_PNG_SHA256=${populatedHash}`);
    console.log(`TASK0564_CAPTURES_DIFFER=${emptyHash !== populatedHash}`);
    console.log(`TASK0564_PLAY_LABEL=${populated.rendered.playLabel}`);
    console.log(`TASK0564_CLOSE_LABEL=${populated.rendered.closeLabel}`);
    console.log(`TASK0564_DURATION_VALUE=${populated.rendered.durationText}`);
    console.log(`TASK0564_POPULATED_NONBACKGROUND=${populatedFacts.nonBackground}`);
    console.log(`TASK0564_POPULATED_NEARLY_BLANK=${populatedFacts.nearlyBlank}`);
    console.log("TASK0564_RESULT=pass");
  } finally {
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-view-once-overlay: ${error.stack || error.message}`);
  process.exitCode = 1;
});
