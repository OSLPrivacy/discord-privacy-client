#!/usr/bin/env node
/**
 * TASK 0858 — photograph the service status page twice, at a fixed size, on
 * Linux: once for a service OSL can only PLACE into, and once for a service OSL
 * has NOT STARTED on.
 *
 * The point of two shots is that they are the same screen. TASK 0856 built the
 * page and photographed the placing-only answer; a single photograph cannot
 * show whether the page is telling the truth or just wearing one shape. Two
 * services at opposite ends of the capability ladder, drawn by the same code at
 * the same window size, can: the title is identical in both images down to the
 * pixel, and the capability chip, the limits and the action are not.
 *
 * Neither service is chosen by hand. The catalog is loaded, every row is put
 * through the TASK 0804 ladder, and this script asserts that exactly one row
 * comes out "Placing only" and that the row it photographs as "Not started"
 * really is one.
 *
 * The check that guards all of this is one function, {@link assertScreen}. It
 * runs against the real screens, and then against THROWAWAY copies of the same
 * screen with one named control removed — one copy per named control. A copy is
 * rendered in the same browser, is never written to disk, and never replaces the
 * real page. Every one of them must make assertScreen throw, or the check is
 * decoration.
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
/** Fixed for both shots. Nothing about the page may reflow between them. */
const WINDOW = { width: 1280, height: 900 };
/** #0a0a0a, the app's --bg. */
const BACKGROUND = [10, 10, 10];
/** The page's own title, the same string on every service. */
const PAGE_TITLE = "Service status";
/** The label `#native-app-back` carries on this page. */
const BACK_LABEL = "Back to Home";

/**
 * The parts of the screen the finish line names, in the order it names them.
 *
 * `selector` is what a throwaway copy deletes to prove the check can go red.
 * `rect` is the region measured in the PNG, so "in the image" means pixels and
 * not merely markup.
 */
const NAMED_CONTROLS = [
  { name: "title", selector: "#tile-status-title", rect: "title" },
  { name: "capability", selector: "[data-tile-status-capability]", rect: "capability" },
  { name: "limits", selector: "[data-tile-status-limit-count]", rect: "limits" },
  { name: "action", selector: ".tile-status-next", rect: "action" },
  { name: "back", selector: "#native-app-back", rect: "back" },
];

