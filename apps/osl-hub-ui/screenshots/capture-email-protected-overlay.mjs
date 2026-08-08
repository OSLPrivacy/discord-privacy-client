#!/usr/bin/env node

/**
 * TASK 1228 -- capture the shared protected email overlay (draft + reading)
 * on Linux, and separately the empty-state fixture the built screen must
 * differ from.
 */

import { existsSync, mkdirSync } from "node:fs";
import { writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { pngFacts } from "./capture-connection-choice.mjs";

export const FIXED_VIEWPORT = { width: 1200, height: 820 };
/** The exact two named elements TASK 1228's finish line requires on screen. */
export const REQUIRED_NAMES = ["draft overlay", "reading overlay"];

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);

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

function parseArgs(args) {
  const value = (flag) => {
    const index = args.indexOf(flag);
    return index === -1 ? null : args[index + 1] ?? null;
  };
  const state = value("--state") ?? "built";
  const output = value("--output");
  return {
    state,
    output: output ? path.resolve(output) : path.join(SCRIPT_DIR, `email-protected-overlay-${state}.png`),
  };
}

export async function captureEmailProtectedOverlay({ state = "built", output } = {}) {
  if (state !== "built" && state !== "empty") throw new Error(`unknown fixture state '${state}'`);
  const target = output ?? path.join(SCRIPT_DIR, `email-protected-overlay-${state}.png`);

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
    const query = state === "empty" ? "?state=empty" : "";
    await page.navigate(`http://127.0.0.1:${address.port}/screenshots/email-protected-overlay-fixture.html${query}`);
    const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
    if (ready !== "email-protected-overlay") throw new Error("email overlay fixture did not become ready");
    const fixtureState = await page.evaluate(`document.documentElement.dataset.fixtureState`);
    if (fixtureState !== state) throw new Error(`fixture reports state '${fixtureState}', expected '${state}'`);
    await new Promise((resolve) => setTimeout(resolve, 250));

    const draftCount = Number(await page.evaluate(`document.querySelectorAll(".email-draft-overlay").length`));
    const readingCount = Number(await page.evaluate(`document.querySelectorAll(".email-reading-overlay").length`));
    const imageText = await page.evaluate(`document.body.innerText.replace(/\\s+/g, " ").trim()`);
    const axTree = await page.send("Accessibility.getFullAXTree");
    const screenTree = flattenAxTree(axTree.nodes);

    if (state === "built") {
      if (draftCount !== 1) throw new Error(`expected 1 draft overlay, found ${draftCount}`);
      if (readingCount !== 1) throw new Error(`expected 1 reading overlay, found ${readingCount}`);
      requireAllStrings(imageText, REQUIRED_NAMES, "visible image text");
      requireAllStrings(screenTree, REQUIRED_NAMES, "screen tree");
    } else {
      if (draftCount !== 0) throw new Error(`empty state must not show a draft overlay, found ${draftCount}`);
      if (readingCount !== 0) throw new Error(`empty state must not show a reading overlay, found ${readingCount}`);
      for (const name of REQUIRED_NAMES) {
        if (imageText.includes(name)) throw new Error(`empty state must not name '${name}' in visible text`);
        if (screenTree.includes(name)) throw new Error(`empty state must not name '${name}' in the screen tree`);
      }
    }

    const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
    const facts = pngFacts(png);
    if (facts.width !== FIXED_VIEWPORT.width || facts.height !== FIXED_VIEWPORT.height) {
      throw new Error(`expected ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height}, captured ${facts.width}x${facts.height}`);
    }

    mkdirSync(path.dirname(target), { recursive: true });
    writeFileSync(target, png);
    return {
      screenshot: target,
      viewport: FIXED_VIEWPORT,
      state,
      draftOverlayCount: draftCount,
      readingOverlayCount: readingCount,
      visibleTextRequiredNames: Object.fromEntries(REQUIRED_NAMES.map((name) => [name, imageText.includes(name) ? 1 : 0])),
      screenTreeRequiredNames: Object.fromEntries(
        REQUIRED_NAMES.map((name) => [name, screenTree.filter((value) => value === name).length]),
      ),
      png: facts,
      pngBytes: png,
    };
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  const { state, output } = parseArgs(process.argv.slice(2));
  captureEmailProtectedOverlay({ state, output })
    .then((result) => {
      if (!existsSync(result.screenshot)) throw new Error("screenshot file was not written");
      const { pngBytes, ...rest } = result;
      console.log(JSON.stringify(rest, null, 2));
    })
    .catch((error) => {
      console.error(`capture-email-protected-overlay: ${error.stack || error.message}`);
      process.exit(1);
    });
}
