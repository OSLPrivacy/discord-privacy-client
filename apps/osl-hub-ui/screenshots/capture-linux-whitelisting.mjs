#!/usr/bin/env node
/**
 * TASK0764 — Linux screenshot of the Whitelisting screen.
 *
 * The finish line names two elements the image has to show, and one thing it
 * has to not be. This script refuses unless all three hold:
 *
 *   1. MIXED SELECTED CONVERSATIONS — the list on screen holds at least one
 *      ticked conversation and at least one unticked one, the tick state in the DOM
 *      matches the checkbox's live `checked` property, and the ticked and
 *      unticked boxes are painted differently in the PNG. A list that renders
 *      all-on or all-off, or that only differs in a data attribute, fails here.
 *   2. A SEARCH RESULT — the search box holds a query, the result line naming
 *      the query and the counts is in the page text, in Chrome's accessibility
 *      tree and painted in the PNG, and the list really is shorter than the
 *      whole list (3 rows of 8), so an unfiltered list cannot pass for a search.
 *   3. IT DIFFERS FROM THE EMPTY-STATE CAPTURE — the same screen with nothing
 *      in it is captured from the same fixture at the same size, and the two
 *      PNGs must differ by hash and by a real share of their pixels.
 *
 * All four controls the screen is built around (Select all, Clear all, Save,
 * Reset) are also checked as live, painted, readable buttons.
 */

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { inkFacts, meanDistance, parsePng, pixelDifference } from "./lib/png.mjs";
import { whitelistingMixedExpectation, whitelistingScenes } from "./whitelisting-scenes.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const MIXED_PNG = path.join(OUTPUT_DIR, "task-0764-linux-whitelisting.png");
const EMPTY_PNG = path.join(OUTPUT_DIR, "task-0764-linux-whitelisting-empty.png");
const WINDOW = { width: 1200, height: 800 };
const MIN_INK = 20;

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed",
    );
  }
  return result.result.value;
}

const READ_SCREEN = `(async () => {
  const deadline = Date.now() + 20000;
  while (document.documentElement.dataset.fixtureReady !== "whitelisting" && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (document.documentElement.dataset.fixtureReady !== "whitelisting") throw new Error("fixture never became ready");
  await document.fonts.ready;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

  const box = (node) => {
    const rect = node.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const rows = [...document.querySelectorAll("[data-whitelisting-row]")].map((row) => {
    const tick = row.querySelector("[data-whitelisting-conversation]");
    return {
      id: row.dataset.whitelistingRow,
      allowedAttribute: row.dataset.whitelistingAllowed === "true",
      // The live property, not the attribute: this is what the browser drew.
      checkedProperty: tick ? tick.checked === true : null,
      name: row.querySelector(".whitelisting-row-text strong")?.textContent?.trim() ?? "",
      account: row.querySelector(".whitelisting-row-text small")?.textContent?.trim() ?? "",
      state: row.querySelector("[data-whitelisting-row-state]")?.textContent?.trim() ?? "",
      rowBox: box(row),
      tickBox: tick ? box(tick) : null,
    };
  });
  const control = (attribute, label) => {
    const button = document.querySelector("[" + attribute + "]");
    if (!button) return null;
    return { label, text: button.textContent.trim(), disabled: button.disabled === true, box: box(button) };
  };
  const search = document.querySelector("#whitelisting-search");
  const result = document.querySelector("[data-whitelisting-result]");
  const unsaved = document.querySelector("[data-whitelisting-unsaved]");
  const title = document.querySelector("#whitelisting-title");
  return {
    scene: document.documentElement.dataset.fixtureScene,
    rows,
    title: title ? { text: title.textContent.trim(), box: box(title) } : null,
    search: search ? { value: search.value, disabled: search.disabled === true, box: box(search) } : null,
    result: result ? { text: result.textContent.trim(), box: box(result) } : null,
    unsaved: unsaved ? { text: unsaved.textContent.trim(), box: box(unsaved) } : null,
    emptyState: document.querySelector("[data-whitelisting-empty]")?.textContent?.trim() ?? null,
    controls: [
      control("data-whitelisting-select-all", "Select all"),
      control("data-whitelisting-clear-all", "Clear all"),
      control("data-whitelisting-save", "Save"),
      control("data-whitelisting-reset", "Reset"),
    ],
    visibleText: document.body.innerText,
  };
})()`;

