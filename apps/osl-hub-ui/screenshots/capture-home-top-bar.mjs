#!/usr/bin/env node

// TASK 0816 -- build the Home top bar.
//
// Finish line: "a Linux screenshot shows all five controls without clipped
// text." So this capture paints the REAL app in headless Chromium on Linux,
// finds the five top-bar controls, and only writes a PNG once every one of
// them has passed a clipping check with three independent parts:
//
//   1. DOM     -- the label's own box does not overflow (scrollWidth ===
//                 clientWidth), and it is not styled to hide or ellipsise.
//   2. Geometry-- the label box sits inside its button, the button inside the
//                 bar, the bar inside the viewport, and no two controls
//                 overlap.
//   3. Pixels  -- the label's box has ink in the captured image, and the two
//                 columns immediately outside it have none, so no glyph is
//                 running into or past the edge of its box.
//
// The bar is not rendered from a hand-made string: the app is booted, painted
// through its own render path, and then driven by clicking its own controls, so
// the current-page state in the image is the state the app produced.

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { cropFacts, parsePng } from "./capture-linux-notifications.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");

export const WINDOW = { width: 1280, height: 800 };

/** The five controls TASK 0816 asks for, left to right, and the word each one shows. */
export const REQUIRED_CONTROLS = [
  { id: "logo", label: "Home" },
  { id: "friends", label: "Friends" },
  { id: "notifications", label: "Notifications" },
  { id: "settings", label: "Settings" },
  { id: "profile", label: "Profile" },
];

/**
 * The pages the capture visits, the control that must be current on each, and
 * the file the image is written to. Home is captured first; the other two are
 * reached by clicking the bar's own controls.
 */
export const CAPTURED_PAGES = [
  { name: "home", click: null, current: "logo", file: "task-0816-home-top-bar.png" },
  { name: "settings", click: "settings", current: "settings", file: "task-0816-home-top-bar-settings.png" },
  { name: "profile", click: "profile", current: "profile", file: "task-0816-home-top-bar-profile.png" },
];

const TAURI_STUB = `
  (() => {
    let nextCallback = 1;
    const callbacks = {};
    window.__TAURI_INTERNALS__ = {
      callbacks,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback(callback, once = false) {
        const id = nextCallback++;
        callbacks[id] = { callback, once };
        return id;
      },
      unregisterCallback(id) { delete callbacks[id]; },
      runCallback(id, args) {
        const entry = callbacks[id];
        if (!entry) return;
        entry.callback(args);
        if (entry.once) delete callbacks[id];
      },
      convertFileSrc(filePath) { return filePath; },
      invoke(cmd) {
        if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
        if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
        return Promise.reject(new Error("TASK0816 capture Tauri stub refused " + cmd));
      },
    };
  })();
`;

/** Boot the real app onto Home with enough state that the badge and dot render. */
const BOOT = `(async () => {
  localStorage.clear();
  const ui = await import("/src/main.ts");
  globalThis.__osl0816 = ui;
  ui.__oslHubUiTest.reset({
    route: "home",
    coreReady: true,
    storageMethod: "tpm-pcp",
    notificationsEnabled: true,
    notificationSecurityActivity: true,
    appNotifications: [{ id: "n-1", title: "Key change", detail: "Rose changed keys", createdAt: "Now" }],
    hubPeople: [
      { personId: "p-1", alias: "Rose", safetyNumberVerified: false },
      { personId: "p-2", alias: "Sam", safetyNumberVerified: true, pendingKeyChange: true },
    ],
    hubIdentities: [{ slotId: "slot-1", label: "OSL Profile", oslUserId: "OSLUSER-1", active: true }],
  });
  ui.__oslHubUiTest.flushRenderForTest();
  await document.fonts.ready;
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return "booted";
})()`;

/** Press one of the bar's own controls and let the app repaint. */
const clickControl = (id) => `(async () => {
  const button = document.querySelector('.home-command-bar [data-top-bar-control="${id}"]');
  if (!button) throw new Error("TASK0816: no ${id} control to click");
  button.click();
  globalThis.__osl0816.__oslHubUiTest.flushRenderForTest();
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  return "clicked ${id}";
})()`;

