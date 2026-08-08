#!/usr/bin/env node

// TASK 0748 -- capture the built verification warning screen on Linux.
//
// The screen is driven through its REAL controls here (a radio click, the reset
// control, the save control) rather than rendered from a hand-made state, so a
// screen whose controls do nothing cannot produce this image.

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
// pngFacts is the same PNG reader the connection-choice capture uses; a fourth
// copy of it in this folder would be a fourth thing to keep in step.
import { pngFacts } from "./capture-connection-choice.mjs";

export const FIXED_VIEWPORT = { width: 800, height: 620 };

export const CAPTURED_CHOICE = "before sending";
export const EXPECTED_EFFECT_TEXT = "OSL reminds you just before you send to someone you have not checked yet. Nothing interrupts your reading, and you still get a nudge before you speak.";
export const REQUIRED_NAMES = ["Verification warning", "every time", "once", "before sending", "never", "Save", "Reset"];
export const REQUIRED_IMAGE_TEXT = [
  "Verification warning",
  "every time",
  "once",
  "before sending",
  "never",
  "Save",
  "Reset",
  EXPECTED_EFFECT_TEXT,
];

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);
// `screenshots/*.png` is gitignored; the captures that are kept as evidence
// live in `screenshots/evidence/`, which is not.
const DEFAULT_OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-0748-verification-warning.png");

function parseArgs(args) {
  const outputIndex = args.indexOf("--output");
  return {
    output: outputIndex === -1 ? DEFAULT_OUTPUT : path.resolve(args[outputIndex + 1] ?? ""),
  };
}

function flattenAxTree(nodes) {
  return nodes
    .map((node) => node.name?.value)
    .filter((name) => typeof name === "string" && name.trim())
    .map((name) => name.trim());
}

function requireAllStrings(haystack, required, label) {
  const missing = required.filter((expected) => !haystack.includes(expected));
  if (missing.length) throw new Error(`${label} missing: ${missing.join(" | ")}`);
}

/** What the screen currently shows, read out of the live DOM. */
const READ_SCREEN = `(() => {
  const checked = [...document.querySelectorAll('input[name="verification-warning"]')].filter((input) => input.checked);
  const selectedCards = [...document.querySelectorAll(".vw-choice.selected")];
  return JSON.stringify({
    choices: [...document.querySelectorAll('input[name="verification-warning"]')].map((input) => input.value),
    selectedCount: checked.length,
    selected: checked.map((input) => input.value).join(","),
    selectedCardCount: selectedCards.length,
    effectCount: document.querySelectorAll("[data-vw-effect]").length,
    effect: document.querySelector("[data-vw-effect]")?.textContent?.trim() ?? "",
    status: document.querySelector("[data-vw-status]")?.textContent?.trim() ?? "",
    saveControls: document.querySelectorAll("[data-vw-save]").length,
    resetControls: document.querySelectorAll("[data-vw-reset]").length,
    savedCalls: window.oslVerificationWarningFixture.saves(),
    state: window.oslVerificationWarningFixture.state(),
  });
})()`;

const clickChoice = (choice) => `document.querySelector('input[value="${choice}"]').click(), "clicked"`;