const SCREENS = [
  {
    key: "placing-only",
    tile: "telegram",
    capabilityLabel: "Placing only",
    png: path.join(OUTPUT_DIR, "task-0858-status-page-placing-only-1280x900.png"),
    tree: path.join(OUTPUT_DIR, "task-0858-status-page-placing-only-screen-tree.json"),
  },
  {
    key: "not-started",
    tile: "whatsapp",
    capabilityLabel: "Not started",
    png: path.join(OUTPUT_DIR, "task-0858-status-page-not-started-1280x900.png"),
    tree: path.join(OUTPUT_DIR, "task-0858-status-page-not-started-screen-tree.json"),
  },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
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

/**
 * Everything the check reads off the rendered screen, including a screen tree.
 *
 * The tree is walked off the live DOM rather than reconstructed from the markup
 * string: an element that is in the markup but not on the screen is exactly the
 * failure this is here to catch.
 */
const READ_SCREEN = `(async () => {
  await document.fonts.ready;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const box = (element) => {
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const text = (element) => (element ? element.textContent.trim() : null);
  const root = document.querySelector("[data-tile-status-page]");
  const title = document.querySelector("#tile-status-title");
  const service = document.querySelector("[data-tile-status-service]");
  const capability = document.querySelector("[data-tile-status-capability]");
  const claim = document.querySelector("[data-tile-status-claim]");
  const sentence = document.querySelector(".tile-status-capability-sentence");
  const explanation = document.querySelector(".tile-status-explanation");
  const limitList = document.querySelector("[data-tile-status-limit-count]");
  const canList = document.querySelector("[data-tile-status-can-count]");
  const action = document.querySelector(".tile-status-next");
  const back = document.querySelector("#native-app-back");
  const burn = document.querySelector("#burn-button");
  const evidence = document.querySelector(".tile-status-evidence");
  const limitItems = [...document.querySelectorAll("[data-tile-status-limit]")];
  const canItems = [...document.querySelectorAll("[data-tile-status-can]")];
  const tree = (element) => {
    if (!element) return null;
    const rect = element.getBoundingClientRect();
    const own = [...element.childNodes]
      .filter((node) => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.textContent.trim())
      .filter(Boolean)
      .join(" ");
    return {
      tag: element.tagName.toLowerCase(),
      id: element.id || undefined,
      class: element.className || undefined,
      role: element.getAttribute("role") || undefined,
      ariaLabel: element.getAttribute("aria-label") || undefined,
      data: Object.fromEntries(Object.entries(element.dataset)),
      text: own || undefined,
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      children: [...element.children].map(tree),
    };
  };
  return {
    documentTitle: document.title,
    tile: document.documentElement.dataset.task0858Tile ?? null,
    throwawayCopy: document.documentElement.dataset.task0858Copy ?? "none",
    mains: document.querySelectorAll("main").length,
    routeHeadings: document.querySelectorAll("#route-heading").length,
    h1s: document.querySelectorAll("h1").length,
    present: {
      title: Boolean(title),
      capability: Boolean(capability),
      limits: Boolean(limitList),
      action: Boolean(action),
      back: Boolean(back),
    },
    titleText: text(title),
    serviceText: text(service),
    capabilityText: text(capability),
    claimText: text(claim),
    sentenceText: text(sentence),
    explanationText: text(explanation),
    limitCount: limitList ? Number(limitList.dataset.tileStatusLimitCount) : null,
    limitTexts: limitItems.map((item) => item.textContent.trim().replace(/^✕\\s*/u, "")),
    canCount: canList ? Number(canList.dataset.tileStatusCanCount) : null,
    canTexts: canItems.map((item) => item.textContent.trim().replace(/^✓\\s*/u, "")),
    actionText: text(action),
    actionTarget: action ? action.dataset.tileStatusAction ?? null : null,
    actionHandler: action ? action.dataset.tileStatusHandler ?? null : null,
    actionId: action ? action.id || null : null,
    actionRoute: action ? action.dataset.route ?? null : null,
    backText: text(back),
    backTarget: back ? back.dataset.tileStatusAction ?? null : null,
    burnText: text(burn),
    evidenceText: text(evidence),
    rects: {
      page: box(root),
      title: box(title),
      service: box(service),
      capability: box(capability),
      sentence: box(sentence),
      explanation: box(explanation),
      limits: box(limitList),
      action: box(action),
      back: box(back),
    },
    tree: tree(root),
    visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

async function readScreen(page) {
  const screen = await evaluate(page, READ_SCREEN);
  await page.send("Accessibility.enable");
  const ax = await page.send("Accessibility.getFullAXTree");
  screen.axNames = ax.nodes
    .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
    .filter(Boolean);
  return screen;
}

/**
 * Non-blank means INK, measured in the PNG.
 *
 * `nonBackground` alone proves nothing here: the card is painted `--panel`, so
 * an empty strip of card already counts as non-background from end to end. What
 * separates drawn text from empty card is that text has many colours (glyph
 * edges are antialiased) and is drawn in a lighter ink than the card it sits on.
 */
function regionIsDrawn(png, rect) {
  if (!rect || rect.width <= 0 || rect.height <= 0) return { drawn: false, crop: null };
  const crop = cropFacts(png, rect, { background: BACKGROUND });
  return { drawn: crop.nonBackground >= 10 && crop.distinctColors >= 20 && crop.brightPixels >= 20, crop };
}

/**
 * THE CHECK. Everything the finish line asks of one screen, in one function, so
 * that the throwaway copies are measured by exactly the same code as the real
 * ones. It throws on the first thing it finds wrong.
 */
function assertScreen(screen, png, expected) {
  const where = `${expected.key} (${expected.tile})`;

  // ---- the four named parts exist on the screen at all.
  const missing = NAMED_CONTROLS.filter((control) => !screen.present[control.name]).map((c) => c.name);
  if (missing.length > 0) throw new Error(`${where}: the screen is missing named control(s): ${missing.join(", ")}`);

  // ---- title.
  if (screen.titleText !== PAGE_TITLE) {
    throw new Error(`${where}: the screen title is "${screen.titleText}", not "${PAGE_TITLE}"`);
  }
  if (screen.documentTitle !== PAGE_TITLE) {
    throw new Error(`${where}: the document title is "${screen.documentTitle}", not "${PAGE_TITLE}"`);
  }
  if (!screen.visibleText.toLowerCase().includes(PAGE_TITLE.toLowerCase())) {
    throw new Error(`${where}: "${PAGE_TITLE}" is not visible text on the screen`);
  }
  if (screen.h1s !== 1 || screen.mains !== 1 || screen.routeHeadings !== 1) {
    throw new Error(`${where}: not one page — h1=${screen.h1s} main=${screen.mains} route-heading=${screen.routeHeadings}`);
  }

  // ---- real capability: the generated label, its sentence, and the service it
  // is about. A capability chip with no sentence under it is a badge again.
  if (screen.capabilityText !== expected.capabilityLabel) {
    throw new Error(`${where}: capability reads "${screen.capabilityText}", expected "${expected.capabilityLabel}"`);
  }
  if (screen.serviceText !== expected.displayName) {
    throw new Error(`${where}: the page names service "${screen.serviceText}", expected "${expected.displayName}"`);
  }
  if (!screen.sentenceText || screen.sentenceText !== expected.capabilitySentence) {
    throw new Error(`${where}: capability sentence is "${screen.sentenceText}", expected the generated "${expected.capabilitySentence}"`);
  }
  if (!screen.explanationText || screen.explanationText.length < 30) {
    throw new Error(`${where}: the reason line is missing or a stub: "${screen.explanationText}"`);
  }

  // ---- plain limits.
  if (!Number.isInteger(screen.limitCount) || screen.limitCount < 1) {
    throw new Error(`${where}: the page states ${screen.limitCount} limits`);
  }
  if (screen.limitTexts.length !== screen.limitCount) {
    throw new Error(`${where}: ${screen.limitCount} limits declared, ${screen.limitTexts.length} drawn`);
  }
  if (screen.limitCount !== expected.limitCount) {
    throw new Error(`${where}: ${screen.limitCount} limits, expected the catalog's ${expected.limitCount}`);
  }
  const stubLimits = screen.limitTexts.filter((limit) => limit.length < 30);
  if (stubLimits.length > 0) throw new Error(`${where}: limit lines too short to say anything: ${stubLimits.join(" | ")}`);
  const vagueLimits = screen.limitTexts.filter((limit) => !/\bOSL\b/u.test(limit) && !limit.includes(expected.displayName));
  if (vagueLimits.length > 0) throw new Error(`${where}: limits that name neither OSL nor ${expected.displayName}: ${vagueLimits.join(" | ")}`);

  // ---- a useful action: one that a shipped handler performs.
  if (!screen.actionText || screen.actionText.length < 8) {
    throw new Error(`${where}: the action reads "${screen.actionText}"`);
  }
  if (screen.actionText !== expected.nextActionLabel) {
    throw new Error(`${where}: the action reads "${screen.actionText}", expected the generated "${expected.nextActionLabel}"`);
  }
  const boundBy = screen.actionId === "embedded-service-setup" ? "#embedded-service-setup"
    : screen.actionRoute ? `[data-route=${screen.actionRoute}]`
      : null;
  if (!boundBy) throw new Error(`${where}: the action is bound to no shipped handler`);

  // ---- Back to Home.
  if (screen.backText !== BACK_LABEL) {
    throw new Error(`${where}: the back control reads "${screen.backText}", not "${BACK_LABEL}"`);
  }
  if (screen.backTarget !== "home") {
    throw new Error(`${where}: the back control declares target "${screen.backTarget}", not "home"`);
  }

  // ---- the screen tree: every named part, by accessible name.
  const axWanted = [
    PAGE_TITLE,
    expected.displayName,
    expected.capabilityLabel,
    "What OSL cannot do here",
    expected.nextActionLabel,
    BACK_LABEL,
  ];
  // Case-insensitively: `.status-tag` is `text-transform: uppercase`, so the
  // capability chip's accessible name is "PLACING ONLY" while its text content
  // is "Placing only". The style shouting is not the page saying something else.
  const axNames = screen.axNames.map((name) => name.toLowerCase());
  const missingAx = axWanted.filter((name) => !axNames.some((candidate) => candidate.includes(name.toLowerCase())));
  if (missingAx.length > 0) throw new Error(`${where}: missing from the screen tree: ${missingAx.join(" | ")}`);
  const treeText = JSON.stringify(screen.tree);
  const missingTree = NAMED_CONTROLS
    .filter((control) => {
      const needle = control.name === "title" ? PAGE_TITLE
        : control.name === "capability" ? expected.capabilityLabel
          : control.name === "limits" ? "tileStatusLimitCount"
            : control.name === "action" ? "tile-status-next"
              : "native-app-back";
      return !treeText.includes(needle);
    })
    .map((control) => control.name);
  if (missingTree.length > 0) throw new Error(`${where}: missing from the DOM screen tree: ${missingTree.join(", ")}`);

  // ---- the image: not blank, everything inside the window, every named part
  // actually drawn.
  const facts = imageFacts(png.buffer, screen.rects, { background: BACKGROUND });
  if (facts.width !== WINDOW.width || facts.height !== WINDOW.height) {
    throw new Error(`${where}: PNG is ${facts.width}x${facts.height}, expected ${WINDOW.width}x${WINDOW.height}`);
  }
  if (facts.nearlyBlank) {
    throw new Error(`${where}: PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
  }
  const undrawn = NAMED_CONTROLS
    .map((control) => ({ control, ...regionIsDrawn(png.png, screen.rects[control.rect]) }))
    .filter((entry) => !entry.drawn)
    .map((entry) => entry.control.name);
  if (undrawn.length > 0) throw new Error(`${where}: named control(s) blank in the image: ${undrawn.join(", ")}`);
  const outside = Object.entries(screen.rects)
    .filter(([, rect]) => rect && (rect.x < 0 || rect.y < 0
      || rect.x + rect.width > WINDOW.width || rect.y + rect.height > WINDOW.height))
    .map(([name]) => name);
  if (outside.length > 0) throw new Error(`${where}: parts of the page fall outside the ${WINDOW.width}x${WINDOW.height} window: ${outside.join(", ")}`);

  return { facts };
}

/**
 * Mean absolute RGB difference between one region of image A and one region of
 * image B, each read from its own origin.
 *
 * The two screens do not have the same height of content, so the card sits at a
 * different y in each shot. Comparing the same absolute rectangle would only
 * measure that the pages are different lengths. Comparing each region from its
 * OWN origin measures what is actually in question: whether the same glyphs were
 * drawn there.
 */
function meanAbsDiff(a, rectA, b, rectB) {
  const width = Math.min(Math.floor(rectA.width), Math.floor(rectB.width));
  const height = Math.min(Math.floor(rectA.height), Math.floor(rectB.height));
  let sum = 0;
  let count = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const offsetA = ((Math.floor(rectA.y) + y) * a.width + Math.floor(rectA.x) + x) * 4;
      const offsetB = ((Math.floor(rectB.y) + y) * b.width + Math.floor(rectB.x) + x) * 4;
      sum += Math.abs(a.pixels[offsetA] - b.pixels[offsetB])
        + Math.abs(a.pixels[offsetA + 1] - b.pixels[offsetB + 1])
        + Math.abs(a.pixels[offsetA + 2] - b.pixels[offsetB + 2]);
      count += 3;
    }
  }
  return count === 0 ? Number.NaN : sum / count;
}

async function main() {
  if (process.platform !== "linux") {
    throw new Error(`TASK 0858 asks for Linux screenshots; this is ${process.platform}`);
  }
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/status-page-screen.html`;
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
  const lines = [];
  const say = (line) => { lines.push(line); console.log(line); };
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
      while (document.documentElement.dataset.task0858 !== "ready") {
        if (Date.now() > deadline) throw new Error("the status page never finished rendering");
        await new Promise((resolve) => setTimeout(resolve, 25));
      }
      return true;
    })()`);

    // Warm-up: draw every screen once and wait for the webfonts before anything
    // is measured. Without this the FIRST render is laid out in the fallback
    // face and the second in the display face, and the two shots then differ by
    // a few pixels of text metrics for no reason a reader could see.
    await evaluate(page, `(async () => {
      for (const tile of ${JSON.stringify(SCREENS.map((screen) => screen.tile))}) {
        window.task0858Render(tile);
        await document.fonts.ready;
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      }
      return document.fonts.status;
    })()`);

    // ---- 1. the catalog decides which service is which, not this script.
    const catalog = await evaluate(page, "window.task0858Catalog()");
    say(`TASK0858_CATALOG_SIZE=${catalog.length}`);
    const byLabel = new Map();
    for (const entry of catalog) {
      byLabel.set(entry.capabilityLabel, [...(byLabel.get(entry.capabilityLabel) ?? []), entry.id]);
      say(`TASK0858_CATALOG ${entry.id}=${entry.capabilityLabel} can=${entry.page.can.length} limits=${entry.page.limits.length}`);
    }
    const placingOnly = byLabel.get("Placing only") ?? [];
    const notStarted = byLabel.get("Not started") ?? [];
    say(`TASK0858_PLACING_ONLY=${placingOnly.join(",") || "(none)"}`);
    say(`TASK0858_NOT_STARTED=${notStarted.join(",") || "(none)"}`);
    if (placingOnly.length !== 1) throw new Error(`expected exactly one placing-only service, got ${placingOnly.join(", ") || "none"}`);
    if (notStarted.length < 1) throw new Error("no service is not-started, so there is nothing to photograph for that half");

    const expectations = SCREENS.map((screen) => {
      const entry = catalog.find((candidate) => candidate.id === screen.tile);
      if (!entry) throw new Error(`the catalog has no ${screen.tile}`);
      if (entry.capabilityLabel !== screen.capabilityLabel) {
        throw new Error(`${screen.tile} is "${entry.capabilityLabel}", not "${screen.capabilityLabel}"`);
      }
      const list = screen.capabilityLabel === "Placing only" ? placingOnly : notStarted;
      if (!list.includes(screen.tile)) throw new Error(`${screen.tile} is not in the ${screen.capabilityLabel} set`);
      return {
        ...screen,
        displayName: entry.page.title,
        capabilitySentence: entry.page.capability,
        limitCount: entry.page.limits.length,
        canCount: entry.page.can.length,
        nextActionLabel: entry.page.nextAction.label,
      };
    });

    // ---- 2. photograph each screen and run the check over it.
    const captured = [];
    for (const expected of expectations) {
      await evaluate(page, `window.task0858Render(${JSON.stringify(expected.tile)})`);
      const screen = await readScreen(page);
      const buffer = await page.screenshot({ captureBeyondViewport: false });
      const png = { buffer, png: parsePng(buffer) };
      const { facts } = assertScreen(screen, png, expected);
      writeFileSync(expected.png, buffer);
      writeFileSync(expected.tree, `${JSON.stringify({
        task: "0858",
        screen: expected.key,
        tile: expected.tile,
        window: WINDOW,
        documentTitle: screen.documentTitle,
        namedControls: NAMED_CONTROLS.map((control) => ({
          name: control.name,
          selector: control.selector,
          rect: screen.rects[control.rect],
          drawn: regionIsDrawn(png.png, screen.rects[control.rect]).crop,
        })),
        capabilityLabel: screen.capabilityText,
        claimLabel: screen.claimText,
        capabilitySentence: screen.sentenceText,
        explanation: screen.explanationText,
        can: screen.canTexts,
        limits: screen.limitTexts,
        action: { label: screen.actionText, id: screen.actionId, target: screen.actionTarget, handler: screen.actionHandler },
        back: { label: screen.backText, target: screen.backTarget },
        accessibleNames: screen.axNames,
        domTree: screen.tree,
      }, null, 2)}\n`);
      captured.push({ expected, screen, png, facts, buffer });

      const key = expected.key.toUpperCase().replace(/[^A-Z0-9]+/gu, "_");
      say(`TASK0858_${key}_TILE=${expected.tile}`);
      say(`TASK0858_${key}_TITLE=${screen.titleText}`);
      say(`TASK0858_${key}_DOCUMENT_TITLE=${screen.documentTitle}`);
      say(`TASK0858_${key}_SERVICE=${screen.serviceText}`);
      say(`TASK0858_${key}_CAPABILITY=${screen.capabilityText}`);
      say(`TASK0858_${key}_CLAIM=${screen.claimText}`);
      say(`TASK0858_${key}_CAPABILITY_SENTENCE=${screen.sentenceText}`);
      say(`TASK0858_${key}_CAN_COUNT=${screen.canCount ?? 0}`);
      for (const line of screen.canTexts) say(`TASK0858_${key}_CAN=${line}`);
      say(`TASK0858_${key}_LIMIT_COUNT=${screen.limitCount}`);
      for (const line of screen.limitTexts) say(`TASK0858_${key}_LIMIT=${line}`);
      say(`TASK0858_${key}_ACTION=${screen.actionText} | id=${screen.actionId ?? "-"} | route=${screen.actionRoute ?? "-"} | target=${screen.actionTarget}`);
      say(`TASK0858_${key}_BACK=${screen.backText} | target=${screen.backTarget}`);
      say(`TASK0858_${key}_PNG=${expected.png}`);
      say(`TASK0858_${key}_PNG_BYTES=${buffer.length}`);
      say(`TASK0858_${key}_PNG_SHA256=${sha256(buffer)}`);
      say(`TASK0858_${key}_WINDOW=${facts.width}x${facts.height}`);
      say(`TASK0858_${key}_DISTINCT_COLORS=${facts.distinctColors}`);
      say(`TASK0858_${key}_NONBACKGROUND=${facts.nonBackground}`);
      say(`TASK0858_${key}_NEARLY_BLANK=${facts.nearlyBlank}`);
      for (const control of NAMED_CONTROLS) {
        const crop = facts.crops[control.rect];
        say(`TASK0858_${key}_DRAWN_${control.name.toUpperCase()}=nonBackground:${crop.nonBackground} colors:${crop.distinctColors} bright:${crop.brightPixels}`);
      }
      say(`TASK0858_${key}_TREE=${expected.tree}`);
      say(`TASK0858_${key}_AX_NAMES=${screen.axNames.length}`);
    }

    // ---- 3. the two shots are the same screen showing different answers.
    const [placing, started] = captured;
    const titleRect = placing.screen.rects.title;
    const otherTitleRect = started.screen.rects.title;
    const chipRect = placing.screen.rects.capability;
    // Same origin and same line height in both. The title BOX is as wide as the
    // chip row under it (the header is a content-sized grid column, and
    // "PLACING ONLY / NOT CLAIMED" is 3px narrower than "NOT STARTED / COMING
    // LATER"), so the widths are allowed to differ; the glyphs are compared over
    // the narrower of the two, from each image's own origin.
    if (titleRect.x !== otherTitleRect.x || titleRect.y !== otherTitleRect.y
      || titleRect.height !== otherTitleRect.height) {
      throw new Error("the title moved between the two screens, so they are not the same screen");
    }
    const titleDiff = meanAbsDiff(placing.png.png, titleRect, started.png.png, otherTitleRect);
    const chipDiff = meanAbsDiff(placing.png.png, chipRect, started.png.png, started.screen.rects.capability);
    say(`TASK0858_TITLE_RECT_PLACING_ONLY=${titleRect.x},${titleRect.y} ${titleRect.width}x${titleRect.height}`);
    say(`TASK0858_TITLE_RECT_NOT_STARTED=${otherTitleRect.x},${otherTitleRect.y} ${otherTitleRect.width}x${otherTitleRect.height}`);
    say(`TASK0858_TITLE_PIXEL_DIFF=${titleDiff.toFixed(4)}`);
    say(`TASK0858_CAPABILITY_PIXEL_DIFF=${chipDiff.toFixed(4)}`);
    if (titleDiff !== 0) throw new Error(`the title is drawn differently in the two images (mean abs diff ${titleDiff})`);
    if (!(chipDiff > 4)) throw new Error(`the capability chip is drawn identically in both images (mean abs diff ${chipDiff}) — it is not per-service`);
    if (sha256(placing.buffer) === sha256(started.buffer)) throw new Error("both screenshots are the same image");

    // ---- 4. the negative control: one named control removed, one copy each.
    const broken = [];
    for (const expected of expectations) {
      for (const control of NAMED_CONTROLS) {
        await evaluate(page, `window.task0858RenderWithoutControl(${JSON.stringify(expected.tile)}, ${JSON.stringify(control.selector)})`);
        const copy = await readScreen(page);
        const buffer = await page.screenshot({ captureBeyondViewport: false });
        const png = { buffer, png: parsePng(buffer) };
        let message = null;
        try {
          assertScreen(copy, png, expected);
        } catch (error) {
          message = error.message;
        }
        if (message === null) {
          throw new Error(`a throwaway copy of ${expected.key} missing ${control.selector} still PASSED the check`);
        }
        broken.push({ key: expected.key, control: control.name, message });
        say(`TASK0858_MISSING_CONTROL ${expected.key} ${control.name} (${control.selector}) -> FAIL: ${message}`);
      }
    }
    say(`TASK0858_MISSING_CONTROL_COPIES=${broken.length}`);
    say(`TASK0858_MISSING_CONTROL_FAILURES=${broken.length}`);

    // ---- 5. and the same again with each named control left in the markup but
    // drawn nowhere, so the pixel half of the check is shown to go red too.
    const hidden = [];
    for (const expected of expectations) {
      for (const control of NAMED_CONTROLS) {
        await evaluate(page, `window.task0858RenderWithControlHidden(${JSON.stringify(expected.tile)}, ${JSON.stringify(control.selector)})`);
        const copy = await readScreen(page);
        const buffer = await page.screenshot({ captureBeyondViewport: false });
        const png = { buffer, png: parsePng(buffer) };
        let message = null;
        try {
          assertScreen(copy, png, expected);
        } catch (error) {
          message = error.message;
        }
        if (message === null) {
          throw new Error(`a throwaway copy of ${expected.key} with ${control.selector} drawn nowhere still PASSED the check`);
        }
        hidden.push({ key: expected.key, control: control.name, message });
        say(`TASK0858_UNDRAWN_CONTROL ${expected.key} ${control.name} (${control.selector}) -> FAIL: ${message}`);
      }
    }
    say(`TASK0858_UNDRAWN_CONTROL_COPIES=${hidden.length}`);
    say(`TASK0858_UNDRAWN_CONTROL_FAILURES=${hidden.length}`);

    // Put the real page back, so nothing a reader opens afterwards is a copy.
    await evaluate(page, `window.task0858Render(${JSON.stringify(SCREENS[0].tile)})`);
    const restored = await readScreen(page);
    say(`TASK0858_RESTORED_COPY=${restored.throwawayCopy}`);
    if (restored.throwawayCopy !== "none") throw new Error("the throwaway copy was left on screen");

    say(`TASK0858_PLATFORM=${process.platform}`);
    say(`TASK0858_URL=${url}`);
    say("TASK0858_RESULT=pass");
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-status-page-screens: ${error.stack || error.message}`);
  process.exitCode = 1;
});
