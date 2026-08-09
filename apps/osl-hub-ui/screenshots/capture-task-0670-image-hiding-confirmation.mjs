#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

export const TASK0670_FIXED_WINDOW = Object.freeze({ width: 1280, height: 800 });
export const TASK0670_COPY_ID = "image-copy-0670-prepared";
export const TASK0670_REQUIRED_VISIBLE_TEXT = Object.freeze([
  "Private original",
  "Prepared post copy",
  "Quality check passed",
  "Post confirmation",
]);

const HERE = path.dirname(fileURLToPath(import.meta.url));
const UI_ROOT = path.dirname(HERE);
const OUTPUT = path.join(HERE, "evidence", "task-0670-linux-image-hidden-post-confirmation.png");

function imageDataUri({ sky, ridge, foreground, sun }) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 720 480"><defs><linearGradient id="sky" x1="0" y1="0" x2="0" y2="1"><stop stop-color="${sky}"/><stop offset="1" stop-color="#e8edf0"/></linearGradient><linearGradient id="ground" x1="0" y1="0" x2="0" y2="1"><stop stop-color="${foreground}"/><stop offset="1" stop-color="#0e2828"/></linearGradient></defs><rect width="720" height="480" fill="url(#sky)"/><circle cx="532" cy="104" r="43" fill="${sun}" opacity=".94"/><path d="M0 318 132 178l91 112 111-151 168 181 79-100 139 98v162H0z" fill="${ridge}"/><path d="M0 344c112-51 184 2 287-22 105-25 170-74 433-11v169H0z" fill="url(#ground)"/><path d="M0 386c95-41 212-21 333-6 134 17 216-21 387-41v141H0z" fill="#0a1c20" opacity=".72"/></svg>`;
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
}

function pageCss() {
  return `
    :root { color-scheme: dark; font-family: Inter, ui-sans-serif, system-ui, sans-serif; background: #071215; color: #e8f4f2; }
    * { box-sizing: border-box; }
    body { margin: 0; min-height: 100vh; overflow: hidden; background: radial-gradient(circle at 12% 0%, #143940 0, #071215 39rem); }
    .task0670-shell { width: min(1120px, calc(100vw - 64px)); margin: 20px auto; }
    .task0670-kicker { margin: 0 0 8px; color: #92d4cb; font-size: 13px; font-weight: 700; letter-spacing: .12em; text-transform: uppercase; }
    .image-comparison-screen { padding: 26px 30px 22px; border: 1px solid #2d5960; border-radius: 18px; background: rgba(11, 30, 34, .93); box-shadow: 0 24px 70px rgba(0, 0, 0, .34); }
    h1 { margin: 0 0 22px; font-size: 30px; letter-spacing: -.03em; }
    .image-comparison-pair { display: grid; grid-template-columns: 1fr 1fr; gap: 22px; }
    .image-comparison-figure { margin: 0; overflow: hidden; border: 1px solid #37656a; border-radius: 12px; background: #0a1f23; }
    .image-comparison-figure img { display: block; width: 100%; height: 235px; object-fit: cover; background: #17363a; }
    .image-comparison-figure figcaption { padding: 12px 14px; font-size: 15px; font-weight: 700; }
    .image-comparison-figure[data-image-comparison-side="prepared"] { border-color: #49b7a9; }
    .image-comparison-quality { display: flex; gap: 11px; align-items: center; margin: 20px 0 0; padding: 13px 15px; border-left: 4px solid #58d8a6; border-radius: 7px; background: #123b36; color: #dcfff1; font-weight: 750; }
    .image-comparison-pointer { color: #a9d8ca; font-family: ui-monospace, SFMono-Regular, monospace; font-size: 12px; font-weight: 500; }
    .task0670-confirmation { display: grid; grid-template-columns: auto 1fr auto; gap: 14px; align-items: center; margin: 20px 0 0; padding: 17px 20px; border: 1px solid #46b895; border-radius: 13px; background: #0d342d; }
    .task0670-confirmation-mark { display: grid; place-items: center; width: 34px; height: 34px; border-radius: 50%; background: #52d5a1; color: #062219; font-weight: 900; }
    .task0670-confirmation h2 { margin: 0 0 3px; font-size: 17px; }
    .task0670-confirmation p { margin: 0; color: #b8ddd0; font-size: 14px; }
    .task0670-confirmation code { padding: 7px 10px; border-radius: 6px; background: #061f1b; color: #bff8dc; font-size: 12px; }
  `;
}

