#!/usr/bin/env node

// TASK 0722: screenshot the privacy level setting screen once per selected
// level. Writes three named 1280x800 Linux screenshots - Basic, Balanced and
// Maximum each selected in turn - and checks every capture for the named
// controls, the selected state (DOM, accessibility tree, and pixels) and the
// selected level's explanation. The run then renders a throwaway copy of the
// same screen with one named control removed and requires the very same check
// to fail on it; a check that accepts that copy fails the whole run.
//
// Set TASK0722_BREAK to a named-control name (e.g. "Selected mark") to remove
// that control during the real captures instead: the run must go red.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

export const CAPTURE_WINDOW = Object.freeze({ width: 1280, height: 800 });
export const LEVELS = Object.freeze(["basic", "balanced", "maximum"]);

// Every control the check looks up by name. The throwaway copy drops one of
// these; validatePrivacyLevelCapture must name it when it fails.
export const NAMED_CONTROLS = Object.freeze({
  "Basic choice": 'input[data-privacy-level-choice="basic"]',
  "Balanced choice": 'input[data-privacy-level-choice="balanced"]',
  "Maximum choice": 'input[data-privacy-level-choice="maximum"]',
  "Back to Privacy": "#privacy-level-back",
  "Selected mark": '[data-privacy-level-card][data-selected="true"] .privacy-level-selected-mark',
});

export const LEVEL_LABELS = Object.freeze({ basic: "Basic", balanced: "Balanced", maximum: "Maximum" });

// The selected level's explanation: its one-line summary plus the effect line
// that distinguishes it from the other two levels. Mirrors
// privacy-level-screen.ts, whose own mirror of the backend rule set is proven
// by task_0720_privacy_level_screen.test.ts.
export const LEVEL_EXPLANATIONS = Object.freeze({
  basic: {
    summary: "The fewest checks. OSL steps in only when you ask.",
    signatureEffect: "No warnings before you send.",
  },
  balanced: {
    summary: "Everyday checks before things leave this device.",
    signatureEffect: "Offers a cleanup review every 30 days.",
  },
  maximum: {
    summary: "Every check on, with the shortest review times.",
    signatureEffect: "Offers a cleanup review every 7 days.",
  },
});

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const MIN_CROP_NON_DOMINANT = 50;
const MIN_CROP_DISTINCT_COLORS = 5;

export function screenshotPathFor(level) {
  return path.join(OUTPUT_DIR, `task-0722-privacy-level-${level}.png`);
}

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
  const colors = new Set();
  for (let y = 0; y < height; y += 4) {
    for (let x = 0; x < width; x += 4) {
      const at = (y * width + x) * 4;
      colors.add(`${pixels[at]},${pixels[at + 1]},${pixels[at + 2]}`);
    }
  }
  return { width, height, pixels, distinctColors: colors.size, bytes: buffer.length };
}

function cropFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const key = `${png.pixels[offset]},${png.pixels[offset + 1]},${png.pixels[offset + 2]}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  const total = (x1 - x0) * (y1 - y0);
  const dominant = Math.max(0, ...counts.values());
  return {
    width: x1 - x0,
    height: y1 - y0,
    distinctColors: counts.size,
    // Pixels that are NOT the crop's own dominant color: text, borders, marks.
    nonDominant: total - dominant,
  };
}

// The check. `capture` holds everything one screen render produced: the DOM
// facts, the accessibility radios, the PNG summary, and the pixel crops. A
// capture missing any control named in NAMED_CONTROLS must be rejected here.
export function validatePrivacyLevelCapture(capture) {
  const level = capture.level;
  assert.ok(LEVELS.includes(level), `unknown level ${JSON.stringify(level)}`);
  assert.equal(capture.heading, "Privacy level", `screen title is wrong: ${JSON.stringify(capture.heading)}`);

  for (const name of Object.keys(NAMED_CONTROLS)) {
    const control = capture.controls?.[name];
    assert.ok(control?.present, `screen copy is missing named control "${name}"`);
    assert.ok(
      control.rect && control.rect.width >= 8 && control.rect.height >= 8,
      `named control "${name}" has no visible box`,
    );
  }

  const ids = capture.cards.map((card) => card.id);
  assert.deepEqual(ids, [...LEVELS], `level choices are wrong: ${ids.join(", ")}`);
  const selectedCards = capture.cards.filter((card) => card.selected);
  assert.equal(selectedCards.length, 1, `expected exactly one selected card, got ${selectedCards.length}`);
  assert.equal(selectedCards[0].id, level, `selected card is ${selectedCards[0].id}, expected ${level}`);
  assert.deepEqual(capture.checkedValues, [level], `checked radios are wrong: ${capture.checkedValues.join(", ")}`);
  assert.equal(capture.selectedMarkText, "Selected", `selected mark text is wrong: ${JSON.stringify(capture.selectedMarkText)}`);

  const explanation = LEVEL_EXPLANATIONS[level];
  assert.equal(
    selectedCards[0].summary,
    explanation.summary,
    `${level} explanation summary is wrong: ${JSON.stringify(selectedCards[0].summary)}`,
  );
  for (const card of capture.cards) {
    assert.equal(card.effectLines.length, 6, `${card.id} lists ${card.effectLines.length} effects, expected 6`);
  }
  assert.ok(
    selectedCards[0].effectLines.includes(explanation.signatureEffect),
    `${level} effects lost ${JSON.stringify(explanation.signatureEffect)}: ${selectedCards[0].effectLines.join(" | ")}`,
  );

  assert.equal(capture.axRadios.length, 3, `accessibility tree has ${capture.axRadios.length} radios, expected 3`);
  const checkedAx = capture.axRadios.filter((radio) => radio.checked);
  assert.equal(checkedAx.length, 1, `accessibility tree has ${checkedAx.length} checked radios, expected 1`);
  assert.ok(
    checkedAx[0].name.startsWith(LEVEL_LABELS[level]),
    `accessibility checked radio is wrong: ${JSON.stringify(checkedAx[0].name)}`,
  );

  assert.equal(capture.png.width, CAPTURE_WINDOW.width, `PNG width is ${capture.png.width}, expected ${CAPTURE_WINDOW.width}`);
  assert.equal(capture.png.height, CAPTURE_WINDOW.height, `PNG height is ${capture.png.height}, expected ${CAPTURE_WINDOW.height}`);
  assert.ok(capture.png.distinctColors >= 16, `PNG has too few distinct colors: ${capture.png.distinctColors}`);
  for (const [name, crop] of Object.entries(capture.crops)) {
    assert.ok(
      crop.nonDominant >= MIN_CROP_NON_DOMINANT && crop.distinctColors >= MIN_CROP_DISTINCT_COLORS,
      `${name} pixels look blank: nonDominant=${crop.nonDominant} distinctColors=${crop.distinctColors}`,
    );
  }

  return {
    level,
    selectedLabel: selectedCards[0].label,
    summary: selectedCards[0].summary,
    effectLines: selectedCards[0].effectLines,
    axCheckedRadio: checkedAx[0].name,
    png: { width: capture.png.width, height: capture.png.height, distinctColors: capture.png.distinctColors },
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

// Renders the privacy level screen with `level` selected, optionally removes
// one named control (the throwaway copy), and returns the full capture facts
// plus the raw screenshot bytes.
async function captureScreen(page, level, { removeControl = null } = {}) {
  const removeSelector = removeControl ? NAMED_CONTROLS[removeControl] : null;
  if (removeControl) assert.ok(removeSelector, `unknown named control ${JSON.stringify(removeControl)}`);
  const domFacts = await evaluate(page, `(async () => {
    const ui = window.__task0722Ui;
    ui.__oslHubUiTest.reset({ route: "privacy", coreReady: true, protectionPreset: ${JSON.stringify(level)}, privacyLevelScreenOpen: true });
    document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderRouteShell("privacy");
    const removeSelector = ${JSON.stringify(removeSelector)};
    if (removeSelector) {
      const doomed = document.querySelector(removeSelector);
      if (!doomed) throw new Error("throwaway copy could not find " + removeSelector + " to remove");
      doomed.remove();
    }
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    const rect = (element) => {
      if (!element) return null;
      const box = element.getBoundingClientRect();
      return { x: box.x, y: box.y, width: box.width, height: box.height };
    };
    const controls = Object.fromEntries(Object.entries(${JSON.stringify(NAMED_CONTROLS)}).map(([name, selector]) => {
      const element = document.querySelector(selector);
      return [name, { present: Boolean(element), rect: rect(element) }];
    }));
    const cards = [...document.querySelectorAll("[data-privacy-level-card]")].map((card) => ({
      id: card.dataset.privacyLevelCard,
      selected: card.dataset.selected === "true",
      label: card.querySelector("label strong")?.textContent?.trim() ?? null,
      summary: card.querySelector("label small")?.textContent?.trim() ?? null,
      effectLines: [...card.querySelectorAll("[data-privacy-level-effect]")].map((item) => item.textContent.trim()),
    }));
    const selectedCard = document.querySelector('[data-privacy-level-card][data-selected="true"]');
    return {
      heading: document.querySelector("#route-heading")?.textContent?.trim() ?? null,
      controls,
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

  const ax = await page.send("Accessibility.getFullAXTree");
  const axRadios = ax.nodes
    .filter((node) => node.role?.value === "radio")
    .map((node) => ({
      name: node.name?.value?.trim().split("\n")[0] ?? "",
      checked: node.properties?.some((property) => property.name === "checked" && property.value?.value === "true") ?? false,
    }));

  const shot = await page.screenshot({ captureBeyondViewport: false });
  const png = parsePng(shot);
  const crops = {};
  for (const [name, box] of Object.entries(domFacts.rects)) {
    crops[name] = box ? cropFacts(png, box) : { width: 0, height: 0, distinctColors: 0, nonDominant: 0 };
  }
  return {
    capture: {
      level,
      heading: domFacts.heading,
      controls: domFacts.controls,
      cards: domFacts.cards,
      checkedValues: domFacts.checkedValues,
      selectedMarkText: domFacts.selectedMarkText,
      axRadios,
      png: { width: png.width, height: png.height, distinctColors: png.distinctColors, bytes: png.bytes },
      crops,
    },
    shot,
  };
}

async function main() {
  const { createServer } = await import("vite");
  const { launchChrome } = await import("../../../scripts/lib/cdp-harness.mjs");
  const breakControl = process.env.TASK0722_BREAK || null;
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
      `--window-size=${CAPTURE_WINDOW.width},${CAPTURE_WINDOW.height}`,
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
              return Promise.reject(new Error("TASK0722 capture Tauri stub refused " + cmd));
            },
          };
        })();
      `,
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: CAPTURE_WINDOW.width,
      height: CAPTURE_WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, `(async () => {
      localStorage.clear();
      window.__task0722Ui = await import("/src/main.ts");
      return true;
    })()`);

    console.log(`TASK0722_URL=${url}`);
    console.log(`TASK0722_WINDOW=${CAPTURE_WINDOW.width}x${CAPTURE_WINDOW.height}`);
    if (breakControl) console.log(`TASK0722_BREAK=${breakControl}`);

    const hashes = new Map();
    for (const level of LEVELS) {
      const { capture, shot } = await captureScreen(page, level, { removeControl: breakControl });
      const checked = validatePrivacyLevelCapture(capture);
      const pngPath = screenshotPathFor(level);
      writeFileSync(pngPath, shot);
      hashes.set(level, sha256(shot));
      console.log(`TASK0722_${level.toUpperCase()}_PNG=${pngPath}`);
      console.log(`TASK0722_${level.toUpperCase()}_PNG_SHA256=${hashes.get(level)}`);
      console.log(`TASK0722_${level.toUpperCase()}_SIZE=${capture.png.width}x${capture.png.height}`);
      console.log(`TASK0722_${level.toUpperCase()}_SELECTED=${checked.selectedLabel}`);
      console.log(`TASK0722_${level.toUpperCase()}_AX_CHECKED_RADIO=${checked.axCheckedRadio}`);
      console.log(`TASK0722_${level.toUpperCase()}_SUMMARY=${checked.summary}`);
      console.log(`TASK0722_${level.toUpperCase()}_EFFECTS=${checked.effectLines.join(" | ")}`);
      for (const [name, crop] of Object.entries(capture.crops)) {
        console.log(`TASK0722_${level.toUpperCase()}_${name.toUpperCase()}_NONDOMINANT=${crop.nonDominant}`);
      }
    }

    const uniqueHashes = new Set(hashes.values());
    if (uniqueHashes.size !== LEVELS.length) {
      throw new Error(`the ${LEVELS.length} level captures are not pairwise distinct: ${[...hashes.values()].join(", ")}`);
    }
    console.log(`TASK0722_CAPTURES_PAIRWISE_DISTINCT=true`);

    // Throwaway copy: same screen, one named control removed. The same check
    // that passed the three real captures must fail here, naming the control.
    const dropControl = process.env.TASK0722_DROP || "Back to Privacy";
    const throwaway = await captureScreen(page, "balanced", { removeControl: dropControl });
    let throwawayError = null;
    try {
      validatePrivacyLevelCapture(throwaway.capture);
    } catch (error) {
      throwawayError = error;
    }
    if (!throwawayError) {
      throw new Error(`the check accepted a throwaway screen copy missing named control "${dropControl}"`);
    }
    if (!throwawayError.message.includes(`missing named control "${dropControl}"`)) {
      throw new Error(`the throwaway check failed for the wrong reason: ${throwawayError.message}`);
    }
    console.log(`TASK0722_THROWAWAY_DROPPED=${dropControl}`);
    console.log(`TASK0722_THROWAWAY_CHECK=failed as required: ${throwawayError.message}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`task-0722-privacy-levels-capture: ${error.stack || error.message}`);
    process.exitCode = 1;
  });
}
