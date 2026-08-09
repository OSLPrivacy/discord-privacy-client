/**
 * TASK 0627 - capture the fixed Linux attachment tray after adding a picture
 * and a document. The assertions read both the rendered screen tree and the
 * output pixels, so a blank, missing, or misplaced preview cannot pass merely
 * because a PNG was produced.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0627-attachment-tray-linux.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0627-attachment-tray-screen-tree.json");
const FIXTURE = "screenshots/task-0627-attachment-tray-fixture.html";

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

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) {
    throw new Error(evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text || "page evaluation threw");
  }
  return evaluated.result.value;
}

test("TASK 0627 Linux fixed screen shows two added attachment cards", async () => {
  assert.equal(process.platform, "linux", "this is a Linux capture task");
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const screenData = await server.ssrLoadModule("/src/attachment-tray-screen-data.ts");
  const window = screenData.ATTACHMENT_TRAY_SCREEN_WINDOW;
  const records = screenData.ATTACHMENT_TRAY_SCREEN_RECORDS;

  assert.equal(records.length, 2, "fixture must add exactly two files");
  const chrome = await launchChrome({
    args: [
      "--headless=new",
      "--remote-debugging-port=0",
      "--no-sandbox",
      "--disable-gpu",
      "--force-device-scale-factor=1",
      `--window-size=${window.width},${window.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: window.width,
      height: window.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: window.width,
      screenHeight: window.height,
    });
    await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });
    await evaluateValue(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task0627 === "ready") {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) return reject(new Error("attachment tray did not finish adding files"));
        setTimeout(tick, 25);
      };
      tick();
    })`);

    const screen = await evaluateValue(page, `(() => {
      const cards = [...document.querySelectorAll(".attachment-tray-card")].map((card) => {
        const image = card.querySelector(".attachment-tray-card-preview-image");
        const preview = card.querySelector(".attachment-tray-card-preview");
        const remove = card.querySelector(".attachment-tray-card-remove");
        const box = preview.getBoundingClientRect();
        return {
          kind: card.dataset.kind,
          name: card.querySelector(".attachment-tray-card-name").textContent.trim(),
          type: card.querySelector(".attachment-tray-card-type").textContent.trim(),
          size: card.querySelector(".attachment-tray-card-size").textContent.trim(),
          preview: preview.dataset.preview,
          previewDecoded: image ? image.complete && image.naturalWidth > 0 : false,
          removeLabel: remove.getAttribute("aria-label"),
          previewBox: { x: box.x, y: box.y, width: box.width, height: box.height },
        };
      });
      return {
        fixtureOs: document.documentElement.dataset.fixtureOs,
        addedCount: Number(document.documentElement.dataset.addedCount),
        cards,
        cardCount: cards.length,
        previewCount: document.querySelectorAll(".attachment-tray-card-preview-image").length,
        removeCount: document.querySelectorAll(".attachment-tray-card-remove").length,
        text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
      };
    })()`);

    assert.equal(screen.fixtureOs, "linux");
    assert.equal(screen.addedCount, 2);
    assert.equal(screen.cardCount, 2);
    assert.deepEqual(screen.cards.map((card) => card.kind), ["picture", "document"]);
    assert.deepEqual(screen.cards.map((card) => card.name), ["harbour sunrise.png", "Quarterly Report.pdf"]);
    assert.deepEqual(screen.cards.map((card) => card.size), ["418 KB", "4 KB"]);
    assert.equal(screen.previewCount, 1);
    assert.equal(screen.cards[0].preview, "picture");
    assert.equal(screen.cards[0].previewDecoded, true);
    assert.equal(screen.cards[1].preview, "none");
    assert.equal(screen.removeCount, 2);
    assert.deepEqual(screen.cards.map((card) => card.removeLabel), ["Remove harbour sunrise.png", "Remove Quarterly Report.pdf"]);
    for (const phrase of ["Attachments", "Picture · image/png", "Document · application/pdf", "418 KB", "4 KB", "Remove"]) {
      assert.ok(screen.text.includes(phrase), `missing visible text: ${phrase}`);
    }

    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);
    const facts = imageFacts(png, {
      picturePreview: screen.cards[0].previewBox,
      documentPreview: screen.cards[1].previewBox,
    });
    writeFileSync(TREE_PATH, JSON.stringify({ url: `${url}${FIXTURE}`, window, screen, facts }, null, 2));

    assert.equal(facts.width, window.width);
    assert.equal(facts.height, window.height);
    assert.ok(png.length > 5_000, `PNG too small: ${png.length}`);
    const picture = facts.crops.picturePreview;
    const document_ = facts.crops.documentPreview;
    const pictureShare = picture.saturatedPixels / picture.pixels;
    const documentShare = document_.saturatedPixels / document_.pixels;
    assert.ok(pictureShare > 0.5, `picture preview did not paint: ${picture.saturatedPixels}/${picture.pixels}`);
    assert.ok(documentShare < 0.05, `document card incorrectly painted a picture: ${document_.saturatedPixels}/${document_.pixels}`);

    console.log(`TASK0627_PNG=${PNG_PATH}`);
    console.log(`TASK0627_TREE=${TREE_PATH}`);
    console.log(`TASK0627_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0627_ADDED=${screen.addedCount}`);
    console.log(`TASK0627_CARD_COUNT=${screen.cardCount}`);
    console.log(`TASK0627_CARD_KINDS=${screen.cards.map((card) => card.kind).join(",")}`);
    console.log(`TASK0627_SIZES=${screen.cards.map((card) => card.size).join(",")}`);
    console.log(`TASK0627_PREVIEW_COUNT=${screen.previewCount}`);
    console.log(`TASK0627_REMOVE_COUNT=${screen.removeCount}`);
    console.log(`TASK0627_PICTURE_PIXELS=${picture.saturatedPixels}/${picture.pixels}`);
    console.log(`TASK0627_DOCUMENT_PIXELS=${document_.saturatedPixels}/${document_.pixels}`);
    console.log(`TASK0627_PNG_SHA256=${createHash("sha256").update(png).digest("hex")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 120_000 });
