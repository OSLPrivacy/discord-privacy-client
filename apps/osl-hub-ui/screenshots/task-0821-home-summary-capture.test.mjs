import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { readPng } from "./lib/png-pixels.mjs";

/**
 * TASK 0821 - the Home protection summary, photographed with its main action
 * pointed at the next safe step.
 *
 * Both models rendered here were written by `cmd_osl_home_protection_summary`
 * (crates/ipc/tests/task_0821_home_summary_action_route.rs keeps them honest),
 * so "the action route matches direct data exactly" is checked against the
 * backend's own answer rather than against hand-typed JSON.
 */

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const FIXTURE_DIR = path.join(APP_ROOT, "screenshots", "fixtures");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0821-home-summary-1280x800.png");
const EMPTY_PNG_PATH = path.join(EVIDENCE_DIR, "task-0821-home-summary-empty-1280x800.png");
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0821-home-summary-screen-tree.json");
const FIXTURE_PAGE = "screenshots/task-0821-home-summary-fixture.html";
const WINDOW = Object.freeze({ width: 1280, height: 800 });

/**
 * The test's own copy of where each step has to land. Kept as literals here on
 * purpose: reusing the screen's table would only prove the table equals itself.
 */
const ROUTE_FOR_STEP = Object.freeze({
  "Finish account protection": "settings",
  "Connect an app": "connections",
  "Add a trusted person": "people",
  "Open a protected conversation": "osl-chat",
});

/**
 * A region that changes when the saved protection state changes is a region
 * that is DRAWING it. The second bound is for a wide row that is mostly padding
 * around one line of text: the same change averages down over the empty space,
 * so it gets its own, lower, floor rather than a fudged shared one. The third
 * bound is the untouched region that proves the difference is the data and not
 * the page relaying out under the camera.
 */
const STATE_MIN_MEAN_ABS_DIFF = 4;
const WIDE_ROW_MIN_MEAN_ABS_DIFF = 1.5;
const UNTOUCHED_MAX_MEAN_ABS_DIFF = 0.5;

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

function imageMeanAbsDiff(left, right) {
  return rectMeanAbsDiff(left, right, { x: 0, y: 0, width: left.width, height: left.height });
}

function changedPixelShare(left, right) {
  let changed = 0;
  for (let index = 0; index < left.width * left.height; index += 1) {
    const offset = index * 4;
    if (left.pixels[offset] !== right.pixels[offset]
      || left.pixels[offset + 1] !== right.pixels[offset + 1]
      || left.pixels[offset + 2] !== right.pixels[offset + 2]) {
      changed += 1;
    }
  }
  return changed / (left.width * left.height);
}

const READY = (summary) => `new Promise((resolve, reject) => {
  document.documentElement.dataset.task0821 = "rendering";
  const summary = ${JSON.stringify(summary)};
  const deadline = Date.now() + 15000;
  const tick = () => {
    if (typeof window.task0821Render === "function") {
      window.task0821Render(summary);
      requestAnimationFrame(() => requestAnimationFrame(resolve));
      return;
    }
    if (Date.now() > deadline) {
      reject(new Error("Home protection summary did not render"));
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
  const screen = document.querySelector("[data-home-protection-summary]");
  const sentence = document.querySelector("[data-home-summary-sentence]");
  const action = document.querySelector("[data-home-summary-action]");
  const fact = (id) => document.querySelector('[data-summary-fact="' + id + '"] .home-summary-fact-value')?.textContent?.trim() || "";
  return {
    title: document.title,
    heading: document.querySelector("#home-summary-title")?.textContent?.trim() || "",
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    summaryElements: document.querySelectorAll("[data-home-summary-sentence]").length,
    actionElements: document.querySelectorAll("[data-home-summary-action]").length,
    summarySentence: sentence?.textContent?.trim() || "",
    facts: {
      protectionState: fact("protection-state"),
      trustedPeople: fact("trusted-people"),
      connectedApps: fact("connected-apps"),
      nextSafeStep: fact("next-safe-step"),
    },
    protectionState: screen?.dataset.protectionState || "",
    nextSafeStep: screen?.dataset.nextSafeStep || "",
    actionLabel: action?.textContent?.trim() || "",
    actionStep: action?.dataset.safeStep || "",
    actionRoute: action?.dataset.route || "",
    actionSettings: action?.dataset.settings || "",
    actionHomeModule: action?.dataset.homeModule || "",
    rects: {
      heading: rect(document.querySelector("#home-summary-title")),
      sentence: rect(sentence),
      facts: rect(document.querySelector("[data-home-summary-facts]")),
      nextStepFact: rect(document.querySelector('[data-summary-fact="next-safe-step"]')),
      actionRow: rect(document.querySelector(".home-summary-action-row")),
      actionButton: rect(action),
    },
  };
})()`;

function assertInsideWindow(screen, label) {
  for (const [name, rect] of Object.entries(screen.rects)) {
    assert.ok(rect, `${label}: ${name} was not laid out`);
    assert.ok(rect.x >= 0 && rect.y >= 0, `${label}: ${name} starts outside the window`);
    assert.ok(rect.x + rect.width <= WINDOW.width, `${label}: ${name} runs past the window width`);
    assert.ok(rect.y + rect.height <= WINDOW.height, `${label}: ${name} runs past the window height`);
  }
}

