import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const SHOT_ROOT = path.join(APP_ROOT, "screenshots");
const ARTIFACT_DIR = path.join(SHOT_ROOT, "artifacts");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const PALETTE = new Set(["#080c0d", "#2ac0f0", "#3dd68c", "#f0b429", "#f0a93a", "#a79bff", "#e05656"]);
const FIXTURES = Object.freeze([
  { name: "allowed", file: "task-0127-allowed-place-fixture.html", expectedControls: 3 },
  { name: "unlisted", file: "task-0127-unlisted-place-fixture.html", expectedControls: 0 },
]);

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

function fixtureCss(html) {
  const match = html.match(/<style>([\s\S]*?)<\/style>/u);
  assert.ok(match, "fixture must contain its checked stylesheet");
  return match[1];
}

function assertDesignConformance(name, html) {
  const css = fixtureCss(html);
  const colors = css.match(/#[0-9a-f]{6}\b/giu) ?? [];
  for (const color of colors) assert.ok(PALETTE.has(color.toLowerCase()), `${name}: unsupported accent ${color}`);
  assert.match(css, /--ground:\s*#080c0d\b/u, `${name}: ground must be #080c0d`);
  assert.doesNotMatch(css, /(?:box|text)-shadow\s*:/iu, `${name}: shadows are forbidden`);
  assert.doesNotMatch(css, /(?:linear|radial|conic)-gradient\s*\(/iu, `${name}: gradients are forbidden`);
  for (const radius of css.matchAll(/border-radius:\s*(\d+)px/giu)) {
    assert.ok(Number(radius[1]) <= 3, `${name}: border radius ${radius[1]}px exceeds 3px`);
  }
  assert.match(css, /\.status[\s\S]*?font-family:\s*Consolas, monospace;/u, `${name}: status must use Consolas`);
  assert.match(css, /\.status[\s\S]*?text-transform:\s*uppercase;/u, `${name}: status must be uppercase`);
  if (html.includes("<button")) {
    const buttonRule = css.match(/button\s*\{([\s\S]*?)\}/u)?.[1] ?? "";
    assert.match(buttonRule, /border:\s*1px solid var\(--cyan\);/u, `${name}: button must have cyan outline`);
    assert.match(buttonRule, /background:\s*transparent;/u, `${name}: button must be outline-only`);
  }
}

function assertProductionGate() {
  const source = readFileSync(path.join(APP_ROOT, "src", "main.ts"), "utf8");
  assert.match(source, /const openPlaceAllowed = context === null \|\| scopeApproved;/u);
  assert.match(source, /const transcriptVisibilityControl = openPlaceAllowed\s*\n\s*\?/u);
  assert.match(source, /const composerControl = openPlaceAllowed && \(discordMarkerAvailable \|\| nativeDiscordProtectionActive\)/u);
}

async function startVite() {
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text);
  return result.result.value;
}

test("TASK0127 captures paired Linux fixtures and rejects visual nonconformance", async () => {
  assertProductionGate();
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const variant = process.env.OSL_TASK0127_VARIANT;
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false, screenWidth: WINDOW.width, screenHeight: WINDOW.height });
    const results = [];
    for (const fixture of FIXTURES) {
      let html = readFileSync(path.join(SHOT_ROOT, fixture.file), "utf8");
      if (variant === "filled-cyan" && fixture.name === "allowed") html = html.replace("background: transparent;", "background: var(--cyan);");
      if (variant === "shadow" && fixture.name === "allowed") html = html.replace("button {", "button { box-shadow: 0 2px 8px var(--cyan);");
      assertDesignConformance(fixture.name, html);
      if (variant) continue;
      await page.navigate(`${url}screenshots/${fixture.file}`, { timeoutMs: 30_000 });
      await evaluate(page, "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
      const screen = await evaluate(page, `(() => { const main = document.querySelector('main'); const box = main?.getBoundingClientRect(); return { fixture: main?.dataset.fixture, controls: document.querySelectorAll('[data-osl-control]').length, status: document.querySelector('.status')?.textContent?.trim(), background: getComputedStyle(document.body).backgroundColor, mainBox: box && { x: box.x, y: box.y, width: box.width, height: box.height, border: getComputedStyle(main).borderTopWidth }, buttons: [...document.querySelectorAll('button')].map(button => ({ fill: getComputedStyle(button).backgroundColor, shadow: getComputedStyle(button).boxShadow, radius: getComputedStyle(button).borderRadius })) }; })()`);
      assert.equal(screen.fixture, fixture.name);
      assert.equal(screen.controls, fixture.expectedControls, `${fixture.name}: wrong visible OSL control count`);
      assert.equal(screen.background, "rgb(8, 12, 13)");
      assert.deepEqual(screen.mainBox, { x: 280, y: 190, width: 720, height: 420, border: "1px" }, `${fixture.name}: fixture frame must match`);
      for (const button of screen.buttons) {
        assert.equal(button.fill, "rgba(0, 0, 0, 0)", `${fixture.name}: filled button`);
        assert.equal(button.shadow, "none", `${fixture.name}: button shadow`);
        assert.ok(Number.parseFloat(button.radius) <= 3, `${fixture.name}: button radius ${button.radius}`);
      }
      const png = await page.screenshot({ fromSurface: true });
      const output = path.join(ARTIFACT_DIR, `task-0127-${fixture.name}-place-linux.png`);
      writeFileSync(output, png);
      const dimensions = pngDimensions(png);
      assert.deepEqual(dimensions, WINDOW);
      assert.ok(png.length > 4_000, `${fixture.name}: PNG too small (${png.length})`);
      results.push({ ...fixture, output, dimensions, bytes: png.length, status: screen.status });
    }
    assert.equal(variant, undefined, `TASK0127 nonconforming fixture ${variant} was accepted`);
    assert.deepEqual(results.map(({ expectedControls }) => expectedControls), [3, 0]);
    console.log(`TASK0127_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0127_ALLOWED_CONTROLS=${results[0].expectedControls}`);
    console.log(`TASK0127_UNLISTED_CONTROLS=${results[1].expectedControls}`);
    console.log(`TASK0127_ALLOWED_PNG=${results[0].output}`);
    console.log(`TASK0127_UNLISTED_PNG=${results[1].output}`);
    console.log(`TASK0127_SCREENSHOT_BYTES=${results.map(({ name, bytes }) => `${name}:${bytes}`).join("|")}`);
    console.log("TASK0127_DESIGN=ground:#080c0d|accents:#2ac0f0,#3dd68c,#f0b429,#f0a93a,#a79bff,#e05656|radii:0-3px|outline-buttons|no-shadows|no-gradients|status:Consolas-uppercase");
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
