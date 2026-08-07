#!/usr/bin/env node

/**
 * TASK 0756 -- Linux capture of the Message defaults screen.
 *
 * Renders the product module `src/message-defaults.ts` in a real Chromium at a
 * fixed 1280x800 Linux window, with four NON-default saved values, and proves
 * from the PNG itself that each saved choice and each explanation is drawn:
 * every required label is located in the layout, and the pixels inside its box
 * are counted. A box that came out empty fails the run.
 */

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-0756-message-defaults.png");

export const FIXED_WINDOW = { width: 1280, height: 800 };
export const TITLE = "Message defaults";

/** The saved values the capture puts on screen. None of them is a factory default. */
export const SAVED_MESSAGE_DEFAULTS = {
  burnScope: "app",
  timerSeconds: 86_400,
  viewOnceLengthSeconds: 45,
  coverWriting: "ai_covertext",
};

/** control -> [heading, expected saved label]. */
export const SAVED_CHOICES = [
  ["timer", "Timer", "1 day"],
  ["burn-scope", "Burn after reading", "This app"],
  ["view-once-length", "View-once length", "45 seconds"],
  ["writing", "AI or Wordbank", "AI"],
];

const MIN_INK_PIXELS = 30;

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parsePng(buffer) {
  if (buffer.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("screenshot is not a PNG");
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
  const pixels = Buffer.alloc(width * height * 3);
  let cursor = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[cursor];
    cursor += 1;
    const decoded = Buffer.alloc(stride);
    for (let index = 0; index < stride; index += 1) {
      const raw = inflated[cursor + index];
      const left = index >= channels ? decoded[index - channels] : 0;
      const up = previous[index];
      const upperLeft = index >= channels ? previous[index - channels] : 0;
      let value;
      if (filter === 0) value = raw;
      else if (filter === 1) value = raw + left;
      else if (filter === 2) value = raw + up;
      else if (filter === 3) value = raw + Math.floor((left + up) / 2);
      else if (filter === 4) {
        const base = left + up - upperLeft;
        const dl = Math.abs(base - left);
        const du = Math.abs(base - up);
        const dul = Math.abs(base - upperLeft);
        value = raw + (dl <= du && dl <= dul ? left : du <= dul ? up : upperLeft);
      } else {
        throw new Error(`unsupported PNG filter ${filter}`);
      }
      decoded[index] = value & 0xff;
    }
    cursor += stride;
    for (let x = 0; x < width; x += 1) {
      const source = x * channels;
      const target = (y * width + x) * 3;
      pixels[target] = decoded[source];
      pixels[target + 1] = decoded[source + 1];
      pixels[target + 2] = decoded[source + 2];
    }
    previous = decoded;
  }
  return { width, height, pixels };
}

/**
 * Ink inside one label box: pixels that differ from the box's own most common
 * colour. Comparing against the box background rather than a hard-coded page
 * colour keeps this honest on a card, a button and a page background alike.
 */
function inkFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 3;
      const key = (png.pixels[offset] << 16) | (png.pixels[offset + 1] << 8) | png.pixels[offset + 2];
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  let background = 0;
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      background = key;
    }
  }
  const br = (background >> 16) & 0xff;
  const bg = (background >> 8) & 0xff;
  const bb = background & 0xff;
  let ink = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 3;
      const distance = Math.abs(png.pixels[offset] - br)
        + Math.abs(png.pixels[offset + 1] - bg)
        + Math.abs(png.pixels[offset + 2] - bb);
      if (distance > 40) ink += 1;
    }
  }
  return {
    box: `${Math.round(rect.x)},${Math.round(rect.y)} ${Math.round(rect.width)}x${Math.round(rect.height)}`,
    distinctColors: counts.size,
    ink,
  };
}

