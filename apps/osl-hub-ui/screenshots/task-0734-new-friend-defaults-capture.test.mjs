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
const FIXTURE_PAGE = "screenshots/task-0734-new-friend-defaults-fixture.html";

/**
 * Fixed size, stated here and asserted against the PNG header afterwards. Tall
 * enough that the action row -- Save default and Reset -- is inside the
 * picture: a control named in the document but below the fold is not a control
 * the screenshot shows.
 */
const WINDOW = Object.freeze({ width: 1280, height: 900 });

const PNG_PATH = path.join(EVIDENCE_DIR, `task-0734-new-friend-defaults-${WINDOW.width}x${WINDOW.height}.png`);
const CONTRAST_PNG_PATH = path.join(EVIDENCE_DIR, `task-0734-new-friend-defaults-contrast-${WINDOW.width}x${WINDOW.height}.png`);
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0734-new-friend-defaults-screen-tree.json");

/** The screen's own title, exported by the module TASK 0732 built. */
const SCREEN_TITLE = "New friend defaults";

/**
 * TASK 0734 calls the screen "New-friend default". TASK 0735 fixes the page
 * title as "New friend defaults". Both are the same words: this is the plan's
 * hyphenated name for the screen against the screen's own heading. The check
 * therefore asserts the heading is exactly what the module exports AND that the
 * task's phrase survives normalisation (hyphens to spaces, case folded) as a
 * prefix of it. Neither half alone would be honest.
 */
const TASK_PHRASE = "New-friend default";

const normalise = (value) => value.toLowerCase().replace(/-/gu, " ").replace(/\s+/gu, " ").trim();

/** The five controls the finish line names. */
const REQUIRED_CONTROLS = Object.freeze(["accounts", "conversations", "checkmark", "Save default", "Reset"]);

/** A region that changes when the saved choice changes is drawing that choice. */
const CHOICE_MIN_MEAN_ABS_DIFF = 4;
/** ...and a region that does not change proves the whole page did not repaint. */
const UNTOUCHED_MAX_MEAN_ABS_DIFF = 0.5;
/** Below this a control's box is paint-free: named, but not shown. */
const MIN_CONTROL_NON_DOMINANT_PIXELS = 50;

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

function axNamed(nodes, name) {
  return (nodes ?? []).filter((node) => (node.name?.value ?? "") === name).length;
}

/**
 * Paint inside a rectangle. `nonDominantPixels` is everything that is not the
 * commonest colour in the box, i.e. everything drawn on top of the background.
 * An empty or off-screen box reports zero rather than a number that reads as
 * infinitely well painted.
 */
