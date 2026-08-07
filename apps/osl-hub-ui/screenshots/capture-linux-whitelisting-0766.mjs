#!/usr/bin/env node
/**
 * TASK0766 — Linux screenshot of the Whitelisting screen, two apps, filtered.
 *
 * The finish line: the image shows the title "Whitelisting" and every app
 * account, conversation, filter, rule, save and reset control named on the
 * page, in its screen tree and in the image; and it is not blank or nearly
 * blank. This script refuses unless every one of those holds:
 *
 *   - TITLE — the heading reads exactly "Whitelisting", is a heading in
 *     Chrome's accessibility tree, and is painted in the PNG.
 *   - TWO APPS — the rows on screen come from exactly two app accounts
 *     (Discord and Signal). Each account line is in the page text, in the
 *     accessibility tree, and painted.
 *   - FILTERED CONVERSATIONS — the search box holds a query, fewer rows are
 *     on screen than exist, and every row on screen matches the query. Each
 *     conversation's name is in the page text, the accessibility tree, and
 *     painted.
 *   - FILTER control — the search input (labelled "Search conversations") and
 *     its result line, both in the accessibility tree and painted.
 *   - RULES — the per-row rule ("Allowed" / "Not allowed") painted on every
 *     row, and the two rule sentences the page names (the header rule and the
 *     Save/Reset note), in the accessibility tree and painted.
 *   - SAVE and RESET controls — live buttons (with Select all and Clear all),
 *     each in the accessibility tree and painted.
 *   - NOT BLANK — the whole 1200x800 image must carry a real share of
 *     non-background ink and more than a handful of distinct colours.
 */

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { inkFacts, parsePng } from "./lib/png.mjs";
import { whitelistingScenes } from "./whitelisting-scenes.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-0766-linux-whitelisting.png");
const WINDOW = { width: 1200, height: 800 };
const MIN_INK = 20;
const MIN_WHOLE_IMAGE_INK_PERCENT = 2;

