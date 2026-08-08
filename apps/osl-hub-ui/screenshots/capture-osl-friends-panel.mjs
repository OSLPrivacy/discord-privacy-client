#!/usr/bin/env node

/**
 * TASK 0832 - capture the Home "OSL Friends" panel with two friends: one
 * showing a permitted picture, one showing a coloured initial.
 *
 * The screen is the REAL one: a real Vite dev server serves the real
 * `src/main.ts`, headless Chromium on Linux renders it, and the two rows come
 * from `oslFriendsPanelMarkup()` (task 0829) fed by `homeFriendRows`, the
 * shape `cmd_osl_read_home_friend_rows` (task 0828) returns. Clicking a row
 * exercises the real `resolveOslFriendsPanelRoute()` connector: the row opens
 * exactly its own friend page, and Back returns to Home.
 *
 * `--throwaway-missing <control>` renders the same screen, deletes exactly one
 * NAMED control from a throwaway copy of it, and runs the identical check. That
 * run must exit 1. A check that cannot fail is decoration.
 */

import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(SCRIPT_DIR, "..");
const OUTPUT_DIR = path.join(SCRIPT_DIR, "evidence");
const PNG_NAME = "task-0832-linux-osl-friends-panel.png";
const THROWAWAY_PNG_NAME = "task-0832-throwaway-missing-control.png";

const FIXED_WINDOW = { width: 1280, height: 1024 };

/** A bounded inline picture, the only shape 0828 ever permits onto a row. */
const ONE_PX_PNG = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

/** The exact two panel rows this capture proves: one picture, one initial. */
const FIXTURE_ROWS = [
  {
    friendId: "friend:aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11",
    oslUserId: "900000000000083201",
    username: "Ada Friend",
    picture: ONE_PX_PNG,
    pictureStatus: "image-present",
    initial: "A",
    initialColour: "#2563eb",
  },
  {
    friendId: "friend:bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22",
    oslUserId: "900000000000083202",
    username: "Bo Friend",
    picture: null,
    pictureStatus: "image-absent",
    initial: "B",
    initialColour: "#dc2626",
  },
];

/** Every control the panel must show BY NAME. Deleting one has to turn the check red. */
const NAMED_CONTROLS = [
  { name: "Ada Friend", selector: '[data-open-osl-friend="friend:aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11"]' },
  { name: "Bo Friend", selector: '[data-open-osl-friend="friend:bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22bb22"]' },
  { name: "OSL Friends", selector: "section.osl-friends-panel h2" },
  { name: "Back", selector: "[data-osl-friends-back]" },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function evaluate(page, expression) {
  const result = await page.send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description || result.exceptionDetails.text || "page evaluation failed");
  }
  return result.result.value;
}

/** The ONE check. The green run and the throwaway run both go through this. */
function verifyScreen(screen) {
  const problems = [];

  const missing = NAMED_CONTROLS.filter((control) => !screen.controls[control.name]?.present).map((control) => control.name);
  if (missing.length) problems.push(`missing named controls: ${missing.join(", ")}`);

  if (screen.friendRowCount !== FIXTURE_ROWS.length) {
    problems.push(`expected ${FIXTURE_ROWS.length} friend rows, found ${screen.friendRowCount}`);
  }
  if (screen.pictureRowCount !== 1) problems.push(`expected exactly 1 friend row with a picture, found ${screen.pictureRowCount}`);
  if (screen.initialRowCount !== 1) problems.push(`expected exactly 1 friend row with a coloured initial, found ${screen.initialRowCount}`);
  if (!screen.initialHasColour) problems.push("the coloured initial row has no background colour painted");
  if (!screen.pictureHasSrc) problems.push("the picture row's <img> has no src painted");

  if (!screen.routedToFriendPage) problems.push("activating Ada Friend's row did not route to her own friend page");
  if (screen.routedFriendId !== FIXTURE_ROWS[0].friendId) problems.push(`friend page opened id "${screen.routedFriendId}", expected Ada Friend's own id`);
  if (!screen.backReturnedHome) problems.push("Back did not return the screen to Home");

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
        return Promise.reject(new Error("TASK0832 capture Tauri stub refused " + cmd));
      },
    };
  })();
