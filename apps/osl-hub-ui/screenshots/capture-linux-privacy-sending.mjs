#!/usr/bin/env node

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
const PNG_PATH = path.join(OUTPUT_DIR, "task-0372-linux-privacy-sending.png");

const TITLE = "Privacy and sending";
const REQUIRED_SEND_CHOICES = ["Manual", "Clipboard", "Double Enter"];
const REQUIRED_IMAGE_LABELS = [
  TITLE,
  "Resist screenshots of OSL",
  ...REQUIRED_SEND_CHOICES,
  "I understand",
  "Continue",
  "Back",
];

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
    for (let x = 0; x < width; x += 1) {
      const source = x * channels;
      const target = (y * width + x) * 4;
      pixels[target] = row[source];
      pixels[target + 1] = row[source + 1];
      pixels[target + 2] = row[source + 2];
      pixels[target + 3] = channels === 4 ? row[source + 3] : 255;
    }
    row.copy(prior);
  }
  return { width, height, pixels };
}

function cropFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const colors = new Set();
  let nonBackground = 0;
  let brightPixels = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const r = png.pixels[offset];
      const g = png.pixels[offset + 1];
      const b = png.pixels[offset + 2];
      colors.add(`${r},${g},${b}`);
      if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
      if (r + g + b > 360) brightPixels += 1;
    }
  }
  return {
    width: x1 - x0,
    height: y1 - y0,
    distinctColors: colors.size,
    nonBackground,
    brightPixels,
  };
}