function pageFacts(png) {
  const colors = new Set();
  for (let index = 0; index < png.pixels.length; index += 3) {
    colors.add((png.pixels[index] << 16) | (png.pixels[index + 1] << 8) | png.pixels[index + 2]);
  }
  return { distinctColors: colors.size };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

export async function captureMessageDefaults() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/`;
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${FIXED_WINDOW.width},${FIXED_WINDOW.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: FIXED_WINDOW.width,
      height: FIXED_WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    const rendered = await evaluate(page, `(async () => {
      await import("/src/styles.css");
      const screen = await import("/src/message-defaults.ts");
      const saved = ${JSON.stringify(SAVED_MESSAGE_DEFAULTS)};
      const state = screen.initialMessageDefaultsScreenState(saved);
      document.title = ${JSON.stringify(TITLE)};
      document.body.innerHTML = '<div id="app" class="app-frame"></div>';
      const app = document.getElementById("app");
      app.style.height = "100vh";
      app.innerHTML = screen.messageDefaultsScreenMarkup(state);
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

      const boxes = {};
      const missing = [];
      const record = (name, element) => {
        if (!element) { missing.push(name); return; }
        const rect = element.getBoundingClientRect();
        if (rect.width < 1 || rect.height < 1) { missing.push(name + " (zero size)"); return; }
        boxes[name] = { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      record(${JSON.stringify(TITLE)}, document.querySelector("#route-heading"));
      const controls = ${JSON.stringify(SAVED_CHOICES)};
      const savedLabels = {};
      const explanations = {};
      for (const [control, heading] of controls) {
        const group = document.querySelector('[data-message-default-group="' + control + '"]');
        record("legend:" + control, group?.querySelector("legend"));
        const why = group?.querySelector('[data-message-default-why="' + control + '"]');
        record("why:" + control, why);
        explanations[control] = why ? why.textContent.trim() : null;
        const savedChoice = group?.querySelector(".msg-def-choice.saved");
        record("saved:" + control, savedChoice?.querySelector(".msg-def-choice-label"));
        record("savedtag:" + control, savedChoice?.querySelector(".msg-def-saved-tag"));
        savedLabels[control] = savedChoice
          ? savedChoice.querySelector(".msg-def-choice-label").textContent.trim()
          : null;
        if (group && group.querySelector("legend").textContent.trim() !== heading) {
          missing.push("heading:" + control);
        }
      }
      record("Save", document.querySelector("#save-message-defaults"));
      record("Reset", document.querySelector("#reset-message-defaults"));

      const checkedValues = {};
      for (const [control] of controls) {
        const checked = document.querySelector('input[name="message-default-' + control + '"]:checked');
        checkedValues[control] = checked ? checked.value : null;
      }
      return {
        boxes,
        missing,
        savedLabels,
        explanations,
        checkedValues,
        savedTagCount: document.querySelectorAll(".msg-def-saved-tag").length,
        status: document.querySelector("[data-message-default-status]")?.dataset.messageDefaultStatus ?? null,
        visibleText: document.body.innerText,
        title: document.title,
      };
    })()`);

    if (rendered.missing.length) throw new Error(`labels not laid out: ${rendered.missing.join(", ")}`);
    for (const [control, , expected] of SAVED_CHOICES) {
      if (rendered.savedLabels[control] !== expected) {
        throw new Error(`saved choice for ${control} reads ${rendered.savedLabels[control]}, wanted ${expected}`);
      }
      const words = (rendered.explanations[control] ?? "").trim().split(/\s+/u).filter(Boolean);
      if (words.length < 12) throw new Error(`explanation for ${control} is ${words.length} words`);
    }
    if (rendered.savedTagCount !== SAVED_CHOICES.length) {
      throw new Error(`expected ${SAVED_CHOICES.length} saved marks, found ${rendered.savedTagCount}`);
    }
    if (rendered.status !== "saved") throw new Error(`screen status is ${rendered.status}`);
    const expectedChecked = {
      timer: String(SAVED_MESSAGE_DEFAULTS.timerSeconds),
      "burn-scope": SAVED_MESSAGE_DEFAULTS.burnScope,
      "view-once-length": String(SAVED_MESSAGE_DEFAULTS.viewOnceLengthSeconds),
      writing: SAVED_MESSAGE_DEFAULTS.coverWriting,
    };
    for (const [control, value] of Object.entries(expectedChecked)) {
      if (rendered.checkedValues[control] !== value) {
        throw new Error(`${control} is set to ${rendered.checkedValues[control]}, wanted ${value}`);
      }
    }

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
      .filter(Boolean);
    const requiredAx = [
      TITLE,
      ...SAVED_CHOICES.map(([, heading]) => heading),
      ...SAVED_CHOICES.map(([, , label]) => label),
      "Save",
      "Reset",
    ];
    const missingAx = requiredAx.filter((name) => !axNames.some((candidate) => candidate === name || candidate.includes(name)));
    if (missingAx.length) throw new Error(`missing screen-tree names: ${missingAx.join(", ")}`);

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    const png = parsePng(screenshot);
    if (png.width !== FIXED_WINDOW.width || png.height !== FIXED_WINDOW.height) {
      throw new Error(`unexpected PNG size ${png.width}x${png.height}`);
    }
    const page_ = pageFacts(png);
    if (page_.distinctColors < 20) throw new Error(`PNG is blank or nearly blank: colors=${page_.distinctColors}`);

    const ink = Object.fromEntries(Object.entries(rendered.boxes).map(([name, rect]) => [name, inkFacts(png, rect)]));
    const blank = Object.entries(ink).filter(([, facts]) => facts.ink < MIN_INK_PIXELS).map(([name]) => name);
    if (blank.length) throw new Error(`label boxes have no drawn text: ${blank.join(", ")}`);

    console.log(`TASK0756_OS=${os.type()} ${os.release()}`);
    console.log(`TASK0756_URL=${url}`);
    console.log(`TASK0756_PNG=${PNG_PATH}`);
    console.log(`TASK0756_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK0756_PNG_BYTES=${screenshot.length}`);
    console.log(`TASK0756_WINDOW=${png.width}x${png.height}`);
    console.log(`TASK0756_PNG_DISTINCT_COLORS=${page_.distinctColors}`);
    console.log(`TASK0756_TITLE=${rendered.title}`);
    console.log(`TASK0756_SAVED_CHOICE_COUNT=${rendered.savedTagCount}`);
    for (const [control, heading, label] of SAVED_CHOICES) {
      console.log(`TASK0756_SAVED_CHOICE ${control} heading=${heading} value=${rendered.checkedValues[control]} label=${label}`);
      console.log(`TASK0756_EXPLANATION ${control} words=${rendered.explanations[control].trim().split(/\s+/u).length} text=${rendered.explanations[control]}`);
      console.log(`TASK0756_IMAGE saved:${control} box=${ink[`saved:${control}`].box} ink=${ink[`saved:${control}`].ink}`);
      console.log(`TASK0756_IMAGE why:${control} box=${ink[`why:${control}`].box} ink=${ink[`why:${control}`].ink}`);
      console.log(`TASK0756_IMAGE legend:${control} box=${ink[`legend:${control}`].box} ink=${ink[`legend:${control}`].ink}`);
    }
    for (const name of [TITLE, "Save", "Reset"]) {
      console.log(`TASK0756_IMAGE ${name} box=${ink[name].box} ink=${ink[name].ink}`);
    }
    console.log(`TASK0756_SCREEN_TREE_NAMES=${requiredAx.join("|")}`);
    console.log(`TASK0756_MIN_INK_PIXELS=${MIN_INK_PIXELS}`);
    console.log(`TASK0756_DONE title=${TITLE} saved_choices=${SAVED_CHOICES.map(([, , label]) => label).join(",")} explanations=4 blank=false`);
    return { png: PNG_PATH, rendered, ink };
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

const invokedDirectly = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  captureMessageDefaults().catch((error) => {
    console.error(`capture-message-defaults: ${error.stack || error.message}`);
    process.exitCode = 1;
  });
}
