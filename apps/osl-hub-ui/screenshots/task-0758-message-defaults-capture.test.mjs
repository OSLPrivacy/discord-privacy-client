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
const FIXTURE_PAGE = "screenshots/task-0758-message-defaults-fixture.html";

/**
 * Fixed size, stated here and asserted against the PNG header afterwards. The
 * gate TASK 0756 built this screen for a 1280x800 Linux window, so that is the
 * window this capture uses; every one of the six controls has to be inside it,
 * because a control named in the document but below the fold is not a control
 * the screenshot shows.
 */
const WINDOW = Object.freeze({ width: 1280, height: 800 });

const PNG_PATH = path.join(EVIDENCE_DIR, `task-0758-message-defaults-${WINDOW.width}x${WINDOW.height}.png`);
const CONTRAST_PNG_PATH = path.join(EVIDENCE_DIR, `task-0758-message-defaults-contrast-${WINDOW.width}x${WINDOW.height}.png`);
const TREE_PATH = path.join(EVIDENCE_DIR, "task-0758-message-defaults-screen-tree.json");

/** The screen's own title, exported by the module TASK 0756 built. */
const SCREEN_TITLE = "Message defaults";

/**
 * The six controls the finish line names, each with the words the SCREEN calls
 * it by.
 *
 * The plan's name and the screen's name are the same control under two names in
 * four of six cases and differ in one: the plan says "burn scope", the screen's
 * heading reads "Burn after reading". Neither half alone would be honest, so
 * every control is checked twice --
 *   * `taskName`  the plan's word, matched against the control's own identifier
 *                 in the markup (`data-message-default-group`, or the button's
 *                 id) with hyphens folded to spaces and case folded away;
 *   * `screenName` the exact string the screen puts in front of a person, which
 *                 is the name that has to be in the screen tree and drawn in
 *                 the picture.
 */
const REQUIRED_CONTROLS = Object.freeze([
  { taskName: "timer", key: "timer", screenName: "Timer", selector: '[data-message-default-group="timer"]', identifier: "timer" },
  { taskName: "burn scope", key: "burn-scope", screenName: "Burn after reading", selector: '[data-message-default-group="burn-scope"]', identifier: "burn-scope" },
  { taskName: "view-once length", key: "view-once-length", screenName: "View-once length", selector: '[data-message-default-group="view-once-length"]', identifier: "view-once-length" },
  { taskName: "AI or Wordbank", key: "writing", screenName: "AI or Wordbank", selector: '[data-message-default-group="writing"]', identifier: "writing" },
  { taskName: "save", key: "save", screenName: "Save", selector: "#save-message-defaults", identifier: "save-message-defaults" },
  { taskName: "reset", key: "reset", screenName: "Reset", selector: "#reset-message-defaults", identifier: "reset-message-defaults" },
]);

/** The four controls that hold one of the saved values. */
const CHOICE_CONTROLS = Object.freeze(["timer", "burn-scope", "view-once-length", "writing"]);

/** Below this a control's box is paint-free: named in the document, not shown. */
const MIN_CONTROL_NON_DOMINANT_PIXELS = 50;
/** A region that changes when the saved choice changes is drawing that choice. */
const CHOICE_MIN_MEAN_ABS_DIFF = 4;
/** ...and a region that does not change proves the whole page did not repaint. */
const UNTOUCHED_MAX_MEAN_ABS_DIFF = 0.5;
/**
 * How far a radio dot is allowed to move between the two captures. It is not
 * zero because the "Saved" pill rides in whichever choice row is the saved one,
 * and that pill's line box is a fraction of a pixel taller than a bare label --
 * so moving the saved choice from one row to another nudges the rows below it
 * by less than a pixel. The fieldsets themselves are asserted pixel-identical,
 * and the dots are compared over the region the two boxes have in COMMON, so
 * the measured difference is the dot being filled or not and never the nudge.
 */
const MAX_DOT_SHIFT_PX = 1;