export async function captureVerificationWarningScreen({ output = DEFAULT_OUTPUT } = {}) {
  const server = await createViteServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0, strictPort: false },
    logLevel: "error",
  });
  await server.listen();
  const address = server.httpServer.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP port");

  const chrome = await launchChrome();
  const page = await chrome.openPage();
  const readScreen = async () => JSON.parse(await page.evaluate(READ_SCREEN));
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...FIXED_VIEWPORT, deviceScaleFactor: 1, mobile: false });
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "reduce" }],
    });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/verification-warning-screen-fixture.html`);
    const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
    if (ready !== "verification-warning-screen") throw new Error("verification warning fixture did not become ready");

    const controlLog = [];
    const opened = await readScreen();
    if (opened.choices.join(",") !== "every time,once,before sending,never") {
      throw new Error(`expected the four saved choices, found: ${opened.choices.join(",")}`);
    }
    controlLog.push(`opened selected=${opened.selected} status=${opened.status}`);

    // Reset control: move off the default, then put it back.
    await page.evaluate(clickChoice("never"));
    const afterNever = await readScreen();
    if (afterNever.selected !== "never") throw new Error(`choice control did not select never: ${afterNever.selected}`);
    await page.evaluate(`document.querySelector("[data-vw-reset]").click(), "clicked"`);
    const afterReset = await readScreen();
    if (afterReset.selected !== "every time") throw new Error(`reset control did not restore the default: ${afterReset.selected}`);
    controlLog.push(`reset never -> ${afterReset.selected}`);

    // Save control: pick the captured choice and commit it.
    await page.evaluate(clickChoice(CAPTURED_CHOICE));
    const beforeSave = await readScreen();
    if (beforeSave.status !== `Not saved yet. Saved choice is still every time.`) {
      throw new Error(`unsaved status is wrong: ${beforeSave.status}`);
    }
    await page.evaluate(`document.querySelector("[data-vw-save]").click(), "clicked"`);
    const screen = await readScreen();
    if (screen.savedCalls.join(",") !== CAPTURED_CHOICE) {
      throw new Error(`save control did not report the choice: ${JSON.stringify(screen.savedCalls)}`);
    }
    controlLog.push(`save ${CAPTURED_CHOICE} -> ${screen.status}`);

    if (screen.selectedCount !== 1 || screen.selectedCardCount !== 1) {
      throw new Error(`expected exactly one selected choice, found ${screen.selectedCount} inputs / ${screen.selectedCardCount} cards`);
    }
    if (screen.selected !== CAPTURED_CHOICE) throw new Error(`expected ${CAPTURED_CHOICE} selected, found ${screen.selected}`);
    if (screen.effectCount !== 1) throw new Error(`expected exactly one effect line, found ${screen.effectCount}`);
    if (screen.effect !== EXPECTED_EFFECT_TEXT) throw new Error(`effect text is wrong: ${screen.effect}`);
    if (screen.saveControls !== 1 || screen.resetControls !== 1) {
      throw new Error(`expected one save and one reset control, found ${screen.saveControls}/${screen.resetControls}`);
    }
    await new Promise((resolve) => setTimeout(resolve, 250));

    const imageText = await page.evaluate(`document.body.innerText.replace(/\\s+/g, " ").trim()`);
    requireAllStrings(imageText, REQUIRED_IMAGE_TEXT, "visible image text");

    const axTree = await page.send("Accessibility.getFullAXTree");
    const screenTree = flattenAxTree(axTree.nodes);
    requireAllStrings(screenTree, REQUIRED_NAMES, "screen tree");

    const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
    const facts = pngFacts(png);
    if (facts.width !== FIXED_VIEWPORT.width || facts.height !== FIXED_VIEWPORT.height) {
      throw new Error(`expected ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height}, captured ${facts.width}x${facts.height}`);
    }
    if (facts.bytes < 10_000 || facts.distinctColors < 50) {
      throw new Error(`PNG is blank or nearly blank: bytes=${facts.bytes} distinctColors=${facts.distinctColors}`);
    }

    mkdirSync(path.dirname(output), { recursive: true });
    writeFileSync(output, png);
    return {
      screenshot: output,
      viewport: FIXED_VIEWPORT,
      selectedChoice: screen.selected,
      selectedChoiceCount: screen.selectedCount,
      effectText: screen.effect,
      effectLineCount: screen.effectCount,
      savedStatus: screen.status,
      controlLog,
      screenTreeRequiredNames: Object.fromEntries(REQUIRED_NAMES.map((name) => [name, screenTree.filter((value) => value === name).length])),
      visibleTextRequiredStrings: Object.fromEntries(REQUIRED_IMAGE_TEXT.map((name) => [name, imageText.includes(name) ? 1 : 0])),
      png: facts,
    };
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureVerificationWarningScreen(parseArgs(process.argv.slice(2))).then((result) => {
    if (!existsSync(result.screenshot)) throw new Error("screenshot file was not written");
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-verification-warning-screen: ${error.stack || error.message}`);
    process.exit(1);
  });
}
