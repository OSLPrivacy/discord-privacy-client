#!/usr/bin/env node

// TASK 0720: capture the privacy level setting screen on Linux.
// Proves the screen offers Basic, Balanced and Maximum, that exactly one
// choice is selected (marked in the DOM, the accessibility tree, and with
// visible pixels), and that the selected choice's real effects are listed.

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const DESTINATION_PNG_PATH = path.join(OUTPUT_DIR, "task-0720-privacy-destination.png");
const SCREEN_PNG_PATH = path.join(OUTPUT_DIR, "task-0720-privacy-level-balanced.png");
const WINDOW = { width: 1280, height: 800 };
const LEVELS = ["basic", "balanced", "maximum"];
const SELECTED_LEVEL = "balanced";

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parsePng(buffer) {
  const signature = buffer.subarray(0, 8);
  if (!signature.equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    throw new Error("screenshot is not a PNG");
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") {
      break;
    }
  }
  if (bitDepth !== 8 || ![2, 6].includes(colorType)) {
    throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType}`);
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * 4);
  let inOffset = 0;
  const prior = Buffer.alloc(stride);
  const row = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[inOffset];
    inOffset += 1;
    for (let x = 0; x < stride; x += 1) {
      const raw = inflated[inOffset + x];
      const left = x >= channels ? row[x - channels] : 0;
      const up = prior[x];
      const upLeft = x >= channels ? prior[x - channels] : 0;
      let value;
      if (filter === 0) value = raw;
      else if (filter === 1) value = raw + left;
      else if (filter === 2) value = raw + up;
      else if (filter === 3) value = raw + Math.floor((left + up) / 2);
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        value = raw + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft);
      } else {
        throw new Error(`unsupported PNG filter ${filter}`);
      }
      row[x] = value & 0xff;
    }
    inOffset += stride;
    row.copy(prior);
    for (let x = 0; x < width; x += 1) {
      const source = x * channels;
      const target = (y * width + x) * 4;
      pixels[target] = row[source];
      pixels[target + 1] = row[source + 1];
      pixels[target + 2] = row[source + 2];
      pixels[target + 3] = channels === 4 ? row[source + 3] : 255;
    }
  }
  return { width, height, pixels };
}

function cropFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const colors = new Set();
  const counts = new Map();
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const key = `${png.pixels[offset]},${png.pixels[offset + 1]},${png.pixels[offset + 2]}`;
      colors.add(key);
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  const total = (x1 - x0) * (y1 - y0);
  const dominant = Math.max(0, ...counts.values());
  return {
    width: x1 - x0,
    height: y1 - y0,
    distinctColors: colors.size,
    // Pixels that are NOT the crop's own dominant color: text, borders, marks.
    nonDominant: total - dominant,
  };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const address = vite.httpServer.address();
  const url = `http://127.0.0.1:${address.port}/`;
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
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
        (() => {
          window.__OSL_HUB_SKIP_AUTO_BOOTSTRAP = true;
          let nextCallback = 1;
          const callbacks = {};
          window.__TAURI_INTERNALS__ = {
            callbacks,
            metadata: {
              currentWindow: { label: "main" },
              currentWebview: { label: "main" },
            },
            transformCallback(callback, once = false) {
              const id = nextCallback++;
              callbacks[id] = { callback, once };
              return id;
            },
            unregisterCallback(id) {
              delete callbacks[id];
            },
            runCallback(id, args) {
              const entry = callbacks[id];
              if (!entry) return;
              entry.callback(args);
              if (entry.once) delete callbacks[id];
            },
            convertFileSrc(filePath) {
              return filePath;
            },
            invoke(cmd) {
              if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
              if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
              if (cmd === "plugin:event|listen") return Promise.resolve(1);
              if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
              if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
              return Promise.reject(new Error("TASK0720 capture Tauri stub refused " + cmd));
            },
          };
        })();
      `,
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });

    // Before: the Privacy destination with the setting screen closed. The
    // privacy level controls must be absent here, or the diff proves nothing.
    const destinationState = await evaluate(page, `(async () => {
      localStorage.clear();
      const ui = await import("/src/main.ts");
      window.__task0720Ui = ui;
      ui.__oslHubUiTest.reset({ route: "privacy", coreReady: true, protectionPreset: ${JSON.stringify(SELECTED_LEVEL)} });
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderRouteShell("privacy");
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      return {
        appRendered: document.querySelector("#app").innerHTML.length > 0,
        heading: document.querySelector("#route-heading")?.textContent?.trim() ?? null,
        choicePresent: Boolean(document.querySelector("[data-privacy-level-choice]")),
      };
    })()`);
    if (!destinationState.appRendered) throw new Error("app shell did not render the Privacy destination");
    if (destinationState.choicePresent) {
      throw new Error("privacy level choices already visible before the setting screen opened");
    }
    const destinationShot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(DESTINATION_PNG_PATH, destinationShot);

    // After: the privacy level setting screen, opened through the same state
    // switch the Change preset button drives.
    const screenState = await evaluate(page, `(async () => {
      const ui = window.__task0720Ui;
      ui.__oslHubUiTest.reset({ route: "privacy", coreReady: true, protectionPreset: ${JSON.stringify(SELECTED_LEVEL)}, privacyLevelScreenOpen: true });
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderRouteShell("privacy");
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const rect = (element) => {
        if (!element) return null;
        const box = element.getBoundingClientRect();
        return { x: box.x, y: box.y, width: box.width, height: box.height };
      };
      const cards = [...document.querySelectorAll("[data-privacy-level-card]")].map((card) => ({
        id: card.dataset.privacyLevelCard,
        selected: card.dataset.selected === "true",
        label: card.querySelector("label strong")?.textContent?.trim() ?? null,
        effectLines: [...card.querySelectorAll("[data-privacy-level-effect]")].map((item) => item.textContent.trim()),
        checked: card.querySelector("input[type=radio]")?.checked ?? false,
      }));
      const selectedCard = document.querySelector('[data-privacy-level-card][data-selected="true"]');
      return {
        heading: document.querySelector("#route-heading")?.textContent?.trim() ?? null,
        cards,
        checkedValues: [...document.querySelectorAll("input[data-privacy-level-choice]")].filter((input) => input.checked).map((input) => input.value),
        selectedMarkText: selectedCard?.querySelector(".privacy-level-selected-mark")?.textContent?.trim() ?? null,
        rects: {
          SelectedCard: rect(selectedCard),
          SelectedEffects: rect(selectedCard?.querySelector(".privacy-level-effects")),
          SelectedMark: rect(selectedCard?.querySelector(".privacy-level-selected-mark")),
        },
      };
    })()`);

    if (screenState.heading !== "Privacy level") {
      throw new Error(`screen title is wrong: ${JSON.stringify(screenState.heading)}`);
    }
    const ids = screenState.cards.map((card) => card.id);
    if (JSON.stringify(ids) !== JSON.stringify(LEVELS)) {
      throw new Error(`level choices are wrong: ${ids.join(", ")}`);
    }
    for (const card of screenState.cards) {
      if (card.effectLines.length !== 6) {
        throw new Error(`${card.id} lists ${card.effectLines.length} effects, expected 6`);
      }
    }
    const selectedCards = screenState.cards.filter((card) => card.selected);
    if (selectedCards.length !== 1 || selectedCards[0].id !== SELECTED_LEVEL) {
      throw new Error(`selected card is wrong: ${selectedCards.map((card) => card.id).join(", ")}`);
    }
    if (JSON.stringify(screenState.checkedValues) !== JSON.stringify([SELECTED_LEVEL])) {
      throw new Error(`checked radios are wrong: ${screenState.checkedValues.join(", ")}`);
    }
    if (screenState.selectedMarkText !== "Selected") {
      throw new Error(`selected mark text is wrong: ${JSON.stringify(screenState.selectedMarkText)}`);
    }
    const selectedEffects = selectedCards[0].effectLines;
    if (!selectedEffects.some((line) => line.includes("every 30 days"))) {
      throw new Error(`Balanced effects lost the 30-day review line: ${selectedEffects.join(" | ")}`);
    }
    for (const [name, rect] of Object.entries(screenState.rects)) {
      if (!rect || rect.width < 8 || rect.height < 8) throw new Error(`${name} has no visible box`);
    }

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const radios = ax.nodes.filter((node) => node.role?.value === "radio");
    const checkedRadioNames = radios
      .filter((node) => node.properties?.some((property) => property.name === "checked" && property.value?.value === "true"))
      .map((node) => node.name?.value?.trim().split("\n")[0] ?? "");
    if (radios.length !== 3) throw new Error(`accessibility tree has ${radios.length} radios, expected 3`);
    if (checkedRadioNames.length !== 1 || !checkedRadioNames[0].startsWith("Balanced")) {
      throw new Error(`accessibility checked radio is wrong: ${JSON.stringify(checkedRadioNames)}`);
    }

    const screenShot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(SCREEN_PNG_PATH, screenShot);
    const png = parsePng(screenShot);
    const crops = Object.fromEntries(
      Object.entries(screenState.rects).map(([name, rect]) => [name, cropFacts(png, rect)]),
    );
    for (const [name, crop] of Object.entries(crops)) {
      if (crop.nonDominant < 50 || crop.distinctColors < 5) {
        throw new Error(`${name} pixels look blank: nonDominant=${crop.nonDominant} distinctColors=${crop.distinctColors}`);
      }
    }
    const destinationSha = sha256(destinationShot);
    const screenSha = sha256(screenShot);
    if (destinationSha === screenSha) throw new Error("setting-screen capture is identical to the destination capture");

    console.log(`TASK0720_URL=${url}`);
    console.log(`TASK0720_DESTINATION_PNG=${DESTINATION_PNG_PATH}`);
    console.log(`TASK0720_DESTINATION_PNG_SHA256=${destinationSha}`);
    console.log(`TASK0720_SCREEN_PNG=${SCREEN_PNG_PATH}`);
    console.log(`TASK0720_SCREEN_PNG_SHA256=${screenSha}`);
    console.log(`TASK0720_SCREEN_SIZE=${png.width}x${png.height}`);
    console.log(`TASK0720_CAPTURES_DIFFER=${destinationSha !== screenSha}`);
    console.log(`TASK0720_TITLE=${screenState.heading}`);
    console.log(`TASK0720_CHOICES=${screenState.cards.map((card) => card.label).join("|")}`);
    console.log(`TASK0720_SELECTED=${selectedCards[0].label}`);
    console.log(`TASK0720_SELECTED_MARK=${screenState.selectedMarkText}`);
    console.log(`TASK0720_AX_CHECKED_RADIO=${checkedRadioNames[0]}`);
    for (const card of screenState.cards) {
      console.log(`TASK0720_EFFECTS_${card.id.toUpperCase()}=${card.effectLines.join(" | ")}`);
    }
    for (const [name, crop] of Object.entries(crops)) {
      console.log(`TASK0720_IMAGE_${name.toUpperCase()}_NONDOMINANT=${crop.nonDominant}`);
      console.log(`TASK0720_IMAGE_${name.toUpperCase()}_DISTINCT_COLORS=${crop.distinctColors}`);
    }
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-privacy-level: ${error.stack || error.message}`);
  process.exitCode = 1;
});
