#!/usr/bin/env node
/**
 * TASK 3161 - prove a language change reaches an already-rendered screen
 * without restarting the app.
 *
 * Loads the verification-warning-screen fixture ONCE (one page, one
 * navigation), reads its English heading, then calls the SAME page's
 * `window.oslRenderVerificationWarningForLanguage("es")` — no reload, no
 * new tab, no process restart — and reads the heading again. The finish
 * line is that the two reads differ and the second one is the Spanish
 * words file's text, all inside that single page load.
 */
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const WINDOW = { width: 1280, height: 800 };

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

async function waitForFixtureReady(page) {
  await evaluate(page, `(async () => {
    const deadline = Date.now() + 20000;
    while (document.body.dataset.verificationWarningFixture !== "ready") {
      if (Date.now() > deadline) throw new Error("fixture never finished rendering");
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
  })()`);
}

async function readScreenState(page) {
  return evaluate(page, `(() => {
    const heading = document.querySelector("#verification-warning-heading");
    const legend = document.querySelector(".vw-options legend");
    const reset = document.querySelector("[data-verification-warning-reset]");
    const save = document.querySelector("[data-verification-warning-save]");
    return {
      language: document.body.dataset.verificationWarningLanguage,
      headingText: heading ? heading.textContent.trim() : null,
      legendText: legend ? legend.textContent.trim() : null,
      resetText: reset ? reset.textContent.trim() : null,
      saveText: save ? save.textContent.trim() : null,
    };
  })()`);
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const vite = await createServer({
    root: APP_ROOT,
    server: { host: "127.0.0.1", port: 0 },
    logLevel: "error",
  });
  await vite.listen();
  const url = `http://127.0.0.1:${vite.httpServer.address().port}/screenshots/verification-warning-screen.html`;
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

    // ONE navigation for this whole script — everything after this is the
    // SAME page, SAME running app instance, no restart.
    await page.navigate(url, { timeoutMs: 30_000 });
    await waitForFixtureReady(page);

    const beforeUrl = await evaluate(page, "location.href");
    const before = await readScreenState(page);

    // Switch language IN PLACE — this is the call a real language-picker
    // control would make via `language-store.ts`'s `setLanguage`.
    await evaluate(page, `window.oslRenderVerificationWarningForLanguage("es")`);
    await waitForFixtureReady(page);

    const afterUrl = await evaluate(page, "location.href");
    const after = await readScreenState(page);

    console.log(`TASK3161_URL=${url}`);
    console.log(`TASK3161_NAVIGATIONS=1 (before.url === after.url: ${beforeUrl === afterUrl})`);
    console.log(`TASK3161_BEFORE_LANGUAGE=${before.language}`);
    console.log(`TASK3161_BEFORE_HEADING=${before.headingText}`);
    console.log(`TASK3161_BEFORE_LEGEND=${before.legendText}`);
    console.log(`TASK3161_BEFORE_RESET=${before.resetText}`);
    console.log(`TASK3161_BEFORE_SAVE=${before.saveText}`);
    console.log(`TASK3161_AFTER_LANGUAGE=${after.language}`);
    console.log(`TASK3161_AFTER_HEADING=${after.headingText}`);
    console.log(`TASK3161_AFTER_LEGEND=${after.legendText}`);
    console.log(`TASK3161_AFTER_RESET=${after.resetText}`);
    console.log(`TASK3161_AFTER_SAVE=${after.saveText}`);

    if (beforeUrl !== afterUrl) {
      throw new Error("the page navigated between reads — this must prove an in-place change, not a reload");
    }
    if (before.language !== "en" || after.language !== "es") {
      throw new Error(`expected en -> es, got ${before.language} -> ${after.language}`);
    }
    if (before.headingText !== "Verification warning") {
      throw new Error(`expected English heading, got "${before.headingText}"`);
    }
    if (after.headingText !== "Advertencia de verificacion") {
      throw new Error(`expected Spanish heading, got "${after.headingText}"`);
    }
    if (before.headingText === after.headingText) {
      throw new Error("heading text did not change after the language switch");
    }
    if (before.resetText === after.resetText || before.saveText === after.saveText) {
      throw new Error("button words did not change after the language switch");
    }

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    const pngPath = path.join(OUTPUT_DIR, "task-3161-language-switch-es.png");
    writeFileSync(pngPath, screenshot);
    console.log(`TASK3161_AFTER_PNG=${pngPath}`);
    console.log("TASK3161_RESULT=pass");
  } finally {
    await page.close().catch(() => {});
    await chrome.close().catch(() => {});
    await vite.close().catch(() => {});
  }
}

main().catch((error) => {
  console.error(`capture-language-switch: ${error.stack || error.message}`);
  process.exitCode = 1;
});
