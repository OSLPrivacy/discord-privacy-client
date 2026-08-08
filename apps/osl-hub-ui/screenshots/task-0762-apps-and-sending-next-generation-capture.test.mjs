import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { readPng } from "./lib/png-pixels.mjs";

/**
 * TASK 0762 - photograph the Apps and sending screen (TASK 0760) at the fixed
 * Linux window size with next-generation messages SAVED ON.
 *
 * The finish line has two halves and this file refuses unless both hold:
 *
 *   1. one named screenshot VISIBLY shows the saved on state. Not "the DOM says
 *      on": every named control has to be laid out inside the 1280x800 window
 *      and to have paint of its own in the decoded PNG, and the three regions
 *      that draw the switch (row, checkbox, state pill) have to look different
 *      from the same screen rendered with the setting saved off, while the
 *      untouched heading looks identical. A screen that reflowed under the
 *      camera, or that prints "On" in invisible ink, fails here.
 *
 *   2. the check can fail. A throwaway copy of the same rendered screen with
 *      exactly ONE named control taken out is put through the SAME check, once
 *      per control, and the check has to come back red naming that one control
 *      and nothing else. A thirteenth pass hides a control that is still in the
 *      DOM, so the pixel half of the check is proved to fail on its own too.
 */

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const FIXTURE_DIR = path.join(APP_ROOT, "screenshots", "fixtures");
const EVIDENCE_DIR = path.join(APP_ROOT, "screenshots", "evidence");
/** The one named screenshot the finish line asks for. */
const PNG_PATH = path.join(EVIDENCE_DIR, "task-0762-linux-apps-and-sending-next-generation-on.png");
const REPORT_PATH = path.join(EVIDENCE_DIR, "task-0762-apps-and-sending-next-generation-report.json");
/** The gate's own fixture page and model: the same screen, the same module. */
const FIXTURE_PAGE = "screenshots/task-0760-apps-and-sending-fixture.html";
const FIXTURE_MODEL = "task-0760-apps-and-sending.json";

/** A region that changes when the saved setting changes is drawing it. */
const CHOICE_MIN_MEAN_ABS_DIFF = 4;
/** A region that must not move: proves the difference is paint, not reflow. */
const UNTOUCHED_MAX_MEAN_ABS_DIFF = 0.5;
/** Below this a control's box is background and nothing else. */
const MIN_INK_PIXELS = 12;
const MIN_DISTINCT_COLOURS = 3;

/**
 * The named controls of the saved-on screen. Each one is a thing the user can
 * read or press; each one is removed in turn to prove the check notices.
 */
const NAMED_CONTROLS = Object.freeze([
  { name: "account-work", selector: '[data-account-choice="discord-work"]', text: "work@example.test" },
  { name: "account-spare", selector: '[data-account-choice="discord-spare"]', text: "spare@example.test" },
  { name: "send-style-manual", selector: '[data-send-style="manual"]', text: "Manual" },
  { name: "send-style-clipboard", selector: '[data-send-style="clipboard"]', text: "Clipboard" },
  { name: "open-app", selector: "#apps-sending-open-discord", text: "Open Discord" },
  { name: "set-up-app", selector: "#apps-sending-set-up-discord", text: "Set up" },
  { name: "remove-app", selector: "#apps-sending-remove-discord", text: "Remove app" },
  {
    name: "next-generation-copy",
    selector: '.apps-sending-switch[data-app-id="discord"] .apps-sending-switch-copy',
    text: "Next-generation messages",
  },
  // No text of its own: it is judged on being laid out and painted.
  { name: "next-generation-switch", selector: "#apps-sending-next-generation-discord", text: "" },
  { name: "next-generation-state", selector: "[data-next-generation-state]", text: "On" },
  { name: "reset", selector: "#apps-sending-reset", text: "Reset" },
  { name: "save", selector: "#apps-sending-save", text: "Save" },
]);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
}

