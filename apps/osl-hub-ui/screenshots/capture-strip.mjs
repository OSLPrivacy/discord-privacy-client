#!/usr/bin/env node

// Photographs the OSL Strip (screenshots/strip-fixture.html) and PROVES its
// two load-bearing behaviours in a real Chrome before writing a single PNG:
//
//   1. TOGGLE-TO-REVEAL — a click on the eye swaps cover text for plaintext,
//      and a second click restores it. The capture fails if either transition
//      does not happen.
//   2. ROOM HONESTY — on the unprovable channel the composer placeholder
//      carries the canonical warning and the per-room chips are aria-disabled.
//
// Outputs: strip-rest.png, strip-revealed.png, strip-unprovable.png.

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createViteServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

export const FIXED_VIEWPORT = { width: 1200, height: 760 };

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.dirname(SCRIPT_DIR);

const COVER_LINE = "sounds good, see you then";
const REAL_LINE = "safe house, 6pm, come alone";
const UNPROVEN_WARNING = "OSL can't see this box — don't send protected text here";

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function settle(ms = 200) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

async function pageText(page) {
  return page.evaluate(`document.body.innerText.replace(/\\s+/g, " ").trim()`);
}

async function toggleEye(page) {
  await page.evaluate(`document.querySelector('[data-chip="eye"]').click(), "ok"`);
}

async function shoot(page, output) {
  const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
  const facts = imageFacts(png);
  assert(facts.width === FIXED_VIEWPORT.width && facts.height === FIXED_VIEWPORT.height,
    `expected ${FIXED_VIEWPORT.width}x${FIXED_VIEWPORT.height}, captured ${facts.width}x${facts.height}`);
  assert(png.length > 15_000 && facts.distinctColors > 50,
    `PNG is blank or nearly blank: bytes=${png.length} distinctColors=${facts.distinctColors}`);
  mkdirSync(path.dirname(output), { recursive: true });
  writeFileSync(output, png);
  return { output, ...facts, crops: undefined };
}

async function openFixture(page, port, query) {
  await page.navigate(`http://127.0.0.1:${port}/screenshots/strip-fixture.html${query}`);
  const ready = await page.evaluate(`document.documentElement.dataset.fixtureReady`);
  assert(ready === "osl-strip", "strip fixture did not become ready");
  await settle(300);
}

export async function captureStrip({ outputDir = SCRIPT_DIR } = {}) {
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
  const results = {};
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      ...FIXED_VIEWPORT,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.send("Emulation.setEmulatedMedia", {
      features: [{ name: "prefers-reduced-motion", value: "reduce" }],
    });

    // ---- Rest: provable room, protection on, cover text showing ----------
    await openFixture(page, address.port, "");
    let text = await pageText(page);
    assert(text.includes(COVER_LINE), "rest state must show the cover text");
    assert(!text.includes(REAL_LINE), "rest state must NOT show the plaintext");
    assert(text.includes("OSL will wrap it"), "rest composer placeholder is wrong");
    assert(text.includes("1/3"), "whitelist chip face 1/3 is missing");
    assert(text.includes("1d"), "timer chip face 1d is missing (and must stay lowercase)");
    assert(text.includes("ONCE OFF"), "ONCE chip face is missing");
    results.rest = await shoot(page, path.join(outputDir, "strip-rest.png"));

    // ---- First click reveals — with zero styling change ------------------
    await toggleEye(page);
    await settle(120);
    text = await pageText(page);
    assert(text.includes(REAL_LINE), "clicking the eye must reveal the plaintext");
    assert(!text.includes(COVER_LINE), "clicking the eye must hide the cover text");
    const revealed = await page.evaluate(`document.querySelector('[data-osl-strip]').dataset.revealVisible`);
    assert(revealed === "true", "strip does not report an active reveal toggle");
    // Zero styling change: the revealed row's computed style must equal the
    // unrevealed row's (same colour, size, family, background).
    const styleProbe = await page.evaluate(`(() => {
      const rows = [...document.querySelectorAll('[data-message]')];
      const revealed = getComputedStyle(rows[0]);
      const plain = getComputedStyle(rows[2]);
      return JSON.stringify(["color", "fontSize", "fontFamily", "fontWeight", "backgroundColor"]
        .map((key) => [revealed[key], plain[key]]));
    })()`);
    for (const [revealedValue, plainValue] of JSON.parse(styleProbe)) {
      assert(revealedValue === plainValue,
        `reveal changed styling: ${revealedValue} != ${plainValue}`);
    }
    results.revealed = await shoot(page, path.join(outputDir, "strip-revealed.png"));

    // ---- Second click restores -------------------------------------------
    await toggleEye(page);
    await settle(120);
    text = await pageText(page);
    assert(text.includes(COVER_LINE) && !text.includes(REAL_LINE),
      "clicking the eye a second time must restore the cover text");
    const restored = await page.evaluate(`document.querySelector('[data-osl-strip]').dataset.revealVisible`);
    assert(restored === "false", "strip does not report the restored cover-text state");

    // ---- Unprovable room: honesty ----------------------------------------
    await openFixture(page, address.port, "?ch=3");
    text = await pageText(page);
    assert(text.includes(UNPROVEN_WARNING), "unproven composer placeholder must warn");
    assert(text.includes("NO ROOM"), "whitelist chip must say NO ROOM");
    const disabledChips = await page.evaluate(`JSON.stringify(["timer","once","eye","whitelist","burn"]
      .map((name) => [name, document.querySelector('[data-chip="' + name + '"]').getAttribute("aria-disabled")]))`);
    for (const [name, ariaDisabled] of JSON.parse(disabledChips)) {
      assert(ariaDisabled === "true", `chip '${name}' must be greyed in an unproven room`);
    }
    // Greyed but never unexplained.
    const reasons = await page.evaluate(`JSON.stringify(["timer","once","eye","whitelist","burn"]
      .map((name) => document.querySelector('[data-chip="' + name + '"]').title))`);
    for (const reason of JSON.parse(reasons)) {
      assert(typeof reason === "string" && reason.includes("OSL can't prove which chat this is"),
        "every greyed chip must carry the room-honesty reason");
    }
    results.unprovable = await shoot(page, path.join(outputDir, "strip-unprovable.png"));

    return results;
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureStrip().then((results) => {
    for (const result of Object.values(results)) {
      if (!existsSync(result.output)) throw new Error(`screenshot ${result.output} was not written`);
    }
    console.log(JSON.stringify(results, null, 2));
  }).catch((error) => {
    console.error(`capture-strip: ${error.stack || error.message}`);
    process.exit(1);
  });
}
