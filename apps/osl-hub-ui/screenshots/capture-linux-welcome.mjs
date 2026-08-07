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
const PNG_PATH = path.join(OUTPUT_DIR, "task-0353-linux-welcome.png");
const REQUIRED_NAMES = ["Welcome", "Create", "Restore", "Unlock"];

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
              return Promise.reject(new Error("TASK0353 capture Tauri stub refused " + cmd));
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
      ui.__oslHubUiTest.reset({ route: "onboarding", onboardingRoute: "welcome", coreReady: true, bootstrapStatus: "setupRequired" });
      document.querySelector("#app").innerHTML = ui.__oslHubUiTest.renderRouteShell("onboarding");
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const required = ${JSON.stringify(REQUIRED_NAMES)};
      const labels = [...document.querySelectorAll(".welcome-choice-actions .signin-unlock-label")];
      const elements = {
        Welcome: document.querySelector("#route-heading"),
        Create: labels.find((element) => element.textContent.trim() === "Create"),
        Restore: labels.find((element) => element.textContent.trim() === "Restore"),
        Unlock: labels.find((element) => element.textContent.trim() === "Unlock"),
      };
      const rects = Object.fromEntries(Object.entries(elements).map(([name, element]) => {
        if (!element) return [name, null];
        const rect = element.getBoundingClientRect();
        return [name, { x: rect.x, y: rect.y, width: rect.width, height: rect.height }];
      }));
      const missing = required.filter((name) => !elements[name]);
      return { title: document.title, visibleText: document.body.innerText, missing, rects };
    })()`);
    if (renderResult.missing.length > 0) {
      throw new Error(`missing visible elements: ${renderResult.missing.join(", ")}`);
    }
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => typeof node.name?.value === "string" ? node.name.value.trim() : "")
      .filter(Boolean);
    const missingAx = REQUIRED_NAMES.filter((name) => !axNames.includes(name));
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

    console.log(`TASK0353_URL=${url}`);
    console.log(`TASK0353_PNG=${PNG_PATH}`);
    console.log(`TASK0353_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK0353_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0353_FIXED_WINDOW=${width}x${height}`);
    console.log(`TASK0353_FIXED_ACCOUNTS=${screenData.LINUX_ONBOARDING_SCREEN_FIXTURES.accounts.map((account) => account.ownerName).join("|")}`);
    console.log(`TASK0353_FIXED_NAMES=${screenData.LINUX_ONBOARDING_SCREEN_FIXTURES.names.join("|")}`);
    console.log(`TASK0353_TREE_NAMES=${REQUIRED_NAMES.filter((name) => axNames.includes(name)).join("|")}`);
    console.log(`TASK0353_TREE_TITLE=${axNames.includes("Welcome") ? "Welcome" : "missing"}`);
    for (const name of ["Create", "Restore", "Unlock"]) {
      console.log(`TASK0353_TREE_CONTROL_${name.toUpperCase()}=${axNames.includes(name) ? name : "missing"}`);
    }
    for (const name of REQUIRED_NAMES) {
      const crop = facts.crops[name];
      console.log(`TASK0353_IMAGE_${name.toUpperCase()}_RECT=${Math.round(renderResult.rects[name].x)},${Math.round(renderResult.rects[name].y)},${Math.round(renderResult.rects[name].width)}x${Math.round(renderResult.rects[name].height)}`);
      console.log(`TASK0353_IMAGE_${name.toUpperCase()}_NONBACKGROUND=${crop.nonBackground}`);
      console.log(`TASK0353_IMAGE_${name.toUpperCase()}_BRIGHT_PIXELS=${crop.brightPixels}`);
    }
    console.log(`TASK0353_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0353_PNG_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`TASK0353_PNG_NEARLY_BLANK=${facts.nearlyBlank}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-linux-welcome: ${error.stack || error.message}`);
  process.exitCode = 1;
});
