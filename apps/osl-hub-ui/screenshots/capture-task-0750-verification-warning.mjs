#!/usr/bin/env node

// TASK 0750 -- capture the fixed-size Linux verification warning screen with
// "before sending" chosen.
//
// TASK 0748 drove the screen through its own controls and proved the required
// names are in the screen tree and in `document.body.innerText`. That is the DOM
// talking about itself: a label pushed off the fixed viewport, painted in the
// background colour, or covered by another box still shows up in `innerText`.
// So this capture measures where each required name is drawn, then decodes the
// PNG it just wrote and counts the ink inside that rectangle. A name only counts
// as "in the image" if pixels for it are actually there.

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";
import {
  CAPTURED_CHOICE,
  FIXED_VIEWPORT,
  REQUIRED_NAMES,
  captureVerificationWarningScreen,
} from "./capture-verification-warning-screen.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-0750-verification-warning-before-sending.png");

/** A pixel counts as ink when it is this far from the background it sits on. */
const INK_DISTANCE = 40;
/** Below this, a rectangle is a flat block of colour -- no glyphs were drawn. */
const MIN_INK_PIXELS = 12;

function parseArgs(args) {
  const outputIndex = args.indexOf("--output");
  return { output: outputIndex === -1 ? DEFAULT_OUTPUT : path.resolve(args[outputIndex + 1] ?? "") };
}

