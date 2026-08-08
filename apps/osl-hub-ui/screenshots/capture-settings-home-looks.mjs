#!/usr/bin/env node

// TASK 0718 — capture the fixed-size Linux Settings home in light and dark
// looks. Refuses to write either screenshot unless the capture proves the
// finish line: the title Settings and the controls Account, Apps,
// Whitelisting, Scrub, Cleanup, Notifications, Appearance and About appear in
// the screen tree AND in the visible image text, the PNG is not blank or
// nearly blank, and the pixels really carry the requested look (a light
// capture must be light, a dark capture dark, and the two must differ).

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { pngFacts } from "./capture-timer-overlay.mjs";
import { FIXED_VIEWPORT } from "./capture-settings-home.mjs";

export const LOOKS = ["dark", "light"];

// Deliberately restated instead of imported from ../src/settings-home.ts: the
// gate is independent of the module it is checking. If the Settings home drops
// or renames a control, this list makes the capture go red.
export const REQUIRED_CONTROLS = [
  "Account",
  "Apps",
  "Whitelisting",
  "Scrub",
  "Cleanup",
  "Notifications",
  "Appearance",
  "About",
];

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);

export function defaultOutputFor(look) {
  return path.join(SCRIPT_DIR, `task-0718-settings-home-${look}.png`);
}

/**
 * Mean luminance (0..255) over sampled pixels of the whole PNG, so the gate
 * can tell a genuinely dark capture from a light one. Restated independently
 * of pngFacts on purpose: this is the fact the look check stands on.
 */
