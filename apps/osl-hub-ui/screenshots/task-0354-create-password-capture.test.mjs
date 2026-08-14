import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { countDistinctRgb, readPng } from "./lib/png-pixels.mjs";
import { blankRgbaPng } from "./lib/png-test-fixtures.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0354-create-password.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0354-create-password-screen-tree.json");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const DISTINCT_RGB_FLOOR = 32;
const REQUIRED = Object.freeze([
  "Create a password",
  "Show password",
  "Confirm",
  "Create account",
  "Back",
]);

function assertFixedGateFixture() {
  const source = readFileSync(path.join(APP_ROOT, "src", "linux-onboarding-screen-data.ts"), "utf8");
  assert.match(source, /width:\s*1280/u);
  assert.match(source, /height:\s*800/u);
  for (const value of [
    "Alma Reed",
    "Miles Chen",
    "Nora Vale",
    "amber cabin delta frost harbor ivory juniper lantern meadow nickel orbit quartz",
    "atlas broom cedar dusk ember flint grove honest iris kettle lunar mint",
  ]) {
    assert.match(source, new RegExp(value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
  }
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

function accessibilityTreeText(nodes) {
  return nodes
    .flatMap((node) => [
      node.role?.value,
      node.name?.value,
      node.value?.value,
      node.description?.value,
    ])
    .filter((value) => typeof value === "string" && value.trim())
    .join("\n");
}

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return {
    width: buffer.readUInt32BE(16),
    height: buffer.readUInt32BE(20),
  };
}

function assertDecodedDistinctRgb(png, label) {
  const decodedDistinctRgb = countDistinctRgb(readPng(png));
  assert.ok(decodedDistinctRgb >= DISTINCT_RGB_FLOOR,
    `${label} has too few decoded distinct RGB colours: ${decodedDistinctRgb} (floor ${DISTINCT_RGB_FLOOR})`);
  return decodedDistinctRgb;
}

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (evaluated.exceptionDetails) {
    throw new Error(evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text || "page evaluation threw");
  }
  return evaluated.result.value;
}

test("TASK 0354 captures the fixed Create password screen", async () => {
  assertFixedGateFixture();
  const blankPng = blankRgbaPng(WINDOW.width, WINDOW.height);
  const blankDecodedDistinctRgb = countDistinctRgb(readPng(blankPng));
  assert.throws(() => assertDecodedDistinctRgb(blankPng, "blank PNG"), /too few decoded distinct RGB colours: 1/u);
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${WINDOW.width},${WINDOW.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: WINDOW.width,
      screenHeight: WINDOW.height,
    });
    await page.navigate(`${url}screenshots/task-0354-create-password-fixture.html`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");

    await evaluateValue(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.querySelector("#identity-password-confirm")) {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) {
          reject(new Error("Create password form did not render"));
          return;
        }
        setTimeout(tick, 25);
      };
      tick();
    })`);

    const screen = await evaluateValue(page, `(() => {
      const text = document.body.innerText.replace(/\\s+/gu, " ").trim();
      const controls = [...document.querySelectorAll("button, input")]
        .map((element) => ({
          tag: element.tagName.toLowerCase(),
          id: element.id,
          type: element.getAttribute("type") || "",
          text: element.innerText.replace(/\\s+/gu, " ").trim(),
          ariaLabel: element.getAttribute("aria-label") || "",
          label: element.id
            ? (document.querySelector('label[for="' + CSS.escape(element.id) + '"]')?.innerText || "").replace(/\\s+/gu, " ").trim()
            : "",
          disabled: Boolean(element.disabled),
        }));
      return { title: document.querySelector("#route-heading")?.textContent || "", text, controls };
    })()`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = [
      screen.title,
      screen.text,
      screen.controls.map((control) => `${control.label} ${control.ariaLabel} ${control.text}`).join("\n"),
      accessibilityTreeText(ax.nodes ?? []),
    ].join("\n");
    const normalizedTree = treeText.toLowerCase();

    for (const required of REQUIRED) {
      assert.match(normalizedTree, new RegExp(required.toLowerCase().replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "u"));
    }

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}screenshots/task-0354-create-password-fixture.html`, window: WINDOW, required: REQUIRED, screen, axNodes: ax.nodes }, null, 2));

    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    const decodedDistinctRgb = assertDecodedDistinctRgb(png, "capture PNG");

    console.log(`TASK0354_PNG=${PNG_PATH}`);
    console.log(`TASK0354_TREE=${TREE_PATH}`);
    console.log(`TASK0354_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0354_PNG_BYTES=${png.length}`);
    console.log(`TASK0354_BLANK_DECODED_DISTINCT_RGB=${blankDecodedDistinctRgb} floor=${DISTINCT_RGB_FLOOR} rejected=true`);
    console.log(`TASK0354_DECODED_DISTINCT_RGB=${decodedDistinctRgb} floor=${DISTINCT_RGB_FLOOR}`);
    console.log(`TASK0354_REQUIRED=${REQUIRED.join("|")}`);
    console.log(`TASK0354_CONTROLS=${screen.controls.map((control) => control.label || control.ariaLabel || control.text).filter(Boolean).join("|")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