/** Full RGBA readback. `pngFacts` samples every 4th pixel; ink counting cannot. */
export function decodePng(bytes) {
  if (bytes.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a") throw new Error("PNG signature is missing");
  if (bytes.subarray(12, 16).toString("ascii") !== "IHDR") throw new Error("PNG IHDR is missing");
  const width = bytes.readUInt32BE(16);
  const height = bytes.readUInt32BE(20);
  const bitDepth = bytes[24];
  const colorType = bytes[25];
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  if (bitDepth !== 8 || channels === 0 || bytes[26] !== 0 || bytes[27] !== 0 || bytes[28] !== 0) {
    throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType}`);
  }

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
  const pixels = Buffer.alloc(rowBytes * height);
  let cursor = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[cursor];
    const encoded = inflated.subarray(cursor + 1, cursor + 1 + rowBytes);
    cursor += rowBytes + 1;
    const rowStart = y * rowBytes;
    const previousStart = (y - 1) * rowBytes;
    for (let index = 0; index < rowBytes; index += 1) {
      const left = index >= channels ? pixels[rowStart + index - channels] : 0;
      const up = y === 0 ? 0 : pixels[previousStart + index];
      const upperLeft = y === 0 || index < channels ? 0 : pixels[previousStart + index - channels];
      const paethBase = left + up - upperLeft;
      const paeth = Math.abs(paethBase - left) <= Math.abs(paethBase - up) && Math.abs(paethBase - left) <= Math.abs(paethBase - upperLeft)
        ? left
        : Math.abs(paethBase - up) <= Math.abs(paethBase - upperLeft) ? up : upperLeft;
      const predictor = filter === 0 ? 0 : filter === 1 ? left : filter === 2 ? up : filter === 3 ? Math.floor((left + up) / 2) : filter === 4 ? paeth : null;
      if (predictor === null) throw new Error(`unsupported PNG filter ${filter}`);
      pixels[rowStart + index] = (encoded[index] + predictor) & 255;
    }
  }
  return { width, height, channels, pixels };
}

const pixelAt = (image, x, y) => {
  const start = y * image.width * image.channels + x * image.channels;
  return [image.pixels[start], image.pixels[start + 1], image.pixels[start + 2]];
};

/**
 * How many pixels in this rectangle differ from the rectangle's own background.
 * The background is taken as the most common colour inside the rectangle, so
 * this works the same on the page, on a choice card and on a filled button.
 */
export function inkInRect(image, rect) {
  const left = Math.max(0, Math.floor(rect.x));
  const top = Math.max(0, Math.floor(rect.y));
  const right = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const bottom = Math.min(image.height, Math.ceil(rect.y + rect.height));
  if (right <= left || bottom <= top) return { inkPixels: 0, area: 0, background: "none", distinctColors: 0 };

  const counts = new Map();
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const key = pixelAt(image, x, y).join(",");
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  let background = "";
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      background = key;
    }
  }

  const [br, bg, bb] = background.split(",").map(Number);
  let inkPixels = 0;
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const [r, g, b] = pixelAt(image, x, y);
      if (Math.abs(r - br) + Math.abs(g - bg) + Math.abs(b - bb) >= INK_DISTANCE) inkPixels += 1;
    }
  }
  return {
    inkPixels,
    area: (right - left) * (bottom - top),
    background: `rgb(${background})`,
    distinctColors: counts.size,
  };
}

const insideViewport = (rect) =>
  rect.width > 0 && rect.height > 0
  && rect.x >= 0 && rect.y >= 0
  && rect.x + rect.width <= FIXED_VIEWPORT.width
  && rect.y + rect.height <= FIXED_VIEWPORT.height;

export async function captureTask0750({ output = DEFAULT_OUTPUT } = {}) {
  mkdirSync(path.dirname(output), { recursive: true });
  const captured = await captureVerificationWarningScreen({ output });

  if (captured.selectedChoice !== CAPTURED_CHOICE) {
    throw new Error(`expected the ${CAPTURED_CHOICE} screen, captured ${captured.selectedChoice}`);
  }

  const image = decodePng(captured.pngBytes);
  if (image.width !== FIXED_VIEWPORT.width || image.height !== FIXED_VIEWPORT.height) {
    throw new Error(`fixed size broken: ${image.width}x${image.height}`);
  }

  const labelsInImage = {};
  const missing = [];
  for (const name of REQUIRED_NAMES) {
    const rect = captured.labelRects[name];
    if (!rect) {
      missing.push(`${name} (not drawn)`);
      continue;
    }
    if (!insideViewport(rect)) {
      missing.push(`${name} (outside the ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height} image at ${JSON.stringify(rect)})`);
      continue;
    }
    // Round first, then count: the rounded rectangle is what gets recorded, so
    // the count next to it has to be the count for that same rectangle.
    const pixelRect = { x: Math.round(rect.x), y: Math.round(rect.y), width: Math.round(rect.width), height: Math.round(rect.height) };
    const ink = inkInRect(image, pixelRect);
    labelsInImage[name] = { rect: pixelRect, ...ink };
    if (ink.inkPixels < MIN_INK_PIXELS) missing.push(`${name} (${ink.inkPixels} ink pixels on ${ink.background})`);
  }
  if (missing.length) throw new Error(`required names not visible in the image: ${missing.join(" | ")}`);

  // Whole-image blankness: a screen that painted only its own labels and left
  // the rest empty would still pass the per-label check above.
  const wholeImage = inkInRect(image, { x: 0, y: 0, width: image.width, height: image.height });
  const inkFraction = wholeImage.inkPixels / wholeImage.area;
  if (inkFraction < 0.005 || wholeImage.distinctColors < 50) {
    throw new Error(`image is blank or nearly blank: inkFraction=${inkFraction} distinctColors=${wholeImage.distinctColors}`);
  }

  writeFileSync(output, captured.pngBytes);
  // The rectangles go next to the PNG so the test target can re-count the ink in
  // the committed image instead of taking this run's word for it.
  const factsPath = output.replace(/\.png$/u, ".json");
  writeFileSync(factsPath, `${JSON.stringify({ viewport: FIXED_VIEWPORT, capturedChoice: captured.selectedChoice, labelsInImage }, null, 2)}\n`);
  return {
    screenshot: output,
    facts: factsPath,
    viewport: FIXED_VIEWPORT,
    capturedChoice: captured.selectedChoice,
    savedStatus: captured.savedStatus,
    controlLog: captured.controlLog,
    screenTreeRequiredNames: captured.screenTreeRequiredNames,
    labelsInImage,
    wholeImage: { ...wholeImage, inkFraction: Number(inkFraction.toFixed(4)) },
    png: captured.png,
  };
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureTask0750(parseArgs(process.argv.slice(2))).then((result) => {
    if (!existsSync(result.screenshot)) throw new Error("screenshot file was not written");
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-task-0750-verification-warning: ${error.stack || error.message}`);
    process.exit(1);
  });
}