`;

function renderExpression(throwawayMissing) {
  return `(async () => {
    localStorage.clear();
    const ui = await import("/src/main.ts");
    const test = ui.__oslHubUiTest;
    test.reset({
      route: "home",
      coreReady: true,
      onboardingComplete: true,
      homeFriendRows: ${JSON.stringify(FIXTURE_ROWS)},
    });
    document.querySelector("#app").innerHTML = test.renderRouteShell("home");
    test.bindWorkspace();

    const throwaway = ${JSON.stringify(throwawayMissing)};
    if (throwaway) {
      // A throwaway COPY of the screen, one named control short.
      const named = ${JSON.stringify(NAMED_CONTROLS)}.find((control) => control.name === throwaway);
      if (!named) throw new Error("TASK0832: no named control called " + throwaway);
      const copy = document.querySelector("#app").cloneNode(true);
      const target = copy.querySelector(named.selector);
      if (!target) throw new Error("TASK0832: " + throwaway + " was not in the copy to remove");
      (target.closest("article.osl-friend-row, button") ?? target).remove();
      document.querySelector("#app").innerHTML = copy.innerHTML;
      // A throwaway copy is a dead clone: re-bind nothing, just count what is left.
      await document.fonts.ready;
      const rows = document.querySelectorAll("article.osl-friend-row");
      const controls = {};
      for (const control of ${JSON.stringify(NAMED_CONTROLS)}) {
        const element = document.querySelector(control.selector);
        controls[control.name] = { present: Boolean(element) };
      }
      return {
        controls,
        friendRowCount: rows.length,
        pictureRowCount: document.querySelectorAll("img.osl-friend-picture").length,
        initialRowCount: document.querySelectorAll("span.osl-friend-initial").length,
        initialHasColour: [...document.querySelectorAll("span.osl-friend-initial")].every((el) => /background:\\s*#[0-9a-f]{6}/i.test(el.getAttribute("style") ?? "")),
        pictureHasSrc: [...document.querySelectorAll("img.osl-friend-picture")].every((el) => (el.getAttribute("src") ?? "").startsWith("data:image/")),
        routedToFriendPage: false,
        routedFriendId: null,
        backReturnedHome: false,
        throwawayRemoved: throwaway,
      };
    }

    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

    const controls = {};
    for (const control of ${JSON.stringify(NAMED_CONTROLS)}) {
      const element = document.querySelector(control.selector);
      controls[control.name] = { present: Boolean(element) };
    }

    // Activate Ada Friend's row through the real control and the real click
    // handler, exactly as 0829's connector expects it.
    const adaRow = document.querySelector('[data-open-osl-friend="${FIXTURE_ROWS[0].friendId}"]');
    if (!adaRow) throw new Error("TASK0832: Ada Friend's row was not on the screen to click");
    adaRow.click();
    test.flushRenderForTest();
    const routedToFriendPage = test.currentRouteForTest() === "osl-friend";
    const routedFriendId = document.querySelector(".osl-friend-page-header h1")?.textContent === "${FIXTURE_ROWS[0].username}"
      ? "${FIXTURE_ROWS[0].friendId}"
      : null;

    // Back returns to Home, through the real control the friend page just painted.
    test.bindWorkspace();
    const homeBack = document.querySelector('[data-route="home"]');
    let backReturnedHome = false;
    if (homeBack) {
      homeBack.click();
      await new Promise((resolve) => setTimeout(resolve, 0));
      test.flushRenderForTest();
      backReturnedHome = test.currentRouteForTest() === "home";
    }

    // Repaint the Home panel for the screenshot itself.
    test.reset({
      route: "home",
      coreReady: true,
      onboardingComplete: true,
      homeFriendRows: ${JSON.stringify(FIXTURE_ROWS)},
    });
    document.querySelector("#app").innerHTML = test.renderRouteShell("home");
    test.bindWorkspace();
    await document.fonts.ready;
    await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));

    const rows = document.querySelectorAll("article.osl-friend-row");
    return {
      controls,
      friendRowCount: rows.length,
      pictureRowCount: document.querySelectorAll("img.osl-friend-picture").length,
      initialRowCount: document.querySelectorAll("span.osl-friend-initial").length,
      initialHasColour: [...document.querySelectorAll("span.osl-friend-initial")].every((el) => /background:\\s*#[0-9a-f]{6}/i.test(el.getAttribute("style") ?? "")),
      pictureHasSrc: [...document.querySelectorAll("img.osl-friend-picture")].every((el) => (el.getAttribute("src") ?? "").startsWith("data:image/")),
      routedToFriendPage,
      routedFriendId,
      backReturnedHome,
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

    const screenshot = await page.screenshot({ captureBeyondViewport: false });
    writeFileSync(pngPath, screenshot);

    const tag = throwawayMissing ? "TASK0832_THROWAWAY" : "TASK0832";
    console.log(`${tag}_MODE=${throwawayMissing ? `copy missing "${throwawayMissing}"` : "the screen"}`);
    console.log(`${tag}_URL=${url}`);
    console.log(`${tag}_PNG=${pngPath}`);
    console.log(`${tag}_PNG_SHA256=${sha256(screenshot)}`);
    console.log(`${tag}_FIXED_WINDOW=${FIXED_WINDOW.width}x${FIXED_WINDOW.height}`);
    console.log(`${tag}_FRIEND_ROW_COUNT=${screen.friendRowCount}`);
    console.log(`${tag}_PICTURE_ROW_COUNT=${screen.pictureRowCount}`);
    console.log(`${tag}_INITIAL_ROW_COUNT=${screen.initialRowCount}`);
    console.log(`${tag}_ROUTED_TO_FRIEND_PAGE=${screen.routedToFriendPage}`);
    console.log(`${tag}_ROUTED_FRIEND_ID=${screen.routedFriendId}`);
    console.log(`${tag}_BACK_RETURNED_HOME=${screen.backReturnedHome}`);
    console.log(`${tag}_NAMED_CONTROLS=${NAMED_CONTROLS.map((control) => control.name).join("|")}`);
    console.log(`${tag}_NAMED_CONTROLS_PRESENT=${NAMED_CONTROLS.filter((control) => screen.controls[control.name]?.present).length}/${NAMED_CONTROLS.length}`);

    const problems = verifyScreen(screen);
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
  console.error(`capture-osl-friends-panel: ${error.message}`);
  process.exitCode = 1;
});
