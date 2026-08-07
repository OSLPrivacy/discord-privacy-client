#!/usr/bin/env node

// TASK 0724 -- build the Notifications screen.
//
// Finish line: "a Linux screenshot shows every choice with its current state."
// So this capture renders the real Notifications settings section in headless
// Chromium on Linux, checks that all eight choices are present, reads the state
// each one is showing, proves the state words are actually painted (not just in
// the DOM), and only then writes the PNG.

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
const PNG_PATH = path.join(OUTPUT_DIR, "task-0724-linux-notifications.png");

const WINDOW = { width: 1280, height: 1024 };
const TITLE = "Settings";
const SECTION_TITLE = "Activity";

/** The eight choices TASK 0724 asks for, in the order they are drawn. */
const REQUIRED_CHOICES = [
  { key: "local", title: "Local OSL activity" },
  { key: "security", title: "Security changes" },
  { key: "details", title: "Show details" },
  { key: "approval", title: "Suggest chat approval" },
  { key: "mute", title: "Mute alerts" },
  { key: "chat", title: "Encrypted chat alerts" },
  { key: "preview", title: "OSL Chat previews" },
  { key: "app:discord", title: "Discord" },
  { key: "app:telegram", title: "Telegram" },
];

/** The state each choice must be showing in this fixture. */
const EXPECTED_STATE = {
  local: "on",
  security: "on",
  details: "off",
  approval: "on",
  mute: "on",
  chat: "off",
  preview: "on",
  "app:discord": "on",
  "app:telegram": "off",
};

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

export function parsePng(buffer) {
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

export function cropFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const colors = new Set();
  let nonBackground = 0;
  let brightPixels = 0;
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const r = png.pixels[offset];
      const g = png.pixels[offset + 1];
      const b = png.pixels[offset + 2];
      colors.add(`${r},${g},${b}`);
      if (Math.abs(r - 8) + Math.abs(g - 12) + Math.abs(b - 13) > 24) nonBackground += 1;
      if (r + g + b > 360) brightPixels += 1;
    }
  }
  return { width: x1 - x0, height: y1 - y0, distinctColors: colors.size, nonBackground, brightPixels };
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
  const crops = Object.fromEntries(
    Object.entries(rects).filter(([, rect]) => rect).map(([name, rect]) => [name, cropFacts(png, rect)]),
  );
  return {
    width: png.width,
    height: png.height,
    distinctColors: colors.size,
    nonBackground,
    nearlyBlank: colors.size < 20 || nonBackground < 2_000,
    crops,
  };
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

function slug(name) {
  return name.toUpperCase().replace(/[^A-Z0-9]+/gu, "_").replace(/^_|_$/gu, "");
}

