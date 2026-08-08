#!/usr/bin/env node

import { createHash } from "node:crypto";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

export const FIXED_VIEWPORT = { width: 800, height: 620 };
export const REQUIRED_NAMES = ["Connection choice", "Tor", "Direct", "Mullvad", "You can use both. Neither replaces the other.", "Continue", "Back"];
const REQUIRED_IMAGE_TEXT = ["Connection choice", "Tor", "Direct", "Mullvad", "You can use both. Neither replaces the other.", "Continue", "Back"];

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);
const DEFAULT_OUTPUT = path.join(SCRIPT_DIR, "connection-choice.png");

function parseArgs(args) {
  const outputIndex = args.indexOf("--output");
  return {
    output: outputIndex === -1 ? DEFAULT_OUTPUT : path.resolve(args[outputIndex + 1] ?? ""),
  };
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function pngFacts(bytes) {
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
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    if (kind === "IDAT") idat.push(data);
    if (kind === "IEND") break;
    offset += 12 + length;
  }
  const inflated = zlib.inflateSync(Buffer.concat(idat));
  const rowBytes = width * channels;
  let cursor = 0;
  let previous = Buffer.alloc(rowBytes);
  const colors = new Set();
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
        const pixel = decoded.subarray(start, start + channels);
        colors.add(channels === 3 ? `${pixel.toString("hex")}ff` : pixel.toString("hex"));
      }
    }
    previous = decoded;
  }

  return { width, height, bytes: bytes.length, sha256: sha256(bytes), distinctColors: colors.size };
}

function flattenAxTree(nodes) {
  return nodes
    .map((node) => node.name?.value)
    .filter((name) => typeof name === "string" && name.trim())
    .map((name) => name.trim());
}

function requireAllStrings(haystack, required, label) {
  const missing = required.filter((expected) => !haystack.includes(expected));
  if (missing.length) throw new Error(`${label} missing: ${missing.join(", ")}`);
}

export async function captureConnectionChoice({ output = DEFAULT_OUTPUT } = {}) {
  const server = await createViteServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0, strictPort: false },
    logLevel: "error",
  });
  await server.listen();
  const address = server.httpServer.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP port");

  const chrome = await launchChrome();
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
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/connection-choice-fixture.html`);
    const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
    if (ready !== "connection-choice") throw new Error("connection choice fixture did not become ready");
    await new Promise((resolve) => setTimeout(resolve, 250));

    const imageText = await page.evaluate(`document.body.innerText.replace(/\\s+/g, " ").trim()`);
    requireAllStrings(imageText, REQUIRED_IMAGE_TEXT, "visible image text");

    const axTree = await page.send("Accessibility.getFullAXTree");
    const screenTree = flattenAxTree(axTree.nodes);
    requireAllStrings(screenTree, REQUIRED_NAMES, "screen tree");

    const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
    const facts = pngFacts(png);
    if (facts.width !== FIXED_VIEWPORT.width || facts.height !== FIXED_VIEWPORT.height) {
      throw new Error(`expected ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height}, captured ${facts.width}x${facts.height}`);
    }
    if (facts.bytes < 10_000 || facts.distinctColors < 50) {
      throw new Error(`PNG is blank or nearly blank: bytes=${facts.bytes} distinctColors=${facts.distinctColors}`);
    }

    mkdirSync(path.dirname(output), { recursive: true });
    writeFileSync(output, png);
    return {
      screenshot: output,
      viewport: FIXED_VIEWPORT,
      screenTreeRequiredNames: Object.fromEntries(REQUIRED_NAMES.map((name) => [name, screenTree.filter((value) => value === name).length])),
      visibleTextRequiredStrings: Object.fromEntries(REQUIRED_IMAGE_TEXT.map((name) => [name, imageText.includes(name) ? 1 : 0])),
      png: facts,
    };
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureConnectionChoice(parseArgs(process.argv.slice(2))).then((result) => {
    if (!existsSync(result.screenshot)) throw new Error("screenshot file was not written");
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-connection-choice: ${error.stack || error.message}`);
    process.exit(1);
  });
}
