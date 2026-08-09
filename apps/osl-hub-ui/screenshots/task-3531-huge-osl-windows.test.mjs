import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createReadStream, existsSync, mkdirSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import test from "node:test";
import { inflateSync } from "node:zlib";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { UI_ROOT, hubScreenshotSurfaceMarkup } from "../../../scripts/lib/hub-surface-fixtures.mjs";

const DIST_DIR = path.join(UI_ROOT, "dist");
const ARTIFACT_DIR = process.env.TASK3531_ARTIFACT_DIR
  ? path.resolve(process.env.TASK3531_ARTIFACT_DIR)
  : path.join(UI_ROOT, "screenshots", "artifacts", "task-3531-huge-osl-windows");
const HIDE_ONE_4K_CONTROL = process.env.TASK3531_HIDE_CONTROL_AT_4K === "1";
const LOG_EACH_RUN = process.env.TASK3531_LOG_EACH_RUN === "1";
const START_AT_SURFACE = process.env.TASK3531_START_AT_SURFACE || "";
const NORMAL = Object.freeze({ name: "normal", width: 1440, height: 900, screenWidth: 1440, screenHeight: 900, maximized: false });
const RUNS = Object.freeze([
  { name: "maximized", width: 3840, height: 2160, screenWidth: 3840, screenHeight: 2160, maximized: true },
  { name: "very-large", width: 3600, height: 2160, screenWidth: 3840, screenHeight: 2160, maximized: false },
  { name: "4k", width: 3840, height: 2160, screenWidth: 3840, screenHeight: 2160, maximized: false },
]);

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
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
  const relative = decodeURIComponent(urlPath.split("?")[0]).replace(/^\/+/, "");
  const candidate = path.resolve(DIST_DIR, relative);
  return candidate.startsWith(`${DIST_DIR}${path.sep}`) && existsSync(candidate) && statSync(candidate).isFile() ? candidate : null;
}

function builtMainStylesheet() {
  assert.ok(existsSync(path.join(DIST_DIR, "index.html")), "missing UI dist; run the Vite build before this capture");
  const stylesheets = readdirSync(path.join(DIST_DIR, "assets")).filter((name) => /^main-.*\.css$/u.test(name));
  assert.equal(stylesheets.length, 1, "built dist must contain one main stylesheet");
  return `/assets/${stylesheets[0]}`;
}

function startServer(surfaces) {
  const stylesheet = builtMainStylesheet();
  const byPath = new Map(surfaces.map((surface) => [`/${encodeURIComponent(surface.name)}`, surface]));
  const server = createServer((request, response) => {
    const requestPath = (request.url || "/").split("?")[0];
    const surface = byPath.get(requestPath);
    if (surface) {
      const is4k = new URL(request.url || "/", "http://localhost").searchParams.get("run") === "4k";
      // Red proof only: hide precisely the first currently visible named control
      // in the 4K capture. Production markup and styles are otherwise unchanged.
      const redProof = HIDE_ONE_4K_CONTROL && is4k ? `<script>addEventListener("DOMContentLoaded",()=>{const v=e=>{const s=getComputedStyle(e),r=e.getBoundingClientRect();return s.display!=="none"&&s.visibility!=="hidden"&&+s.opacity!==0&&r.width>0&&r.height>0};const n=e=>(e.getAttribute("aria-label")||e.textContent||e.getAttribute("placeholder")||"").trim();const e=[...document.querySelectorAll("a[href],button,input,select,textarea,[role=button],[role=link]")].find(e=>v(e)&&!e.disabled&&n(e));if(e)e.style.display="none"})</script>` : "";
      const bodyMarkup = surface.name.startsWith("onboarding:")
        ? `<div class="app-frame with-titlebar"><header class="desktop-titlebar" aria-hidden="true"></header><div class="onboarding-shell"><main class="onboarding-panel">${surface.markup}</main></div></div>`
        : `<main data-task3531-surface="${surface.name}">${surface.markup}</main>`;
      response.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
      response.end(`<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1.0"><title>${surface.name}</title><link rel="stylesheet" href="${stylesheet}"></head><body>${bodyMarkup}${redProof}</body></html>`);
      return;
    }
    const file = fileForRequest(requestPath);
    if (!file) {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("not found");
      return;
    }
    response.writeHead(200, { "content-type": contentType(file), "cache-control": "no-store" });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function paeth(left, up, upLeft) {
  const prediction = left + up - upLeft;
  const leftDistance = Math.abs(prediction - left);
  const upDistance = Math.abs(prediction - up);
  const upLeftDistance = Math.abs(prediction - upLeft);
  return leftDistance <= upDistance && leftDistance <= upLeftDistance ? left : upDistance <= upLeftDistance ? up : upLeft;
}

function decodePng(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "capture must be a PNG");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const idat = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.subarray(offset + 4, offset + 8).toString("ascii");
    const data = bytes.subarray(offset + 8, offset + 8 + length);
    offset += length + 12;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      assert.equal(data[8], 8, "PNG must be 8-bit");
      colorType = data[9];
    } else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
  }
  const channels = colorType === 6 ? 4 : colorType === 2 ? 3 : 0;
  assert.ok(channels > 0, `unsupported PNG color type ${colorType}`);
  const stride = width * channels;
  const raw = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * channels);
  let source = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = raw[source++];
    const row = raw.subarray(source, source + stride);
    source += stride;
    const output = y * stride;
    const previous = y === 0 ? -1 : output - stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? pixels[output + x - channels] : 0;
      const up = previous >= 0 ? pixels[previous + x] : 0;
      const upLeft = previous >= 0 && x >= channels ? pixels[previous + x - channels] : 0;
      const value = row[x];
      pixels[output + x] = filter === 0 ? value : filter === 1 ? (value + left) & 255 : filter === 2 ? (value + up) & 255 : filter === 3 ? (value + Math.floor((left + up) / 2)) & 255 : filter === 4 ? (value + paeth(left, up, upLeft)) & 255 : assert.fail(`unsupported PNG filter ${filter}`);
    }
  }
  return { width, height, channels, pixels };
}