function pixelStats(image, rect) {
  const left = Math.max(0, Math.floor(rect.x));
  const top = Math.max(0, Math.floor(rect.y));
  const right = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const bottom = Math.min(image.height, Math.ceil(rect.y + rect.height));
  const colours = new Map();
  let count = 0;
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const offset = (y * image.width + x) * 4;
      const key = `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
      colours.set(key, (colours.get(key) ?? 0) + 1);
      count += 1;
    }
  }
  if (count === 0) return { pixels: 0, uniqueColours: 0, nonDominantPixels: 0 };
  return { pixels: count, uniqueColours: colours.size, nonDominantPixels: count - Math.max(...colours.values()) };
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

const RENDER = (choices) => `new Promise((resolve, reject) => {
  const choices = ${JSON.stringify(choices)};
  const deadline = Date.now() + 15000;
  const tick = () => {
    if (typeof window.task0734Render === "function") {
      window.task0734Render(choices);
      requestAnimationFrame(() => requestAnimationFrame(resolve));
      return;
    }
    if (Date.now() > deadline) {
      reject(new Error("the new-friend default screen did not render"));
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
  const checked = (group) => {
    const input = document.querySelector('input[data-new-friend-default="' + group + '"]:checked');
    return {
      value: input?.value ?? "",
      label: input?.closest(".nfd-option")?.querySelector("strong")?.textContent?.trim() ?? "",
    };
  };
  return {
    documentTitle: document.title,
    heading: document.querySelector(".nfd-title")?.textContent?.trim() ?? "",
    sectionLabel: document.querySelector(".new-friend-defaults")?.getAttribute("aria-label") ?? "",
    lead: document.querySelector(".nfd-lead")?.textContent?.trim() ?? "",
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    accounts: checked("accounts"),
    conversations: checked("conversations"),
    checkmark: checked("checkmark"),
    defaults: window.task0734Defaults,
    rects: {
      title: rect(document.querySelector(".nfd-title")),
      accounts: rect(document.querySelector('[data-new-friend-group="accounts"]')),
      conversations: rect(document.querySelector('[data-new-friend-group="conversations"]')),
      checkmark: rect(document.querySelector('[data-new-friend-group="checkmark"]')),
      "Save default": rect(document.querySelector("#save-new-friend-default")),
      Reset: rect(document.querySelector("#reset-new-friend-default")),
    },
    // Every radio's own box. The screen draws a choice by filling one of these
    // dots, so this is the rectangle that has to change when the choice does --
    // a whole fieldset averages one dot over hundreds of unchanged pixels.
    radios: Object.fromEntries([...document.querySelectorAll("input[data-new-friend-default]")].map((input) => [
      input.dataset.newFriendDefault + ":" + input.value,
      rect(input),
    ])),
  };
})()`;

test("TASK 0734 captures the fixed-size Linux new-friend default screen with sample account and conversation choices", async () => {
  // "the fixed-size Linux screen": this capture is the Linux one.
  assert.equal(process.platform, "linux", `TASK 0734 captures the Linux screen; ran on ${process.platform}`);

  const fixture = JSON.parse(readFileSync(path.join(FIXTURE_DIR, "task-0734-new-friend-defaults.json"), "utf8"));
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

    await evaluateValue(page, RENDER(fixture.sample));
    const screen = await evaluateValue(page, READ_SCREEN);
    const ax = await page.send("Accessibility.getFullAXTree");
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const shot = readPng(png);

    // Fixed size: what was asked for is what the PNG header says.
    assert.equal(shot.width, WINDOW.width);
    assert.equal(shot.height, WINDOW.height);

    // The title, in the screen tree.
    assert.equal(screen.heading, SCREEN_TITLE);
    assert.equal(screen.sectionLabel, SCREEN_TITLE);
    assert.equal(screen.documentTitle, SCREEN_TITLE);
    const titleInTree = axNamed(ax.nodes, SCREEN_TITLE);
    assert.ok(titleInTree > 0, `screen tree does not name the title ${SCREEN_TITLE}`);
    assert.ok(
      normalise(screen.heading).startsWith(normalise(TASK_PHRASE)),
      `the heading ${screen.heading} is not the task's ${TASK_PHRASE}`,
    );

    // The sample choices are choices somebody made, not the starting point.
    assert.equal(screen.accounts.value, fixture.sample.accountReach);
    assert.equal(screen.conversations.value, fixture.sample.conversationRule);
    assert.equal(screen.accounts.label, fixture.expected.accountsLabel);
    assert.equal(screen.conversations.label, fixture.expected.conversationsLabel);
    assert.notEqual(screen.accounts.value, screen.defaults.accountReach, "the sample account choice is the default");
    assert.notEqual(screen.conversations.value, screen.defaults.conversationRule, "the sample conversation choice is the default");

    // The five named controls: in the screen tree, and inside the picture with
    // paint in their boxes. Named is not shown, and shown is not shown HERE.
    const treeCounts = {};
    const textCounts = {};
    const painted = {};
    for (const name of REQUIRED_CONTROLS) {
      treeCounts[name] = axNamed(ax.nodes, name);
      textCounts[name] = screen.text.split(name).length - 1;
      assert.ok(treeCounts[name] > 0, `screen tree missing ${name}`);
      assert.ok(textCounts[name] > 0, `visible words missing ${name}`);
      const rect = screen.rects[name];
      assert.ok(rect, `${name} was not laid out`);
      assert.ok(rect.width > 0 && rect.height > 0, `${name} has an empty box: ${JSON.stringify(rect)}`);
      assert.ok(
        rect.x >= 0 && rect.y >= 0 && rect.x + rect.width <= WINDOW.width && rect.y + rect.height <= WINDOW.height,
        `${name} falls outside the ${WINDOW.width}x${WINDOW.height} picture: ${JSON.stringify(rect)}`,
      );
      painted[name] = pixelStats(shot, rect).nonDominantPixels;
      assert.ok(
        painted[name] > MIN_CONTROL_NON_DOMINANT_PIXELS,
        `${name} is named but not drawn: ${painted[name]} non-dominant pixels`,
      );
    }
    const titleRect = screen.rects.title;
    assert.ok(
      titleRect.x >= 0 && titleRect.y >= 0
        && titleRect.x + titleRect.width <= WINDOW.width && titleRect.y + titleRect.height <= WINDOW.height,
      `the title falls outside the picture: ${JSON.stringify(titleRect)}`,
    );
    const titlePainted = pixelStats(shot, titleRect).nonDominantPixels;
    assert.ok(titlePainted > MIN_CONTROL_NON_DOMINANT_PIXELS, `the title is not drawn: ${titlePainted} non-dominant pixels`);

    // Not blank, and not nearly blank.
    const whole = pixelStats(shot, { x: 0, y: 0, width: shot.width, height: shot.height });
    const inkFraction = whole.nonDominantPixels / whole.pixels;
    assert.ok(whole.uniqueColours >= 64, `picture nearly blank: ${whole.uniqueColours} colours`);
    assert.ok(whole.nonDominantPixels >= 10_000, `picture nearly blank: ${whole.nonDominantPixels} non-dominant pixels`);
    assert.ok(inkFraction >= 0.05, `picture nearly blank: ${(inkFraction * 100).toFixed(2)}% of pixels are not the background`);

    // Second capture: the same screen with the starting-point choices. The two
    // choice groups have to look different; the title has to be untouched, or
    // the difference is the page relaying out rather than the choices drawing.
    await evaluateValue(page, RENDER(fixture.contrast));
    const contrastScreen = await evaluateValue(page, READ_SCREEN);
    const contrastPng = await page.screenshot({ fromSurface: true });
    writeFileSync(CONTRAST_PNG_PATH, contrastPng);
    const contrastShot = readPng(contrastPng);
    assert.equal(contrastScreen.accounts.value, fixture.contrast.accountReach);
    assert.equal(contrastScreen.conversations.value, fixture.contrast.conversationRule);
    for (const key of ["title", "accounts", "conversations", "checkmark"]) {
      assert.deepEqual(
        screen.rects[key],
        contrastScreen.rects[key],
        `${key} moved between the two states; the pixel comparison would prove nothing`,
      );
    }
    const radioKeys = {
      accountsSample: `accounts:${fixture.sample.accountReach}`,
      accountsContrast: `accounts:${fixture.contrast.accountReach}`,
      conversationsSample: `conversations:${fixture.sample.conversationRule}`,
      conversationsContrast: `conversations:${fixture.contrast.conversationRule}`,
    };
    for (const key of Object.values(radioKeys)) {
      assert.ok(screen.radios[key], `no radio box for ${key}`);
      assert.deepEqual(screen.radios[key], contrastScreen.radios[key], `the ${key} radio moved between the two states`);
    }
    const diffs = {
      accountsSampleDot: rectMeanAbsDiff(shot, contrastShot, screen.radios[radioKeys.accountsSample]),
      accountsContrastDot: rectMeanAbsDiff(shot, contrastShot, screen.radios[radioKeys.accountsContrast]),
      conversationsSampleDot: rectMeanAbsDiff(shot, contrastShot, screen.radios[radioKeys.conversationsSample]),
      conversationsContrastDot: rectMeanAbsDiff(shot, contrastShot, screen.radios[radioKeys.conversationsContrast]),
      accountsGroup: rectMeanAbsDiff(shot, contrastShot, screen.rects.accounts),
      conversationsGroup: rectMeanAbsDiff(shot, contrastShot, screen.rects.conversations),
      checkmarkGroup: rectMeanAbsDiff(shot, contrastShot, screen.rects.checkmark),
      title: rectMeanAbsDiff(shot, contrastShot, screen.rects.title),
    };
    assert.ok(
      diffs.accountsSampleDot >= CHOICE_MIN_MEAN_ABS_DIFF,
      `the sample account choice is not drawn: the ${fixture.sample.accountReach} dot looks the same chosen and not (${diffs.accountsSampleDot})`,
    );
    assert.ok(
      diffs.accountsContrastDot >= CHOICE_MIN_MEAN_ABS_DIFF,
      `the unchosen account option is not drawn as unchosen: ${diffs.accountsContrastDot}`,
    );
    assert.ok(
      diffs.conversationsSampleDot >= CHOICE_MIN_MEAN_ABS_DIFF,
      `the sample conversation choice is not drawn: the ${fixture.sample.conversationRule} dot looks the same chosen and not (${diffs.conversationsSampleDot})`,
    );
    assert.ok(
      diffs.conversationsContrastDot >= CHOICE_MIN_MEAN_ABS_DIFF,
      `the unchosen conversation option is not drawn as unchosen: ${diffs.conversationsContrastDot}`,
    );
    // The checkmark group is the same choice in both captures, so it must not
    // move a pixel -- proof the two diffs above are the choices, not the camera.
    assert.ok(
      diffs.checkmarkGroup <= UNTOUCHED_MAX_MEAN_ABS_DIFF,
      `the untouched checkmark group repainted between captures (${diffs.checkmarkGroup})`,
    );
    assert.ok(
      diffs.title <= UNTOUCHED_MAX_MEAN_ABS_DIFF,
      `the whole screen repainted, so the choice regions prove nothing (${diffs.title})`,
    );

    const sha256 = createHash("sha256").update(png).digest("hex");
    const contrastSha256 = createHash("sha256").update(contrastPng).digest("hex");
    assert.notEqual(sha256, contrastSha256, "the two captures are the same picture");

    writeFileSync(TREE_PATH, JSON.stringify({
      task: "0734",
      url: `${url}${FIXTURE_PAGE}`,
      platform: process.platform,
      window: WINDOW,
      png: { path: PNG_PATH, bytes: png.length, sha256 },
      contrastPng: { path: CONTRAST_PNG_PATH, bytes: contrastPng.length, sha256: contrastSha256 },
      title: { heading: screen.heading, sectionLabel: screen.sectionLabel, documentTitle: screen.documentTitle, screenTreeCount: titleInTree, taskPhrase: TASK_PHRASE, normalised: normalise(screen.heading), rect: titleRect, nonDominantPixels: titlePainted },
      sample: { accounts: screen.accounts, conversations: screen.conversations, checkmark: screen.checkmark, defaults: screen.defaults },
      contrast: { accounts: contrastScreen.accounts, conversations: contrastScreen.conversations },
      controls: REQUIRED_CONTROLS.map((name) => ({
        name,
        screenTree: treeCounts[name],
        visibleText: textCounts[name],
        rect: screen.rects[name],
        nonDominantPixels: painted[name],
      })),
      image: { uniqueColours: whole.uniqueColours, nonDominantPixels: whole.nonDominantPixels, inkFraction },
      diffs,
      lead: screen.lead,
      axNodes: ax.nodes,
    }, null, 2));

    console.log(`TASK0734_PLATFORM=${process.platform}`);
    console.log(`TASK0734_PNG=${PNG_PATH}`);
    console.log(`TASK0734_CONTRAST_PNG=${CONTRAST_PNG_PATH}`);
    console.log(`TASK0734_TREE=${TREE_PATH}`);
    console.log(`TASK0734_WINDOW=${shot.width}x${shot.height}`);
    console.log(`TASK0734_PNG_BYTES=${png.length}`);
    console.log(`TASK0734_PNG_SHA256=${sha256}`);
    console.log(`TASK0734_CONTRAST_SHA256=${contrastSha256}`);
    console.log(`TASK0734_TITLE=${screen.heading}|tree=${titleInTree}|painted=${titlePainted}|rect=${Math.round(titleRect.x)},${Math.round(titleRect.y)},${Math.round(titleRect.width)}x${Math.round(titleRect.height)}|task_phrase=${TASK_PHRASE}|normalised=${normalise(screen.heading)}`);
    console.log(`TASK0734_SAMPLE_ACCOUNTS=${screen.accounts.value}|${screen.accounts.label}|default=${screen.defaults.accountReach}`);
    console.log(`TASK0734_SAMPLE_CONVERSATIONS=${screen.conversations.value}|${screen.conversations.label}|default=${screen.defaults.conversationRule}`);
    console.log(`TASK0734_SAMPLE_CHECKMARK=${screen.checkmark.value}|${screen.checkmark.label}`);
    console.log(`TASK0734_SCREEN_TREE=${REQUIRED_CONTROLS.map((name) => `${name}=${treeCounts[name]}`).join(" ")}`);
    console.log(`TASK0734_VISIBLE_TEXT=${REQUIRED_CONTROLS.map((name) => `${name}=${textCounts[name]}`).join(" ")}`);
    console.log(`TASK0734_PAINTED=${REQUIRED_CONTROLS.map((name) => `${name}=${painted[name]}`).join(" ")}`);
    console.log(`TASK0734_RECTS=${REQUIRED_CONTROLS.map((name) => `${name}=${Math.round(screen.rects[name].x)},${Math.round(screen.rects[name].y)},${Math.round(screen.rects[name].width)}x${Math.round(screen.rects[name].height)}`).join(" ")}`);
    console.log(`TASK0734_IMAGE=unique_colours=${whole.uniqueColours} non_dominant_pixels=${whole.nonDominantPixels} of ${whole.pixels} ink=${(inkFraction * 100).toFixed(2)}%`);
    console.log(`TASK0734_CHOICE_DIFFS=${Object.entries(diffs).map(([key, value]) => `${key}=${value.toFixed(4)}`).join(" ")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
