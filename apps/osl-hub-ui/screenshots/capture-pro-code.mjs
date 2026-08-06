#!/usr/bin/env node

import assert from "node:assert/strict";
import { createReadStream, existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import zlib from "node:zlib";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { shippedHubCspHeaders } from "../../../scripts/lib/csp-mirror.mjs";

export const PRO_CODE_CAPTURE_WINDOW = Object.freeze({ width: 1280, height: 800 });
export const PRO_CODE_FIXED_FIXTURE = Object.freeze({
  run: "task-0366-enter-pro-code",
  accounts: Object.freeze([
    Object.freeze({ id: "osl-linux-screen-alma", service: "Signal", handle: "+15550130324", ownerName: "Alma Reed" }),
    Object.freeze({ id: "osl-linux-screen-miles", service: "Discord", handle: "miles.fixed.0324", ownerName: "Miles Chen" }),
  ]),
  names: Object.freeze(["Alma Reed", "Miles Chen", "Nora Vale"]),
  phrases: Object.freeze([
    "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
    "atlas broom cedar dusk ember flint grove honest iris kettle lunar mint",
  ]),
});

const REQUIRED_TITLE = "Enter Pro code";
const REQUIRED_CONTROLS = Object.freeze(["Continue", "Skip", "Back"]);
const MIN_SCREENSHOT_BYTES = 5_000;
const MIN_SCREENSHOT_DISTINCT_COLORS = 16;
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const CHANNELS = Object.freeze({ 2: 3, 6: 4 });

const HERE = path.dirname(fileURLToPath(import.meta.url));
const UI_ROOT = path.dirname(HERE);
const REPO_ROOT = path.resolve(UI_ROOT, "..", "..");
const DIST_DIR = path.join(UI_ROOT, "dist");
const DEFAULT_OUT = path.join(REPO_ROOT, "evidence", "screenshots", "task0366-enter-pro-code.png");

function parseArgs(argv) {
  const outIndex = argv.indexOf("--out");
  const out = outIndex === -1 ? DEFAULT_OUT : argv[outIndex + 1];
  if (outIndex !== -1 && !out) throw new Error("--out requires a PNG path");
  const timeoutIndex = argv.indexOf("--timeout-ms");
  const timeoutMs = timeoutIndex === -1 ? 15_000 : Number(argv[timeoutIndex + 1]);
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new Error("--timeout-ms must be a positive number");
  return { out: path.resolve(out), timeoutMs };
}

function contentType(file) {
  const extension = path.extname(file).toLowerCase();
  if (extension === ".html") return "text/html; charset=utf-8";
  if (extension === ".css") return "text/css; charset=utf-8";
  if (extension === ".js") return "text/javascript; charset=utf-8";
  if (extension === ".svg") return "image/svg+xml";
  if (extension === ".png") return "image/png";
  if (extension === ".woff2") return "font/woff2";
  return "application/octet-stream";
}

function fileForRequest(urlPath) {
  const decoded = decodeURIComponent(urlPath.split("?")[0]);
  const relative = decoded.replace(/^\/+/, "");
  const candidate = path.resolve(DIST_DIR, relative);
  return candidate.startsWith(`${DIST_DIR}${path.sep}`) && existsSync(candidate) && statSync(candidate).isFile()
    ? candidate
    : null;
}

function builtMainStylesheet() {
  if (!existsSync(path.join(DIST_DIR, "index.html"))) {
    throw new Error("missing apps/osl-hub-ui/dist/index.html; run npm run build in apps/osl-hub-ui first");
  }
  const stylesheets = readdirSync(path.join(DIST_DIR, "assets")).filter((name) => /^main-.*\.css$/u.test(name));
  assert.equal(stylesheets.length, 1, "built dist must contain exactly one main stylesheet");
  return `/assets/${stylesheets[0]}`;
}

function memoryStorage() {
  const values = new Map();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => { values.set(key, String(value)); },
    removeItem: (key) => { values.delete(key); },
    clear: () => { values.clear(); },
  };
}

async function proCodeMarkup() {
  const requireFromUi = createRequire(path.join(UI_ROOT, "package.json"));
  const { createServer: createViteServer } = requireFromUi("vite");
  const previousVitest = process.env.VITEST;
  const previousStorage = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: memoryStorage() });
  process.env.VITEST = "task-0366-pro-code-capture";
  const vite = await createViteServer({
    root: UI_ROOT,
    configFile: false,
    appType: "custom",
    logLevel: "error",
    server: { middlewareMode: true, watch: { ignored: ["**/*"] } },
  });
  try {
    const { __oslHubUiTest } = await vite.ssrLoadModule("/src/main.ts");
    __oslHubUiTest.reset({
      route: "onboarding",
      onboardingRoute: "pro",
      coreReady: true,
      bootstrapStatus: "ready",
      licenseAccess: "free",
      services: PRO_CODE_FIXED_FIXTURE.accounts.map((account) => ({
        id: account.service.toLowerCase(),
        displayName: account.service,
        sidebarGlyph: account.service.slice(0, 2).toUpperCase(),
        sidebarOrder: 0,
        category: "consumer",
        launchState: "available",
        supportsNativePreview: true,
        supportsProtectedPreview: true,
        accounts: [{ id: account.id, label: account.ownerName, handle: account.handle }],
      })),
      servicesChecked: true,
    });
    return __oslHubUiTest.renderOnboardingCaptureShell("pro");
  } finally {
    await vite.close();
    if (previousVitest === undefined) delete process.env.VITEST;
    else process.env.VITEST = previousVitest;
    if (previousStorage) Object.defineProperty(globalThis, "localStorage", previousStorage);
    else delete globalThis.localStorage;
  }
}