function startServer(markup) {
  const server = createServer((request, response) => {
    if ((request.url || "/") !== "/") {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("not found");
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
    response.end(`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>OSL Privacy — Image post confirmation</title><style>${pageCss()}</style></head><body>${markup}</body></html>`);
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

function pngSize(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "capture is a PNG");
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

async function sourceMarkup() {
  const requireFromUi = createRequire(path.join(UI_ROOT, "package.json"));
  const { createServer: createViteServer } = requireFromUi("vite");
  const vite = await createViteServer({ root: UI_ROOT, configFile: false, appType: "custom", logLevel: "error", optimizeDeps: { noDiscovery: true }, server: { middlewareMode: true, watch: { ignored: ["**/*"] } } });
  try {
    const { imageComparisonScreenMarkup } = await vite.ssrLoadModule("/src/image-comparison-screen.ts");
    const { confirmAndSendPreparedImageCopy, imagePostConfirmationState } = await vite.ssrLoadModule("/src/image-post-confirmation.ts");
    const originalSrc = imageDataUri({ sky: "#79c9df", ridge: "#395e71", foreground: "#2a7668", sun: "#ffe3a3" });
    const preparedSrc = imageDataUri({ sky: "#68b9cf", ridge: "#31576c", foreground: "#247161", sun: "#ffe1a0" });
    const fixture = { preparedCopy: { imageCopyId: TASK0670_COPY_ID }, quality: { passed: true }, confirmed: true };
    const state = imagePostConfirmationState(fixture);
    assert.equal(state.disabled, false, "passing fixture enables the post");
    const sent = [];
    const receipt = confirmAndSendPreparedImageCopy(fixture, (copyId) => sent.push(copyId));
    assert.deepEqual(receipt, { sent: true, imageCopyId: TASK0670_COPY_ID });
    assert.deepEqual(sent, [TASK0670_COPY_ID]);
    const comparison = imageComparisonScreenMarkup({ originalSrc, preparedSrc, qualityPassed: true, pointerHex: "0670cafe0670cafe0670cafe0670cafe0670cafe" });
    return { markup: `<main class="task0670-shell"><p class="task0670-kicker">One image-hidden photo post</p>${comparison}<section class="task0670-confirmation" aria-labelledby="task0670-confirmation-title" data-post-confirmation="confirmed"><span class="task0670-confirmation-mark" aria-hidden="true">✓</span><div><h2 id="task0670-confirmation-title">Post confirmation</h2><p>Confirmed — the prepared post copy is ready to send.</p></div><code>${TASK0670_COPY_ID}</code></section></main>`, state, receipt, sent };
  } finally {
    await vite.close();
  }
}

async function main() {
  if (process.platform !== "linux") throw new Error(`TASK0670 requires Linux, received ${process.platform}`);
  const prepared = await sourceMarkup();
  const server = await startServer(prepared.markup);
  const address = server.address();
  const url = `http://127.0.0.1:${address.port}/`;
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${TASK0670_FIXED_WINDOW.width},${TASK0670_FIXED_WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { width: TASK0670_FIXED_WINDOW.width, height: TASK0670_FIXED_WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    const visible = await page.evaluate(`(() => {
      const required = ${JSON.stringify(TASK0670_REQUIRED_VISIBLE_TEXT)};
      const rect = (element) => { const box = element.getBoundingClientRect(); return { x: box.x, y: box.y, width: box.width, height: box.height }; };
      const matches = Object.fromEntries(required.map((text) => {
        const element = [...document.querySelectorAll("body *")].find((node) => node.children.length === 0 && node.textContent.trim() === text)
          || [...document.querySelectorAll("body *")].find((node) => node.textContent.includes(text));
        return [text, element ? rect(element) : null];
      }));
      return { text: document.body.innerText, matches, confirmation: document.querySelector("[data-post-confirmation]")?.getAttribute("data-post-confirmation"), quality: document.querySelector("[data-quality-result]")?.getAttribute("data-quality-result") };
    })()`);
    const missing = TASK0670_REQUIRED_VISIBLE_TEXT.filter((text) => !visible.text.includes(text) || !visible.matches[text]);
    if (missing.length) throw new Error(`missing visible screenshot content: ${missing.join(" | ")}`);
    if (visible.confirmation !== "confirmed") throw new Error(`post confirmation is ${visible.confirmation}`);
    if (visible.quality !== "passed") throw new Error(`quality result is ${visible.quality}`);
    const outOfBounds = Object.entries(visible.matches).filter(([, box]) => box.x < 0 || box.y < 0 || box.x + box.width > TASK0670_FIXED_WINDOW.width || box.y + box.height > TASK0670_FIXED_WINDOW.height);
    if (outOfBounds.length) throw new Error(`required content outside fixed window: ${outOfBounds.map(([text]) => text).join(", ")}`);
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes.map((node) => typeof node.name?.value === "string" ? node.name.value : "");
    const missingAx = TASK0670_REQUIRED_VISIBLE_TEXT.filter((text) => !axNames.some((name) => name.includes(text)));
    if (missingAx.length) throw new Error(`missing accessibility content: ${missingAx.join(" | ")}`);
    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    mkdirSync(path.dirname(OUTPUT), { recursive: true });
    writeFileSync(OUTPUT, screenshot);
    const size = pngSize(readFileSync(OUTPUT));
    assert.deepEqual(size, TASK0670_FIXED_WINDOW, "PNG matches the fixed window");
    assert.ok(screenshot.length > 20_000, `screenshot unexpectedly small: ${screenshot.length}`);
    console.log("TASK0670_CHECK=passed");
    console.log(`TASK0670_PLATFORM=${process.platform}`);
    console.log(`TASK0670_FIXED_WINDOW=${TASK0670_FIXED_WINDOW.width}x${TASK0670_FIXED_WINDOW.height}`);
    console.log(`TASK0670_PNG_SIZE=${size.width}x${size.height}`);
    console.log(`TASK0670_PNG=${OUTPUT}`);
    console.log(`TASK0670_PNG_SHA256=${createHash("sha256").update(screenshot).digest("hex")}`);
    console.log(`TASK0670_PNG_BYTES=${screenshot.length}`);
    console.log(`TASK0670_ORIGINAL=Private original`);
    console.log(`TASK0670_POST_COPY=Prepared post copy`);
    console.log(`TASK0670_QUALITY=Quality check passed`);
    console.log(`TASK0670_CONFIRMATION=Post confirmation`);
    console.log(`TASK0670_POST_STATE=${JSON.stringify(prepared.state)}`);
    console.log(`TASK0670_SEND_RECEIPT=${JSON.stringify(prepared.receipt)}`);
    console.log(`TASK0670_SENT_COPY_IDS=${prepared.sent.join("|")}`);
    console.log(`TASK0670_VISIBLE_CONTENT=${TASK0670_REQUIRED_VISIBLE_TEXT.join("|")}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await new Promise((resolve) => server.close(resolve));
  }
}

main().catch((error) => {
  console.error(`capture-task-0670-image-hiding-confirmation: ${error.stack || error.message}`);
  process.exitCode = 1;
});