/** The part of the picture both boxes cover, snapped inward to whole pixels. */
function intersectRects(left, right) {
  const x = Math.ceil(Math.max(left.x, right.x));
  const y = Math.ceil(Math.max(left.y, right.y));
  const width = Math.floor(Math.min(left.x + left.width, right.x + right.width)) - x;
  const height = Math.floor(Math.min(left.y + left.height, right.y + right.height)) - y;
  return { x, y, width, height };
}

const normalise = (value) => value.toLowerCase().replace(/-/gu, " ").replace(/\s+/gu, " ").trim();

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
  return (nodes ?? []).filter((node) => (node.name?.value ?? "").trim() === name).length;
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

const RENDER = (saved) => `new Promise((resolve, reject) => {
  const saved = ${JSON.stringify(saved)};
  const deadline = Date.now() + 15000;
  const tick = () => {
    if (typeof window.task0758Render === "function") {
      window.task0758Render(saved);
      requestAnimationFrame(() => requestAnimationFrame(resolve));
      return;
    }
    if (Date.now() > deadline) {
      reject(new Error("the message defaults screen did not render"));
      return;
    }
    setTimeout(tick, 25);
  };
  tick();
})`;

const CONTROLS_JSON = JSON.stringify(REQUIRED_CONTROLS.map(({ key, selector }) => [key, selector]));
const CHOICES_JSON = JSON.stringify(CHOICE_CONTROLS);

const READ_SCREEN = `(() => {
  const rect = (element) => {
    const box = element?.getBoundingClientRect();
    return box ? { x: box.x, y: box.y, width: box.width, height: box.height } : null;
  };
  const rects = {};
  const headings = {};
  for (const [key, selector] of ${CONTROLS_JSON}) {
    const element = document.querySelector(selector);
    rects[key] = rect(element);
    headings[key] = (element?.tagName === "FIELDSET"
      ? element.querySelector("legend")?.textContent
      : element?.textContent) ?? "";
    headings[key] = headings[key].trim();
  }
  const chosen = {};
  const radios = {};
  for (const key of ${CHOICES_JSON}) {
    const input = document.querySelector('input[data-message-default="' + key + '"]:checked');
    chosen[key] = {
      value: input?.value ?? "",
      label: input?.closest(".msg-def-choice")?.querySelector(".msg-def-choice-label")?.textContent?.trim() ?? "",
    };
    for (const each of document.querySelectorAll('input[data-message-default="' + key + '"]')) {
      radios[key + ":" + each.value] = rect(each.closest(".msg-def-choice")?.querySelector(".osl-radio-dot") ?? each);
    }
  }
  return {
    documentTitle: document.title,
    heading: document.querySelector("#route-heading")?.textContent?.trim() ?? "",
    titleRect: rect(document.querySelector("#route-heading")),
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    status: document.querySelector("[data-message-default-status]")?.dataset.messageDefaultStatus ?? "",
    savedTagCount: document.querySelectorAll(".msg-def-saved-tag").length,
    factory: window.task0758Factory,
    rects,
    headings,
    chosen,
    radios,
  };
})()`;

