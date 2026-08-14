#!/usr/bin/env node
/**
 * TASK 4852 - drive and capture the Linux role permission screen.
 *
 * The finish line is that the three enforcement explanations are ON SCREEN, so
 * this does not read the source and call it done. In a real browser it:
 *   - mounts the real screen at /screenshots/role-permission-screen.html;
 *   - reads the LIVE DOM: for all 40 rows it takes the words, the tag element's
 *     text and the sentence element's text, and builds a screen dump out of
 *     what is actually drawn;
 *   - proves each tag and each sentence has a real, non-zero box on the page and
 *     is part of document.body.innerText, so nothing counted here is hidden;
 *   - proves the three sentences are drawn in real pixels, by checking the
 *     screenshot under each sentence's box is not empty background;
 *   - runs the same TASK 4852 check over the DOM dump that
 *     check-role-permission-screen.mjs runs over the source render, and writes
 *     the dump out so the check can be re-run - and broken - on its own.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { cropFacts, imageFacts, parsePng } from "./png-facts.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-4852-role-permission-screen.png");
const TOP_PNG_PATH = path.join(OUTPUT_DIR, "task-4852-role-permission-legend.png");
const DUMP_PATH = path.join(OUTPUT_DIR, "task-4852-role-screen-dump.txt");
const WINDOW = { width: 1280, height: 1000 };
/** #0a0a0a, the app's --bg. */
const BACKGROUND = [10, 10, 10];
const SENTENCES = {
  KEY: "Not a rule. They do not have the key.",
  RELAY: "OSL's relay refuses it. It still cannot read what you write.",
  TRUST: "A modified app could ignore this. Everyone else's app will still hide it.",
};

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

/**
 * Everything below is read off the rendered page, never off the module: the
 * tag and sentence strings come from the elements' textContent, and their boxes
 * come from getBoundingClientRect.
 */