async function startVite() {
  const server = await createServer({
    root: APP_ROOT,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("Vite did not expose a TCP address");
  return { server, url: `http://127.0.0.1:${address.port}/` };
}

async function evaluateValue(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (evaluated.exceptionDetails) {
    throw new Error(
      evaluated.exceptionDetails.exception?.description
        || evaluated.exceptionDetails.text
        || "page evaluation threw",
    );
  }
  return evaluated.result.value;
}

function accessibilityTreeText(nodes) {
  return nodes
    .flatMap((node) => [node.role?.value, node.name?.value, node.value?.value, node.description?.value])
    .filter((value) => typeof value === "string" && value.trim())
    .join("\n");
}

/**
 * Ink is measured against the most common colour of the box itself, so a light
 * control on a dark screen and a dark control on a light one both count.
 */
function regionInk(image, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(image.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  const samples = [];
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * image.width + x) * 4;
      const rgb = [image.pixels[offset], image.pixels[offset + 1], image.pixels[offset + 2]];
      const key = rgb.join(",");
      counts.set(key, (counts.get(key) ?? 0) + 1);
      samples.push(rgb);
    }
  }
  if (samples.length === 0) return { pixels: 0, ink: 0, distinctColours: 0 };
  let modal = "0,0,0";
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      modal = key;
    }
  }
  const [mr, mg, mb] = modal.split(",").map(Number);
  let ink = 0;
  for (const [r, g, b] of samples) {
    if (Math.abs(r - mr) + Math.abs(g - mg) + Math.abs(b - mb) > 24) ink += 1;
  }
  return { pixels: samples.length, ink, distinctColours: counts.size, modal };
}

/** Mean absolute RGB difference between the same rectangle of two screenshots. */
function rectMeanAbsDiff(left, right, rect) {
  const x0 = Math.round(rect.x);
  const y0 = Math.round(rect.y);
  const width = Math.round(rect.width);
  const height = Math.round(rect.height);
  assert.ok(width > 0 && height > 0, "compared rectangle is empty");
  let total = 0;
  let samples = 0;
  for (let y = y0; y < y0 + height; y += 1) {
    for (let x = x0; x < x0 + width; x += 1) {
      if (x < 0 || y < 0 || x >= left.width || y >= left.height) {
        throw new Error(`compared rectangle falls outside the screenshot at ${x},${y}`);
      }
      const offset = (y * left.width + x) * 4;
      total += Math.abs(left.pixels[offset] - right.pixels[offset]);
      total += Math.abs(left.pixels[offset + 1] - right.pixels[offset + 1]);
      total += Math.abs(left.pixels[offset + 2] - right.pixels[offset + 2]);
      samples += 3;
    }
  }
  return total / samples;
}

const RENDER = (model) => `new Promise((resolve, reject) => {
  const model = ${JSON.stringify(model)};
  const deadline = Date.now() + 15000;
  const tick = () => {
    if (typeof window.task0760Render === "function") {
      window.task0760Render(model);
      document.fonts.ready.then(() => requestAnimationFrame(() => requestAnimationFrame(() => resolve(true))));
      return;
    }
    if (Date.now() > deadline) {
      reject(new Error("Apps and sending screen did not render"));
      return;
    }
    setTimeout(tick, 25);
  };
  tick();
})`;

/** Take one named control out of the rendered copy, or refuse if it is absent. */
const REMOVE = (selector) => `new Promise((resolve, reject) => {
  const selector = ${JSON.stringify(selector)};
  const node = document.querySelector(selector);
  if (!node) { reject(new Error("nothing to remove for " + selector)); return; }
  node.remove();
  requestAnimationFrame(() => requestAnimationFrame(() => resolve(true)));
})`;

/** Leave the control in the DOM but paint nothing: the pixel half, on its own. */
const HIDE = (selector) => `new Promise((resolve, reject) => {
  const selector = ${JSON.stringify(selector)};
  const node = document.querySelector(selector);
  if (!node) { reject(new Error("nothing to hide for " + selector)); return; }
  node.style.visibility = "hidden";
  requestAnimationFrame(() => requestAnimationFrame(() => resolve(true)));
})`;