function colorAt(image, x, y) {
  const offset = (y * image.width + x) * image.channels;
  return `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
}

function regionDistinctColors(image, rect) {
  const colors = new Set();
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(image.height, Math.ceil(rect.y + rect.height));
  // At most 200k samples per region: colour-count evidence, never luminance.
  // This is dense enough to retain thin text in a wide summary-row control.
  const step = Math.max(1, Math.ceil(Math.sqrt(Math.max(1, (x1 - x0) * (y1 - y0) / 200_000))));
  for (let y = y0; y < y1; y += step) for (let x = x0; x < x1; x += step) colors.add(colorAt(image, x, y));
  return colors.size;
}

function imageFacts(bytes, title, controls) {
  const image = decodePng(bytes);
  const sampled = new Set();
  for (let y = 0; y < image.height; y += 8) for (let x = 0; x < image.width; x += 8) sampled.add(colorAt(image, x, y));
  return {
    width: image.width,
    height: image.height,
    bytes: bytes.length,
    sha256: sha256(bytes),
    sampledDistinctColors: sampled.size,
    titleDistinctColors: regionDistinctColors(image, title.rect),
    controlDistinctColors: Object.fromEntries(controls.map((control) => [control.uid, regionDistinctColors(image, control.rect)])),
  };
}

function treeExpression() {
  return `(() => {
    const text = value => (value || '').replace(/\\s+/gu, ' ').trim();
    const visible = element => {
      const ownSummary = element.closest('summary');
      for (let parent = element.parentElement; parent; parent = parent.parentElement) {
        if (parent.localName === 'details' && !parent.open && ownSummary?.parentElement !== parent) return false;
      }
      const style = getComputedStyle(element), rect = element.getBoundingClientRect();
      if (style.display === 'none' || style.visibility === 'hidden' || Number.parseFloat(style.opacity) === 0 || rect.width <= 2 || rect.height <= 2) return false;
      return true;
    };
    const rect = element => { const r = element.getBoundingClientRect(); return { x:r.x, y:r.y, width:r.width, height:r.height }; };
    const name = element => {
      const aria = text(element.getAttribute('aria-label'));
      if (aria) return aria;
      const ids = text(element.getAttribute('aria-labelledby')).split(/\\s+/u).filter(Boolean);
      const labelled = text(ids.map(id => document.getElementById(id)?.textContent || '').join(' '));
      if (labelled) return labelled;
      if (element.id) { const label = document.querySelector('label[for="' + CSS.escape(element.id) + '"]'); if (text(label?.textContent)) return text(label.textContent); }
      const wrapped = element.closest('label'); if (text(wrapped?.textContent)) return text(wrapped.textContent);
      return text(element.textContent) || text(element.getAttribute('placeholder')) || text(element.getAttribute('title'));
    };
    const uid = (element, index) => element.tagName.toLowerCase() + '#' + (element.id || '') + ':' + index + ':' + name(element);
    const titleElement = [...document.querySelectorAll('h1, [role="heading"][aria-level="1"]')].find(visible)
      || [...document.querySelectorAll('h1,h2,[role="heading"]')].find(visible)
      || [...document.querySelectorAll('.service-context strong')].find(visible)
      // The pared-back account entry intentionally makes its sole primary
      // action the visible screen identity; its semantic h1 is screen-reader-only.
      || [...document.querySelectorAll('.signin-unlock-label')].find(visible);
    const title = titleElement ? { name: name(titleElement), rect: rect(titleElement), tag: titleElement.tagName.toLowerCase() } : null;
    const controls = [...document.querySelectorAll('a[href],button,input:not([type="hidden"]),select,textarea,summary,label[for],label:has(input,select,textarea),[role="button"],[role="link"],[role="checkbox"],[role="radio"],[role="switch"]')]
      .filter(element => visible(element) && !element.disabled && element.getAttribute('aria-hidden') !== 'true')
      // A labelled native input is represented by its visible label. This
      // avoids treating the checkbox glyph as a separate text-bearing control.
      .filter(element => {
        if (!element.matches('input,select,textarea')) return true;
        const label = element.closest('label') || (element.id ? document.querySelector('label[for="' + CSS.escape(element.id) + '"]') : null);
        return !label || !visible(label);
      })
      .map((element, index) => ({ uid: uid(element, index), name: name(element), tag: element.tagName.toLowerCase(), rect: rect(element) }));
    return { title, controls, visibleText: text(document.body.innerText), viewport: { width: innerWidth, height: innerHeight }, screen: { width: screen.width, height: screen.height } };
  })()`;
}

function safeName(name) {
  return name.replaceAll(/[^a-z0-9]+/giu, "-").replaceAll(/^-|-$/gu, "");
}

async function setMetrics(page, run) {
  await page.send("Emulation.setDeviceMetricsOverride", { width: run.width, height: run.height, deviceScaleFactor: 1, mobile: false, screenWidth: run.screenWidth, screenHeight: run.screenHeight });
  await page.send("Emulation.setVisibleSize", { width: run.width, height: run.height });
}

function assertBounds(item, viewport, surface, run) {
  const { x, y, width, height } = item.rect;
  assert.ok(width > 0 && height > 0, `${surface} ${run}: ${item.name} has an empty rect`);
  assert.ok(x >= 0 && y >= 0 && x + width <= viewport.width && y + height <= viewport.height, `${surface} ${run}: ${item.name} is outside ${viewport.width}x${viewport.height}: ${JSON.stringify(item.rect)}`);
}

async function captureRun(page, baseUrl, surface, run, baseline) {
  await setMetrics(page, run);
  await page.navigate(`${baseUrl}/${encodeURIComponent(surface.name)}?run=${encodeURIComponent(run.name)}`);
  await page.evaluate("document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
  const tree = await page.evaluate(treeExpression());
  assert.ok(tree.title?.name, `${surface.name} ${run.name}: page title is missing from the screen tree`);
  assert.ok(tree.visibleText.includes(tree.title.name), `${surface.name} ${run.name}: page title ${tree.title.name} is not visible text`);
  assert.ok(tree.controls.length > 0, `${surface.name} ${run.name}: zero named controls`);
  const hugeNameCounts = new Map();
  for (const control of tree.controls) hugeNameCounts.set(control.name, (hugeNameCounts.get(control.name) ?? 0) + 1);
  const missingControls = baseline.controls.map((control) => control.name).filter((name) => {
    const available = hugeNameCounts.get(name) ?? 0;
    if (available <= 0) return true;
    hugeNameCounts.set(name, available - 1);
    return false;
  });
  assert.equal(tree.controls.length, baseline.controls.length, `${surface.name} ${run.name}: named control count differs from normal size; missing ${run.name} control(s)=${JSON.stringify(missingControls)}; normal=${JSON.stringify(baseline.controls.map((control) => control.name))}; huge=${JSON.stringify(tree.controls.map((control) => control.name))}`);
  assert.deepEqual(tree.controls.map((control) => control.name), baseline.controls.map((control) => control.name), `${surface.name} ${run.name}: named controls differ from normal size`);
  assertBounds(tree.title, tree.viewport, surface.name, run.name);
  for (const control of tree.controls) {
    assert.ok(control.name, `${surface.name} ${run.name}: unnamed control ${control.uid}`);
    assertBounds(control, tree.viewport, surface.name, run.name);
  }
  const ax = await page.send("Accessibility.getFullAXTree");
  const axNames = ax.nodes.map((node) => node.name?.value).filter((value) => typeof value === "string");
  assert.ok(axNames.includes(tree.title.name), `${surface.name} ${run.name}: AX tree is missing title ${tree.title.name}`);
  const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false, clip: { x: 0, y: 0, width: run.width, height: run.height, scale: 1 } });
  const image = imageFacts(png, tree.title, tree.controls);
  const directory = path.join(ARTIFACT_DIR, run.name);
  mkdirSync(directory, { recursive: true });
  const stem = safeName(surface.name);
  // Preserve failed captures too: a red proof needs the image and tree that
  // made it red, not merely an assertion message.
  writeFileSync(path.join(directory, `${stem}.png`), png);
  writeFileSync(path.join(directory, `${stem}-screen-tree.json`), JSON.stringify({ surface: surface.name, run, title: tree.title, controls: tree.controls, visibleText: tree.visibleText, axTree: ax.nodes, image }, null, 2));
  assert.deepEqual({ width: image.width, height: image.height }, { width: run.width, height: run.height }, `${surface.name} ${run.name}: capture dimensions`);
  assert.ok(image.sampledDistinctColors > 20, `${surface.name} ${run.name}: nearly blank image has ${image.sampledDistinctColors} distinct colours`);
  assert.ok(image.titleDistinctColors > 2, `${surface.name} ${run.name}: title ${tree.title.name} has no rendered image detail`);
  for (const control of tree.controls) {
    if (image.controlDistinctColors[control.uid] <= 2) console.log(`TASK3531_LOW_DETAIL surface=${surface.name} run=${run.name} control=${JSON.stringify(control.name)} rect=${JSON.stringify(control.rect)} colours=${image.controlDistinctColors[control.uid]}`);
    assert.ok(image.controlDistinctColors[control.uid] > 2, `${surface.name} ${run.name}: control ${control.name} has no rendered image detail`);
  }
  if (LOG_EACH_RUN) console.log(`TASK3531_RUN surface=${surface.name} run=${run.name} title=${JSON.stringify(tree.title.name)} controls=${tree.controls.length} outside=0 distinct_colours=${image.sampledDistinctColors} png=${path.join(directory, `${stem}.png`)}`);
  // A decoded 4K PNG is tens of megabytes. CI captures 120 of them in one
  // process, so explicitly collect when the runner provides this standard hook.
  globalThis.gc?.();
  return { surface: surface.name, run: run.name, title: tree.title.name, controls: tree.controls.length, outside: 0, distinctColors: image.sampledDistinctColors, png: path.join(directory, `${stem}.png`) };
}

test("TASK 3531 captures every canonical OSL surface three times at huge 4K metrics", async () => {
  const surfaces = await hubScreenshotSurfaceMarkup("task-3531-huge-window-capture");
  assert.equal(surfaces.length, 40, "canonical OSL surface inventory changed; update this audit deliberately");
  const startAt = START_AT_SURFACE ? surfaces.findIndex((surface) => surface.name === START_AT_SURFACE) : 0;
  assert.ok(startAt >= 0, `unknown TASK3531_START_AT_SURFACE ${START_AT_SURFACE}`);
  const auditedSurfaces = surfaces.slice(startAt);
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const server = await startServer(surfaces);
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", "--window-size=3840,2160", "--start-maximized", "about:blank"] });
  const { port } = server.address();
  const page = await chrome.openPage();
  const results = [];
  try {
    for (const surface of auditedSurfaces) {
      await setMetrics(page, NORMAL);
      await page.navigate(`http://127.0.0.1:${port}/${encodeURIComponent(surface.name)}?run=normal`);
      await page.evaluate("document.fonts.ready.then(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))");
      const baseline = await page.evaluate(treeExpression());
      assert.ok(baseline.title?.name, `${surface.name} normal: page title is missing`);
      assert.ok(baseline.controls.length > 0, `${surface.name} normal: zero named controls`);
      for (const control of baseline.controls) assert.ok(control.name, `${surface.name} normal: unnamed control`);
      for (const run of RUNS) results.push(await captureRun(page, `http://127.0.0.1:${port}`, surface, run, baseline));
    }
    assert.equal(results.length, auditedSurfaces.length * RUNS.length, "must capture exactly three huge runs for every OSL surface");
    assert.equal(results.filter((result) => result.outside !== 0).length, 0, "controls outside window");
    console.log(`TASK3531_DONE surfaces=${auditedSurfaces.length} runs=${results.length} runs_per_surface=${RUNS.length} normal_baselines=${auditedSurfaces.length} 4k=3840x2160 all_controls_outside=0 scoring=distinct-colours`);
  } finally {
    await page.close();
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  }
}, { timeout: 900_000 });
