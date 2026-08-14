#!/usr/bin/env node

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_PATH = path.join(OUTPUT_DIR, "task-1404-scrub-account-choice.png");
const FIXTURE = "screenshots/task-1404-scrub-account-choice-fixture.html";
const WINDOW = { width: 1280, height: 800 };

const ACCOUNTS = [
  { accountId: "discord-maple", accountLabel: "Personal Discord", appOrBrowserLabel: "Discord app" },
  { accountId: "gmail-birch", accountLabel: "Work Gmail", appOrBrowserLabel: "Firefox" },
];
const TICKED_ACCOUNT_ID = "discord-maple";
const REQUIRED_TEXT = [
  "Which accounts can Scrub use?",
  "Tick an account to let Scrub work on it. Anything you leave unticked is not saved and Scrub never touches it.",
  ...ACCOUNTS.flatMap((account) => [account.accountLabel, account.appOrBrowserLabel]),
  "Back",
  "Continue",
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
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });

  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const address = vite.httpServer.address();
  const url = `http://127.0.0.1:${address.port}/`;

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
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });

    await evaluate(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task1404 === "ready") {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) {
          reject(new Error("account choice screen did not render"));
          return;
        }
        setTimeout(tick, 25);
      };
      tick();
    })`);

    const readScreen = await evaluate(page, `(() => {
      const box = (element) => {
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      const rows = [...document.querySelectorAll(".scrub-account-row")].map((row) => ({
        accountId: row.dataset.accountId,
        ticked: row.dataset.ticked,
        label: row.querySelector(".scrub-account-label")?.textContent?.trim(),
        where: row.querySelector(".scrub-account-where")?.textContent?.trim(),
        box: box(row),
        tickBox: box(row.querySelector(".osl-tick")),
      }));
      const titleElement = document.querySelector(".scrub-account-title");
      const leadElement = document.querySelector(".scrub-account-lead");
      return {
        title: { text: titleElement?.textContent?.trim(), box: box(titleElement) },
        lead: { text: leadElement?.textContent?.trim(), box: box(leadElement) },
        tickedCount: document.querySelector(".scrub-account-screen")?.dataset?.tickedCount,
        back: {
          text: document.querySelector("#scrub-accounts-back")?.textContent?.trim(),
          disabled: document.querySelector("#scrub-accounts-back")?.disabled ?? false,
          box: box(document.querySelector("#scrub-accounts-back")),
        },
        continue: {
          text: document.querySelector("#scrub-accounts-continue")?.textContent?.trim(),
          disabled: document.querySelector("#scrub-accounts-continue")?.disabled ?? false,
          ariaDisabled: document.querySelector("#scrub-accounts-continue")?.getAttribute("aria-disabled") ?? null,
          box: box(document.querySelector("#scrub-accounts-continue")),
        },
        rows,
        text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
      };
    })()`);

    // Finish-line structure checks.
    if (readScreen.title.text !== "Which accounts can Scrub use?") {
      throw new Error(`unexpected title: ${readScreen.title.text}`);
    }
    if (readScreen.rows.length !== 2) {
      throw new Error(`expected 2 account rows, got ${readScreen.rows.length}`);
    }
    const tickedRows = readScreen.rows.filter((row) => row.ticked === "yes");
    if (tickedRows.length !== 1) {
      throw new Error(`expected 1 ticked row, got ${tickedRows.length}`);
    }
    if (tickedRows[0].accountId !== TICKED_ACCOUNT_ID) {
      throw new Error(`expected ticked row ${TICKED_ACCOUNT_ID}, got ${tickedRows[0].accountId}`);
    }
    if (readScreen.back.text !== "Back") {
      throw new Error(`unexpected Back text: ${readScreen.back.text}`);
    }
    if (readScreen.continue.text !== "Continue") {
      throw new Error(`unexpected Continue text: ${readScreen.continue.text}`);
    }
    if (readScreen.continue.disabled || readScreen.continue.ariaDisabled === "true") {
      throw new Error("Continue is disabled even though one account is ticked");
    }
    for (const account of ACCOUNTS) {
      const row = readScreen.rows.find((r) => r.accountId === account.accountId);
      if (!row) throw new Error(`missing row for ${account.accountId}`);
      if (row.label !== account.accountLabel) throw new Error(`row label mismatch for ${account.accountId}`);
      if (row.where !== account.appOrBrowserLabel) throw new Error(`row where mismatch for ${account.accountId}`);
    }

    // Accessibility tree checks.
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes
      .map((node) => typeof node.name?.value === "string" ? node.name.value.trim() : "")
      .filter(Boolean);
    const axRoles = ax.nodes
      .map((node) => ({ role: node.role?.value, name: typeof node.name?.value === "string" ? node.name.value.trim() : "" }))
      .filter((entry) => entry.name);

    for (const text of REQUIRED_TEXT) {
      if (!axNames.includes(text)) {
        throw new Error(`missing accessibility name: ${text}`);
      }
    }
    const headings = axRoles.filter((entry) => entry.role === "heading").map((entry) => entry.name);
    if (!headings.includes("Which accounts can Scrub use?")) {
      throw new Error(`title is not a heading in the accessibility tree: ${headings.join(" | ")}`);
    }

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(PNG_PATH, screenshot);

    const rects = {
      title: readScreen.title.box,
      lead: readScreen.lead.box,
      back: readScreen.back.box,
      continue: readScreen.continue.box,
      tickedRow: tickedRows[0].box,
      tickedTick: tickedRows[0].tickBox,
      untickedRow: readScreen.rows.find((row) => row.ticked === "no")?.box,
    };
    for (const [name, rect] of Object.entries(rects)) {
      if (!rect) throw new Error(`missing rectangle for ${name}`);
    }
    const facts = imageFacts(screenshot, rects);
    const whole = facts.crops ? null : null;
    const blankCrops = Object.entries(facts.crops)
      .filter(([, crop]) => crop.distinctColors < 3 || crop.saturatedPixels < 10)
      .map(([name]) => name);

    if (facts.width !== WINDOW.width || facts.height !== WINDOW.height) {
      throw new Error(`unexpected PNG size ${facts.width}x${facts.height}`);
    }
    if (facts.distinctColors < 20) {
      throw new Error(`PNG nearly blank: ${facts.distinctColors} distinct colours`);
    }
    if (blankCrops.length > 0) {
      throw new Error(`visible element boxes are blank: ${blankCrops.join(", ")}`);
    }

    console.log(`TASK1404_URL=${url}${FIXTURE}`);
    console.log(`TASK1404_PNG=${PNG_PATH}`);
    console.log(`TASK1404_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`TASK1404_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK1404_TITLE=${readScreen.title.text}`);
    console.log(`TASK1404_LEAD=${readScreen.lead.text}`);
    console.log(`TASK1404_ROW_COUNT=${readScreen.rows.length}`);
    console.log(`TASK1404_TICKED_COUNT=${tickedRows.length}`);
    console.log(`TASK1404_TICKED_ACCOUNT=${tickedRows[0].accountId}`);
    console.log(`TASK1404_BACK=${readScreen.back.text}`);
    console.log(`TASK1404_CONTINUE=${readScreen.continue.text}`);
    console.log(`TASK1404_CONTINUE_DISABLED=${readScreen.continue.disabled || readScreen.continue.ariaDisabled === "true"}`);
    console.log(`TASK1404_PNG_BYTES=${screenshot.length}`);
    console.log(`TASK1404_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    for (const [name, crop] of Object.entries(facts.crops)) {
      console.log(`TASK1404_CROP_${name.toUpperCase()}=${crop.width}x${crop.height} colors=${crop.distinctColors} saturated=${crop.saturatedPixels}`);
    }
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`task-1404-scrub-account-choice-capture: ${error.stack || error.message}`);
  process.exitCode = 1;
});
