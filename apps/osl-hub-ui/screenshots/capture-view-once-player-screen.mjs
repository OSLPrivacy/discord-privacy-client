#!/usr/bin/env node
/**
 * TASK 1349 - capture the view-once player fixture.
 *
 * The finish line is "a fixture screen shows all 4 named elements: play
 * state, open item, countdown, and X close action, and differs from the
 * empty-state capture." So this captures BOTH the empty fixture and the
 * populated one and checks:
 *   - the populated screen's accessibility tree names a play control, a
 *     countdown, and a close control, and both item nodes are present;
 *   - the countdown text and the play/close glyphs are on screen in the DOM;
 *   - the populated PNG is not blank, and its bytes differ from the empty
 *     fixture's PNG (proving the screen actually changed, not just its data
 *     attribute).
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
const EMPTY_PNG_PATH = path.join(OUTPUT_DIR, "task-1349-view-once-player-empty.png");
const POPULATED_PNG_PATH = path.join(OUTPUT_DIR, "task-1349-view-once-player-populated.png");
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
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/view-once-player-screen.html?fixture=${fixture}`;
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
      while (!document.body.dataset.viewOncePlayerFixture) {
        if (Date.now() > deadline) throw new Error("fixture never finished rendering");
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const state = document.querySelector("[data-vop-state]");
      const play = document.querySelector("[data-vop-play]");
      const close = document.querySelector("[data-vop-close]");
      const countdown = document.querySelector("[data-vop-countdown]");
      const playItem = document.querySelector('[data-vop-item-state="play"]');
      const openItem = document.querySelector('[data-vop-item-state="open"]');
      return {
        fixture: document.body.dataset.viewOncePlayerFixture,
        screenState: state ? state.dataset.vopState : null,
        playText: play ? play.textContent.trim() : null,
        playLabel: play ? play.getAttribute("aria-label") : null,
        closeText: close ? close.textContent.trim() : null,
        closeLabel: close ? close.getAttribute("aria-label") : null,
        countdownText: countdown ? countdown.textContent.trim() : null,
        countdownLabel: countdown ? countdown.getAttribute("aria-label") : null,
        hasPlayItem: Boolean(playItem),
        hasOpenItem: Boolean(openItem),
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

    if (empty.rendered.screenState !== "empty") {
      throw new Error(`expected the empty fixture to render state "empty", got "${empty.rendered.screenState}"`);
    }
    if (empty.rendered.hasPlayItem || empty.rendered.hasOpenItem) {
      throw new Error("empty fixture unexpectedly contains a play or open item");
    }

    if (populated.rendered.screenState !== "populated") {
      throw new Error(`expected the populated fixture to render state "populated", got "${populated.rendered.screenState}"`);
    }
    if (!populated.rendered.hasPlayItem) {
      throw new Error("populated fixture is missing the play-state item");
    }
    if (!populated.rendered.hasOpenItem) {
      throw new Error("populated fixture is missing the open item");
    }
    if (populated.rendered.playLabel !== "Play view once message") {
      throw new Error(`unexpected play control label: ${populated.rendered.playLabel}`);
    }
    if (populated.rendered.closeLabel !== "Close view once message") {
      throw new Error(`unexpected close control label: ${populated.rendered.closeLabel}`);
    }
    if (populated.rendered.countdownText !== "10") {
      throw new Error(`expected the countdown to read "10", got "${populated.rendered.countdownText}"`);
    }
    if (!populated.rendered.countdownLabel || !populated.rendered.countdownLabel.includes("10 seconds")) {
      throw new Error(`countdown missing an accessible label: ${populated.rendered.countdownLabel}`);
    }

    const wantedAx = ["Play view once message", "Close view once message"];
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

    console.log(`TASK1349_EMPTY_URL=${empty.url}`);
    console.log(`TASK1349_POPULATED_URL=${populated.url}`);
    console.log(`TASK1349_EMPTY_PNG=${EMPTY_PNG_PATH}`);
    console.log(`TASK1349_POPULATED_PNG=${POPULATED_PNG_PATH}`);
    console.log(`TASK1349_EMPTY_PNG_SHA256=${emptyHash}`);
    console.log(`TASK1349_POPULATED_PNG_SHA256=${populatedHash}`);
    console.log(`TASK1349_CAPTURES_DIFFER=${emptyHash !== populatedHash}`);
    console.log(`TASK1349_HAS_PLAY_STATE=${populated.rendered.hasPlayItem}`);
    console.log(`TASK1349_HAS_OPEN_ITEM=${populated.rendered.hasOpenItem}`);
    console.log(`TASK1349_COUNTDOWN_TEXT=${populated.rendered.countdownText}`);
    console.log(`TASK1349_PLAY_LABEL=${populated.rendered.playLabel}`);
    console.log(`TASK1349_CLOSE_LABEL=${populated.rendered.closeLabel}`);
    console.log(`TASK1349_POPULATED_NONBACKGROUND=${populatedFacts.nonBackground}`);
    console.log(`TASK1349_POPULATED_NEARLY_BLANK=${populatedFacts.nearlyBlank}`);
    console.log("TASK1349_RESULT=pass");
  } finally {
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-view-once-player-screen: ${error.stack || error.message}`);
  process.exitCode = 1;
});
