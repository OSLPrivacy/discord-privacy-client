import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { readPng } from "./lib/png-pixels.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const FIXTURE_DIR = path.join(APP_ROOT, "screenshots", "fixtures");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0760-apps-and-sending-1280x800.png");
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0760-apps-and-sending-screen-tree.json");
const FIXTURE_PAGE = "screenshots/task-0760-apps-and-sending-fixture.html";
const WINDOW = Object.freeze({ width: 1280, height: 800 });

/**
 * A region that changes when the saved choice changes is a region that is
 * DRAWING that choice. Above this, the two states are plainly different paint;
 * below the second bound, an untouched region proves the difference is the
 * choice and not the page relaying out under the camera.
 */
const CHOICE_MIN_MEAN_ABS_DIFF = 4;
const UNTOUCHED_MAX_MEAN_ABS_DIFF = 0.5;

const REQUIRED_TEXT = Object.freeze([
  "Apps and sending",
  "Connected apps",
  "Discord",
  "work@example.test",
  "Send messages",
  "Clipboard",
  "Open Discord",
  "Set up",
  "Remove app",
  "Next-generation messages",
  "Save",
]);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
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
    .flatMap((node) => [node.role?.value, node.name?.value, node.value?.value, node.description?.value])
    .filter((value) => typeof value === "string" && value.trim())
    .join("\n");
}

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (evaluated.exceptionDetails) {
    throw new Error(
      evaluated.exceptionDetails.exception?.description
        || evaluated.exceptionDetails.text
        || "page evaluation threw",
    );
  }
  return evaluated.result.value;
}

function uniqueColoursInRegion(image, rect) {
  const seen = new Set();
  for (let y = Math.floor(rect.y); y < Math.ceil(rect.y + rect.height); y += 1) {
    for (let x = Math.floor(rect.x); x < Math.ceil(rect.x + rect.width); x += 1) {
      const offset = (y * image.width + x) * 4;
      seen.add(`${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`);
    }
  }
  return seen.size;
}

/** Mean absolute RGB difference between the same rectangle of two screenshots. */
function rectMeanAbsDiff(left, right, rect) {
  const x0 = Math.round(rect.x);
  const y0 = Math.round(rect.y);
  const width = Math.round(rect.width);
  const height = Math.round(rect.height);
  assert.ok(width > 0 && height > 0, "compared rectangle is empty");
  let total = 0;
  let samples = 0;
  for (let y = y0; y < y0 + height; y += 1) {
    for (let x = x0; x < x0 + width; x += 1) {
      if (x < 0 || y < 0 || x >= left.width || y >= left.height) {
        throw new Error(`compared rectangle falls outside the screenshot at ${x},${y}`);
      }
      const offset = (y * left.width + x) * 4;
      total += Math.abs(left.pixels[offset] - right.pixels[offset]);
      total += Math.abs(left.pixels[offset + 1] - right.pixels[offset + 1]);
      total += Math.abs(left.pixels[offset + 2] - right.pixels[offset + 2]);
      samples += 3;
    }
  }
  return total / samples;
}

const READY = (model) => `new Promise((resolve, reject) => {
  document.documentElement.dataset.task0760 = "rendering";
  const model = ${JSON.stringify(model)};
  const deadline = Date.now() + 15000;
  const tick = () => {
    if (typeof window.task0760Render === "function") {
      window.task0760Render(model);
      requestAnimationFrame(() => requestAnimationFrame(resolve));
      return;
    }
    if (Date.now() > deadline) {
      reject(new Error("Apps and sending screen did not render"));
      return;
    }
    setTimeout(tick, 25);
  };
  tick();
})`;

const READ_SCREEN = `(() => {
  const rect = (element) => {
    const box = element?.getBoundingClientRect();
    return box ? { x: box.x, y: box.y, width: box.width, height: box.height } : null;
  };
  const screen = document.querySelector("[data-apps-sending-screen]");
  const connectedCards = [...document.querySelectorAll('.apps-sending-app[data-app-state="connected"]')];
  const card = connectedCards[0];
  const activeStyles = [...document.querySelectorAll('[data-send-style-active="true"]')];
  const activeAccounts = [...document.querySelectorAll('[data-account-selected="true"]')];
  const switchRow = document.querySelector(".apps-sending-switch");
  const switchInput = document.querySelector(".apps-sending-switch-input");
  return {
    title: document.title,
    heading: document.querySelector("#apps-sending-title")?.textContent?.trim() || "",
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    connectedCount: Number(screen?.dataset.connectedCount ?? -1),
    connectedCards: connectedCards.length,
    connectedAppName: card?.querySelector(".apps-sending-app-name")?.textContent?.trim() || "",
    sendStyleFields: document.querySelectorAll('[data-field="send-style"]').length,
    activeSendStyles: activeStyles.map((node) => node.dataset.sendStyle),
    activeSendStyleLabel: activeStyles[0]?.querySelector(".apps-sending-choice-name")?.textContent?.trim() || "",
    activeAccounts: activeAccounts.map((node) => node.dataset.accountChoice),
    activeAccountLabel: activeAccounts[0]?.querySelector(".apps-sending-choice-name")?.textContent?.trim() || "",
    nextGeneration: switchRow?.dataset.nextGeneration || "",
    nextGenerationLabel: document.querySelector("[data-next-generation-state]")?.textContent?.trim() || "",
    nextGenerationChecked: Boolean(switchInput?.checked),
    actions: [...document.querySelectorAll(".apps-sending-app[data-app-state=\\"connected\\"] [data-app-action]")]
      .map((node) => node.dataset.appAction + ":" + node.textContent.trim() + ":" + (node.disabled ? "disabled" : "enabled")),
    footerButtons: [...document.querySelectorAll(".apps-sending-footer button")].map((node) => node.textContent.trim()),
    rects: {
      card: rect(card),
      heading: rect(document.querySelector("#apps-sending-title")),
      // Named by style, not by which one happens to be active, so the same
      // rectangle can be compared across two different saved choices.
      clipboardStyle: rect(document.querySelector('[data-send-style="clipboard"]')),
      manualStyle: rect(document.querySelector('[data-send-style="manual"]')),
      switchRow: rect(switchRow),
      footer: rect(document.querySelector(".apps-sending-footer")),
    },
  };
})()`;