async function main() {
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
  try {
    await page.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
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
              return Promise.reject(new Error("TASK0724 capture Tauri stub refused " + cmd));
            },
          };
        })();
      `,
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 30_000 });

    const rendered = await evaluate(page, `(async () => {
      localStorage.clear();
      const ui = await import("/src/main.ts");
      const app = (id, displayName, order) => ({
        id,
        displayName,
        sidebarGlyph: displayName.slice(0, 2).toUpperCase(),
        sidebarOrder: order,
        category: "consumer",
        launchState: "available",
        accounts: [],
      });
      ui.__oslHubUiTest.reset({
        route: "settings",
        coreReady: true,
        services: [app("discord", "Discord", 0), app("telegram", "Telegram", 1)],
        hubPeople: [{ personId: "friend-1", alias: "Rose", safetyNumberVerified: true }],
        appNotifications: [{ id: "key-1", title: "Key change", detail: "Rose changed keys", createdAt: "Now" }],
        notificationsEnabled: true,
        notificationSecurityActivity: true,
        notificationPreviewContent: false,
        notificationScopeSuggestions: true,
        notificationsMuted: true,
        notificationChatActivity: false,
        oslChatPreviewsVisible: true,
        oslChatMutedPeople: ["friend-1"],
        notificationAppPreferences: { discord: true, telegram: false },
      });
      ui.__oslHubUiTest.renderSettingsSection("notifications");
      document.querySelector("#app").innerHTML =
        '<div class="app-frame with-titlebar"><header class="desktop-titlebar" aria-hidden="true"></header>'
        + ui.__oslHubUiTest.renderRouteShell("settings")
        + '</div>';
      await document.fonts.ready;
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

      const box = (element) => {
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      const rows = [...document.querySelectorAll("[data-notification-choice]")];
      const choices = rows.map((row) => {
        const word = row.querySelector(".choice-state-word");
        const tick = row.querySelector('input[type="checkbox"]');
        return {
          key: row.dataset.notificationChoice,
          state: row.dataset.choiceState ?? null,
          title: row.querySelector("strong")?.textContent.trim() ?? "",
          explanation: row.querySelector("small")?.textContent.trim() ?? "",
          stateWord: word ? word.textContent.trim() : null,
          checked: tick ? tick.checked : null,
          titleRect: box(row.querySelector("strong")),
          stateRect: box(word),
          rowRect: box(row),
        };
      });
      return {
        title: document.querySelector("#route-heading")?.textContent.trim() ?? "",
        sectionTitle: document.querySelector(".settings-detail h2")?.textContent.trim() ?? "",
        choices,
        mutedCount: document.querySelector('[data-notification-choice="mute-chats"]')?.dataset.mutedCount ?? null,
        appsSummary: document.querySelector(".notification-apps summary small")?.textContent.trim() ?? "",
        visibleText: document.body.innerText,
        headingRect: box(document.querySelector("#route-heading")),
        sectionRect: box(document.querySelector(".settings-detail h2")),
      };
    })()`);

    const byKey = new Map(rendered.choices.map((choice) => [choice.key, choice]));
    const missing = REQUIRED_CHOICES.filter((choice) => !byKey.has(choice.key)).map((choice) => choice.key);
    if (missing.length > 0) throw new Error(`missing notification choices: ${missing.join(", ")}`);

    for (const required of REQUIRED_CHOICES) {
      const choice = byKey.get(required.key);
      if (choice.title !== required.title) {
        throw new Error(`choice ${required.key} is named "${choice.title}", expected "${required.title}"`);
      }
      if (!choice.explanation) throw new Error(`choice ${required.key} has no explanation under it`);
      const expected = EXPECTED_STATE[required.key];
      if (choice.state !== expected) {
        throw new Error(`choice ${required.key} shows state "${choice.state}", expected "${expected}"`);
      }
      const word = expected === "on" ? "On" : "Off";
      if (choice.stateWord !== word) {
        throw new Error(`choice ${required.key} state word is "${choice.stateWord}", expected "${word}"`);
      }
      if (choice.checked !== (expected === "on")) {
        throw new Error(`choice ${required.key} tick is ${choice.checked}, which disagrees with its state word`);
      }
      const rect = choice.rowRect;
      if (!rect || rect.width <= 0 || rect.height <= 0) throw new Error(`choice ${required.key} has no painted box`);
      if (rect.y < 0 || rect.y + rect.height > WINDOW.height) {
        throw new Error(`choice ${required.key} falls outside the ${WINDOW.width}x${WINDOW.height} window at y=${Math.round(rect.y)}`);
      }
    }
    if (rendered.title !== TITLE) throw new Error(`page title is "${rendered.title}", expected "${TITLE}"`);
    if (rendered.sectionTitle !== SECTION_TITLE) throw new Error(`section title is "${rendered.sectionTitle}"`);
    if (rendered.mutedCount !== "1") throw new Error(`muted chat count is ${rendered.mutedCount}, expected 1`);

    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => (typeof node.name?.value === "string" ? node.name.value.trim() : ""))
      .filter(Boolean);
    const requiredAx = REQUIRED_CHOICES.map((choice) => `${choice.title}: ${EXPECTED_STATE[choice.key] === "on" ? "On" : "Off"}`);
    const missingAx = requiredAx.filter((name) => !axNames.some((candidate) => candidate === name || candidate.includes(name)));
    if (missingAx.length > 0) throw new Error(`missing accessibility names: ${missingAx.join(", ")}`);

    const rects = { Settings: rendered.headingRect, Activity: rendered.sectionRect };
    for (const required of REQUIRED_CHOICES) {
      const choice = byKey.get(required.key);
      rects[required.title] = choice.titleRect;
      rects[`${required.title} state`] = choice.stateRect;
    }

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);
    const facts = imageFacts(screenshot, rects);
    if (facts.width !== WINDOW.width || facts.height !== WINDOW.height) {
      throw new Error(`unexpected PNG size ${facts.width}x${facts.height}`);
    }
    if (facts.nearlyBlank) {
      throw new Error(`PNG is blank or nearly blank distinctColors=${facts.distinctColors} nonBackground=${facts.nonBackground}`);
    }
    const blank = Object.entries(facts.crops).filter(([, crop]) => crop.nonBackground < 10 || crop.brightPixels < 5).map(([name]) => name);
    if (blank.length > 0) throw new Error(`label boxes are blank in the image: ${blank.join(", ")}`);

    console.log(`TASK0724_URL=${url}`);
    console.log(`TASK0724_PNG=${PNG_PATH}`);
    console.log(`TASK0724_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK0724_FIXED_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0724_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0724_TITLE=${rendered.title}`);
    console.log(`TASK0724_SECTION_TITLE=${rendered.sectionTitle}`);
    console.log(`TASK0724_CHOICE_COUNT=${REQUIRED_CHOICES.length}`);
    for (const required of REQUIRED_CHOICES) {
      const choice = byKey.get(required.key);
      const key = slug(required.key);
      console.log(`TASK0724_CHOICE_${key}=${choice.title}`);
      console.log(`TASK0724_CHOICE_${key}_STATE=${choice.stateWord}`);
      console.log(`TASK0724_CHOICE_${key}_TICK=${choice.checked}`);
      console.log(`TASK0724_CHOICE_${key}_EXPLANATION=${choice.explanation}`);
      const crop = facts.crops[`${required.title} state`];
      console.log(`TASK0724_CHOICE_${key}_STATE_RECT=${Math.round(choice.stateRect.x)},${Math.round(choice.stateRect.y)},${Math.round(choice.stateRect.width)}x${Math.round(choice.stateRect.height)}`);
      console.log(`TASK0724_CHOICE_${key}_STATE_NONBACKGROUND=${crop.nonBackground}`);
    }
    console.log(`TASK0724_MUTED_CHATS=${rendered.mutedCount}`);
    console.log(`TASK0724_APPS_SUMMARY=${rendered.appsSummary}`);
    console.log(`TASK0724_TREE_NAMES=${requiredAx.join("|")}`);
    console.log(`TASK0724_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0724_PNG_NONBACKGROUND=${facts.nonBackground}`);
    console.log(`TASK0724_PNG_NEARLY_BLANK=${facts.nearlyBlank}`);
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

// Guarded so `parsePng`/`cropFacts` can be imported by other captures instead
// of being copied a fourth time; running the file directly still captures.
const isCli = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isCli) {
  main().catch((error) => {
    console.error(`capture-linux-notifications: ${error.stack || error.message}`);
    process.exitCode = 1;
  });
}