async function captureScene(chrome, url, scene, pngPath) {
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(`${url}?scene=${scene}`, { timeoutMs: 30_000 });
    const rendered = await evaluate(page, READ_SCREEN);
    if (rendered.scene !== scene) throw new Error(`fixture rendered scene "${rendered.scene}", asked for "${scene}"`);

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axText = ax.nodes
      .flatMap((node) => [node.name?.value, node.value?.value])
      .filter((value) => typeof value === "string")
      .join("\n");

    const bytes = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(pngPath, bytes);
    const png = parsePng(bytes);
    if (png.width !== WINDOW.width || png.height !== WINDOW.height) {
      throw new Error(`${scene}: unexpected PNG size ${png.width}x${png.height}`);
    }
    return { rendered, axText, bytes, png };
  } finally {
    await page.close().catch(() => {});
  }
}

function requirePainted(png, label, box) {
  const fact = inkFacts(png, box);
  if (!fact.insidePng) throw new Error(`${label} falls outside the screenshot`);
  if (fact.ink < MIN_INK) throw new Error(`${label} is blank in the PNG (ink=${fact.ink})`);
  return fact;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/whitelisting-fixture.html`;
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
  try {
    const empty = await captureScene(chrome, url, "empty", EMPTY_PNG);
    const mixed = await captureScene(chrome, url, "mixed", MIXED_PNG);
    const want = whitelistingMixedExpectation;

    // ---- the empty-state capture is genuinely empty -------------------------
    if (empty.rendered.rows.length !== 0) {
      throw new Error(`the empty-state capture drew ${empty.rendered.rows.length} conversations`);
    }
    if (!empty.rendered.emptyState) throw new Error("the empty-state capture has no empty-state block");
    requirePainted(empty.png, "empty-state title", empty.rendered.title.box);

    // ---- element 1: mixed selected conversations ----------------------------
    const rows = mixed.rendered.rows;
    if (rows.length !== want.matchCount) {
      throw new Error(`expected ${want.matchCount} conversations on screen, saw ${rows.length}`);
    }
    const mismatched = rows.filter((row) => row.allowedAttribute !== row.checkedProperty).map((row) => row.id);
    if (mismatched.length > 0) throw new Error(`tick state disagrees with the checkbox: ${mismatched.join(", ")}`);
    const allowed = rows.filter((row) => row.checkedProperty);
    const cleared = rows.filter((row) => !row.checkedProperty);
    if (allowed.length === 0 || cleared.length === 0) {
      throw new Error(`the list is not mixed: ${allowed.length} ticked, ${cleared.length} unticked`);
    }
    const allowedIds = allowed.map((row) => row.id).sort();
    const clearedIds = cleared.map((row) => row.id).sort();
    if (allowedIds.join(",") !== [...want.allowedMatches].sort().join(",")) {
      throw new Error(`ticked conversations are ${allowedIds.join(",")}, expected ${want.allowedMatches.join(",")}`);
    }
    if (clearedIds.join(",") !== [...want.clearedMatches].sort().join(",")) {
      throw new Error(`unticked conversations are ${clearedIds.join(",")}, expected ${want.clearedMatches.join(",")}`);
    }
    for (const row of rows) {
      const expectedState = row.checkedProperty ? "Allowed" : "Not allowed";
      if (row.state !== expectedState) throw new Error(`${row.id} is ticked=${row.checkedProperty} but reads "${row.state}"`);
      if (!mixed.rendered.visibleText.includes(row.name)) throw new Error(`${row.id} name is not in the page text`);
      if (!mixed.axText.includes(row.name)) throw new Error(`${row.id} name is not in the accessibility tree`);
      requirePainted(mixed.png, `row ${row.id}`, row.rowBox);
    }
    // The two tick states have to look different, or "mixed" is invisible.
    const tickFacts = new Map(rows.map((row) => [row.id, inkFacts(mixed.png, row.tickBox)]));
    let minTickDistance = Number.POSITIVE_INFINITY;
    for (const on of allowed) {
      for (const off of cleared) {
        const distance = meanDistance(tickFacts.get(on.id).mean, tickFacts.get(off.id).mean);
        minTickDistance = Math.min(minTickDistance, distance);
      }
    }
    if (minTickDistance < 24) {
      throw new Error(`ticked and unticked boxes are painted the same (closest mean distance ${minTickDistance})`);
    }

    // ---- element 2: a search result ----------------------------------------
    const search = mixed.rendered.search;
    if (!search || search.value !== whitelistingScenes.mixed.search) {
      throw new Error(`the search box holds "${search?.value ?? ""}", expected "${whitelistingScenes.mixed.search}"`);
    }
    if (rows.length >= want.totalCount) throw new Error("the list was not filtered by the search");
    const offQuery = rows.filter((row) => !`${row.name} ${row.account}`.toLowerCase().includes(search.value.toLowerCase()));
    if (offQuery.length > 0) throw new Error(`rows on screen do not match the search: ${offQuery.map((row) => row.id).join(", ")}`);
    if (mixed.rendered.result?.text !== want.resultLine) {
      throw new Error(`the result line reads "${mixed.rendered.result?.text ?? ""}", expected "${want.resultLine}"`);
    }
    if (!mixed.rendered.visibleText.includes(want.resultLine)) throw new Error("the result line is not in the page text");
    if (!mixed.axText.includes(want.resultLine)) throw new Error("the result line is not in the accessibility tree");
    requirePainted(mixed.png, "search result line", mixed.rendered.result.box);
    requirePainted(mixed.png, "search box", search.box);

    // ---- the six controls the screen is built around ------------------------
    const missingControls = want.controls.filter((label) =>
      !mixed.rendered.controls.some((control) => control && control.text === label)
    );
    if (missingControls.length > 0) throw new Error(`controls missing from the screen: ${missingControls.join(", ")}`);
    for (const control of mixed.rendered.controls) {
      if (control.disabled) throw new Error(`the ${control.label} control is greyed out in the capture`);
      if (!mixed.axText.includes(control.label)) throw new Error(`${control.label} is not in the accessibility tree`);
      requirePainted(mixed.png, `${control.label} control`, control.box);
    }
    const unsavedLine = `${want.unsavedChangeCount} change is not saved yet.`;
    if (mixed.rendered.unsaved?.text !== unsavedLine) {
      throw new Error(`the unsaved line reads "${mixed.rendered.unsaved?.text ?? ""}", expected "${unsavedLine}"`);
    }

    // ---- element 3: it differs from the empty-state capture ------------------
    const mixedHash = sha256(mixed.bytes);
    const emptyHash = sha256(empty.bytes);
    if (mixedHash === emptyHash) throw new Error("the Whitelisting capture is byte-identical to the empty-state capture");
    const diff = pixelDifference(empty.png, mixed.png);
    if (diff.differing === 0) throw new Error("the two captures paint identical pixels");
    if (diff.percent < 1) throw new Error(`the two captures differ by only ${diff.percent}% of their pixels`);

    console.log(`TASK0764_URL=${url}`);
    console.log(`TASK0764_PLATFORM=${process.platform}`);
    console.log(`TASK0764_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0764_TITLE=${mixed.rendered.title.text}`);
    console.log(`TASK0764_PNG=${MIXED_PNG}`);
    console.log(`TASK0764_PNG_SHA256=${mixedHash}`);
    console.log(`TASK0764_EMPTY_PNG=${EMPTY_PNG}`);
    console.log(`TASK0764_EMPTY_PNG_SHA256=${emptyHash}`);
    console.log(`TASK0764_EMPTY_ROWS=${empty.rendered.rows.length}`);
    console.log(`TASK0764_EMPTY_STATE_TEXT=${empty.rendered.emptyState}`);
    console.log(`TASK0764_ROWS_ON_SCREEN=${rows.length}`);
    console.log(`TASK0764_TOTAL_CONVERSATIONS=${want.totalCount}`);
    console.log(`TASK0764_TICKED=${allowed.length}`);
    console.log(`TASK0764_UNTICKED=${cleared.length}`);
    console.log(`TASK0764_MIXED=${allowed.length > 0 && cleared.length > 0}`);
    for (const row of rows) {
      const fact = tickFacts.get(row.id);
      console.log(
        `TASK0764_ROW_${row.id.toUpperCase().replace(/-/g, "_")}=${row.name} | ${row.account} | ${row.state}`
        + ` | checked=${row.checkedProperty} | tick_mean=${fact.mean.join(",")}`,
      );
    }
    console.log(`TASK0764_TICK_MEAN_DISTANCE=${minTickDistance}`);
    console.log(`TASK0764_SEARCH_QUERY=${search.value}`);
    console.log(`TASK0764_SEARCH_RESULT_LINE=${mixed.rendered.result.text}`);
    console.log(`TASK0764_SEARCH_RESULT_INK=${inkFacts(mixed.png, mixed.rendered.result.box).ink}`);
    console.log(`TASK0764_UNSAVED_LINE=${mixed.rendered.unsaved.text}`);
    for (const control of mixed.rendered.controls) {
      console.log(`TASK0764_CONTROL_${control.label.toUpperCase().replace(/ /g, "_")}=enabled ink=${inkFacts(mixed.png, control.box).ink}`);
    }
    console.log(`TASK0764_DIFF_PIXELS=${diff.differing}`);
    console.log(`TASK0764_DIFF_TOTAL=${diff.total}`);
    console.log(`TASK0764_DIFF_PERCENT=${diff.percent}`);
    console.log(`TASK0764_DIFFERS_FROM_EMPTY=${mixedHash !== emptyHash && diff.differing > 0}`);
    console.log("TASK0764_RESULT=PASS");
  } finally {
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-linux-whitelisting: ${error.stack || error.message}`);
  console.log("TASK0764_RESULT=FAIL");
  process.exitCode = 1;
});