/** Read the bar out of the live page: text, geometry, computed style, state. */
const READ_BAR = `(() => {
  const box = (element) => {
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom };
  };
  const bar = document.querySelector(".home-command-bar");
  if (!bar) throw new Error("TASK0816: the Home top bar is not on this page");
  const controls = [...bar.querySelectorAll("[data-top-bar-control]")].map((button) => {
    const label = button.querySelector(".home-top-bar-label");
    if (!label) throw new Error("TASK0816: control " + button.dataset.topBarControl + " has no label");
    const style = getComputedStyle(label);
    return {
      id: button.dataset.topBarControl,
      text: label.textContent,
      current: button.dataset.currentPage === "true",
      ariaCurrent: button.getAttribute("aria-current"),
      ariaLabel: button.getAttribute("aria-label"),
      labelBox: box(label),
      buttonBox: box(button),
      scrollWidth: label.scrollWidth,
      clientWidth: label.clientWidth,
      scrollHeight: label.scrollHeight,
      clientHeight: label.clientHeight,
      overflowX: style.overflowX,
      textOverflow: style.textOverflow,
      whiteSpace: style.whiteSpace,
      display: style.display,
      buttonBackground: getComputedStyle(button).backgroundColor,
    };
  });
  return JSON.stringify({
    controls,
    barBox: box(bar),
    viewport: { width: window.innerWidth, height: window.innerHeight },
    visibleText: document.body.innerText.replace(/\\s+/g, " ").trim(),
  });
})()`;

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

function fail(message) {
  throw new Error(message);
}

/** Does `inner` sit wholly inside `outer`? A half-pixel of rounding is allowed. */
function contains(outer, inner, slack = 0.5) {
  return inner.x >= outer.x - slack
    && inner.y >= outer.y - slack
    && inner.right <= outer.right + slack
    && inner.bottom <= outer.bottom + slack;
}

/**
 * Ink in a rectangle, measured against the surface it is drawn on.
 *
 * `cropFacts` counts colours; for a clipping check what matters is which
 * COLUMNS carry glyph pixels, because a clipped word loses ink at one end and
 * an overflowing word gains ink outside its box.
 */
