import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const WINDOW = Object.freeze({ width: 1280, height: 800 });
const CREATED_LINK = "https://invite.osl.local/one-use/N2mI-dRDm7RQEQ4ywSzIZg.ujrDhhQJfGFdkiBLkHxv6tDuL4Pxp4nyj1z6WecEHX4";
const REQUEST_ID = "900000000000022802";

function pngDimensions(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function startVite() {
  const server = await createServer({ root: APP_ROOT, logLevel: "error", server: { host: "127.0.0.1", port: 0, strictPort: false } });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  return result.result.value;
}

function assertPng(png) {
  const dimensions = pngDimensions(png);
  assert.deepEqual(dimensions, WINDOW);
  assert.ok(png.length > 12_000, `PNG too small: ${png.length}`);
  assert.ok(new Set(png).size > 96, "PNG is nearly blank");
  return dimensions;
}

test("TASK 0222 captures fixed Linux create-link and paste-link result screens", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, screenWidth: WINDOW.width, screenHeight: WINDOW.height, deviceScaleFactor: 1, mobile: false });

    await page.navigate(`${url}screenshots/task-0222-create-link-fixture.html`, { timeoutMs: 30_000 });
    await evaluate(page, "document.querySelector('[data-invite-link-copy]').click()");
    const created = await evaluate(page, `(() => ({
      os: document.documentElement.dataset.fixtureOs,
      state: document.documentElement.getAttribute('data-task-0222'),
      link: document.querySelector('[data-invite-link-value]')?.textContent?.trim(),
      copyLabel: document.querySelector('[data-invite-link-copy]')?.getAttribute('aria-label'),
      copied: document.querySelector('[data-copy-status]')?.textContent?.trim(),
      linkFocusable: document.querySelector('[data-invite-link-value]')?.getAttribute('tabindex'),
      shareEnabled: !document.querySelector('[data-invite-link-share]')?.disabled,
    }))()`);
    assert.equal(created.os, "linux");
    assert.equal(created.state, "create-ready");
    assert.equal(created.link, CREATED_LINK);
    assert.equal(created.copyLabel, "Copy invite link");
    assert.equal(created.copied, "Copied invite link");
    assert.equal(created.linkFocusable, "0");
    assert.equal(created.shareEnabled, true);
    const createdPng = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    assertPng(createdPng);
    writeFileSync(path.join(ARTIFACT_DIR, "task-0222-create-link-linux.png"), createdPng);
    const createdAx = await page.send("Accessibility.getFullAXTree");
    writeFileSync(path.join(ARTIFACT_DIR, "task-0222-create-link-linux-screen-tree.json"), JSON.stringify({ fixture: "screenshots/task-0222-create-link-fixture.html", window: WINDOW, screen: created, axNodes: createdAx.nodes }, null, 2));

    await page.navigate(`${url}screenshots/task-0222-paste-link-fixture.html`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");
    await evaluate(page, "new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))");
    const pasted = await evaluate(page, `(() => ({
      os: document.documentElement.dataset.fixtureOs,
      state: document.documentElement.getAttribute('data-task-0222'),
      result: document.querySelector('[data-paste-result]')?.innerText.replace(/\\s+/gu, ' ').trim(),
      count: document.querySelector('[data-pending-count]')?.textContent?.trim(),
      requestIds: [...document.querySelectorAll('[data-request-tab-panel="pending"] [data-request-id]')].map((row) => row.dataset.requestId),
      pendingLabels: [...document.querySelectorAll('.status')].map((node) => node.textContent?.trim()),
    }))()`);
    assert.equal(pasted.os, "linux");
    assert.equal(pasted.state, "paste-ready");
    assert.match(pasted.result, /Pending request created/u);
    assert.match(pasted.result, new RegExp(REQUEST_ID, "u"));
    assert.equal(pasted.count, "2 pending");
    assert.deepEqual(pasted.requestIds, ["REQ-0228", REQUEST_ID]);
    assert.deepEqual(pasted.pendingLabels, ["Pending", "Pending"]);
    const pastedPng = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    assertPng(pastedPng);
    writeFileSync(path.join(ARTIFACT_DIR, "task-0222-paste-link-linux.png"), pastedPng);
    const pastedAx = await page.send("Accessibility.getFullAXTree");
    writeFileSync(path.join(ARTIFACT_DIR, "task-0222-paste-link-linux-screen-tree.json"), JSON.stringify({ fixture: "screenshots/task-0222-paste-link-fixture.html", window: WINDOW, screen: pasted, axNodes: pastedAx.nodes }, null, 2));

    console.log(`TASK0222_CREATE_PNG=${path.join(ARTIFACT_DIR, "task-0222-create-link-linux.png")}`);
    console.log(`TASK0222_PASTE_PNG=${path.join(ARTIFACT_DIR, "task-0222-paste-link-linux.png")}`);
    console.log(`TASK0222_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0222_COPYABLE_LINK=${created.link}`);
    console.log(`TASK0222_COPY_STATUS=${created.copied}`);
    console.log(`TASK0222_PENDING_RESULT=${pasted.result}`);
    console.log(`TASK0222_PENDING_COUNT=${pasted.count}`);
    console.log(`TASK0222_PENDING_IDS=${pasted.requestIds.join("|")}`);
    console.log(`TASK0222_CREATE_SHA256=${createHash("sha256").update(createdPng).digest("hex")}`);
    console.log(`TASK0222_PASTE_SHA256=${createHash("sha256").update(pastedPng).digest("hex")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 60_000 });
