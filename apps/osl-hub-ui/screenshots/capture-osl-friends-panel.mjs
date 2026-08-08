#!/usr/bin/env node

// TASK 0834 -- capture the built OSL Friends panel on Linux.
//
// The panel is rendered with the two standard test friends: Ada Friend (with permitted picture)
// and Cleo Friend (without permitted picture). The screenshot shows both friend rows with their
// names, avatars (picture or coloured initial), and the page link.

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
// pngFacts is the same PNG reader the connection-choice capture uses
import { pngFacts } from "./capture-connection-choice.mjs";

export const FIXED_VIEWPORT = { width: 800, height: 600 };

export const REQUIRED_NAMES = ["OSL Friends", "Ada Friend", "Cleo Friend"];
export const REQUIRED_IMAGE_TEXT = [
  "OSL Friends",
  "Ada Friend",
  "Cleo Friend",
  "Home",
];

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);
// `screenshots/*.png` is gitignored; the captures that are kept as evidence
// live in `screenshots/evidence/`, which is not.
const DEFAULT_OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-0834-osl-friends-panel.png");

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

export async function captureOslFriendsPanelScreen({ output = DEFAULT_OUTPUT } = {}) {
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
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...FIXED_VIEWPORT, deviceScaleFactor: 1, mobile: false });
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "reduce" }],
    });
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/osl-friends-panel-fixture.html`);
    // Wait a bit for the module to load
    await new Promise((resolve) => setTimeout(resolve, 1000));
    const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
    if (ready !== "osl-friends-panel") {
      const error = await page.evaluate(`document.documentElement.dataset.fixtureError || window.__fixtureError__ || "unknown"`);
      throw new Error(`OSL Friends panel fixture did not become ready: ${error}`);
    }

    // Verify the friends are loaded
    const friends = await page.evaluate(`window.oslFriendsPanelFixture.friends()`);
    if (!Array.isArray(friends) || friends.length !== 2) {
      throw new Error(`expected 2 friends, found ${friends.length}`);
    }

    // Verify the panel title is present
    const title = await page.evaluate(`document.querySelector(".osl-friends-panel h2")?.textContent?.trim()`);
    if (title !== "OSL Friends") {
      throw new Error(`expected title "OSL Friends", found "${title}"`);
    }

    // Verify both friend rows are present
    const rows = await page.evaluate(`document.querySelectorAll(".osl-friend-row").length`);
    if (rows !== 2) {
      throw new Error(`expected 2 friend rows, found ${rows}`);
    }

    // Verify friend names in rows
    const adaName = await page.evaluate(`
      [...document.querySelectorAll(".osl-friend-row")].find(row =>
        row.querySelector(".osl-friend-name")?.textContent?.trim() === "Ada Friend"
      ) ? "found" : "not-found"
    `);
    if (adaName !== "found") throw new Error("Ada Friend row not found");

    const cleoName = await page.evaluate(`
      [...document.querySelectorAll(".osl-friend-row")].find(row =>
        row.querySelector(".osl-friend-name")?.textContent?.trim() === "Cleo Friend"
      ) ? "found" : "not-found"
    `);
    if (cleoName !== "found") throw new Error("Cleo Friend row not found");

    // Verify Ada has a picture (image element)
    const adaHasPicture = await page.evaluate(`
      [...document.querySelectorAll(".osl-friend-row")].some(row => {
        const name = row.querySelector(".osl-friend-name")?.textContent?.trim();
        return name === "Ada Friend" && row.querySelector("img.osl-friend-picture") !== null;
      })
    `);
    if (!adaHasPicture) throw new Error("Ada Friend should have a picture");

    // Verify Cleo has an initial (no picture)
    const cleoHasInitial = await page.evaluate(`
      [...document.querySelectorAll(".osl-friend-row")].some(row => {
        const name = row.querySelector(".osl-friend-name")?.textContent?.trim();
        return name === "Cleo Friend" && row.querySelector("span.osl-friend-initial") !== null;
      })
    `);
    if (!cleoHasInitial) throw new Error("Cleo Friend should have an initial");

    // Verify back button
    const backButton = await page.evaluate(`document.querySelector("[data-osl-friends-back]")?.textContent?.trim()`);
    if (!backButton?.includes("Home")) throw new Error(`expected back button with "Home", found "${backButton}"`);

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
    console.log(`Screenshot: bytes=${facts.bytes} distinctColors=${facts.distinctColors}`);
    if (facts.bytes < 5_000 || facts.distinctColors < 10) {
      throw new Error(`PNG is blank or nearly blank: bytes=${facts.bytes} distinctColors=${facts.distinctColors}`);
    }

    mkdirSync(path.dirname(output), { recursive: true });
    writeFileSync(output, png);
    return {
      screenshot: output,
      viewport: FIXED_VIEWPORT,
      friendsLoaded: friends.length,
      friendNames: friends.map((f) => f.username).join("|"),
      titleText: title,
      backButtonText: backButton,
      friendRowsFound: rows,
      adaHasPicture,
      cleoHasInitial,
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
  captureOslFriendsPanelScreen(parseArgs(process.argv.slice(2))).then((result) => {
    if (!existsSync(result.screenshot)) throw new Error("screenshot file was not written");
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-osl-friends-panel: ${error.stack || error.message}`);
    process.exit(1);
  });
}