function imageFacts(buffer, rects) {
  const png = parsePng(buffer);
  const colors = new Set();
  let nonBackground = 0;
  for (let index = 0; index < png.pixels.length; index += 4) {
    const r = png.pixels[index];
    const g = png.pixels[index + 1];
    const b = png.pixels[index + 2];
    colors.add(`${r},${g},${b}`);
    if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
  }
  const crops = Object.fromEntries(
    Object.entries(rects).map(([name, rect]) => [name, cropFacts(png, rect)]),
  );
  return {
    width: png.width,
    height: png.height,
    distinctColors: colors.size,
    nonBackground,
    nearlyBlank: colors.size < 20 || nonBackground < 2_000,
    crops,
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
  const screenData = await vite.ssrLoadModule("/src/linux-onboarding-screen-data.ts");
  const { width, height } = screenData.LINUX_ONBOARDING_SCREEN_WINDOW;
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${width},${height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
        (() => {
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
              return Promise.reject(new Error("TASK0372 capture Tauri stub refused " + cmd));
            },
          };
        })();
      `,
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    const renderResult = await evaluate(page, `(async () => {
      localStorage.clear();
      const ui = await import("/src/main.ts");
      ui.__oslHubUiTest.reset({
        route: "onboarding",
        onboardingRoute: "sending",
        coreReady: true,
        bootstrapStatus: "setupRequired",
        setup: { sendMode: "double", acceptedRisk: true, acceptedRiskForMode: "double" },
      });
      document.querySelector("#app").innerHTML = '<div class="app-frame with-titlebar"><header class="desktop-titlebar" aria-hidden="true"></header><div class="onboarding-shell"><main class="onboarding-panel onboarding-sending"></main></div></div>';
      const panel = document.querySelector(".onboarding-panel");
      panel.innerHTML = ui.__oslHubUiTest.renderOnboardingRoute("sending")
        + '<div class="setup-footer onboarding-actions onboarding-nav"><button class="button ghost onboarding-back" id="onboarding-back" type="button">Back</button></div>';
      const nav = panel.querySelector(".onboarding-nav");
      const back = nav.querySelector("#onboarding-back");
      const primaryRow = [...panel.querySelectorAll(".setup-footer.onboarding-actions")].find((row) => row !== nav);
      primaryRow.prepend(back);
      nav.remove();
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const sendLabels = [...document.querySelectorAll(".snd-card-head strong")].map((element) => element.textContent.trim());
      const elements = {
        "${TITLE}": document.querySelector("#route-heading"),
        "Resist screenshots of OSL": document.querySelector(".snd-capture-copy strong"),
        Manual: [...document.querySelectorAll(".snd-card-head strong")].find((element) => element.textContent.trim() === "Manual"),
        Clipboard: [...document.querySelectorAll(".snd-card-head strong")].find((element) => element.textContent.trim() === "Clipboard"),
        "Double Enter": [...document.querySelectorAll(".snd-card-head strong")].find((element) => element.textContent.trim() === "Double Enter"),
        "I understand": document.querySelector(".snd-risk-copy strong"),
        Continue: [...document.querySelectorAll(".signin-unlock-label")].find((element) => element.textContent.trim() === "Continue"),
        Back: document.querySelector("#onboarding-back"),
      };
      const rects = Object.fromEntries(Object.entries(elements).map(([name, element]) => {
        if (!element) return [name, null];
        const rect = element.getBoundingClientRect();
        return [name, { x: rect.x, y: rect.y, width: rect.width, height: rect.height }];
      }));
      const missing = Object.entries(elements).filter(([, element]) => !element).map(([name]) => name);
      const checked = {
        capture: document.querySelector("#window-capture-enabled")?.checked ?? null,
        risk: document.querySelector("#accept-send-risk")?.checked ?? null,
        selectedMode: document.querySelector('input[name="send-mode"]:checked')?.dataset.sendMode ?? null,
        continueDisabled: document.querySelector("#finish-onboarding")?.disabled ?? null,
      };
      return { title: document.title, visibleText: document.body.innerText, missing, rects, sendLabels, checked };
    })()`);
    if (renderResult.missing.length > 0) {
      throw new Error(`missing visible elements: ${renderResult.missing.join(", ")}`);
    }
    if (renderResult.checked.capture !== true) {
      throw new Error(`capture choice is not checked: ${renderResult.checked.capture}`);
    }
    if (renderResult.checked.risk !== true) {
      throw new Error(`understanding tick is not checked: ${renderResult.checked.risk}`);
    }
    if (renderResult.checked.selectedMode !== "double") {
      throw new Error(`unexpected selected send mode: ${renderResult.checked.selectedMode}`);
    }
    if (renderResult.checked.continueDisabled !== false) {
      throw new Error(`Continue is not enabled: disabled=${renderResult.checked.continueDisabled}`);
    }
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => typeof node.name?.value === "string" ? node.name.value.trim() : "")
      .filter(Boolean);
    const requiredAx = [
      TITLE,
      "Resist screenshots of OSL",
      ...REQUIRED_SEND_CHOICES,
      "I understand",
      "Continue",
      "Back",
    ];
    const missingAx = requiredAx.filter((name) => !axNames.some((candidate) => candidate === name || candidate.includes(name)));
    if (missingAx.length > 0) {
      throw new Error(`missing accessibility names: ${missingAx.join(", ")}`);
    }
    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    const facts = imageFacts(screenshot, renderResult.rects);
    const blankCrops = Object.entries(facts.crops)
      .filter(([, crop]) => crop.nonBackground < 10 || crop.brightPixels < 10)
      .map(([name]) => name);
    if (facts.width !== width || facts.height !== height) {
      throw new Error(`unexpected PNG size ${facts.width}x${facts.height}`);
    }
    if (facts.nearlyBlank) {
      throw new Error(`PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
    }
    if (blankCrops.length > 0) {
      throw new Error(`visible label boxes are blank: ${blankCrops.join(", ")}`);
    }

    console.log(`TASK0372_URL=${url}`);
    console.log(`TASK0372_PNG=${PNG_PATH}`);
    console.log(`TASK0372_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK0372_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0372_FIXED_WINDOW=${width}x${height}`);
    console.log(`TASK0372_FIXED_ACCOUNTS=${screenData.LINUX_ONBOARDING_SCREEN_FIXTURES.accounts.map((account) => account.ownerName).join("|")}`);
    console.log(`TASK0372_FIXED_NAMES=${screenData.LINUX_ONBOARDING_SCREEN_FIXTURES.names.join("|")}`);
    console.log(`TASK0372_TITLE=${TITLE}`);
    console.log(`TASK0372_CAPTURE_CHOICE=Resist screenshots of OSL`);
    console.log(`TASK0372_CAPTURE_CHECKED=${renderResult.checked.capture}`);
    console.log(`TASK0372_SEND_CHOICE_COUNT=${REQUIRED_SEND_CHOICES.length}`);
    REQUIRED_SEND_CHOICES.forEach((choice, index) => {
      console.log(`TASK0372_SEND_CHOICE_${index + 1}=${choice}`);
    });
    console.log(`TASK0372_VISIBLE_SEND_CHOICES=${renderResult.sendLabels.join("|")}`);
    console.log(`TASK0372_SELECTED_SEND_MODE=${renderResult.checked.selectedMode}`);
    console.log(`TASK0372_UNDERSTANDING_TICK=I understand`);
    console.log(`TASK0372_UNDERSTANDING_CHECKED=${renderResult.checked.risk}`);
    console.log(`TASK0372_CONTINUE=Continue`);
    console.log(`TASK0372_CONTINUE_DISABLED=${renderResult.checked.continueDisabled}`);
    console.log(`TASK0372_BACK=Back`);
    console.log(`TASK0372_TREE_NAMES=${requiredAx.filter((name) => axNames.some((candidate) => candidate === name || candidate.includes(name))).join("|")}`);
    for (const name of REQUIRED_IMAGE_LABELS) {
      const crop = facts.crops[name];
      console.log(`TASK0372_IMAGE_${name.toUpperCase().replace(/[^A-Z0-9]+/gu, "_").replace(/^_|_$/gu, "")}_RECT=${Math.round(renderResult.rects[name].x)},${Math.round(renderResult.rects[name].y)},${Math.round(renderResult.rects[name].width)}x${Math.round(renderResult.rects[name].height)}`);
      console.log(`TASK0372_IMAGE_${name.toUpperCase().replace(/[^A-Z0-9]+/gu, "_").replace(/^_|_$/gu, "")}_NONBACKGROUND=${crop.nonBackground}`);
      console.log(`TASK0372_IMAGE_${name.toUpperCase().replace(/[^A-Z0-9]+/gu, "_").replace(/^_|_$/gu, "")}_BRIGHT_PIXELS=${crop.brightPixels}`);
    }
    console.log(`TASK0372_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0372_PNG_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`TASK0372_PNG_NEARLY_BLANK=${facts.nearlyBlank}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-linux-privacy-sending: ${error.stack || error.message}`);
  process.exitCode = 1;
});