test("TASK 0760 captures the Apps and sending screen with one connected app and its active sending choices", async () => {
  const saved = JSON.parse(readFileSync(path.join(FIXTURE_DIR, "task-0760-apps-and-sending.json"), "utf8"));

  // The same screen with the two saved sending choices changed. Nothing else
  // moves, so any pixel difference between the two captures is those choices.
  const changed = JSON.parse(JSON.stringify(saved));
  changed.apps[0].sendStyles = changed.apps[0].sendStyles.map((style) => ({
    ...style,
    active: style.id === "manual",
  }));
  changed.apps[0].nextGeneration.requested = false;

  mkdirSync(EVIDENCE_DIR, { recursive: true });
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
    await page.navigate(`${url}${FIXTURE_PAGE}`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");

    await evaluateValue(page, READY(saved));
    const screen = await evaluateValue(page, READ_SCREEN);
    const ax = await page.send("Accessibility.getFullAXTree");
    const pageText = [screen.text, accessibilityTreeText(ax.nodes ?? [])].join("\n").toLowerCase();

    for (const required of REQUIRED_TEXT) {
      assert.match(pageText, new RegExp(escapeRegExp(required.toLowerCase()), "u"), `missing screen text: ${required}`);
    }

    // One connected app -- not zero, not two.
    assert.equal(screen.connectedCount, 1);
    assert.equal(screen.connectedCards, 1);
    assert.equal(screen.connectedAppName, "Discord");
    // Sending choices belong to that one app and to no other card on screen.
    assert.equal(screen.sendStyleFields, 1);

    // Its active sending choices.
    assert.deepEqual(screen.activeSendStyles, ["clipboard"]);
    assert.equal(screen.activeSendStyleLabel, "Clipboard");
    assert.deepEqual(screen.activeAccounts, ["discord-work"]);
    assert.equal(screen.activeAccountLabel, "work@example.test");
    assert.equal(screen.nextGeneration, "on");
    assert.equal(screen.nextGenerationLabel, "On");
    assert.equal(screen.nextGenerationChecked, true);

    // Open, set up and remove are all reachable on the connected card.
    assert.deepEqual(screen.actions, [
      "open:Open Discord:enabled",
      "set-up:Set up:enabled",
      "remove:Remove app:enabled",
    ]);
    assert.deepEqual(screen.footerButtons, ["Reset", "Save"]);
    assert.equal(screen.title, "Apps and sending");
    assert.equal(screen.heading, "Apps and sending");

    for (const [name, rect] of Object.entries(screen.rects)) {
      assert.ok(rect, `${name} was not laid out`);
      assert.ok(rect.x >= 0 && rect.y >= 0, `${name} starts outside the window`);
      assert.ok(rect.x + rect.width <= WINDOW.width, `${name} runs past the window width`);
      assert.ok(rect.y + rect.height <= WINDOW.height, `${name} runs past the window height`);
    }

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const shot = readPng(png);
    assert.equal(shot.width, WINDOW.width);
    assert.equal(shot.height, WINDOW.height);

    // Second capture: same screen, different saved sending choices.
    await evaluateValue(page, READY(changed));
    const changedScreen = await evaluateValue(page, READ_SCREEN);
    const changedShot = readPng(await page.screenshot({ fromSurface: true }));
    assert.deepEqual(changedScreen.activeSendStyles, ["manual"]);
    assert.equal(changedScreen.nextGeneration, "off");
    assert.equal(changedScreen.nextGenerationChecked, false);
    for (const key of ["card", "clipboardStyle", "manualStyle", "switchRow", "heading"]) {
      assert.deepEqual(
        screen.rects[key],
        changedScreen.rects[key],
        `${key} moved between the two states; the pixel comparison would be meaningless`,
      );
    }

    const diffs = {
      clipboardChip: rectMeanAbsDiff(shot, changedShot, screen.rects.clipboardStyle),
      manualChip: rectMeanAbsDiff(shot, changedShot, screen.rects.manualStyle),
      nextGenerationRow: rectMeanAbsDiff(shot, changedShot, screen.rects.switchRow),
      heading: rectMeanAbsDiff(shot, changedShot, screen.rects.heading),
    };
    const colours = {
      card: uniqueColoursInRegion(shot, screen.rects.card),
      activeStyleChip: uniqueColoursInRegion(shot, screen.rects.clipboardStyle),
      nextGenerationRow: uniqueColoursInRegion(shot, screen.rects.switchRow),
    };
    const sha256 = createHash("sha256").update(png).digest("hex");

    writeFileSync(TREE_PATH, JSON.stringify({
      url: `${url}${FIXTURE_PAGE}`,
      window: WINDOW,
      required: REQUIRED_TEXT,
      png: { path: PNG_PATH, bytes: png.length, sha256 },
      saved: screen,
      changed: changedScreen,
      diffs,
      colours,
      axNodes: ax.nodes,
    }, null, 2));

    assert.ok(
      diffs.clipboardChip >= CHOICE_MIN_MEAN_ABS_DIFF,
      `Clipboard being the active style is not drawn: its chip is unchanged when it stops being active (${diffs.clipboardChip})`,
    );
    assert.ok(
      diffs.manualChip >= CHOICE_MIN_MEAN_ABS_DIFF,
      `Manual not being the active style is not drawn: its chip is unchanged when it becomes active (${diffs.manualChip})`,
    );
    assert.ok(
      diffs.nextGenerationRow >= CHOICE_MIN_MEAN_ABS_DIFF,
      `the next-generation switch looks the same on and off (${diffs.nextGenerationRow})`,
    );
    assert.ok(
      diffs.heading <= UNTOUCHED_MAX_MEAN_ABS_DIFF,
      `the whole screen repainted, so the choice regions prove nothing (${diffs.heading})`,
    );
    assert.ok(colours.card > 8, `the connected app card looks blank: ${colours.card} colours`);
    assert.ok(colours.activeStyleChip > 8, `the active sending style looks blank: ${colours.activeStyleChip} colours`);
    assert.ok(colours.nextGenerationRow > 8, `the next-generation row looks blank: ${colours.nextGenerationRow} colours`);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);

    console.log(`TASK0760_PNG=${PNG_PATH}`);
    console.log(`TASK0760_TREE=${TREE_PATH}`);
    console.log(`TASK0760_WINDOW=${shot.width}x${shot.height}`);
    console.log(`TASK0760_PNG_BYTES=${png.length}`);
    console.log(`TASK0760_PNG_SHA256=${sha256}`);
    console.log(`TASK0760_PAGE_TITLE=${screen.title}`);
    console.log(`TASK0760_CONNECTED_COUNT=${screen.connectedCount}`);
    console.log(`TASK0760_CONNECTED_APP=${screen.connectedAppName}`);
    console.log(`TASK0760_SEND_STYLE_FIELDS=${screen.sendStyleFields}`);
    console.log(`TASK0760_ACTIVE_ACCOUNT=${screen.activeAccountLabel}`);
    console.log(`TASK0760_ACTIVE_SEND_STYLE=${screen.activeSendStyles.join(",")}|${screen.activeSendStyleLabel}`);
    console.log(`TASK0760_NEXT_GENERATION=${screen.nextGeneration}|${screen.nextGenerationLabel}|checked=${screen.nextGenerationChecked}`);
    console.log(`TASK0760_ACTIONS=${screen.actions.join("|")}`);
    console.log(`TASK0760_FOOTER_BUTTONS=${screen.footerButtons.join("|")}`);
    console.log(`TASK0760_CARD_RECT=${Math.round(screen.rects.card.x)},${Math.round(screen.rects.card.y)},${Math.round(screen.rects.card.width)}x${Math.round(screen.rects.card.height)}`);
    console.log(`TASK0760_CLIPBOARD_CHIP_DIFF=${diffs.clipboardChip.toFixed(4)}`);
    console.log(`TASK0760_MANUAL_CHIP_DIFF=${diffs.manualChip.toFixed(4)}`);
    console.log(`TASK0760_NEXT_GENERATION_DIFF=${diffs.nextGenerationRow.toFixed(4)}`);
    console.log(`TASK0760_HEADING_DIFF=${diffs.heading.toFixed(4)}`);
    console.log(`TASK0760_CARD_COLOURS=${colours.card}`);
    console.log(`TASK0760_ACTIVE_STYLE_COLOURS=${colours.activeStyleChip}`);
    console.log(`TASK0760_NEXT_GENERATION_COLOURS=${colours.nextGenerationRow}`);
    console.log(`TASK0760_REQUIRED_TEXT=${REQUIRED_TEXT.join("|")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 120_000 });
