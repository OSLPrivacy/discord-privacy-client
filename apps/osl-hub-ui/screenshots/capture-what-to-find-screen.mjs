#!/usr/bin/env node
/**
 * TASK 1413 - drive and capture the Linux "What counts as a bad message" page.
 *
 * The finish line is "choosing Everything above saves all five finding rules",
 * so this does not merely render markup. In a real browser it:
 *   - checks the six TASK 1411 choices, the private words box, Back and
 *     Continue are all on screen, with Continue refused while nothing is ticked;
 *   - types two private words and clicks the "Everything above" box;
 *   - proves all five finding rules are drawn as ticked IN THE PIXELS (each
 *     choice row carries brand ink only once it is selected);
 *   - clicks Continue and reads back what the page actually handed the save
 *     port: the rule names, their count, and the private words;
 *   - clicks Back and checks it leaves without saving a second time.
 */
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { cropFacts, imageFacts, parsePng } from "./png-facts.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-1413-what-to-find.png");
const SAVED_PNG_PATH = path.join(OUTPUT_DIR, "task-1413-what-to-find-saved.png");
const WINDOW = { width: 1280, height: 1000 };
/** #0a0a0a, the app's --bg. */
const BACKGROUND = [10, 10, 10];
/** #06b6d4, the app's --brand: the colour a ticked choice is drawn in. */
const BRAND = [6, 182, 212];
const FINDING_RULES = [
  "passwords and codes",
  "personal details",
  "money details",
  "private words",
  "private pictures",
];
const EVERYTHING_ABOVE = "everything above";
const CHOICES = [...FINDING_RULES, EVERYTHING_ABOVE];
const PRIVATE_WORDS_TYPED = "project bluebird\nMAPLE-4172";
const PRIVATE_WORDS_EXPECTED = ["project bluebird", "MAPLE-4172"];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function brandPixels(png, rect) {
  if (!rect) return 0;
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  let count = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const distance = Math.abs(png.pixels[offset] - BRAND[0])
        + Math.abs(png.pixels[offset + 1] - BRAND[1])
        + Math.abs(png.pixels[offset + 2] - BRAND[2]);
      if (distance <= 60) count += 1;
    }
  }
  return count;
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description
      || result.exceptionDetails.text
      || "page evaluation failed",
    );
  }
  return result.result.value;
}

