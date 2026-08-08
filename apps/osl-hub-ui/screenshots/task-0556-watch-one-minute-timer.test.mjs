import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { randomUUID } from "node:crypto";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts", "task-0556-one-minute-timer");
const FIXTURE_PATH = "screenshots/task-0556-one-minute-timer-fixture.html";
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const ONE_MINUTE_MS = 60_000;
const SCREENS = Object.freeze(["OSL Screen A", "OSL Screen B"]);
const DISABLE_REMOVAL_ON = process.env.TASK0556_DISABLE_TIMER_REMOVAL_ON || "";

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

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "capture must be a PNG");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
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

async function waitForReady(page) {
  await evaluateValue(page, `new Promise((resolve, reject) => {
    const deadline = Date.now() + 10000;
    const tick = () => {
      if (document.documentElement.dataset.task0556 === "ready") return resolve();
      if (Date.now() > deadline) return reject(new Error("TASK0556 fixture did not render"));
      setTimeout(tick, 25);
    };
    tick();
  })`);
}

async function screenTree(page) {
  return evaluateValue(page, `(() => {
    const messages = [...document.querySelectorAll("[data-task0556-marked-message]")];
    return {
      screen: document.documentElement.dataset.task0556Screen,
      state: document.documentElement.dataset.task0556State,
      heading: document.querySelector("#task0556-heading")?.textContent?.trim() || "",
      markedMessageCount: messages.length,
      markedMessageTexts: messages.map((message) => message.querySelector(".task0556-message-text")?.textContent?.trim() || ""),
      emptyStateText: document.querySelector("[data-task0556-empty]")?.textContent?.trim() || "",
      visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    };
  })()`);
}

async function capture(page, phase, screen) {
  const png = await page.screenshot({ fromSurface: true });
  const safeScreen = screen.toLowerCase().replaceAll(" ", "-");
  const pngPath = path.join(ARTIFACT_DIR, `${phase}-${safeScreen}.png`);
  const treePath = path.join(ARTIFACT_DIR, `${phase}-${safeScreen}-screen-tree.json`);
  const tree = await screenTree(page);
  writeFileSync(pngPath, png);
  writeFileSync(treePath, JSON.stringify(tree, null, 2));

  const image = readFileSync(pngPath);
  const dimensions = pngDimensions(image);
  const uniqueBytes = new Set(image).size;
  assert.deepEqual(dimensions, WINDOW, `${phase} ${screen} must use the fixed capture size`);
  assert.ok(image.length > 10_000, `${phase} ${screen} PNG is nearly blank: ${image.length} bytes`);
  assert.ok(uniqueBytes > 64, `${phase} ${screen} PNG has too little visual variation: ${uniqueBytes} unique bytes`);
  assert.ok(tree.visibleText.length > 80, `${phase} ${screen} screen tree is nearly blank`);

  console.log(`TASK0556_CAPTURE phase=${phase} screen=${screen} png=${pngPath} tree=${treePath} bytes=${image.length} unique_bytes=${uniqueBytes}`);
  return tree;
}

test("TASK 0556 watches one random marked one-minute message disappear on both screens", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const mark = `TASK0556-MARKED-ONE-MINUTE-${randomUUID()}`;
  const expiresAt = Date.now() + ONE_MINUTE_MS;
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
  const pages = await Promise.all(SCREENS.map(() => chrome.openPage()));

  try {
    for (const [index, page] of pages.entries()) {
      await page.send("Emulation.setDeviceMetricsOverride", {
        width: WINDOW.width,
        height: WINDOW.height,
        deviceScaleFactor: 1,
        mobile: false,
        screenWidth: WINDOW.width,
        screenHeight: WINDOW.height,
      });
      const fixture = new URL(FIXTURE_PATH, url);
      fixture.searchParams.set("screen", SCREENS[index]);
      fixture.searchParams.set("mark", mark);
      fixture.searchParams.set("expiresAt", String(expiresAt));
      if (SCREENS[index] === DISABLE_REMOVAL_ON) fixture.searchParams.set("disableRemoval", "1");
      await page.navigate(fixture.toString(), { timeoutMs: 30_000 });
      await waitForReady(page);
    }

    const before = await Promise.all(pages.map((page, index) => capture(page, "before", SCREENS[index])));
    for (const tree of before) {
      assert.equal(tree.state, "visible", `${tree.screen} must show its message before one minute`);
      assert.equal(tree.markedMessageCount, 1, `${tree.screen} must have exactly one marked timed message before expiry`);
      assert.deepEqual(tree.markedMessageTexts, [mark], `${tree.screen} must read the exact random mark before expiry`);
      console.log(`TASK0556_BEFORE screen=${tree.screen} exact_text=${tree.markedMessageTexts[0]} count=${tree.markedMessageCount}`);
    }

    const waitMs = Math.max(0, expiresAt - Date.now()) + 750;
    console.log(`TASK0556_WAIT actual_milliseconds=${waitMs} requested_milliseconds=${ONE_MINUTE_MS}`);
    await new Promise((resolve) => setTimeout(resolve, waitMs));

    const after = await Promise.all(pages.map((page, index) => capture(page, "after", SCREENS[index])));
    for (const tree of after) {
      assert.equal(tree.state, "expired", `${tree.screen} still showing the marked message after one minute`);
      assert.equal(tree.markedMessageCount, 0, `${tree.screen} still showing the marked message after one minute`);
      assert.deepEqual(tree.markedMessageTexts, [], `${tree.screen} must not show the random mark after expiry`);
      assert.match(tree.emptyStateText, /disappeared/u, `${tree.screen} must show a nonblank expired conversation state`);
      console.log(`TASK0556_AFTER screen=${tree.screen} count=${tree.markedMessageCount} marked_absent=${!tree.visibleText.includes(mark)}`);
    }

    console.log(`TASK0556_MARKED_TEXT=${mark}`);
    console.log("TASK0556_DONE before_counts=1,1 after_counts=0,0 screenshots=4 screen_trees=4 nonblank=true");
  } finally {
    await Promise.all(pages.map((page) => page.close()));
    await chrome.close();
    await server.close();
  }
}, { timeout: 95_000 });
