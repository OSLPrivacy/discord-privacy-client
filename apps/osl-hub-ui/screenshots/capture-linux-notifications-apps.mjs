#!/usr/bin/env node

/**
 * TASK 0726 - capture the fixed-size Linux Notifications screen with two
 * connected apps, and prove the capture's check can go red.
 *
 * The screen is the REAL one: a real Vite dev server serves the real
 * `src/main.ts`, headless Chromium on Linux renders it, and the two app ticks
 * come from two linked (connected) services plus a real click on the Telegram
 * tick. The disabled tick then survives a full re-render, so it is app state
 * being shown, not a checkbox this script poked in the DOM.
 *
 * `--throwaway-missing <control>` renders the same screen, deletes exactly one
 * NAMED control from a throwaway copy of it, and runs the identical check. That
 * run must exit 1. A check that cannot fail is decoration.
 */

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_NAME = "task-0726-linux-notifications-two-apps.png";
const THROWAWAY_PNG_NAME = "task-0726-throwaway-missing-control.png";

/** The window is fixed: the screenshot must come out at exactly this size. */
const FIXED_WINDOW = { width: 1280, height: 1024 };

/** The two connected apps this screen is captured with. */
const CONNECTED_APPS = [
  { id: "discord", displayName: "Discord", tick: "on" },
  { id: "telegram", displayName: "Telegram", tick: "off" },
];

/**
 * Every control the Notifications screen must show BY NAME. Deleting any single
 * one of these from a copy of the screen has to turn the check red, which is
 * what `--throwaway-missing` proves.
 */
const NAMED_CONTROLS = [
  { name: "Local OSL activity", selector: "#notifications-opt-in" },
  { name: "Security changes", selector: "#notification-security-activity" },
  { name: "Show details", selector: "#notification-previews" },
  { name: "Suggest chat approval", selector: "#notification-scope-suggestions" },
  { name: "Encrypted chat alerts", selector: "#notification-chat-activity" },
  { name: "OSL Chat previews", selector: "#osl-chat-preview-toggle" },
  { name: "Connected apps", selector: "details.notification-apps > summary" },
  { name: "Discord", selector: '[data-notification-app="discord"]' },
  { name: "Telegram", selector: '[data-notification-app="telegram"]' },
];

const FIXTURE_SERVICES = [
  {
    id: "discord",
    displayName: "Discord",
    sidebarGlyph: "DC",
    sidebarOrder: 0,
    category: "consumer",
    launchState: "available",
    supportsNativePreview: true,
    supportsProtectedPreview: true,
    accounts: [{ id: "acct-discord-1", label: "Local profile", displayHandle: "local", state: "demoLinked", provider: null }],
  },
  {
    id: "telegram",
    displayName: "Telegram",
    sidebarGlyph: "TG",
    sidebarOrder: 1,
    category: "consumer",
    launchState: "available",
    supportsNativePreview: false,
    supportsProtectedPreview: true,
    accounts: [{ id: "acct-telegram-1", label: "Local profile", displayHandle: "local", state: "demoLinked", provider: null }],
  },
];

const FIXTURE_NOTICES = [
  { id: "notice-key-change", title: "Key change", detail: "A friend's encryption key changed", createdAt: "09:04" },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function parsePng(buffer) {
  const signature = buffer.subarray(0, 8);
  if (!signature.equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    throw new Error("screenshot is not a PNG");
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let bitDepth = 0;
  let colorType = 0;
  const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("ascii", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") {
      break;
    }
  }
  if (bitDepth !== 8 || ![2, 6].includes(colorType)) {
    throw new Error(`unsupported PNG encoding bitDepth=${bitDepth} colorType=${colorType}`);
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(idat));
  const pixels = Buffer.alloc(width * height * 4);
  let inOffset = 0;
  const prior = Buffer.alloc(stride);
  const row = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[inOffset];
    inOffset += 1;
    for (let x = 0; x < stride; x += 1) {
      const raw = inflated[inOffset + x];
      const left = x >= channels ? row[x - channels] : 0;
      const up = prior[x];
      const upLeft = x >= channels ? prior[x - channels] : 0;
      let value;
      if (filter === 0) value = raw;
      else if (filter === 1) value = raw + left;
      else if (filter === 2) value = raw + up;
      else if (filter === 3) value = raw + Math.floor((left + up) / 2);
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        value = raw + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft);
      } else {
        throw new Error(`unsupported PNG filter ${filter}`);
      }
      row[x] = value & 0xff;
    }
    inOffset += stride;
    for (let x = 0; x < width; x += 1) {
      const source = x * channels;
      const target = (y * width + x) * 4;
      pixels[target] = row[source];
      pixels[target + 1] = row[source + 1];
      pixels[target + 2] = row[source + 2];
      pixels[target + 3] = channels === 4 ? row[source + 3] : 255;
    }
    row.copy(prior);
  }
  return { width, height, pixels };
}

function cropFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const colors = new Set();
  let nonBackground = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const r = png.pixels[offset];
      const g = png.pixels[offset + 1];
      const b = png.pixels[offset + 2];
      colors.add(`${r},${g},${b}`);
      if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
    }
  }
  return { distinctColors: colors.size, nonBackground };
}

function imageFacts(buffer, rects) {
  const png = parsePng(buffer);
  const colors = new Set();
  let nonBackground = 0;
  for (let index = 0; index < png.pixels.length; index += 4) {
    const r = png.pixels[index];
    const g = png.pixels[index + 1];
    const b = png.pixels[index + 2];
    colors.add(`${r},${g},${b}`);
    if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
  }
  return {
    width: png.width,
    height: png.height,
    distinctColors: colors.size,
    nonBackground,
    nearlyBlank: colors.size < 20 || nonBackground < 2_000,
    crops: Object.fromEntries(Object.entries(rects).filter(([, rect]) => rect).map(([name, rect]) => [name, cropFacts(png, rect)])),
  };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

/**
 * The ONE check. The green run and the throwaway run both go through this, so
 * "the copy fails" cannot come from a softer check being applied to the copy.
 */
function verifyScreen(screen, facts) {
  const problems = [];

  const missing = NAMED_CONTROLS.filter((control) => !screen.controls[control.name]?.present).map((control) => control.name);
  if (missing.length) problems.push(`missing named controls: ${missing.join(", ")}`);

  for (const control of NAMED_CONTROLS) {
    const found = screen.controls[control.name];
    if (!found?.present) continue;
    if (!found.name.includes(control.name)) problems.push(`control ${control.selector} is named "${found.name}", expected "${control.name}"`);
    const rect = found.rect;
    if (!rect || rect.width <= 0 || rect.height <= 0) problems.push(`control ${control.name} has no box on screen`);
    else if (rect.x < 0 || rect.y < 0 || rect.x + rect.width > FIXED_WINDOW.width || rect.y + rect.height > FIXED_WINDOW.height) {
      problems.push(`control ${control.name} falls outside the fixed ${FIXED_WINDOW.width}x${FIXED_WINDOW.height} window`);
    }
  }

  if (screen.appRows.length !== CONNECTED_APPS.length) {
    problems.push(`expected ${CONNECTED_APPS.length} connected apps, found ${screen.appRows.length}: ${screen.appRows.map((row) => row.id).join(", ") || "none"}`);
  }
  for (const app of CONNECTED_APPS) {
    const row = screen.appRows.find((candidate) => candidate.id === app.id);
    if (!row) { problems.push(`connected app ${app.displayName} is missing from the Connected apps list`); continue; }
    if (row.displayName !== app.displayName) problems.push(`connected app ${app.id} is named "${row.displayName}", expected "${app.displayName}"`);
    const wanted = app.tick === "on";
    if (row.checked !== wanted) problems.push(`${app.displayName} tick is ${row.checked ? "enabled" : "disabled"}, expected ${wanted ? "enabled" : "disabled"}`);
  }
  const enabled = screen.appRows.filter((row) => row.checked).length;
  const disabled = screen.appRows.filter((row) => !row.checked).length;
  if (enabled < 1 || disabled < 1) problems.push(`the screenshot must show both an enabled and a disabled app tick; enabled=${enabled} disabled=${disabled}`);

  if (!screen.appsDisclosureOpen) problems.push("the Connected apps disclosure is shut, so the app ticks are not visible in the screenshot");

  const missingAx = NAMED_CONTROLS.filter((control) => !screen.axNames.some((name) => name.includes(control.name))).map((control) => control.name);
  if (missingAx.length) problems.push(`missing accessibility names: ${missingAx.join(", ")}`);

  if (facts) {
    if (facts.width !== FIXED_WINDOW.width || facts.height !== FIXED_WINDOW.height) {
      problems.push(`PNG is ${facts.width}x${facts.height}, expected the fixed ${FIXED_WINDOW.width}x${FIXED_WINDOW.height}`);
    }
    if (facts.nearlyBlank) problems.push(`PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
    for (const app of CONNECTED_APPS) {
      const crop = facts.crops[app.displayName];
      if (!crop) { problems.push(`no pixels were measured under the ${app.displayName} tick`); continue; }
      if (crop.nonBackground < 10) problems.push(`the ${app.displayName} tick box is blank in the PNG (nonBackground=${crop.nonBackground})`);
    }
  }

  return problems;
}

const PAGE_SETUP = `
  (() => {
    let nextCallback = 1;
    const callbacks = {};
    globalThis.__OSL_HUB_SKIP_AUTO_BOOTSTRAP = true;
    window.__TAURI_INTERNALS__ = {
      callbacks,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      transformCallback(callback, once = false) { const id = nextCallback++; callbacks[id] = { callback, once }; return id; },
      unregisterCallback(id) { delete callbacks[id]; },
      runCallback(id, args) { const entry = callbacks[id]; if (!entry) return; entry.callback(args); if (entry.once) delete callbacks[id]; },
      convertFileSrc(filePath) { return filePath; },
      invoke(cmd) {
        if (cmd === "plugin:window|is_maximized") return Promise.resolve(false);
        if (cmd === "plugin:window|is_focused") return Promise.resolve(true);
        if (cmd === "plugin:event|listen") return Promise.resolve(1);
        if (cmd === "plugin:event|unlisten") return Promise.resolve(null);
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") return Promise.resolve(null);
        return Promise.reject(new Error("TASK0726 capture Tauri stub refused " + cmd));
      },
    };
  })();
`;

function renderExpression(throwawayMissing) {
  return `(async () => {
    localStorage.clear();
    const ui = await import("/src/main.ts");
    const test = ui.__oslHubUiTest;
    const paint = () => {
      test.reset({
        route: "settings",
        coreReady: true,
        onboardingComplete: true,
        services: ${JSON.stringify(FIXTURE_SERVICES)},
        servicesChecked: true,
        notificationsEnabled: true,
        appNotifications: ${JSON.stringify(FIXTURE_NOTICES)},
      });
      test.renderSettingsSection("notifications");
    };
    paint();
    document.querySelector("#app").innerHTML = test.renderRouteShell("settings");
    test.bindWorkspace();
    // Turn one app's tick off through the control itself, not by writing state.
    const telegram = document.querySelector('[data-notification-app="telegram"]');
    if (!telegram) throw new Error("TASK0726: the Telegram tick was not on the screen to click");
    const before = telegram.checked;
    telegram.click();
    const after = document.querySelector('[data-notification-app="telegram"]').checked;
    // Repaint from app state: the disabled tick has to survive a full re-render.
    document.querySelector("#app").innerHTML = test.renderRouteShell("settings");
    test.bindWorkspace();
    const survived = document.querySelector('[data-notification-app="telegram"]').checked;

    const throwaway = ${JSON.stringify(throwawayMissing)};
    if (throwaway) {
      // A throwaway COPY of the screen, one named control short.
      const named = ${JSON.stringify(NAMED_CONTROLS)}.find((control) => control.name === throwaway);
      if (!named) throw new Error("TASK0726: no named control called " + throwaway);
      const copy = document.querySelector("#app").cloneNode(true);
      const target = copy.querySelector(named.selector);
      if (!target) throw new Error("TASK0726: " + throwaway + " was not in the copy to remove");
      (target.closest("label.notification-app-row, label.setting-line, summary") ?? target).remove();
      document.querySelector("#app").innerHTML = copy.innerHTML;
    }

    const details = document.querySelector("details.notification-apps");
    const summary = details?.querySelector("summary");
    if (summary) summary.click();

    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

    const box = (element) => {
      if (!element) return null;
      const rect = element.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    };
    const rowName = (element) => (element.closest("label")?.querySelector("strong")?.textContent
      ?? element.querySelector("strong")?.textContent
      ?? element.textContent
      ?? "").trim();

    const controls = {};
    for (const control of ${JSON.stringify(NAMED_CONTROLS)}) {
      const element = document.querySelector(control.selector);
      controls[control.name] = element
        ? { present: true, name: rowName(element), checked: element.checked ?? null, rect: box(element.closest("label") ?? element) }
        : { present: false, name: "", checked: null, rect: null };
    }

    const appRows = [...document.querySelectorAll("[data-notification-app]")].map((input) => ({
      id: input.dataset.notificationApp,
      displayName: (input.closest("label")?.querySelector("strong")?.textContent ?? "").trim(),
      checked: input.checked,
      rect: box(input),
    }));

    return {
      title: document.title,
      heading: document.querySelector(".hub-workspace h2")?.textContent?.trim() ?? "",
      tickClick: { before, after, survived },
      appsDisclosureOpen: Boolean(document.querySelector("details.notification-apps")?.open),
      appsSummary: document.querySelector("details.notification-apps > summary")?.textContent?.trim() ?? "",
      controls,
      appRows,
      throwawayRemoved: throwaway,
    };
  })()`;
}

async function main() {
  const throwawayIndex = process.argv.indexOf("--throwaway-missing");
  const throwawayMissing = throwawayIndex === -1 ? null : process.argv[throwawayIndex + 1];
  if (throwawayIndex !== -1 && !throwawayMissing) throw new Error("--throwaway-missing needs the name of one control");
  const pngPath = path.join(OUTPUT_DIR, throwawayMissing ? THROWAWAY_PNG_NAME : PNG_NAME);

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
      `--window-size=${FIXED_WINDOW.width},${FIXED_WINDOW.height}`,
      "about:blank",
    ],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", { source: PAGE_SETUP });
    await page.send("Emulation.setDeviceMetricsOverride", { ...FIXED_WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 30_000 });
    const screen = await evaluate(page, renderExpression(throwawayMissing));

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    screen.axNames = ax.nodes
      .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
      .filter(Boolean);

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(pngPath, screenshot);
    const rects = Object.fromEntries(screen.appRows.map((row) => [row.displayName, row.rect]));
    const facts = imageFacts(screenshot, rects);

    const tag = throwawayMissing ? "TASK0726_THROWAWAY" : "TASK0726";
    console.log(`${tag}_MODE=${throwawayMissing ? `copy missing "${throwawayMissing}"` : "the screen"}`);
    console.log(`${tag}_URL=${url}`);
    console.log(`${tag}_PNG=${pngPath}`);
    console.log(`${tag}_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`${tag}_FIXED_WINDOW=${FIXED_WINDOW.width}x${FIXED_WINDOW.height}`);
    console.log(`${tag}_PNG_SIZE=${facts.width}x${facts.height}`);
    console.log(`${tag}_SECTION=${screen.heading}`);
    console.log(`${tag}_APPS_SUMMARY=${screen.appsSummary}`);
    console.log(`${tag}_APPS_DISCLOSURE_OPEN=${screen.appsDisclosureOpen}`);
    console.log(`${tag}_CONNECTED_APP_COUNT=${screen.appRows.length}`);
    for (const row of screen.appRows) {
      const key = String(row.id).toUpperCase();
      console.log(`${tag}_APP_${key}=${row.displayName}`);
      console.log(`${tag}_APP_${key}_TICK=${row.checked ? "enabled" : "disabled"}`);
      console.log(`${tag}_APP_${key}_RECT=${Math.round(row.rect.x)},${Math.round(row.rect.y)},${Math.round(row.rect.width)}x${Math.round(row.rect.height)}`);
      console.log(`${tag}_APP_${key}_NONBACKGROUND=${facts.crops[row.displayName]?.nonBackground ?? 0}`);
    }
    console.log(`${tag}_TICK_CLICK=before=${screen.tickClick.before} after=${screen.tickClick.after} survivedRerender=${screen.tickClick.survived}`);
    console.log(`${tag}_NAMED_CONTROLS=${NAMED_CONTROLS.map((control) => control.name).join("|")}`);
    console.log(`${tag}_NAMED_CONTROLS_PRESENT=${NAMED_CONTROLS.filter((control) => screen.controls[control.name]?.present).length}/${NAMED_CONTROLS.length}`);
    console.log(`${tag}_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`${tag}_PNG_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`${tag}_PNG_NEARLY_BLANK=${facts.nearlyBlank}`);

    const problems = verifyScreen(screen, facts);
    if (screen.tickClick.after !== false || screen.tickClick.survived !== false) {
      problems.push(`clicking the Telegram tick did not leave it off (after=${screen.tickClick.after} afterRerender=${screen.tickClick.survived})`);
    }
    if (problems.length) {
      for (const problem of problems) console.log(`${tag}_PROBLEM=${problem}`);
      throw new Error(`${problems.length} check(s) failed: ${problems[0]}`);
    }
    console.log(`${tag}_CHECK=passed`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-linux-notifications-apps: ${error.message}`);
  process.exitCode = 1;
});