test("TASK 0758 captures the fixed-size Linux message defaults screen with non-default choices", async () => {
  // "the fixed-size Linux screen": this capture is the Linux one.
  assert.equal(process.platform, "linux", `TASK 0758 captures the Linux screen; ran on ${process.platform}`);

  const fixture = JSON.parse(readFileSync(path.join(FIXTURE_DIR, "task-0758-message-defaults.json"), "utf8"));
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

    // The title: the screen's own words, in the screen tree, and drawn.
    assert.equal(screen.heading, SCREEN_TITLE);
    assert.equal(screen.documentTitle, SCREEN_TITLE);
    const titleInTree = axNamed(ax.nodes, SCREEN_TITLE);
    assert.ok(titleInTree > 0, `screen tree does not name the title ${SCREEN_TITLE}`);
    const titleRect = screen.titleRect;
    assert.ok(titleRect, "the title was not laid out");
    assert.ok(
      titleRect.x >= 0 && titleRect.y >= 0
        && titleRect.x + titleRect.width <= WINDOW.width && titleRect.y + titleRect.height <= WINDOW.height,
      `the title falls outside the picture: ${JSON.stringify(titleRect)}`,
    );
    const titlePainted = pixelStats(shot, titleRect).nonDominantPixels;
    assert.ok(titlePainted > MIN_CONTROL_NON_DOMINANT_PIXELS, `the title is not drawn: ${titlePainted} non-dominant pixels`);

    // The choices are choices somebody made: every one of the four differs from
    // the factory starting point the module exports, and the screen reads them
    // back with the words the fixture says they should read.
    for (const key of CHOICE_CONTROLS) {
      const wanted = {
        timer: String(fixture.sample.timerSeconds),
        "burn-scope": fixture.sample.burnScope,
        "view-once-length": String(fixture.sample.viewOnceLengthSeconds),
        writing: fixture.sample.coverWriting,
      }[key];
      const factory = {
        timer: String(screen.factory.timerSeconds),
        "burn-scope": screen.factory.burnScope,
        "view-once-length": String(screen.factory.viewOnceLengthSeconds),
        writing: screen.factory.coverWriting,
      }[key];
      assert.equal(screen.chosen[key].value, wanted, `${key} is not set to the sample choice`);
      assert.equal(screen.chosen[key].label, fixture.expected[key], `${key} reads the wrong words`);
      assert.notEqual(screen.chosen[key].value, factory, `the sample ${key} choice IS the factory default`);
    }
    assert.equal(screen.savedTagCount, CHOICE_CONTROLS.length, "not every choice is marked saved");
    assert.equal(screen.status, "saved");

    // The six named controls: in the screen tree, in the visible words, and
    // inside the picture with paint in their boxes. Named is not shown, and
    // shown is not shown HERE.
    const treeCounts = {};
    const textCounts = {};
    const painted = {};
    const matchedBy = {};
    for (const control of REQUIRED_CONTROLS) {
      // The plan's word IS this control. Case folded and with hyphens folded to
      // spaces, it is either the control's own identifier in the markup
      // ("burn scope" -> data-message-default-group="burn-scope") or the words
      // the screen puts on it ("AI or Wordbank" -> the legend). Which of the two
      // matched is recorded rather than assumed, so a control that answers to
      // neither cannot slip through as one that answers to the other.
      const byIdentifier = normalise(control.identifier) === normalise(control.taskName);
      const byScreenName = normalise(control.screenName) === normalise(control.taskName);
      assert.ok(
        byIdentifier || byScreenName,
        `the control the plan calls "${control.taskName}" is neither the identifier ${control.identifier} nor the screen's words ${control.screenName}`,
      );
      matchedBy[control.key] = byIdentifier ? (byScreenName ? "identifier+screen-name" : "identifier") : "screen-name";
      assert.equal(screen.headings[control.key], control.screenName, `${control.taskName} does not read ${control.screenName}`);

      treeCounts[control.key] = axNamed(ax.nodes, control.screenName);
      textCounts[control.key] = screen.text.split(control.screenName).length - 1;
      assert.ok(treeCounts[control.key] > 0, `screen tree missing ${control.taskName} (${control.screenName})`);
      assert.ok(textCounts[control.key] > 0, `visible words missing ${control.taskName} (${control.screenName})`);

      const rect = screen.rects[control.key];
      assert.ok(rect, `${control.taskName} was not laid out`);
      assert.ok(rect.width > 0 && rect.height > 0, `${control.taskName} has an empty box: ${JSON.stringify(rect)}`);
      assert.ok(
        rect.x >= 0 && rect.y >= 0 && rect.x + rect.width <= WINDOW.width && rect.y + rect.height <= WINDOW.height,
        `${control.taskName} falls outside the ${WINDOW.width}x${WINDOW.height} picture: ${JSON.stringify(rect)}`,
      );
      painted[control.key] = pixelStats(shot, rect).nonDominantPixels;
      assert.ok(
        painted[control.key] > MIN_CONTROL_NON_DOMINANT_PIXELS,
        `${control.taskName} is named but not drawn: ${painted[control.key]} non-dominant pixels`,
      );
    }

    // Not blank, and not nearly blank.
    const whole = pixelStats(shot, { x: 0, y: 0, width: shot.width, height: shot.height });
    const inkFraction = whole.nonDominantPixels / whole.pixels;
    assert.ok(whole.uniqueColours >= 64, `picture nearly blank: ${whole.uniqueColours} colours`);
    assert.ok(whole.nonDominantPixels >= 10_000, `picture nearly blank: ${whole.nonDominantPixels} non-dominant pixels`);
    assert.ok(inkFraction >= 0.05, `picture nearly blank: ${(inkFraction * 100).toFixed(2)}% of pixels are not the background`);

    // Second capture: the same screen at the factory starting point. Each of
    // the four chosen dots has to look different between the two, and the title
    // has to be untouched -- otherwise the difference is the page relaying out
    // rather than the picture showing the non-default choices.
    await evaluateValue(page, RENDER(fixture.contrast));
    const contrastScreen = await evaluateValue(page, READ_SCREEN);
    const contrastPng = await page.screenshot({ fromSurface: true });
    writeFileSync(CONTRAST_PNG_PATH, contrastPng);
    const contrastShot = readPng(contrastPng);
    for (const key of CHOICE_CONTROLS) {
      const wanted = {
        timer: String(fixture.contrast.timerSeconds),
        "burn-scope": fixture.contrast.burnScope,
        "view-once-length": String(fixture.contrast.viewOnceLengthSeconds),
        writing: fixture.contrast.coverWriting,
      }[key];
      assert.equal(contrastScreen.chosen[key].value, wanted, `the contrast ${key} choice did not take`);
      assert.deepEqual(screen.rects[key], contrastScreen.rects[key], `${key} moved between the two states`);
    }
    assert.deepEqual(screen.titleRect, contrastScreen.titleRect, "the title moved between the two states");

    const diffs = { title: rectMeanAbsDiff(shot, contrastShot, screen.titleRect) };
    const dotShifts = {};
    for (const key of CHOICE_CONTROLS) {
      const sampleValue = screen.chosen[key].value;
      const contrastValue = contrastScreen.chosen[key].value;
      for (const [suffix, value] of [["Sample", sampleValue], ["Contrast", contrastValue]]) {
        const radioKey = `${key}:${value}`;
        const before = screen.radios[radioKey];
        const after = contrastScreen.radios[radioKey];
        assert.ok(before && after, `no radio dot box for ${radioKey}`);
        const shift = Math.max(Math.abs(before.x - after.x), Math.abs(before.y - after.y));
        dotShifts[radioKey] = shift;
        assert.ok(shift <= MAX_DOT_SHIFT_PX, `the ${radioKey} dot moved ${shift}px between the two states`);
        const common = intersectRects(before, after);
        assert.ok(common.width > 0 && common.height > 0, `the ${radioKey} dot boxes do not overlap`);
        const diff = rectMeanAbsDiff(shot, contrastShot, common);
        diffs[`${key}${suffix}Dot`] = diff;
        assert.ok(
          diff >= CHOICE_MIN_MEAN_ABS_DIFF,
          `the ${radioKey} dot looks the same chosen and not (${diff}); the picture is not showing the choice`,
        );
      }
    }
    assert.ok(
      diffs.title <= UNTOUCHED_MAX_MEAN_ABS_DIFF,
      `the whole screen repainted, so the choice regions prove nothing (${diffs.title})`,
    );

    const sha256 = createHash("sha256").update(png).digest("hex");
    const contrastSha256 = createHash("sha256").update(contrastPng).digest("hex");
    assert.notEqual(sha256, contrastSha256, "the two captures are the same picture");

    writeFileSync(TREE_PATH, JSON.stringify({
      task: "0758",
      url: `${url}${FIXTURE_PAGE}`,
      platform: process.platform,
      window: WINDOW,
      png: { path: PNG_PATH, bytes: png.length, sha256 },
      contrastPng: { path: CONTRAST_PNG_PATH, bytes: contrastPng.length, sha256: contrastSha256 },
      title: {
        heading: screen.heading,
        documentTitle: screen.documentTitle,
        screenTreeCount: titleInTree,
        rect: titleRect,
        nonDominantPixels: titlePainted,
      },
      choices: Object.fromEntries(CHOICE_CONTROLS.map((key) => [key, {
        sample: screen.chosen[key],
        contrast: contrastScreen.chosen[key],
      }])),
      factory: screen.factory,
      controls: REQUIRED_CONTROLS.map((control) => ({
        taskName: control.taskName,
        screenName: control.screenName,
        identifier: control.identifier,
        matchedBy: matchedBy[control.key],
        screenTree: treeCounts[control.key],
        visibleText: textCounts[control.key],
        rect: screen.rects[control.key],
        nonDominantPixels: painted[control.key],
      })),
      image: { uniqueColours: whole.uniqueColours, nonDominantPixels: whole.nonDominantPixels, pixels: whole.pixels, inkFraction },
      diffs,
      dotShifts,
      axNodes: ax.nodes,
    }, null, 2));

    console.log(`TASK0758_PLATFORM=${process.platform}`);
    console.log(`TASK0758_PNG=${PNG_PATH}`);
    console.log(`TASK0758_CONTRAST_PNG=${CONTRAST_PNG_PATH}`);
    console.log(`TASK0758_TREE=${TREE_PATH}`);
    console.log(`TASK0758_WINDOW=${shot.width}x${shot.height}`);
    console.log(`TASK0758_PNG_BYTES=${png.length}`);
    console.log(`TASK0758_PNG_SHA256=${sha256}`);
    console.log(`TASK0758_CONTRAST_SHA256=${contrastSha256}`);
    console.log(`TASK0758_TITLE=${screen.heading}|tree=${titleInTree}|painted=${titlePainted}|rect=${Math.round(titleRect.x)},${Math.round(titleRect.y)},${Math.round(titleRect.width)}x${Math.round(titleRect.height)}`);
    for (const key of CHOICE_CONTROLS) {
      console.log(`TASK0758_CHOICE ${key}=${screen.chosen[key].value} label=${screen.chosen[key].label} factory=${contrastScreen.chosen[key].value} non_default=${screen.chosen[key].value !== contrastScreen.chosen[key].value}`);
    }
    console.log(`TASK0758_CONTROL_NAMES=${REQUIRED_CONTROLS.map((c) => `${c.taskName}->${c.screenName}(${matchedBy[c.key]})`).join(" ")}`);
    console.log(`TASK0758_SCREEN_TREE=${REQUIRED_CONTROLS.map((c) => `${c.taskName}[${c.screenName}]=${treeCounts[c.key]}`).join(" ")}`);
    console.log(`TASK0758_VISIBLE_TEXT=${REQUIRED_CONTROLS.map((c) => `${c.taskName}=${textCounts[c.key]}`).join(" ")}`);
    console.log(`TASK0758_PAINTED=${REQUIRED_CONTROLS.map((c) => `${c.taskName}=${painted[c.key]}`).join(" ")}`);
    console.log(`TASK0758_RECTS=${REQUIRED_CONTROLS.map((c) => `${c.taskName}=${Math.round(screen.rects[c.key].x)},${Math.round(screen.rects[c.key].y)},${Math.round(screen.rects[c.key].width)}x${Math.round(screen.rects[c.key].height)}`).join(" ")}`);
    console.log(`TASK0758_IMAGE=unique_colours=${whole.uniqueColours} non_dominant_pixels=${whole.nonDominantPixels} of ${whole.pixels} ink=${(inkFraction * 100).toFixed(2)}%`);
    console.log(`TASK0758_CHOICE_DIFFS=${Object.entries(diffs).map(([key, value]) => `${key}=${value.toFixed(4)}`).join(" ")}`);
    console.log(`TASK0758_DOT_SHIFTS_PX=${Object.entries(dotShifts).map(([key, value]) => `${key}=${value}`).join(" ")}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