const READ_SCREEN = `(async () => {
  await document.fonts.ready;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const box = (element) => {
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const boxes = [...document.querySelectorAll('input[type="checkbox"][name="bad-message-rule"]')];
  const words = document.querySelector("[data-private-words]");
  const back = document.querySelector("[data-what-to-find-back]");
  const proceed = document.querySelector("[data-what-to-find-continue]");
  const heading = document.querySelector("#what-to-find-heading");
  const line = document.querySelector("[data-what-to-find-state]");
  const count = document.querySelector("[data-private-words-count]");
  const section = document.querySelector("[data-what-to-find]");
  return {
    title: document.title,
    values: boxes.map((input) => input.value),
    checked: boxes.filter((input) => input.checked).map((input) => input.value),
    choiceRects: Object.fromEntries(boxes.map((input) => [input.value, box(input.closest(".wtf-choice"))])),
    headingText: heading ? heading.textContent.trim() : null,
    backText: back ? back.textContent.trim() : null,
    continueText: proceed ? proceed.textContent.trim() : null,
    continueDisabled: proceed ? proceed.disabled : null,
    wordsValue: words ? words.value : null,
    wordsPlaceholder: words ? words.placeholder : null,
    wordsCountText: count ? count.textContent.trim() : null,
    stateText: line ? line.textContent.trim() : null,
    saveState: section ? section.dataset.whatToFindSave ?? "none" : null,
    savedRules: section ? section.dataset.whatToFindSavedRules ?? "none" : null,
    saves: (window.oslWhatToFindSaves || []).map((save) => ({
      runId: save.runId,
      ruleNames: [...save.ruleNames],
      privateWords: [...save.privateWords],
      matchTreatment: save.matchTreatment,
    })),
    backs: window.oslWhatToFindBacks,
    rects: {
      heading: box(heading),
      words: box(words),
      state: box(line),
      back: box(back),
      continue: box(proceed),
    },
    visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/what-to-find-screen.html`;
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
    });
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "reduce" }],
    });
    await page.navigate(url, { timeoutMs: 30_000 });
    await evaluate(page, `(async () => {
      const deadline = Date.now() + 20000;
      while (document.body.dataset.whatToFindFixture !== "ready") {
        if (Date.now() > deadline) throw new Error("fixture never finished rendering");
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      return true;
    })()`);

    // ---- 1. the page as it opens: six choices, nothing ticked, Continue refused.
    const before = await evaluate(page, READ_SCREEN);
    if (before.values.join("|") !== CHOICES.join("|")) {
      throw new Error(`expected the six choices ${CHOICES.join(", ")}; got ${before.values.join(", ")}`);
    }
    if (before.checked.length !== 0) {
      throw new Error(`expected nothing ticked on open, found ${before.checked.join(", ")}`);
    }
    if (before.continueDisabled !== true) {
      throw new Error("Continue must be refused before anything is chosen");
    }
    if (before.wordsValue !== "") throw new Error("the private words box should open empty");
    if (before.backText !== "Back") throw new Error(`expected a Back control, found "${before.backText}"`);
    if (before.continueText !== "Continue") {
      throw new Error(`expected a Continue control, found "${before.continueText}"`);
    }
    if (before.saves.length !== 0) throw new Error("nothing may be saved before Continue is pressed");
    const beforePng = parsePng(await page.screenshot({ captureBeyondViewport: false }));
    const beforeBrand = Object.fromEntries(
      CHOICES.map((choice) => [choice, brandPixels(beforePng, before.choiceRects[choice])]),
    );
    const alreadyDrawnTicked = CHOICES.filter((choice) => beforeBrand[choice] >= 20);
    if (alreadyDrawnTicked.length > 0) {
      throw new Error(`choices are drawn as ticked before anything was chosen: ${alreadyDrawnTicked.join(", ")}`);
    }

    // ---- 2. type private words, then choose "Everything above".
    await evaluate(page, `(() => {
      const words = document.querySelector("[data-private-words]");
      words.value = ${JSON.stringify(PRIVATE_WORDS_TYPED)};
      words.dispatchEvent(new Event("input", { bubbles: true }));
      words.dispatchEvent(new Event("change", { bubbles: true }));
      return true;
    })()`);
    await evaluate(page, `(() => {
      const box = [...document.querySelectorAll('input[name="bad-message-rule"]')]
        .find((input) => input.value === ${JSON.stringify(EVERYTHING_ABOVE)});
      if (!box) throw new Error("the Everything above choice is missing");
      box.click();
      return true;
    })()`);

    const chosen = await evaluate(page, READ_SCREEN);
    if (chosen.checked.length !== 6) {
      throw new Error(`expected all six boxes ticked after Everything above, found ${chosen.checked.length}: ${chosen.checked.join(", ")}`);
    }
    for (const rule of FINDING_RULES) {
      if (!chosen.checked.includes(rule)) throw new Error(`"${rule}" was not ticked by Everything above`);
    }
    if (chosen.continueDisabled !== false) throw new Error("Continue is still refused after choosing Everything above");
    if (chosen.wordsValue !== PRIVATE_WORDS_TYPED) throw new Error("the typed private words were lost");
    if (chosen.saves.length !== 0) throw new Error("choosing a rule must not save on its own");

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    const png = parsePng(screenshot);
    const chosenBrand = Object.fromEntries(
      CHOICES.map((choice) => [choice, brandPixels(png, chosen.choiceRects[choice])]),
    );
    const notDrawnTicked = CHOICES.filter((choice) => chosenBrand[choice] < 20);
    if (notDrawnTicked.length > 0) {
      throw new Error(`ticked choices are not drawn as ticked: ${notDrawnTicked.join(", ")}`);
    }
    const facts = imageFacts(screenshot, chosen.rects, { background: BACKGROUND });
    if (facts.width !== WINDOW.width || facts.height !== WINDOW.height) {
      throw new Error(`unexpected PNG size ${facts.width}x${facts.height}`);
    }
    if (facts.nearlyBlank) {
      throw new Error(`PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
    }
    const blank = Object.entries(facts.crops)
      .filter(([, crop]) => crop.nonBackground < 10 || crop.brightPixels < 10)
      .map(([name]) => name);
    if (blank.length > 0) throw new Error(`boxes are blank in the image: ${blank.join(", ")}`);
    for (const choice of CHOICES) {
      const label = choice.replace(/^./u, (character) => character.toUpperCase());
      if (!chosen.visibleText.includes(label)) throw new Error(`choice "${choice}" is not visible text on the screen`);
    }

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
      .filter(Boolean);
    const axWanted = [
      ...CHOICES.map((choice) => choice.replace(/^./u, (character) => character.toUpperCase())),
      "Private words",
      "Back",
      "Continue",
      "What counts as a bad message",
    ];
    const missingAx = axWanted.filter((text) => !axNames.some((name) => name.includes(text)));
    if (missingAx.length > 0) throw new Error(`missing accessibility names: ${missingAx.join(", ")}`);

    // ---- 3. Continue: what did the page actually hand the save port?
    await evaluate(page, `(async () => {
      document.querySelector("[data-what-to-find-continue]").click();
      await window.oslWhatToFindHandle.settled();
      return true;
    })()`);
    const saved = await evaluate(page, READ_SCREEN);
    if (saved.saves.length !== 1) throw new Error(`expected exactly 1 save, got ${saved.saves.length}`);
    const [save] = saved.saves;
    if (save.ruleNames.length !== 5) {
      throw new Error(`expected 5 saved finding rules, got ${save.ruleNames.length}: ${save.ruleNames.join(", ")}`);
    }
    if (save.ruleNames.join("|") !== FINDING_RULES.join("|")) {
      throw new Error(`saved rules are not the five finding rules: ${save.ruleNames.join(", ")}`);
    }
    if (save.ruleNames.includes(EVERYTHING_ABOVE)) {
      throw new Error(`"${EVERYTHING_ABOVE}" was saved as a rule name; the backend refuses unknown rule names`);
    }
    if (save.privateWords.join("|") !== PRIVATE_WORDS_EXPECTED.join("|")) {
      throw new Error(`saved private words are ${save.privateWords.join(", ")}`);
    }
    if (save.matchTreatment !== "possible_match") {
      throw new Error(`saved match treatment is "${save.matchTreatment}"`);
    }
    if (save.runId !== "task-1413-run") throw new Error(`saved run id is "${save.runId}"`);
    if (saved.saveState !== "saved" || saved.savedRules !== "5") {
      throw new Error(`the screen does not report the save: state=${saved.saveState} rules=${saved.savedRules}`);
    }
    const savedShot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(SAVED_PNG_PATH, savedShot);
    const savedStateCrop = cropFacts(parsePng(savedShot), saved.rects.state, { background: BACKGROUND });
    if (savedStateCrop.nonBackground < 10) throw new Error("the saved line is blank in the image");

    // ---- 4. Back leaves without saving again.
    await evaluate(page, `(() => { document.querySelector("[data-what-to-find-back]").click(); return true; })()`);
    const afterBack = await evaluate(page, READ_SCREEN);
    if (afterBack.backs !== 1) throw new Error(`Back fired ${afterBack.backs} times`);
    if (afterBack.saves.length !== 1) throw new Error(`Back saved again: ${afterBack.saves.length} saves`);

    console.log(`TASK1413_URL=${url}`);
    console.log(`TASK1413_TITLE=${chosen.headingText}`);
    console.log(`TASK1413_CHOICES=${chosen.values.join("|")}`);
    console.log(`TASK1413_CHOICE_COUNT=${chosen.values.length}`);
    console.log(`TASK1413_OPEN_TICKED_COUNT=${before.checked.length}`);
    console.log(`TASK1413_OPEN_CONTINUE_DISABLED=${before.continueDisabled}`);
    console.log(`TASK1413_OPEN_SAVE_COUNT=${before.saves.length}`);
    console.log(`TASK1413_PRIVATE_WORDS_TYPED=${JSON.stringify(PRIVATE_WORDS_TYPED)}`);
    console.log(`TASK1413_TICKED_AFTER_EVERYTHING_ABOVE=${chosen.checked.join("|")}`);
    console.log(`TASK1413_TICKED_COUNT=${chosen.checked.length}`);
    console.log(`TASK1413_CONTINUE_DISABLED=${chosen.continueDisabled}`);
    console.log(`TASK1413_BACK_CONTROL=${chosen.backText}`);
    console.log(`TASK1413_CONTINUE_CONTROL=${chosen.continueText}`);
    console.log(`TASK1413_WORDS_COUNT_LINE=${chosen.wordsCountText}`);
    console.log(`TASK1413_SAVE_COUNT=${saved.saves.length}`);
    console.log(`TASK1413_SAVED_RULE_COUNT=${save.ruleNames.length}`);
    console.log(`TASK1413_SAVED_RULES=${save.ruleNames.join("|")}`);
    console.log(`TASK1413_SAVED_PRIVATE_WORDS=${save.privateWords.join("|")}`);
    console.log(`TASK1413_SAVED_MATCH_TREATMENT=${save.matchTreatment}`);
    console.log(`TASK1413_SAVED_RUN_ID=${save.runId}`);
    console.log(`TASK1413_SAVED_STATE_LINE=${saved.stateText}`);
    console.log(`TASK1413_BACK_PRESSES=${afterBack.backs}`);
    console.log(`TASK1413_SAVES_AFTER_BACK=${afterBack.saves.length}`);
    for (const choice of CHOICES) {
      const key = choice.toUpperCase().replace(/[^A-Z0-9]+/gu, "_");
      console.log(`TASK1413_BRAND_PIXELS_${key}=${beforeBrand[choice]}->${chosenBrand[choice]}`);
    }
    console.log(`TASK1413_PNG=${PNG_PATH}`);
    console.log(`TASK1413_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK1413_SAVED_PNG=${SAVED_PNG_PATH}`);
    console.log(`TASK1413_SAVED_PNG_SHA256=${sha256(savedShot)}`);
    console.log(`TASK1413_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK1413_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK1413_PNG_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`TASK1413_PNG_NEARLY_BLANK=${facts.nearlyBlank}`);
    console.log("TASK1413_RESULT=pass");
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-what-to-find-screen: ${error.stack || error.message}`);
  process.exitCode = 1;
});