export function pngMeanLuma(bytes) {
  if (bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("PNG signature is missing");
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  const bitDepth = bytes[24];
  const colorType = bytes[25];
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  if (bitDepth !== 8 || channels === 0) throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType}`);

  let offset = 8;
  const idat = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const kind = bytes.subarray(offset + 4, offset + 8).toString("ascii");
    if (kind === "IDAT") idat.push(bytes.subarray(offset + 8, offset + 8 + length));
    if (kind === "IEND") break;
    offset += 12 + length;
  }
  const inflated = zlib.inflateSync(Buffer.concat(idat));
  const rowBytes = width * channels;
  let cursor = 0;
  let previous = Buffer.alloc(rowBytes);
  let lumaSum = 0;
  let samples = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[cursor];
    const encoded = inflated.subarray(cursor + 1, cursor + 1 + rowBytes);
    cursor += rowBytes + 1;
    const decoded = Buffer.alloc(rowBytes);
    for (let index = 0; index < rowBytes; index += 1) {
      const left = index >= channels ? decoded[index - channels] : 0;
      const up = previous[index] ?? 0;
      const upperLeft = index >= channels ? previous[index - channels] : 0;
      const paethBase = left + up - upperLeft;
      const paeth = Math.abs(paethBase - left) <= Math.abs(paethBase - up) && Math.abs(paethBase - left) <= Math.abs(paethBase - upperLeft)
        ? left
        : Math.abs(paethBase - up) <= Math.abs(paethBase - upperLeft) ? up : upperLeft;
      const predictor = filter === 0 ? 0 : filter === 1 ? left : filter === 2 ? up : filter === 3 ? Math.floor((left + up) / 2) : filter === 4 ? paeth : null;
      if (predictor === null) throw new Error(`unsupported PNG filter ${filter}`);
      decoded[index] = (encoded[index] + predictor) & 255;
    }
    if (y % 4 === 0) {
      for (let x = 0; x < width; x += 4) {
        const start = x * channels;
        lumaSum += 0.2126 * decoded[start] + 0.7152 * decoded[start + 1] + 0.0722 * decoded[start + 2];
        samples += 1;
      }
    }
    previous = decoded;
  }
  if (samples === 0) throw new Error("PNG has no pixels to sample");
  return lumaSum / samples;
}

/**
 * Pure gate over one look's captured facts. Throws unless the capture proves
 * the finish line for that look.
 */
export function validateSettingsHomeLookCapture({ look, platform, imageText, axNames, png }) {
  if (!LOOKS.includes(look)) throw new Error(`unknown look "${look}", expected one of ${LOOKS.join(", ")}`);
  if (platform !== "linux") throw new Error(`${look}: screenshot must be captured on Linux, got ${platform}`);

  if (!axNames.includes("Settings")) throw new Error(`${look}: screen tree is missing the title Settings`);
  if (!/\bSettings\b/u.test(imageText)) throw new Error(`${look}: visible text is missing the title Settings`);
  for (const control of REQUIRED_CONTROLS) {
    if (!axNames.some((name) => name.includes(control))) {
      throw new Error(`${look}: screen tree is missing the control ${control}`);
    }
    if (!imageText.includes(control)) throw new Error(`${look}: visible text is missing the control ${control}`);
  }

  if (png.width !== FIXED_VIEWPORT.width || png.height !== FIXED_VIEWPORT.height) {
    throw new Error(`${look}: expected ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height}, captured ${png.width}x${png.height}`);
  }
  if (png.bytes < 10_000 || png.distinctColors < 50) {
    throw new Error(`${look}: PNG is blank or nearly blank: bytes=${png.bytes} distinctColors=${png.distinctColors}`);
  }
  if (look === "dark" && png.meanLuma >= 60) {
    throw new Error(`dark: pixels are not dark, mean luminance ${png.meanLuma.toFixed(1)} >= 60`);
  }
  if (look === "light" && png.meanLuma <= 180) {
    throw new Error(`light: pixels are not light, mean luminance ${png.meanLuma.toFixed(1)} <= 180`);
  }

  return { look, platform, controls: [...REQUIRED_CONTROLS], png };
}

/** Gate over the pair: both looks must pass, and the images must differ. */
export function validateSettingsHomeLooks(captures) {
  for (const look of LOOKS) {
    if (!captures[look]) throw new Error(`missing ${look} capture`);
  }
  const checked = Object.fromEntries(LOOKS.map((look) => [look, validateSettingsHomeLookCapture(captures[look])]));
  if (checked.dark.png.sha256 === checked.light.png.sha256) {
    throw new Error("dark and light captures are pixel-identical; the look was not applied");
  }
  return checked;
}

export async function captureSettingsHomeLooks({ outputFor = defaultOutputFor } = {}) {
  const { createServer: createViteServer } = await import("vite");
  const server = await createViteServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0, strictPort: false },
    logLevel: "error",
  });
  await server.listen();
  const address = server.httpServer.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP port");

  const chrome = await launchChrome();
  try {
    const captures = {};
    const outputs = {};
    for (const look of LOOKS) {
      const page = await chrome.openPage();
      try {
        await page.send("Emulation.setDeviceMetricsOverride", {
          ...FIXED_VIEWPORT,
          deviceScaleFactor: 1,
          mobile: false,
        });
        await page.send("Emulation.setEmulatedMedia", {
          features: [{ name: "prefers-reduced-motion", value: "reduce" }],
        });
        await page.navigate(`http://127.0.0.1:${address.port}/screenshots/task-0716-settings-home-fixture.html`);
        const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
        if (ready !== "settings-home") throw new Error("settings home fixture did not become ready");
        // Same mechanism the app uses (main.ts applyTheme): dark is the
        // default palette, :root[data-theme="light"] switches the look.
        await page.evaluate(`document.documentElement.dataset.theme = ${JSON.stringify(look)}`);
        await new Promise((resolve) => setTimeout(resolve, 250));

        const imageText = await page.evaluate(`document.body.innerText.replace(/\\s+/g, " ").trim()`);
        const axTree = await page.send("Accessibility.getFullAXTree");
        const axNames = axTree.nodes
          .map((node) => node.name?.value)
          .filter((name) => typeof name === "string" && name.trim())
          .map((name) => name.trim());

        const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
        captures[look] = {
          look,
          platform: process.platform,
          imageText,
          axNames,
          png: { ...pngFacts(png), meanLuma: pngMeanLuma(png) },
        };
        outputs[look] = { path: outputFor(look), bytes: png };
      } finally {
        await page.close();
      }
    }

    const checked = validateSettingsHomeLooks(captures);
    for (const look of LOOKS) {
      mkdirSync(path.dirname(outputs[look].path), { recursive: true });
      writeFileSync(outputs[look].path, outputs[look].bytes);
    }
    return {
      viewport: FIXED_VIEWPORT,
      screenshots: Object.fromEntries(LOOKS.map((look) => [look, outputs[look].path])),
      ...checked,
    };
  } finally {
    await chrome.close();
    await server.close();
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureSettingsHomeLooks().then((result) => {
    for (const look of LOOKS) {
      if (!existsSync(result.screenshots[look])) throw new Error(`${look} screenshot file was not written`);
    }
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-settings-home-looks: ${error.stack || error.message}`);
    process.exit(1);
  });
}