test("TASK 0821 captures the Home protection summary with its action routed to the next safe step", async () => {
  const direct = JSON.parse(readFileSync(path.join(FIXTURE_DIR, "task-0821-home-summary-direct.json"), "utf8"));
  const empty = JSON.parse(readFileSync(path.join(FIXTURE_DIR, "task-0821-home-summary-empty.json"), "utf8"));

  // The two named elements, worded from the direct data rather than repeated
  // here as prose the screen might not be showing.
  const REQUIRED_TEXT = Object.freeze([
    "Home",
    "Protection",
    "Trusted people",
    "Connected apps",
    "Next safe step",
    direct.trusted_people,
    direct.apps,
    direct.next_safe_step,
  ]);

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

    await evaluateValue(page, READY(direct));
    const screen = await evaluateValue(page, READ_SCREEN);
    const ax = await page.send("Accessibility.getFullAXTree");
    const pageText = [screen.text, accessibilityTreeText(ax.nodes ?? [])].join("\n").toLowerCase();

    for (const required of REQUIRED_TEXT) {
      assert.match(pageText, new RegExp(escapeRegExp(required.toLowerCase()), "u"), `missing screen text: ${required}`);
    }

    // Named element 1: the summary.
    assert.equal(screen.summaryElements, 1);
    assert.equal(
      screen.summarySentence,
      `Your account is protected · ${direct.trusted_people} · ${direct.apps}`,
    );
    assert.equal(screen.facts.protectionState, "Protected");
    assert.equal(screen.facts.trustedPeople, direct.trusted_people);
    assert.equal(screen.facts.connectedApps, direct.apps);
    assert.equal(screen.facts.nextSafeStep, direct.next_safe_step);
    assert.equal(screen.protectionState, direct.protection_state);

    // Named element 2: the action route.
    assert.equal(screen.actionElements, 1);
    assert.equal(screen.actionStep, direct.next_safe_step, "the action is pointed at some other step");
    assert.equal(screen.actionRoute, ROUTE_FOR_STEP[direct.next_safe_step]);
    assert.equal(screen.actionRoute, "osl-chat");
    assert.equal(screen.actionHomeModule, "osl-chats");
    assert.equal(screen.actionSettings, "");
    assert.equal(screen.actionLabel, "Open OSL Chat");
    assert.equal(screen.nextSafeStep, direct.next_safe_step);
    assert.equal(screen.title, "Home");
    assert.equal(screen.heading, "Home");
    assertInsideWindow(screen, "protected");

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const shot = readPng(png);
    assert.equal(shot.width, WINDOW.width);
    assert.equal(shot.height, WINDOW.height);

    // The empty-state capture: the same screen for an account with nothing
    // saved, straight from the same direct command.
    await evaluateValue(page, READY(empty));
    const emptyScreen = await evaluateValue(page, READ_SCREEN);
    const emptyPng = await page.screenshot({ fromSurface: true });
    writeFileSync(EMPTY_PNG_PATH, emptyPng);
    const emptyShot = readPng(emptyPng);

    assert.equal(emptyScreen.facts.protectionState, "Needs attention");
    assert.equal(emptyScreen.facts.trustedPeople, empty.trusted_people);
    assert.equal(emptyScreen.facts.connectedApps, empty.apps);
    assert.equal(emptyScreen.actionStep, empty.next_safe_step);
    assert.equal(emptyScreen.actionRoute, ROUTE_FOR_STEP[empty.next_safe_step]);
    assert.equal(emptyScreen.actionRoute, "settings");
    assert.equal(emptyScreen.actionSettings, "account");
    assertInsideWindow(emptyScreen, "empty");

    for (const key of ["heading", "sentence", "facts", "nextStepFact", "actionRow", "actionButton"]) {
      assert.deepEqual(
        screen.rects[key],
        emptyScreen.rects[key],
        `${key} moved between the two states; the pixel comparison would be meaningless`,
      );
    }

    const diffs = {
      whole: imageMeanAbsDiff(shot, emptyShot),
      sentence: rectMeanAbsDiff(shot, emptyShot, screen.rects.sentence),
      nextStepFact: rectMeanAbsDiff(shot, emptyShot, screen.rects.nextStepFact),
      actionRow: rectMeanAbsDiff(shot, emptyShot, screen.rects.actionRow),
      actionButton: rectMeanAbsDiff(shot, emptyShot, screen.rects.actionButton),
      heading: rectMeanAbsDiff(shot, emptyShot, screen.rects.heading),
    };
    const colours = {
      sentence: uniqueColoursInRegion(shot, screen.rects.sentence),
      facts: uniqueColoursInRegion(shot, screen.rects.facts),
      actionRow: uniqueColoursInRegion(shot, screen.rects.actionRow),
    };
    const changedShare = changedPixelShare(shot, emptyShot);
    const sha256 = createHash("sha256").update(png).digest("hex");
    const emptySha256 = createHash("sha256").update(emptyPng).digest("hex");

    writeFileSync(TREE_PATH, JSON.stringify({
      url: `${url}${FIXTURE_PAGE}`,
      window: WINDOW,
      required: REQUIRED_TEXT,
      routeForStep: ROUTE_FOR_STEP,
      direct,
      empty,
      png: { path: PNG_PATH, bytes: png.length, sha256 },
      emptyPng: { path: EMPTY_PNG_PATH, bytes: emptyPng.length, sha256: emptySha256 },
      protected: screen,
      emptyState: emptyScreen,
      diffs,
      colours,
      changedShare,
      axNodes: ax.nodes,
    }, null, 2));

    // The screenshot differs from the empty-state capture.
    assert.notEqual(sha256, emptySha256, "the two captures are the same image");
    assert.ok(diffs.whole > 0, `the two captures are pixel-identical (${diffs.whole})`);
    assert.ok(
      diffs.sentence >= STATE_MIN_MEAN_ABS_DIFF,
      `the summary sentence looks the same protected and empty (${diffs.sentence})`,
    );
    assert.ok(
      diffs.nextStepFact >= STATE_MIN_MEAN_ABS_DIFF,
      `the next safe step is not drawn: it looks the same in both states (${diffs.nextStepFact})`,
    );
    assert.ok(
      diffs.actionButton >= STATE_MIN_MEAN_ABS_DIFF,
      `the main action button looks the same whichever step it routes to (${diffs.actionButton})`,
    );
    assert.ok(
      diffs.actionRow >= WIDE_ROW_MIN_MEAN_ABS_DIFF,
      `the action row is unchanged between the two states (${diffs.actionRow})`,
    );
    assert.ok(
      diffs.heading <= UNTOUCHED_MAX_MEAN_ABS_DIFF,
      `the whole screen repainted, so the compared regions prove nothing (${diffs.heading})`,
    );
    assert.ok(colours.sentence > 8, `the summary sentence looks blank: ${colours.sentence} colours`);
    assert.ok(colours.facts > 8, `the summary facts look blank: ${colours.facts} colours`);
    assert.ok(colours.actionRow > 8, `the action row looks blank: ${colours.actionRow} colours`);
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);

    console.log(`TASK0821_PNG=${PNG_PATH}`);
    console.log(`TASK0821_EMPTY_PNG=${EMPTY_PNG_PATH}`);
    console.log(`TASK0821_TREE=${TREE_PATH}`);
    console.log(`TASK0821_WINDOW=${shot.width}x${shot.height}`);
    console.log(`TASK0821_PNG_BYTES=${png.length}`);
    console.log(`TASK0821_PNG_SHA256=${sha256}`);
    console.log(`TASK0821_EMPTY_PNG_BYTES=${emptyPng.length}`);
    console.log(`TASK0821_EMPTY_PNG_SHA256=${emptySha256}`);
    console.log(`TASK0821_PAGE_TITLE=${screen.title}`);
    console.log(`TASK0821_NAMED_ELEMENTS=summary:${screen.summaryElements}|action-route:${screen.actionElements}`);
    console.log(`TASK0821_SUMMARY=${screen.summarySentence}`);
    console.log(`TASK0821_SUMMARY_FACTS=${Object.values(screen.facts).join("|")}`);
    console.log(`TASK0821_DIRECT_NEXT_SAFE_STEP=${direct.next_safe_step}`);
    console.log(`TASK0821_ACTION_STEP=${screen.actionStep}`);
    console.log(`TASK0821_ACTION_ROUTE=${screen.actionRoute}|module=${screen.actionHomeModule}|label=${screen.actionLabel}`);
    console.log(`TASK0821_EMPTY_DIRECT_NEXT_SAFE_STEP=${empty.next_safe_step}`);
    console.log(`TASK0821_EMPTY_ACTION_ROUTE=${emptyScreen.actionRoute}|settings=${emptyScreen.actionSettings}|label=${emptyScreen.actionLabel}`);
    console.log(`TASK0821_WHOLE_IMAGE_DIFF=${diffs.whole.toFixed(4)}`);
    console.log(`TASK0821_CHANGED_PIXEL_SHARE=${(changedShare * 100).toFixed(2)}%`);
    console.log(`TASK0821_SENTENCE_DIFF=${diffs.sentence.toFixed(4)}`);
    console.log(`TASK0821_NEXT_STEP_FACT_DIFF=${diffs.nextStepFact.toFixed(4)}`);
    console.log(`TASK0821_ACTION_BUTTON_DIFF=${diffs.actionButton.toFixed(4)}`);
    console.log(`TASK0821_ACTION_ROW_DIFF=${diffs.actionRow.toFixed(4)}`);
    console.log(`TASK0821_HEADING_DIFF=${diffs.heading.toFixed(4)}`);
    console.log(`TASK0821_SENTENCE_COLOURS=${colours.sentence}`);
    console.log(`TASK0821_FACTS_COLOURS=${colours.facts}`);
    console.log(`TASK0821_ACTION_ROW_COLOURS=${colours.actionRow}`);
    console.log(`TASK0821_REQUIRED_TEXT=${REQUIRED_TEXT.join("|")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 120_000 });