function inkColumns(png, rect, background) {
  const x0 = Math.max(0, Math.round(rect.x));
  const y0 = Math.max(0, Math.round(rect.y));
  const x1 = Math.min(png.width, Math.round(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.round(rect.y + rect.height));
  const columns = [];
  for (let x = x0; x < x1; x += 1) {
    let ink = 0;
    for (let y = y0; y < y1; y += 1) {
      const offset = (y * png.width + x) * 4;
      const distance = Math.abs(png.pixels[offset] - background[0])
        + Math.abs(png.pixels[offset + 1] - background[1])
        + Math.abs(png.pixels[offset + 2] - background[2]);
      if (distance > 60) ink += 1;
    }
    if (ink > 0) columns.push({ x, ink, sample: [png.pixels[(y0 * png.width + x) * 4], png.pixels[(y0 * png.width + x) * 4 + 1], png.pixels[(y0 * png.width + x) * 4 + 2]] });
  }
  return { first: columns[0]?.x ?? null, last: columns[columns.length - 1]?.x ?? null, count: columns.length, columns, x0, x1 };
}

/**
 * The colour THIS control is painted on.
 *
 * Not the bar's colour: the current control is highlighted, so measuring its
 * text against the bar would score the whole highlight as ink. The sample is
 * taken directly above the middle of the label -- inside the button, clear of
 * its border, clear of the icon to the left, and clear of the current-page
 * underline at the bottom.
 */
function controlBackground(png, control) {
  const x = Math.round(control.labelBox.x + control.labelBox.width / 2);
  const y = Math.round(control.buttonBox.y + 4);
  const offset = (y * png.width + x) * 4;
  return [png.pixels[offset], png.pixels[offset + 1], png.pixels[offset + 2]];
}

/** The whole clipping check for one page. Throws on the first thing that is wrong. */
function checkPage(pageName, bar, pngBytes) {
  const png = parsePng(pngBytes);
  if (png.width !== WINDOW.width || png.height !== WINDOW.height) {
    fail(`${pageName}: expected a ${WINDOW.width}x${WINDOW.height} capture, got ${png.width}x${png.height}`);
  }

  const ids = bar.controls.map((control) => control.id);
  const expected = REQUIRED_CONTROLS.map((control) => control.id);
  if (ids.join(",") !== expected.join(",")) {
    fail(`${pageName}: expected controls ${expected.join(",")}, found ${ids.join(",") || "(none)"}`);
  }

  const report = [];

  for (const required of REQUIRED_CONTROLS) {
    const control = bar.controls.find((candidate) => candidate.id === required.id);
    if (control.text !== required.label) {
      fail(`${pageName}/${control.id}: label reads "${control.text}", expected "${required.label}"`);
    }
    if (!bar.visibleText.includes(required.label)) {
      fail(`${pageName}/${control.id}: "${required.label}" is not in the page's visible text`);
    }

    // 1. DOM -- the word is not overflowing its own box, and is not styled away.
    if (control.scrollWidth !== control.clientWidth) {
      fail(`${pageName}/${control.id}: label text is clipped horizontally -- scrollWidth ${control.scrollWidth} vs clientWidth ${control.clientWidth}`);
    }
    if (control.scrollHeight !== control.clientHeight) {
      fail(`${pageName}/${control.id}: label text is clipped vertically -- scrollHeight ${control.scrollHeight} vs clientHeight ${control.clientHeight}`);
    }
    if (control.textOverflow === "ellipsis") fail(`${pageName}/${control.id}: label is set to ellipsise`);
    if (control.overflowX === "hidden") fail(`${pageName}/${control.id}: label overflow-x is hidden`);
    if (control.whiteSpace !== "nowrap") fail(`${pageName}/${control.id}: label white-space is "${control.whiteSpace}", expected nowrap`);
    if (control.labelBox.width < 4 || control.labelBox.height < 4) {
      fail(`${pageName}/${control.id}: label box is ${control.labelBox.width}x${control.labelBox.height} -- there is nothing to read`);
    }

    // 2. Geometry -- label inside button inside bar inside viewport.
    if (!contains(control.buttonBox, control.labelBox)) {
      fail(`${pageName}/${control.id}: the label box escapes its button (label ${JSON.stringify(control.labelBox)}, button ${JSON.stringify(control.buttonBox)})`);
    }
    if (!contains(bar.barBox, control.buttonBox)) {
      fail(`${pageName}/${control.id}: the control escapes the top bar`);
    }
    if (control.buttonBox.x < 0 || control.buttonBox.right > bar.viewport.width) {
      fail(`${pageName}/${control.id}: the control is off-screen at ${bar.viewport.width}px wide`);
    }

    // 3. Pixels -- ink inside the label box, none in the two columns outside it.
    const background = controlBackground(png, control);
    const ink = inkColumns(png, control.labelBox, background);
    if (ink.count < 4) {
      fail(`${pageName}/${control.id}: the label box has ${ink.count} inked columns -- the word was not painted`);
    }
    const gutter = 2;
    const left = inkColumns(png, { ...control.labelBox, x: control.labelBox.x - gutter, width: gutter }, background);
    const right = inkColumns(png, { ...control.labelBox, x: control.labelBox.right, width: gutter }, background);
    if (left.count > 0 || right.count > 0) {
      fail(`${pageName}/${control.id}: ink found outside the label box (${left.count} columns left, ${right.count} right) -- the word is running past its box; background=${background.join(",")} left=${JSON.stringify(left.columns)} right=${JSON.stringify(right.columns)} labelBox=${JSON.stringify(control.labelBox)}`);
    }

    report.push({
      id: control.id,
      label: control.text,
      current: control.current,
      ariaCurrent: control.ariaCurrent,
      ariaLabel: control.ariaLabel,
      labelBox: control.labelBox,
      scrollWidth: control.scrollWidth,
      clientWidth: control.clientWidth,
      inkedColumns: ink.count,
      inkOutsideLabelBox: left.count + right.count,
      background,
      pixels: cropFacts(png, control.labelBox),
    });
  }

  // No control may sit on top of another; overlapping words read as clipped.
  const sorted = [...bar.controls].sort((a, b) => a.buttonBox.x - b.buttonBox.x);
  for (let index = 1; index < sorted.length; index += 1) {
    if (sorted[index].buttonBox.x < sorted[index - 1].buttonBox.right - 0.5) {
      fail(`${pageName}: ${sorted[index - 1].id} and ${sorted[index].id} overlap`);
    }
  }

  return { controls: report };
}

export async function captureHomeTopBar() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({ root: APP_ROOT, server: { host: "127.0.0.1", port: 0 }, logLevel: "error" });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/`;
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
  const pages = [];
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: TAURI_STUB });
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "reduce" }] });
    await page.navigate(url, { timeoutMs: 30_000 });

    const booted = await evaluate(page, BOOT);
    if (booted !== "booted") fail(`the app did not boot: ${booted}`);

    for (const target of CAPTURED_PAGES) {
      if (target.click) {
        const clicked = await evaluate(page, clickControl(target.click));
        if (clicked !== `clicked ${target.click}`) fail(`clicking ${target.click} reported ${clicked}`);
      }
      const bar = JSON.parse(await evaluate(page, READ_BAR));
      const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
      const checked = checkPage(target.name, bar, png);

      // The current-page state has to be the one this page owns, and only one
      // control may claim it.
      const current = checked.controls.filter((control) => control.current);
      if (current.length !== 1) {
        fail(`${target.name}: expected exactly one current control, found ${current.length} (${current.map((c) => c.id).join(",") || "none"})`);
      }
      if (current[0].id !== target.current) {
        fail(`${target.name}: expected "${target.current}" to be the current page, the bar says "${current[0].id}"`);
      }
      if (current[0].ariaCurrent !== "page") {
        fail(`${target.name}: the current control's aria-current is ${current[0].ariaCurrent}`);
      }
      if (!current[0].ariaLabel.endsWith(", current page")) {
        fail(`${target.name}: the current control's aria-label does not say so: "${current[0].ariaLabel}"`);
      }
      for (const control of checked.controls.filter((candidate) => !candidate.current)) {
        if (control.ariaCurrent !== null) fail(`${target.name}/${control.id}: a control that is not current carries aria-current`);
      }

      const output = path.join(OUTPUT_DIR, target.file);
      writeFileSync(output, png);
      pages.push({
        page: target.name,
        screenshot: output,
        sha256: createHash("sha256").update(png).digest("hex"),
        bytes: png.length,
        viewport: WINDOW,
        currentControl: current[0].id,
        barBox: bar.barBox,
        controls: checked.controls,
      });
    }

    return { url, window: WINDOW, pages };
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  captureHomeTopBar().then((result) => {
    for (const captured of result.pages) {
      console.log(`TASK0816_PAGE=${captured.page} current=${captured.currentControl} sha256=${captured.sha256} file=${captured.screenshot}`);
      for (const control of captured.controls) {
        console.log(`  ${control.id.padEnd(13)} label="${control.label}" width=${control.clientWidth} scrollWidth=${control.scrollWidth} inkedColumns=${control.inkedColumns} inkOutside=${control.inkOutsideLabelBox} current=${control.current}`);
      }
    }
    console.log(`TASK0816_CONTROLS=${result.pages[0].controls.length}`);
    console.log(JSON.stringify(result, null, 2));
  }).catch((error) => {
    console.error(`capture-home-top-bar: ${error.stack || error.message}`);
    process.exit(1);
  });
}