function startFixtureServer(markup) {
  const stylesheet = builtMainStylesheet();
  const cspHeaders = shippedHubCspHeaders();
  const server = createServer((request, response) => {
    const requestPath = (request.url || "/").split("?")[0];
    if (requestPath === "/" || requestPath === "/onboarding/pro") {
      response.writeHead(200, { ...cspHeaders, "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
      response.end(`<!doctype html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0"><meta name="color-scheme" content="dark light"><title>OSL Privacy</title><link rel="stylesheet" href="${stylesheet}"></head><body><div id="app">${markup}</div></body></html>`);
      return;
    }
    const file = fileForRequest(requestPath);
    if (!file) {
      response.writeHead(404, { ...cspHeaders, "content-type": "text/plain; charset=utf-8" });
      response.end("not found");
      return;
    }
    response.writeHead(200, { ...cspHeaders, "content-type": contentType(file), "cache-control": "no-store" });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function pngChunks(buffer) {
  if (!buffer.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)) throw new Error("PNG signature is missing");
  const chunks = [];
  let offset = PNG_SIGNATURE.length;
  while (offset < buffer.length) {
    if (offset + 12 > buffer.length) throw new Error("truncated PNG chunk header");
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    const end = offset + 12 + length;
    if (end > buffer.length) throw new Error("truncated PNG chunk body");
    chunks.push({ type, data });
    offset = end;
    if (type === "IEND") break;
  }
  if (offset !== buffer.length) throw new Error("PNG has trailing bytes");
  return chunks;
}

function paeth(left, up, upperLeft) {
  const prediction = left + up - upperLeft;
  const leftDistance = Math.abs(prediction - left);
  const upDistance = Math.abs(prediction - up);
  const upperLeftDistance = Math.abs(prediction - upperLeft);
  if (leftDistance <= upDistance && leftDistance <= upperLeftDistance) return left;
  return upDistance <= upperLeftDistance ? up : upperLeft;
}

export function pngFacts(buffer) {
  const chunks = pngChunks(buffer);
  const ihdr = chunks.find((chunk) => chunk.type === "IHDR")?.data;
  if (!ihdr || ihdr.length !== 13) throw new Error("PNG IHDR is missing or invalid");
  const width = ihdr.readUInt32BE(0);
  const height = ihdr.readUInt32BE(4);
  const bitDepth = ihdr[8];
  const colorType = ihdr[9];
  const compression = ihdr[10];
  const filtering = ihdr[11];
  const interlace = ihdr[12];
  const channels = CHANNELS[colorType];
  if (!width || !height) throw new Error("PNG dimensions are empty");
  if (bitDepth !== 8 || !channels || compression !== 0 || filtering !== 0 || interlace !== 0) {
    throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType} interlace=${interlace}`);
  }
  const compressed = Buffer.concat(chunks.filter((chunk) => chunk.type === "IDAT").map((chunk) => chunk.data));
  if (!compressed.length) throw new Error("PNG has no IDAT data");
  const inflated = zlib.inflateSync(compressed);
  const rowBytes = width * channels;
  if (inflated.length !== height * (rowBytes + 1)) throw new Error("PNG inflated byte count does not match dimensions");

  const rows = [];
  let previous = Buffer.alloc(rowBytes);
  let cursor = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[cursor];
    const encoded = inflated.subarray(cursor + 1, cursor + 1 + rowBytes);
    const decoded = Buffer.alloc(rowBytes);
    cursor += rowBytes + 1;
    for (let index = 0; index < encoded.length; index += 1) {
      const left = index >= channels ? decoded[index - channels] : 0;
      const up = previous[index];
      const upperLeft = index >= channels ? previous[index - channels] : 0;
      let predictor = 0;
      if (filter === 1) predictor = left;
      else if (filter === 2) predictor = up;
      else if (filter === 3) predictor = Math.floor((left + up) / 2);
      else if (filter === 4) predictor = paeth(left, up, upperLeft);
      else if (filter !== 0) throw new Error(`unsupported PNG filter ${filter}`);
      decoded[index] = (encoded[index] + predictor) & 0xff;
    }
    rows.push(decoded);
    previous = decoded;
  }

  const colors = new Set();
  for (let y = 0; y < height; y += 4) {
    const row = rows[y];
    for (let x = 0; x < width; x += 4) {
      const start = x * channels;
      const pixel = row.subarray(start, start + channels);
      colors.add(channels === 3 ? `${pixel.toString("hex")}ff` : pixel.toString("hex"));
    }
  }
  return { width, height, bitDepth, colorType, sampleStride: 4, distinctColors: colors.size, bytes: buffer.length };
}

function plainAxNode(node) {
  return {
    role: node.role?.value ?? "",
    name: node.name?.value ?? "",
  };
}

export function validateProCodeCapture(capture) {
  const axNodes = capture.axNodes.map(plainAxNode);
  const axButtons = axNodes.filter((node) => node.role === "button").map((node) => node.name);
  const axTitles = axNodes.filter((node) => node.name === REQUIRED_TITLE).map((node) => node.role);
  const axTitleHeading = axNodes.some((node) => node.role === "heading" && node.name === REQUIRED_TITLE);
  const visibleNames = capture.visibleElements.map((element) => element.name);
  const visibleByName = new Map(capture.visibleElements.map((element) => [element.name, element]));

  assert.ok(axTitleHeading, `screen tree is missing heading ${REQUIRED_TITLE}`);
  for (const name of REQUIRED_CONTROLS) {
    assert.ok(axButtons.includes(name), `screen tree is missing control ${name}`);
    const visible = visibleByName.get(name);
    assert.ok(visible, `image fixture is missing visible element ${name}`);
    assert.ok(visible.rect.width > 0 && visible.rect.height > 0, `visible element ${name} has an empty rect`);
  }
  assert.ok(visibleNames.includes(REQUIRED_TITLE), `image fixture is missing visible title ${REQUIRED_TITLE}`);
  assert.equal(capture.png.width, PRO_CODE_CAPTURE_WINDOW.width, "PNG width must match the fixed capture window");
  assert.equal(capture.png.height, PRO_CODE_CAPTURE_WINDOW.height, "PNG height must match the fixed capture window");
  assert.ok(capture.png.bytes >= MIN_SCREENSHOT_BYTES, `PNG is too small: ${capture.png.bytes} bytes`);
  assert.ok(capture.png.distinctColors >= MIN_SCREENSHOT_DISTINCT_COLORS, `PNG has too few distinct colors: ${capture.png.distinctColors}`);

  return {
    title: REQUIRED_TITLE,
    controls: Object.fromEntries(REQUIRED_CONTROLS.map((name) => [name, {
      ax: axButtons.includes(name),
      image: Boolean(visibleByName.get(name)),
      rect: visibleByName.get(name)?.rect ?? null,
    }])),
    axTitleRoles: axTitles,
    visibleTitle: visibleByName.get(REQUIRED_TITLE)?.rect ?? null,
    png: capture.png,
  };
}

async function capture({ out, timeoutMs }) {
  const markup = await proCodeMarkup();
  const server = await startFixtureServer(markup);
  const chrome = await launchChrome({ timeoutMs });
  const { port } = server.address();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...PRO_CODE_CAPTURE_WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(`http://127.0.0.1:${port}/onboarding/pro`, { timeoutMs });
    await page.send("Accessibility.enable");
    const visibleElements = await page.evaluate(`(() => {
      const selectors = [
        ["Enter Pro code", "#route-heading"],
        ["Continue", ".pro-code-continue"],
        ["Skip", "#skip-pro-setup"],
        ["Back", "#onboarding-back"],
      ];
      return selectors.map(([name, selector]) => {
        const element = document.querySelector(selector);
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return {
          name,
          text: (element.textContent || element.getAttribute("aria-label") || "").replace(/\\s+/g, " ").trim(),
          rect: {
            x: Math.round(rect.x),
            y: Math.round(rect.y),
            width: Math.round(rect.width),
            height: Math.round(rect.height),
          },
        };
      }).filter(Boolean);
    })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const png = await page.screenshot({ fromSurface: true });
    mkdirSync(path.dirname(out), { recursive: true });
    writeFileSync(out, png);
    const captureResult = {
      fixture: PRO_CODE_FIXED_FIXTURE,
      window: PRO_CODE_CAPTURE_WINDOW,
      url: `http://127.0.0.1:${port}/onboarding/pro`,
      pngPath: out,
      png: pngFacts(png),
      axNodes: ax.nodes,
      visibleElements,
    };
    return { ...captureResult, checked: validateProCodeCapture(captureResult) };
  } finally {
    await page.close();
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}

async function main() {
  const result = await capture(parseArgs(process.argv.slice(2)));
  console.log(JSON.stringify({
    fixture: result.fixture,
    window: result.window,
    pngPath: result.pngPath,
    png: result.png,
    ax: {
      titleRoles: result.checked.axTitleRoles,
      controls: Object.fromEntries(Object.entries(result.checked.controls).map(([name, facts]) => [name, facts.ax])),
    },
    image: {
      title: result.checked.visibleTitle,
      controls: Object.fromEntries(Object.entries(result.checked.controls).map(([name, facts]) => [name, facts.rect])),
    },
  }, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`capture-pro-code: fatal error: ${error.stack || error.message}`);
    process.exit(1);
  });
}
