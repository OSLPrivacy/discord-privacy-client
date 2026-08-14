/**
 * TASK 7062 — D27(a)'s deliberately hard-to-fake pixel measurement.
 *
 * This module compares only complete, unscaled viewport captures.  It does
 * not have a "close enough" threshold: one changed RGBA pixel is one changed
 * pixel.  Demo data is canonicalised in both rendered documents before either
 * screenshot is taken, using TASK 7051's content-blind semantic classifier.
 */
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { parsePng } from "../screenshots/lib/png.mjs";
import { classifyPageText } from "./task-7051-demo-content.mjs";
import { PINNED_CANVAS } from "./task-7053-geometry-relationships.mjs";

export const NEUTRALISATION_MODE = "semantic-demo-canonical-v1";
export const MEASUREMENT_VERSION = "D27(a)-exact-rgba-v1";

function fail(knob, detail = "") {
  throw new Error(`TASK 7062 D27(a): ${knob}${detail ? `: ${detail}` : ""}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function sameCanvas(value) {
  return value?.width === PINNED_CANVAS.width && value?.height === PINNED_CANVAS.height;
}

function finiteDpr(value) {
  return Number.isFinite(value) && value > 0 && Math.round(value * 1000) === value * 1000;
}

function fullPhysicalPixelCount(dpr) {
  return PINNED_CANVAS.width * dpr * PINNED_CANVAS.height * dpr;
}

function expectedDimensions(dpr) {
  return { width: PINNED_CANVAS.width * dpr, height: PINNED_CANVAS.height * dpr };
}

function pngIsBlank(png) {
  const [red, green, blue, alpha] = png.pixels;
  for (let offset = 4; offset < png.pixels.length; offset += 4) {
    if (png.pixels[offset] !== red || png.pixels[offset + 1] !== green || png.pixels[offset + 2] !== blue || png.pixels[offset + 3] !== alpha) return false;
  }
  return true;
}

function canonicalPlan(markup) {
  const inventory = classifyPageText(markup);
  const textPaths = [];
  const imagePaths = [];
  for (const demo of inventory.demo) {
    if (demo.key.endsWith("@img")) {
      imagePaths.push(demo.key.slice(0, -4));
      continue;
    }
    const separator = demo.key.indexOf(":");
    if (separator === -1 || !demo.key.slice(separator + 1).startsWith("/")) fail("neutralisation", "TASK 7051 returned an unusable demo path");
    textPaths.push(demo.key.slice(separator + 1));
  }
  const plan = {
    mode: NEUTRALISATION_MODE,
    textPaths: [...new Set(textPaths)].sort(),
    imagePaths: [...new Set(imagePaths)].sort(),
  };
  return { ...plan, digest: sha256(JSON.stringify(plan)) };
}

/** Browser-side counterpart to TASK 7051's parser paths.  No literal demo name
 * is accepted here; this receives only semantic positions from canonicalPlan. */
function neutraliseDocument(plan) {
  const isComparableNode = (node) => node.nodeType === Node.ELEMENT_NODE || node.nodeType === Node.TEXT_NODE;
  const nodePath = (node) => {
    const chain = [];
    for (let current = node; current && current.nodeType !== Node.DOCUMENT_NODE; current = current.parentNode) {
      if (!isComparableNode(current)) continue;
      let ordinal = 0;
      for (let sibling = current.parentNode?.firstChild; sibling; sibling = sibling.nextSibling) {
        if (!isComparableNode(sibling)) continue;
        const sameKind = sibling.nodeType === current.nodeType && (current.nodeType !== Node.ELEMENT_NODE || sibling.tagName === current.tagName);
        if (sameKind) ordinal += 1;
        if (sibling === current) break;
      }
      const kind = current.nodeType === Node.TEXT_NODE ? "#text" : current.tagName.toLowerCase();
      chain.unshift(`${kind}[${ordinal}]`);
    }
    return `/${chain.join("/")}`;
  };
  const textPaths = new Set(plan.textPaths);
  const imagePaths = new Set(plan.imagePaths);
  let textNodes = 0;
  let images = 0;
  const walker = document.createTreeWalker(document, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!textPaths.has(nodePath(node))) continue;
    // One invariant token, on both sides.  It is not a tolerance mask and it
    // leaves every non-demo control/prose pixel exposed to exact comparison.
    node.nodeValue = "DEMO";
    textNodes += 1;
  }
  for (const image of document.querySelectorAll("img")) {
    if (!imagePaths.has(nodePath(image))) continue;
    image.setAttribute("alt", "");
    image.setAttribute("src", "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='1' height='1'/%3E");
    images += 1;
  }
  document.documentElement.dataset.oslD27Neutralisation = plan.mode;
  return { applied: true, mode: plan.mode, textNodes, images };
}

function requireNeutralisation(capture, side) {
  const neutralisation = capture.neutralisation;
  if (!neutralisation || neutralisation.applied !== true || neutralisation.mode !== NEUTRALISATION_MODE || typeof neutralisation.digest !== "string") {
    fail("neutralisation", `${side} capture has no complete ${NEUTRALISATION_MODE} record`);
  }
  return neutralisation;
}

function validateCapture(capture, side) {
  if (!capture || typeof capture !== "object") fail("capture", `${side} capture is missing`);
  if (!sameCanvas(capture.viewport)) fail("pinned canvas", `${side} capture viewport must be ${PINNED_CANVAS.width}x${PINNED_CANVAS.height}`);
  if (!finiteDpr(capture.devicePixelRatio)) fail("device pixel ratio", `${side} capture has no finite device pixel ratio`);
  if (!capture.region || capture.region.x !== 0 || capture.region.y !== 0 || !sameCanvas(capture.region)) {
    fail("cropping", `${side} capture region must be the complete 0,0 ${PINNED_CANVAS.width}x${PINNED_CANVAS.height} canvas`);
  }
  if (capture.cropping !== false) fail("cropping", `${side} capture did not record cropping=false`);
  if (capture.scale !== 1 || capture.downscaled !== false) fail("downscaling", `${side} capture must record scale=1 and downscaled=false`);
  if (capture.blurRadius !== 0) fail("blurring", `${side} capture must record blurRadius=0`);
  if (capture.tolerance !== 0) fail("tolerance band", `${side} capture must record tolerance=0`);
  const neutralisation = requireNeutralisation(capture, side);
  if (!Buffer.isBuffer(capture.png) || !capture.png.length) fail("capture", `${side} PNG bytes are missing`);
  const png = parsePng(capture.png);
  const expected = expectedDimensions(capture.devicePixelRatio);
  if (png.width !== expected.width || png.height !== expected.height) {
    fail("region missing", `${side} capture is ${png.width}x${png.height}; the complete pinned region is ${expected.width}x${expected.height} at device pixel ratio ${capture.devicePixelRatio}`);
  }
  const expectedCount = fullPhysicalPixelCount(capture.devicePixelRatio);
  if (png.width * png.height < expectedCount || capture.recordedPixelCount !== expectedCount) {
    fail("region missing", `${side} compared pixel count is ${capture.recordedPixelCount ?? "missing"}; the full pinned region needs ${expectedCount}`);
  }
  if (pngIsBlank(png)) fail("blank capture", `${side} capture is blank`);
  return { png, neutralisation, expectedCount };
}

/**
 * Measure already captured inputs.  Kept public so a CI collector can use the
 * same anti-faking validation as the browser renderer rather than reimplement
 * a friendlier comparison.
 */
export function measureCapturedPair({ design, build, buildId, designPage, buildRoute }) {
  if (typeof buildId !== "string" || !buildId.trim()) fail("recorded metadata", "build is missing");
  if (typeof designPage !== "string" || !designPage.trim()) fail("recorded metadata", "design page is missing");
  if (typeof buildRoute !== "string" || !buildRoute.trim()) fail("recorded metadata", "build route is missing");
  // Reject this before interpreting either image: a differently scaled build
  // image must never be reframed as merely a missing region.
  if (design?.devicePixelRatio !== build?.devicePixelRatio) {
    fail("device pixel ratio", `design=${design?.devicePixelRatio ?? "missing"} build=${build?.devicePixelRatio ?? "missing"}`);
  }
  const checkedDesign = validateCapture(design, "design");
  const checkedBuild = validateCapture(build, "build");
  if (checkedDesign.neutralisation.digest !== checkedBuild.neutralisation.digest) {
    fail("one-sided neutralisation", `design=${checkedDesign.neutralisation.digest} build=${checkedBuild.neutralisation.digest}`);
  }
  if (checkedDesign.expectedCount !== checkedBuild.expectedCount) fail("compared pixel count", "captures do not cover the same full canvas");
  let differing = 0;
  for (let offset = 0; offset < checkedDesign.png.pixels.length; offset += 4) {
    if (checkedDesign.png.pixels[offset] !== checkedBuild.png.pixels[offset]
      || checkedDesign.png.pixels[offset + 1] !== checkedBuild.png.pixels[offset + 1]
      || checkedDesign.png.pixels[offset + 2] !== checkedBuild.png.pixels[offset + 2]
      || checkedDesign.png.pixels[offset + 3] !== checkedBuild.png.pixels[offset + 3]) differing += 1;
  }
  const pixelCount = checkedDesign.expectedCount;
  return {
    measurement: MEASUREMENT_VERSION,
    percent_different: Number(((differing * 100) / pixelCount).toFixed(6)),
    differing_pixel_count: differing,
    compared_pixel_count: pixelCount,
    viewport: { ...PINNED_CANVAS },
    device_pixel_ratio: design.devicePixelRatio,
    region: { x: 0, y: 0, ...PINNED_CANVAS },
    design_image_digest: sha256(design.png),
    build_image_digest: sha256(build.png),
    build: buildId.trim(),
    design_page: designPage.trim(),
    build_route: buildRoute.trim(),
    neutralisation: { mode: NEUTRALISATION_MODE, digest: checkedDesign.neutralisation.digest, applied_to: ["design", "build"] },
    comparison: { crop: "none", scale: 1, blur_radius: 0, tolerance: 0, channels: "RGBA exact" },
  };
}

async function settle(page) {
  await page.send("Runtime.evaluate", {
    expression: "new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)))",
    awaitPromise: true,
    returnByValue: true,
  });
}

async function preparePage(page, { url, side, devicePixelRatio }) {
  if (typeof url !== "string" || !url) fail("capture", `${side} URL is missing`);
  await page.send("Emulation.setDeviceMetricsOverride", {
    width: PINNED_CANVAS.width,
    height: PINNED_CANVAS.height,
    deviceScaleFactor: devicePixelRatio,
    mobile: false,
    screenWidth: PINNED_CANVAS.width,
    screenHeight: PINNED_CANVAS.height,
  });
  await page.navigate(url, { timeoutMs: 30_000 });
  const metrics = await page.evaluate("({ dpr: window.devicePixelRatio, markup: document.documentElement.outerHTML })");
  if (metrics.dpr !== devicePixelRatio) fail("device pixel ratio", `${side} requested=${devicePixelRatio} rendered=${metrics.dpr}`);
  const plan = canonicalPlan(metrics.markup);
  const applied = await page.evaluate(`(${neutraliseDocument.toString()})(${JSON.stringify(plan)})`);
  if (!applied?.applied || applied.mode !== NEUTRALISATION_MODE) fail("neutralisation", `${side} DOM neutralisation did not apply`);
  await settle(page);
  return { plan, applied };
}

/** Render the design and built route in independent pages, neutralise both,
 * then take their complete same-DPR viewport captures. */
export async function renderAndMeasure({ designUrl, buildUrl, buildId, designPage, buildRoute, devicePixelRatio = 1 }) {
  if (!finiteDpr(devicePixelRatio)) fail("device pixel ratio", "requested DPR must be a positive value with at most three decimals");
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", `--force-device-scale-factor=${devicePixelRatio}`, `--window-size=${PINNED_CANVAS.width},${PINNED_CANVAS.height}`, "about:blank"] });
  const designCapturePage = await chrome.openPage();
  const buildCapturePage = await chrome.openPage();
  try {
    // Both documents are canonicalised before either Page.captureScreenshot.
    const [designPrepared, buildPrepared] = await Promise.all([
      preparePage(designCapturePage, { url: designUrl, side: "design", devicePixelRatio }),
      preparePage(buildCapturePage, { url: buildUrl, side: "build", devicePixelRatio }),
    ]);
    const [designPng, buildPng] = await Promise.all([
      designCapturePage.screenshot({ fromSurface: true }),
      buildCapturePage.screenshot({ fromSurface: true }),
    ]);
    const capture = (png, prepared) => ({
      png,
      viewport: { ...PINNED_CANVAS },
      devicePixelRatio,
      region: { x: 0, y: 0, ...PINNED_CANVAS },
      cropping: false,
      scale: 1,
      downscaled: false,
      blurRadius: 0,
      tolerance: 0,
      recordedPixelCount: fullPhysicalPixelCount(devicePixelRatio),
      neutralisation: { applied: true, mode: NEUTRALISATION_MODE, digest: prepared.plan.digest, text_nodes: prepared.applied.textNodes, images: prepared.applied.images },
    });
    return measureCapturedPair({ design: capture(designPng, designPrepared), build: capture(buildPng, buildPrepared), buildId, designPage, buildRoute });
  } finally {
    await Promise.allSettled([designCapturePage.close(), buildCapturePage.close()]);
    await chrome.close();
  }
}

async function main() {
  const index = process.argv.indexOf("--fixture");
  if (index === -1 || !process.argv[index + 1]) fail("recorded metadata", "usage: node scripts/task-7062-pixel-difference.mjs --fixture <JSON>");
  const fixture = JSON.parse(await readFile(path.resolve(process.cwd(), process.argv[index + 1]), "utf8"));
  const result = await renderAndMeasure({
    designUrl: fixture.designUrl,
    buildUrl: fixture.buildUrl,
    buildId: fixture.build,
    designPage: fixture.designPage,
    buildRoute: fixture.buildRoute,
    devicePixelRatio: fixture.devicePixelRatio ?? 1,
  });
  console.log(`TASK7062 ${JSON.stringify(result)}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