const READ_SCREEN = (controls) => `(() => {
  const rect = (element) => {
    const box = element?.getBoundingClientRect();
    return box ? { x: box.x, y: box.y, width: box.width, height: box.height } : null;
  };
  const controls = ${JSON.stringify(controls)};
  const found = {};
  for (const control of controls) {
    const node = document.querySelector(control.selector);
    found[control.name] = node
      ? { present: true, text: (node.textContent || "").replace(/\\s+/gu, " ").trim(), rect: rect(node) }
      : { present: false, text: "", rect: null };
  }
  const switchRow = document.querySelector('.apps-sending-switch[data-app-id="discord"]');
  const switchInput = document.querySelector("#apps-sending-next-generation-discord");
  return {
    controls: found,
    title: document.title,
    heading: document.querySelector("#apps-sending-title")?.textContent?.trim() || "",
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    connectedCount: Number(document.querySelector("[data-apps-sending-screen]")?.dataset.connectedCount ?? -1),
    nextGeneration: switchRow?.dataset.nextGeneration || "",
    nextGenerationLabel: document.querySelector("[data-next-generation-state]")?.textContent?.trim() || "",
    nextGenerationChecked: Boolean(switchInput?.checked),
    rects: {
      heading: rect(document.querySelector("#apps-sending-title")),
      switchRow: rect(switchRow),
      switchInput: rect(switchInput),
      statePill: rect(document.querySelector("[data-next-generation-state]")),
    },
  };
})()`;

/**
 * THE check. The saved screen and every throwaway copy go through this one
 * function, so "the check goes red" means the same code said no.
 */
function checkNamedControls(screen, shot, axText, windowSize) {
  const failures = [];
  const facts = {};
  for (const control of NAMED_CONTROLS) {
    const seen = screen.controls[control.name];
    if (!seen?.present || !seen.rect) {
      failures.push(`${control.name}: missing from the screen`);
      facts[control.name] = { present: false };
      continue;
    }
    const { rect } = seen;
    if (rect.width <= 0 || rect.height <= 0) {
      failures.push(`${control.name}: laid out with no area`);
      facts[control.name] = { present: true, rect, ink: 0 };
      continue;
    }
    if (rect.x < 0 || rect.y < 0 || rect.x + rect.width > windowSize.width || rect.y + rect.height > windowSize.height) {
      failures.push(`${control.name}: falls outside the ${windowSize.width}x${windowSize.height} window`);
    }
    if (control.text && !seen.text.includes(control.text)) {
      failures.push(`${control.name}: reads "${seen.text}" instead of "${control.text}"`);
    }
    if (control.text && !axText.toLowerCase().includes(control.text.toLowerCase())) {
      failures.push(`${control.name}: "${control.text}" is not in the accessibility tree`);
    }
    const ink = regionInk(shot, rect);
    if (ink.ink < MIN_INK_PIXELS || ink.distinctColours < MIN_DISTINCT_COLOURS) {
      failures.push(`${control.name}: its box is blank in the PNG (ink=${ink.ink} colours=${ink.distinctColours})`);
    }
    facts[control.name] = {
      present: true,
      rect: {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height),
      },
      ink: ink.ink,
      distinctColours: ink.distinctColours,
    };
  }
  return { failures, facts };
}