/** The two rule sentences the page names. */
const HEADER_RULE = "Tick the conversations OSL may protect. OSL never touches a conversation that is not ticked here.";
const ACTIONS_RULE = "Save writes these ticks to this device. Reset undoes unsaved ticks and puts the saved list back.";

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
    const name = row.querySelector(".whitelisting-row-text strong");
    const account = row.querySelector(".whitelisting-row-text small");
    const state = row.querySelector("[data-whitelisting-row-state]");
    return {
      id: row.dataset.whitelistingRow,
      checkedProperty: tick ? tick.checked === true : null,
      name: name?.textContent?.trim() ?? "",
      nameBox: name ? box(name) : null,
      account: account?.textContent?.trim() ?? "",
      accountBox: account ? box(account) : null,
      state: state?.textContent?.trim() ?? "",
      stateBox: state ? box(state) : null,
      rowBox: box(row),
    };
  });
  const control = (attribute, label) => {
    const button = document.querySelector("[" + attribute + "]");
    if (!button) return null;
    return { label, text: button.textContent.trim(), disabled: button.disabled === true, box: box(button) };
  };
  const search = document.querySelector("#whitelisting-search");
  const searchLabel = document.querySelector('label[for="whitelisting-search"]');
  const result = document.querySelector("[data-whitelisting-result]");
  const title = document.querySelector("#whitelisting-title");
  const headerRule = document.querySelector(".whitelisting-screen > header p");
  const actionsRule = document.querySelector(".whitelisting-actions-note");
  return {
    scene: document.documentElement.dataset.fixtureScene,
    rows,
    title: title ? { text: title.textContent.trim(), box: box(title) } : null,
    search: search ? { value: search.value, disabled: search.disabled === true, box: box(search) } : null,
    searchLabel: searchLabel ? { text: searchLabel.textContent.trim(), box: box(searchLabel) } : null,
    result: result ? { text: result.textContent.trim(), box: box(result) } : null,
    headerRule: headerRule ? { text: headerRule.textContent.trim(), box: box(headerRule) } : null,
    actionsRule: actionsRule ? { text: actionsRule.textContent.trim(), box: box(actionsRule) } : null,
    controls: [
      control("data-whitelisting-select-all", "Select all"),
      control("data-whitelisting-clear-all", "Clear all"),
      control("data-whitelisting-save", "Save"),
      control("data-whitelisting-reset", "Reset"),
    ],
    visibleText: document.body.innerText,
  };
})()`;

/** Fails unless the box is inside the PNG and carries real ink. */
function requirePainted(png, label, rect) {
  if (!rect) throw new Error(`${label} is not on the page, so it has no box to paint`);
  const fact = inkFacts(png, rect);
  if (!fact.insidePng) throw new Error(`${label} falls outside the screenshot`);
  if (fact.ink < MIN_INK) throw new Error(`${label} is blank in the PNG (ink=${fact.ink})`);
  return fact;
}

/** Fails unless the text is in the page and in the accessibility tree. */
function requireNamed(rendered, axText, label, text) {
  if (!text) throw new Error(`${label} has no text`);
  if (!rendered.visibleText.includes(text)) throw new Error(`${label} ("${text}") is not in the page text`);
  if (!axText.includes(text)) throw new Error(`${label} ("${text}") is not in the accessibility tree`);
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
    const page = await chrome.openPage();
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(`${url}?scene=mixed`, { timeoutMs: 30_000 });
    const rendered = await evaluate(page, READ_SCREEN);
    if (rendered.scene !== "mixed") throw new Error(`fixture rendered scene "${rendered.scene}", asked for "mixed"`);

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axText = ax.nodes
      .flatMap((node) => [node.name?.value, node.value?.value])
      .filter((value) => typeof value === "string")
      .join("\n");
    const axHeadings = ax.nodes
      .filter((node) => node.role?.value === "heading")
      .map((node) => node.name?.value)
      .filter((value) => typeof value === "string");

    const bytes = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, bytes);
    const png = parsePng(bytes);

    // ---- fixed size ---------------------------------------------------------
    if (png.width !== WINDOW.width || png.height !== WINDOW.height) {
      throw new Error(`unexpected PNG size ${png.width}x${png.height}, wanted ${WINDOW.width}x${WINDOW.height}`);
    }

    // ---- title "Whitelisting" ----------------------------------------------
    if (rendered.title?.text !== "Whitelisting") {
      throw new Error(`the title reads "${rendered.title?.text ?? ""}", expected "Whitelisting"`);
    }
    if (!axHeadings.includes("Whitelisting")) {
      throw new Error(`"Whitelisting" is not a heading in the accessibility tree (headings: ${axHeadings.join(" | ")})`);
    }
    const titleInk = requirePainted(png, 'title "Whitelisting"', rendered.title.box);

    // ---- two apps -----------------------------------------------------------
    const rows = rendered.rows;
    if (rows.length === 0) throw new Error("no conversations are on screen");
    const accounts = [...new Set(rows.map((row) => row.account))].sort();
    const apps = [...new Set(accounts.map((account) => account.split(" · ")[0]))].sort();
    if (apps.length !== 2) {
      throw new Error(`expected conversations from exactly 2 apps, saw ${apps.length}: ${apps.join(" | ")}`);
    }
    const accountInk = new Map();
    for (const account of accounts) {
      requireNamed(rendered, axText, "app account", account);
      const carrier = rows.find((row) => row.account === account);
      accountInk.set(account, requirePainted(png, `app account "${account}"`, carrier.accountBox));
    }

    // ---- filtered conversations --------------------------------------------
    const scene = whitelistingScenes.mixed;
    const query = rendered.search?.value ?? "";
    if (!query.trim()) throw new Error("the search box is empty, so the conversations are not filtered");
    if (query !== scene.search) throw new Error(`the search box holds "${query}", expected "${scene.search}"`);
    if (rows.length >= scene.conversations.length) {
      throw new Error(`the list is not filtered: ${rows.length} rows on screen of ${scene.conversations.length}`);
    }
    const offQuery = rows.filter((row) => !`${row.name} ${row.account}`.toLowerCase().includes(query.toLowerCase()));
    if (offQuery.length > 0) {
      throw new Error(`rows on screen do not match the filter: ${offQuery.map((row) => row.id).join(", ")}`);
    }
    const rowFacts = [];
    for (const row of rows) {
      requireNamed(rendered, axText, `conversation ${row.id}`, row.name);
      const nameInk = requirePainted(png, `conversation "${row.name}"`, row.nameBox);
      // ---- the per-row rule -------------------------------------------------
      const expectedState = row.checkedProperty ? "Allowed" : "Not allowed";
      if (row.state !== expectedState) {
        throw new Error(`${row.id} is ticked=${row.checkedProperty} but its rule reads "${row.state}"`);
      }
      const stateInk = requirePainted(png, `rule "${row.state}" on ${row.id}`, row.stateBox);
      rowFacts.push({ row, nameInk, stateInk });
    }
    if (!axText.includes("Allowed") || !axText.includes("Not allowed")) {
      throw new Error("the per-row rules (Allowed / Not allowed) are not in the accessibility tree");
    }

    // ---- the filter control -------------------------------------------------
    if (rendered.search.disabled) throw new Error("the filter (search box) is greyed out");
    if (rendered.searchLabel?.text !== "Search conversations") {
      throw new Error(`the filter label reads "${rendered.searchLabel?.text ?? ""}", expected "Search conversations"`);
    }
    requireNamed(rendered, axText, "filter label", "Search conversations");
    const filterInk = requirePainted(png, "filter (search box)", rendered.search.box);
    if (!rendered.result?.text) throw new Error("the filter has no result line");
    requireNamed(rendered, axText, "filter result line", rendered.result.text);
    const resultInk = requirePainted(png, "filter result line", rendered.result.box);

    // ---- the named rule sentences ------------------------------------------
    if (rendered.headerRule?.text !== HEADER_RULE) {
      throw new Error(`the header rule reads "${rendered.headerRule?.text ?? ""}", expected "${HEADER_RULE}"`);
    }
    requireNamed(rendered, axText, "header rule", HEADER_RULE);
    const headerRuleInk = requirePainted(png, "header rule", rendered.headerRule.box);
    if (rendered.actionsRule?.text !== ACTIONS_RULE) {
      throw new Error(`the Save/Reset rule reads "${rendered.actionsRule?.text ?? ""}", expected "${ACTIONS_RULE}"`);
    }
    requireNamed(rendered, axText, "Save/Reset rule", ACTIONS_RULE);
    const actionsRuleInk = requirePainted(png, "Save/Reset rule", rendered.actionsRule.box);

    // ---- save and reset (and the other two buttons the page names) ----------
    const controlFacts = [];
    for (const label of ["Select all", "Clear all", "Save", "Reset"]) {
      const found = rendered.controls.find((entry) => entry && entry.text === label);
      if (!found) throw new Error(`the ${label} control is missing from the screen`);
      if (found.disabled) throw new Error(`the ${label} control is greyed out in the capture`);
      requireNamed(rendered, axText, `${label} control`, label);
      controlFacts.push({ label, ink: requirePainted(png, `${label} control`, found.box) });
    }

    // ---- not blank or nearly blank -----------------------------------------
    const whole = inkFacts(png, { x: 0, y: 0, width: png.width, height: png.height });
    const wholeInkPercent = Number(((whole.ink / (png.width * png.height)) * 100).toFixed(2));
    if (whole.distinctColors < 16) {
      throw new Error(`the image is nearly blank: only ${whole.distinctColors} distinct colours`);
    }
    if (wholeInkPercent < MIN_WHOLE_IMAGE_INK_PERCENT) {
      throw new Error(`the image is nearly blank: ${wholeInkPercent}% non-background ink`);
    }

    console.log(`TASK0766_URL=${url}`);
    console.log(`TASK0766_PLATFORM=${process.platform}`);
    console.log(`TASK0766_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0766_PNG=${PNG_PATH}`);
    console.log(`TASK0766_PNG_SHA256=${sha256(bytes)}`);
    console.log(`TASK0766_TITLE=${rendered.title.text} ink=${titleInk.ink} ax_heading=true`);
    console.log(`TASK0766_APPS=${apps.join(" | ")}`);
    for (const account of accounts) {
      console.log(`TASK0766_APP_ACCOUNT=${account} ink=${accountInk.get(account).ink}`);
    }
    console.log(`TASK0766_FILTER_QUERY=${query}`);
    console.log(`TASK0766_FILTERED=${rows.length} of ${scene.conversations.length} conversations on screen`);
    console.log(`TASK0766_FILTER_BOX_INK=${filterInk.ink}`);
    console.log(`TASK0766_FILTER_RESULT=${rendered.result.text} ink=${resultInk.ink}`);
    for (const { row, nameInk, stateInk } of rowFacts) {
      console.log(
        `TASK0766_CONVERSATION_${row.id.toUpperCase().replace(/-/g, "_")}=${row.name}`
        + ` | ${row.account} | rule=${row.state} | name_ink=${nameInk.ink} | rule_ink=${stateInk.ink}`,
      );
    }
    console.log(`TASK0766_RULE_HEADER=${HEADER_RULE} ink=${headerRuleInk.ink}`);
    console.log(`TASK0766_RULE_ACTIONS=${ACTIONS_RULE} ink=${actionsRuleInk.ink}`);
    for (const { label, ink } of controlFacts) {
      console.log(`TASK0766_CONTROL_${label.toUpperCase().replace(/ /g, "_")}=enabled ink=${ink.ink}`);
    }
    console.log(`TASK0766_WHOLE_IMAGE_INK=${whole.ink} of ${png.width * png.height} pixels (${wholeInkPercent}%)`);
    console.log(`TASK0766_WHOLE_IMAGE_DISTINCT_COLORS=${whole.distinctColors}`);
    console.log(`TASK0766_NOT_BLANK=${wholeInkPercent >= MIN_WHOLE_IMAGE_INK_PERCENT}`);
    console.log("TASK0766_RESULT=PASS");
    await page.close().catch(() => {});
  } finally {
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-linux-whitelisting-0766: ${error.stack || error.message}`);
  console.log("TASK0766_RESULT=FAIL");
  process.exitCode = 1;
});