const READ_SCREEN = `(async () => {
  await document.fonts.ready;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const box = (element) => {
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const text = (element) => (element ? element.textContent.trim() : "");
  const screen = document.querySelector("[data-role-permission-screen]");
  const rows = [...document.querySelectorAll("[data-permission-row]")].map((row) => {
    const tag = row.querySelector(".role-permission-tag");
    const sentence = row.querySelector(".role-permission-sentence");
    return {
      words: text(row.querySelector(".role-permission-words")),
      tag: text(tag),
      sentence: text(sentence),
      tagBox: box(tag),
      sentenceBox: box(sentence),
      allowed: row.classList.contains("allowed"),
    };
  });
  const legend = [...document.querySelectorAll("[data-enforcement-legend]")].map((item) => ({
    tag: text(item.querySelector(".role-permission-tag")),
    sentence: text(item.querySelector(".role-permission-sentence")),
    box: box(item.querySelector(".role-permission-sentence")),
  }));
  return {
    title: document.title,
    heading: text(document.querySelector("#role-permission-heading")),
    roleName: screen ? screen.dataset.roleName : null,
    countLine: text(document.querySelector("[data-permission-count]")),
    sections: [...document.querySelectorAll("[data-permission-section]")].map((section) => section.dataset.permissionSection),
    rows,
    legend,
    pageHeight: document.documentElement.scrollHeight,
    visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

function buildDump(screen) {
  const lines = ["TASK4852 role screen dump", `role: ${screen.roleName ?? ""}`];
  for (const row of screen.rows) lines.push(`row: ${row.words} | ${row.tag} | ${row.sentence}`);
  for (const item of screen.legend) lines.push(`legend: ${item.tag} | ${item.sentence}`);
  return `${lines.join("\n")}\n`;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const { checkRolePermissionScreenDump, ENFORCEMENT_SENTENCES } = await vite.ssrLoadModule(
    "/src/role-permission-rows.ts",
  );
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/role-permission-screen.html`;
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
      while (document.body.dataset.rolePermissionFixture !== "ready") {
        if (Date.now() > deadline) throw new Error("fixture never finished rendering");
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      return true;
    })()`);

    const screen = await evaluate(page, READ_SCREEN);

    // ---- 1. 40 rows, and a tag beside every one of them.
    if (screen.rows.length !== 40) throw new Error(`expected 40 permission rows on screen, found ${screen.rows.length}`);
    const untagged = screen.rows.filter((row) => row.tag === "");
    if (untagged.length > 0) throw new Error(`rows with no tag: ${untagged.map((row) => row.words).join(", ")}`);
    const tagged = screen.rows.filter((row) => ["KEY", "RELAY", "TRUST"].includes(row.tag));
    if (tagged.length !== 40) throw new Error(`expected 40 enforcement tags, found ${tagged.length}`);

    // ---- 2. every tag and every sentence is really on the page, with a box.
    const hidden = screen.rows.filter((row) => {
      const boxes = [row.tagBox, row.sentenceBox];
      return boxes.some((rect) => !rect || rect.width < 1 || rect.height < 1);
    });
    if (hidden.length > 0) {
      throw new Error(`rows whose tag or sentence has no box on the page: ${hidden.map((row) => row.words).join(", ")}`);
    }
    const wrongSentence = screen.rows.filter((row) => row.sentence !== SENTENCES[row.tag]);
    if (wrongSentence.length > 0) {
      throw new Error(`rows not showing their class sentence: ${wrongSentence.map((row) => `${row.words} \`${row.tag}\``).join(", ")}`);
    }
    for (const [tag, sentence] of Object.entries(SENTENCES)) {
      if (!screen.visibleText.includes(sentence)) {
        throw new Error(`the ${tag} sentence is not visible text on the screen: "${sentence}"`);
      }
      if (ENFORCEMENT_SENTENCES[tag] !== sentence) {
        throw new Error(`the module's ${tag} sentence is "${ENFORCEMENT_SENTENCES[tag]}"`);
      }
    }

    // ---- 3. the same check the standalone screen check runs, over the live DOM.
    const dump = buildDump(screen);
    writeFileSync(DUMP_PATH, dump);
    const report = checkRolePermissionScreenDump(dump);

    // ---- 4. the pixels: the whole page, plus the legend at the top.
    const fullShot = await page.screenshot({ captureBeyondViewport: true });
    writeFileSync(PNG_PATH, fullShot);
    const topShot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(TOP_PNG_PATH, topShot);

    const topPng = parsePng(topShot);
    const legendCrops = screen.legend.map((item) => ({
      tag: item.tag,
      crop: cropFacts(topPng, item.box, { background: BACKGROUND }),
    }));
    const blankLegend = legendCrops.filter(({ crop }) => crop.nonBackground < 10);
    if (blankLegend.length > 0) {
      throw new Error(`legend sentences are blank in the image: ${blankLegend.map(({ tag }) => tag).join(", ")}`);
    }
    // The first row that fits inside the first viewport, proved to be drawn.
    const firstVisibleRow = screen.rows.find((row) => row.sentenceBox.y + row.sentenceBox.height < WINDOW.height);
    if (!firstVisibleRow) throw new Error("no permission row sits inside the first screenful");
    const rowCrop = cropFacts(topPng, firstVisibleRow.sentenceBox, { background: BACKGROUND });
    if (rowCrop.nonBackground < 10) {
      throw new Error(`the sentence on row "${firstVisibleRow.words}" is blank in the image`);
    }
    const facts = imageFacts(fullShot, {}, { background: BACKGROUND });
    if (facts.nearlyBlank) {
      throw new Error(`PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
    }

    const counts = { KEY: 0, RELAY: 0, TRUST: 0 };
    for (const row of screen.rows) counts[row.tag] += 1;

    console.log(`TASK4852_URL=${url}`);
    console.log(`TASK4852_TITLE=${screen.heading}`);
    console.log(`TASK4852_ROLE=${screen.roleName}`);
    console.log(`TASK4852_SECTIONS=${screen.sections.join("|")}`);
    console.log(`TASK4852_ROWS=${screen.rows.length}`);
    console.log(`TASK4852_TAGS=${tagged.length}`);
    console.log(`TASK4852_TAGS_WITH_A_BOX=${screen.rows.length - hidden.length}`);
    console.log(`TASK4852_ROW_SENTENCES=${screen.rows.filter((row) => row.sentence !== "").length}`);
    console.log(`TASK4852_KEY_ROWS=${counts.KEY}`);
    console.log(`TASK4852_RELAY_ROWS=${counts.RELAY}`);
    console.log(`TASK4852_TRUST_ROWS=${counts.TRUST}`);
    console.log(`TASK4852_KEY_SENTENCE=${SENTENCES.KEY}`);
    console.log(`TASK4852_RELAY_SENTENCE=${SENTENCES.RELAY}`);
    console.log(`TASK4852_TRUST_SENTENCE=${SENTENCES.TRUST}`);
    console.log(`TASK4852_LEGEND_SENTENCES=${report.legendSentences}`);
    console.log(`TASK4852_COUNT_LINE=${screen.countLine}`);
    console.log(`TASK4852_CHECK_ROWS=${report.rows}`);
    console.log(`TASK4852_CHECK_TAGS=${report.tags}`);
    console.log(`TASK4852_CHECK_SENTENCES=${report.sentences}`);
    console.log(`TASK4852_DUMP=${DUMP_PATH}`);
    console.log(`TASK4852_PNG=${PNG_PATH}`);
    console.log(`TASK4852_PNG_SIZE=${facts.width}x${facts.height}`);
    console.log(`TASK4852_PAGE_HEIGHT=${screen.pageHeight}`);
    console.log(`TASK4852_LEGEND_PNG=${TOP_PNG_PATH}`);
  } finally {
    await page.close();
    await chrome.close();
    await vite.close();
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