test("TASK 0762 photographs the fixed-size Linux Apps and sending screen with next-generation messages saved on, and the check fails when one named control is gone", async (t) => {
  const saved = JSON.parse(readFileSync(path.join(FIXTURE_DIR, FIXTURE_MODEL), "utf8"));
  const discord = saved.apps.find((app) => app.id === "discord");
  assert.ok(discord, "the fixture has no Discord app");
  assert.equal(discord.nextGeneration.requested, true, "the fixture does not save next-generation messages on");
  assert.equal(discord.nextGeneration.buildEnabled, true, "the fixture build cannot honour next-generation messages");

  // The same screen with the one setting saved off. Nothing else changes, so a
  // pixel difference between the two is that setting being drawn.
  const savedOff = JSON.parse(JSON.stringify(saved));
  savedOff.apps.find((app) => app.id === "discord").nextGeneration.requested = false;

  mkdirSync(EVIDENCE_DIR, { recursive: true });
  const { server, url } = await startVite();
  // The fixed Linux window size comes from the app itself, not from a number
  // typed into this file.
  const screenData = await server.ssrLoadModule("/src/linux-onboarding-screen-data.ts");
  const WINDOW = screenData.LINUX_ONBOARDING_SCREEN_WINDOW;
  assert.ok(WINDOW?.width > 0 && WINDOW?.height > 0, "the app does not declare a fixed Linux window size");
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

  /** Render, photograph and judge one copy of the screen. */
  const inspect = async (mutations = []) => {
    await evaluateValue(page, RENDER(saved));
    for (const mutation of mutations) await evaluateValue(page, mutation);
    const screen = await evaluateValue(page, READ_SCREEN(NAMED_CONTROLS));
    const png = await page.screenshot({ fromSurface: true });
    const shot = readPng(png);
    const ax = await page.send("Accessibility.getFullAXTree");
    const axText = [screen.text, accessibilityTreeText(ax.nodes ?? [])].join("\n");
    const verdict = checkNamedControls(screen, shot, axText, WINDOW);
    return { screen, png, shot, verdict };
  };

  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: WINDOW.width,
      screenHeight: WINDOW.height,
    });
    await page.navigate(`${url}${FIXTURE_PAGE}`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");

    // ---- 1. the named screenshot of the saved ON state -------------------
    const on = await inspect();
    assert.deepEqual(on.verdict.failures, [], `the saved-on screen failed its own check: ${on.verdict.failures.join(" | ")}`);
    assert.equal(on.shot.width, WINDOW.width);
    assert.equal(on.shot.height, WINDOW.height);
    assert.equal(on.screen.title, "Apps and sending");
    assert.equal(on.screen.heading, "Apps and sending");
    assert.equal(on.screen.connectedCount, 1);
    assert.equal(on.screen.nextGeneration, "on");
    assert.equal(on.screen.nextGenerationLabel, "On");
    assert.equal(on.screen.nextGenerationChecked, true);
    assert.match(on.screen.text, new RegExp(escapeRegExp("Next-generation messages"), "u"));
    assert.match(on.screen.text, new RegExp(escapeRegExp(discord.nextGeneration.detail), "u"));
    writeFileSync(PNG_PATH, on.png);
    const sha256 = createHash("sha256").update(on.png).digest("hex");

    // ---- the on state is PAINTED, not just asserted ----------------------
    await evaluateValue(page, RENDER(savedOff));
    const offScreen = await evaluateValue(page, READ_SCREEN(NAMED_CONTROLS));
    const offShot = readPng(await page.screenshot({ fromSurface: true }));
    assert.equal(offScreen.nextGeneration, "off");
    assert.equal(offScreen.nextGenerationLabel, "Off");
    assert.equal(offScreen.nextGenerationChecked, false);
    for (const key of ["heading", "switchRow", "switchInput", "statePill"]) {
      assert.deepEqual(
        on.screen.rects[key],
        offScreen.rects[key],
        `${key} moved between on and off; comparing its pixels would prove nothing`,
      );
    }
    const diffs = {
      switchRow: rectMeanAbsDiff(on.shot, offShot, on.screen.rects.switchRow),
      switchInput: rectMeanAbsDiff(on.shot, offShot, on.screen.rects.switchInput),
      statePill: rectMeanAbsDiff(on.shot, offShot, on.screen.rects.statePill),
      heading: rectMeanAbsDiff(on.shot, offShot, on.screen.rects.heading),
    };
    assert.ok(diffs.switchRow >= CHOICE_MIN_MEAN_ABS_DIFF, `the switch row looks the same on and off (${diffs.switchRow})`);
    assert.ok(diffs.switchInput >= CHOICE_MIN_MEAN_ABS_DIFF, `the tick box looks the same on and off (${diffs.switchInput})`);
    assert.ok(diffs.statePill >= CHOICE_MIN_MEAN_ABS_DIFF, `the On/Off pill looks the same either way (${diffs.statePill})`);
    assert.ok(diffs.heading <= UNTOUCHED_MAX_MEAN_ABS_DIFF, `the whole screen repainted, so the switch regions prove nothing (${diffs.heading})`);

    // ---- 2. a throwaway copy missing ONE named control fails the check ----
    const breakages = [];
    for (const control of NAMED_CONTROLS) {
      const broken = await inspect([REMOVE(control.selector)]);
      const failures = broken.verdict.failures;
      assert.ok(failures.length > 0, `removing ${control.name} did not fail the check`);
      assert.deepEqual(
        failures,
        [`${control.name}: missing from the screen`],
        `removing ${control.name} produced the wrong complaint: ${failures.join(" | ")}`,
      );
      breakages.push({ control: control.name, mutation: "removed", failures });
    }

    // The pixel half of the check, proved on its own: Save is still in the DOM
    // and still laid out, and the check still refuses because it paints nothing.
    const hidden = await inspect([HIDE("#apps-sending-save")]);
    assert.equal(hidden.screen.controls.save.present, true, "the hidden Save button left the DOM");
    assert.equal(hidden.verdict.failures.length, 1, `hiding Save produced ${hidden.verdict.failures.length} complaints`);
    assert.match(hidden.verdict.failures[0], /^save: its box is blank in the PNG/u);
    breakages.push({ control: "save", mutation: "hidden", failures: hidden.verdict.failures });

    // ---- and the screen put back is green again --------------------------
    const restored = await inspect();
    assert.deepEqual(restored.verdict.failures, [], "the restored screen did not come back green");

    writeFileSync(REPORT_PATH, JSON.stringify({
      task: "0762",
      platform: process.platform,
      url: `${url}${FIXTURE_PAGE}`,
      window: WINDOW,
      png: { path: PNG_PATH, bytes: on.png.length, sha256 },
      namedControls: NAMED_CONTROLS.map((control) => control.name),
      savedOn: {
        nextGeneration: on.screen.nextGeneration,
        label: on.screen.nextGenerationLabel,
        checked: on.screen.nextGenerationChecked,
        detail: discord.nextGeneration.detail,
      },
      controlFacts: on.verdict.facts,
      diffs,
      breakages,
    }, null, 2));

    console.log(`TASK0762_PLATFORM=${process.platform}`);
    console.log(`TASK0762_URL=${url}${FIXTURE_PAGE}`);
    console.log(`TASK0762_PNG=${PNG_PATH}`);
    console.log(`TASK0762_REPORT=${REPORT_PATH}`);
    console.log(`TASK0762_WINDOW=${on.shot.width}x${on.shot.height}`);
    console.log(`TASK0762_FIXED_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0762_PNG_BYTES=${on.png.length}`);
    console.log(`TASK0762_PNG_SHA256=${sha256}`);
    console.log(`TASK0762_PAGE_TITLE=${on.screen.title}`);
    console.log(`TASK0762_NEXT_GENERATION=${on.screen.nextGeneration}|${on.screen.nextGenerationLabel}|checked=${on.screen.nextGenerationChecked}`);
    console.log(`TASK0762_NEXT_GENERATION_DETAIL=${discord.nextGeneration.detail}`);
    console.log(`TASK0762_NAMED_CONTROL_COUNT=${NAMED_CONTROLS.length}`);
    console.log(`TASK0762_NAMED_CONTROLS=${NAMED_CONTROLS.map((control) => control.name).join("|")}`);
    for (const control of NAMED_CONTROLS) {
      const fact = on.verdict.facts[control.name];
      console.log(
        `TASK0762_CONTROL_${control.name.toUpperCase().replace(/-/gu, "_")}=rect=${fact.rect.x},${fact.rect.y},${fact.rect.width}x${fact.rect.height} ink=${fact.ink} colours=${fact.distinctColours}`,
      );
    }
    console.log(`TASK0762_SWITCH_ROW_ON_OFF_DIFF=${diffs.switchRow.toFixed(4)}`);
    console.log(`TASK0762_TICK_BOX_ON_OFF_DIFF=${diffs.switchInput.toFixed(4)}`);
    console.log(`TASK0762_STATE_PILL_ON_OFF_DIFF=${diffs.statePill.toFixed(4)}`);
    console.log(`TASK0762_HEADING_ON_OFF_DIFF=${diffs.heading.toFixed(4)}`);
    console.log(`TASK0762_SAVED_ON_FAILURES=${on.verdict.failures.length}`);
    for (const breakage of breakages) {
      console.log(`TASK0762_BREAK_${breakage.control.toUpperCase().replace(/-/gu, "_")}_${breakage.mutation.toUpperCase()}=${breakage.failures.join(" | ")}`);
    }
    console.log(`TASK0762_BREAKAGE_COUNT=${breakages.length}`);
    console.log(`TASK0762_RESTORED_FAILURES=${restored.verdict.failures.length}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
