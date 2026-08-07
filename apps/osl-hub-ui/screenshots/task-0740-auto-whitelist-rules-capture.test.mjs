/**
 * TASK 0740 - a Linux screenshot of the auto-whitelist rules screen shows every
 * place kind and the rule selected for it.
 *
 * The check does not stop at "a PNG was written". It reads the screenshot back
 * and, for every one of the 38 place kind rows the page reported, asserts that
 * the row's name and its selected-rule chip carry ink, that the marker beside
 * the selected choice is painted in the brand colour, and that an unselected
 * marker on the same row is not. Rows are also required to sit inside the
 * captured viewport, so a screen that renders every kind below the fold fails
 * here rather than passing on DOM evidence alone.
 *
 * After the capture it drives the two buttons in the real browser: a change
 * makes the status line say so, Save hands back one rule per place kind, and
 * Reset puts all 38 back to never.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0740-auto-whitelist-rules.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0740-auto-whitelist-rules-screen-tree.json");
const FIXTURE = "screenshots/task-0740-auto-whitelist-rules-fixture.html";

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
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) {
    throw new Error(
      evaluated.exceptionDetails.exception?.description || evaluated.exceptionDetails.text || "page evaluation threw",
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

/** What the page can tell us about every drawn row. */
const READ_SCREEN = `(() => {
  const rows = [...document.querySelectorAll(".rule-row")].map((row) => {
    const box = row.getBoundingClientRect();
    const name = row.querySelector(".rule-row-name");
    const chip = row.querySelector(".rule-row-selected");
    const choices = [...row.querySelectorAll(".rule-choice")].map((choice) => {
      const marker = choice.querySelector(".rule-choice-marker").getBoundingClientRect();
      return {
        choice: choice.dataset.choice,
        state: choice.dataset.state,
        checked: choice.querySelector(".rule-choice-input").checked,
        label: choice.querySelector(".rule-choice-label").textContent.trim(),
        marker: { x: marker.x, y: marker.y, width: marker.width, height: marker.height },
      };
    });
    const nameBox = name.getBoundingClientRect();
    const chipBox = chip.getBoundingClientRect();
    return {
      ruleKey: row.dataset.ruleKey,
      app: row.dataset.app,
      kind: row.dataset.kind,
      selected: row.dataset.selected,
      name: name.textContent.trim(),
      chip: chip.textContent.trim(),
      choices,
      box: { x: box.x, y: box.y, width: box.width, height: box.height },
      nameBox: { x: nameBox.x, y: nameBox.y, width: nameBox.width, height: nameBox.height },
      chipBox: { x: chipBox.x, y: chipBox.y, width: chipBox.width, height: chipBox.height },
    };
  });
  return {
    rows,
    rowCount: rows.length,
    groups: [...document.querySelectorAll(".rule-group")].map((group) => ({
      app: group.dataset.app,
      heading: group.querySelector(".rule-group-heading").textContent.trim(),
      rows: group.querySelectorAll(".rule-row").length,
    })),
    legend: [...document.querySelectorAll(".rule-legend-item")].map((item) => ({
      choice: item.dataset.choice,
      name: item.querySelector(".rule-legend-name").textContent.trim(),
      text: item.querySelector(".rule-legend-text").textContent.trim(),
    })),
    buttons: [...document.querySelectorAll("[data-rule-action]")].map((button) => ({
      action: button.dataset.ruleAction,
      text: button.textContent.trim(),
      explanation: button.parentElement.querySelector(".rule-action-text").textContent.trim(),
    })),
    status: document.querySelector(".rule-status").textContent.trim(),
    scrollHeight: document.documentElement.scrollHeight,
    scrollWidth: document.documentElement.scrollWidth,
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

test("TASK 0740 captures every place kind with its selected rule", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const screenData = await server.ssrLoadModule("/src/auto-whitelist-rules-screen-data.ts");
  const screenModule = await server.ssrLoadModule("/src/auto-whitelist-rules-screen.ts");
  const window = screenData.AUTO_WHITELIST_RULES_SCREEN_WINDOW;
  const required = screenData.AUTO_WHITELIST_RULES_SCREEN_REQUIRED_TEXT;
  const saved = screenData.AUTO_WHITELIST_RULES_SCREEN_SAVED;
  const places = screenModule.AUTO_WHITELIST_PLACE_KINDS;
  const choices = screenModule.AUTO_WHITELIST_CHOICES;

  // Everything before the browser opens still has to leave the dev server shut,
  // or a failure here hangs the runner instead of reporting itself.
  const started = await (async () => {
    try {
      // Every place kind carries a rule in the fixture, and all four choices are
      // on screen, or this task's finish line is not the one being measured.
      assert.equal(places.length, 38);
      assert.equal(Object.keys(saved).length, places.length);
      assert.deepEqual(
        [...new Set(places.map((place) => saved[place.ruleKey]))].sort(),
        choices.map((choice) => choice.id).sort(),
      );
      const browser = await launchChrome({
        args: [
          "--headless=new",
          "--remote-debugging-port=0",
          "--no-sandbox",
          "--disable-gpu",
          "--force-device-scale-factor=1",
          `--window-size=${window.width},${window.height}`,
          "about:blank",
        ],
      });
      return { chrome: browser, page: await browser.openPage() };
    } catch (error) {
      await server.close();
      throw error;
    }
  })();
  const { chrome, page } = started;
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: window.width,
      height: window.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: window.width,
      screenHeight: window.height,
    });
    await page.navigate(`${url}${FIXTURE}`, { timeoutMs: 30_000 });
    await page.send("Accessibility.enable");

    await evaluateValue(page, `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task0740 === "ready"
          && document.querySelectorAll(".rule-row").length === ${places.length}) {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) {
          reject(new Error("rules screen did not render"));
          return;
        }
        setTimeout(tick, 25);
      };
      tick();
    })`);

    const screen = await evaluateValue(page, READ_SCREEN);

    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = [screen.text, accessibilityTreeText(ax.nodes ?? [])].join("\n").toLowerCase();
    for (const phrase of required) {
      assert.match(treeText, new RegExp(escapeRegExp(phrase.toLowerCase()), "u"), `missing on screen: ${phrase}`);
    }

    // One row per place kind, in catalogue order, each showing its saved rule.
    assert.equal(screen.rowCount, places.length);
    assert.deepEqual(screen.rows.map((row) => row.ruleKey), places.map((place) => place.ruleKey));
    assert.deepEqual(screen.rows.map((row) => row.name), places.map((place) => place.kindLabel));
    assert.deepEqual(screen.rows.map((row) => row.selected), places.map((place) => saved[place.ruleKey]));
    assert.deepEqual(
      screen.rows.map((row) => row.chip),
      places.map((place) => choices.find((choice) => choice.id === saved[place.ruleKey]).label),
    );
    assert.deepEqual(
      [...new Set(screen.rows.map((row) => row.selected))].sort(),
      choices.map((choice) => choice.id).sort(),
    );

    // Each row offers all four choices and has exactly one of them on.
    for (const row of screen.rows) {
      assert.deepEqual(row.choices.map((choice) => choice.choice), choices.map((choice) => choice.id));
      const on = row.choices.filter((choice) => choice.state === "on");
      assert.equal(on.length, 1, `${row.ruleKey} has ${on.length} choices on`);
      assert.equal(on[0].choice, row.selected);
      assert.equal(row.choices.filter((choice) => choice.checked).length, 1);
      assert.equal(row.choices.find((choice) => choice.checked).choice, row.selected);
    }

    // Everything is inside the captured picture, not below the fold.
    assert.ok(screen.scrollHeight <= window.height, `page is ${screen.scrollHeight}px tall, taller than the ${window.height}px capture`);
    assert.ok(screen.scrollWidth <= window.width, `page is ${screen.scrollWidth}px wide, wider than the ${window.width}px capture`);
    for (const row of screen.rows) {
      assert.ok(
        row.box.x >= 0 && row.box.y >= 0
          && row.box.x + row.box.width <= window.width
          && row.box.y + row.box.height <= window.height,
        `${row.ruleKey} row is outside the capture: ${JSON.stringify(row.box)}`,
      );
    }

    // The legend and the two buttons, each with its short explanation.
    assert.deepEqual(screen.legend.map((item) => item.choice), choices.map((choice) => choice.id));
    assert.deepEqual(screen.legend.map((item) => item.text), choices.map((choice) => choice.explanation));
    assert.deepEqual(screen.buttons.map((button) => button.action), ["save", "reset"]);
    assert.deepEqual(screen.buttons.map((button) => button.text), ["Save rules", "Reset"]);
    assert.equal(screen.buttons[0].explanation, screenModule.SAVE_RULES_EXPLANATION);
    assert.equal(screen.buttons[1].explanation, screenModule.RESET_RULES_EXPLANATION);
    assert.equal(screen.status, `All ${places.length} place rules saved.`);

    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);

    // Two crops per row for the rule, two for the words: the marker beside the
    // selected choice, one unselected marker on the same row, the place kind
    // name and the chip that spells the selected rule out.
    const rects = {};
    for (const row of screen.rows) {
      const on = row.choices.find((choice) => choice.state === "on");
      const off = row.choices.find((choice) => choice.state === "off");
      rects[`${row.ruleKey}|on`] = on.marker;
      rects[`${row.ruleKey}|off`] = off.marker;
      rects[`${row.ruleKey}|name`] = row.nameBox;
      rects[`${row.ruleKey}|chip`] = row.chipBox;
    }
    const facts = imageFacts(png, rects);
    writeFileSync(
      TREE_PATH,
      JSON.stringify({ url: `${url}${FIXTURE}`, window, required, screen, facts, axNodes: ax.nodes }, null, 2),
    );

    assert.equal(facts.width, window.width);
    assert.equal(facts.height, window.height);
    assert.ok(png.length > 5_000, `PNG too small: ${png.length}`);
    assert.ok(facts.distinctColors > 20, `PNG nearly blank: ${facts.distinctColors} distinct colours`);

    // The selected rule is visible in the pixels, not merely set in the DOM.
    // The marker for the chosen rule is the brand colour, which no part of the
    // grey chrome can produce; the marker beside a rule that was not chosen is
    // grey. A screen that drew 38 identical rows fails both halves.
    let paintedRows = 0;
    for (const row of screen.rows) {
      const on = facts.crops[`${row.ruleKey}|on`];
      const off = facts.crops[`${row.ruleKey}|off`];
      const name = facts.crops[`${row.ruleKey}|name`];
      const chip = facts.crops[`${row.ruleKey}|chip`];
      const onShare = on.saturatedPixels / on.pixels;
      const offShare = off.saturatedPixels / off.pixels;
      assert.ok(onShare > 0.6, `${row.ruleKey}: selected marker did not paint (${on.saturatedPixels}/${on.pixels})`);
      assert.ok(offShare < 0.05, `${row.ruleKey}: an unselected marker is painted (${off.saturatedPixels}/${off.pixels})`);
      assert.ok(name.distinctColors > 3, `${row.ruleKey}: place kind name has no ink (${name.distinctColors} colours)`);
      assert.ok(chip.distinctColors > 3, `${row.ruleKey}: selected rule chip has no ink (${chip.distinctColors} colours)`);
      paintedRows += 1;
    }
    assert.equal(paintedRows, places.length);

    console.log(`TASK0740_PNG=${PNG_PATH}`);
    console.log(`TASK0740_TREE=${TREE_PATH}`);
    console.log(`TASK0740_URL=${url}${FIXTURE}`);
    console.log(`TASK0740_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0740_PNG_BYTES=${png.length}`);
    console.log(`TASK0740_PNG_SHA256=${createHash("sha256").update(png).digest("hex")}`);
    console.log(`TASK0740_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0740_PLACE_KIND_COUNT=${screen.rowCount}`);
    console.log(`TASK0740_APP_COUNT=${screen.groups.length}`);
    console.log(`TASK0740_CHOICES=${choices.map((choice) => choice.label).join("|")}`);
    console.log(`TASK0740_STATUS=${screen.status}`);
    for (const group of screen.groups) {
      console.log(`TASK0740_GROUP=${group.app}|${group.rows} rows|${group.heading}`);
    }
    for (const row of screen.rows) {
      const on = facts.crops[`${row.ruleKey}|on`];
      const off = facts.crops[`${row.ruleKey}|off`];
      console.log(
        `TASK0740_ROW=${row.ruleKey}|${row.app} ${row.name}|selected=${row.chip}`
        + `|marker_on=${on.saturatedPixels}/${on.pixels} ${on.meanColor}`
        + `|marker_off=${off.saturatedPixels}/${off.pixels} ${off.meanColor}`,
      );
    }
    const counts = {};
    for (const row of screen.rows) counts[row.selected] = (counts[row.selected] ?? 0) + 1;
    for (const choice of choices) console.log(`TASK0740_SELECTED_${choice.id}=${counts[choice.id] ?? 0}`);

    // Save rules and Reset, driven in the browser the screenshot came from.
    const changed = await evaluateValue(page, `(() => {
      const row = document.querySelector('.rule-row[data-rule-key="discord:thread"]');
      row.querySelector('.rule-choice[data-choice="always"] .rule-choice-input').click();
      const after = document.querySelector('.rule-row[data-rule-key="discord:thread"]');
      return {
        status: document.querySelector(".rule-status").textContent.trim(),
        selected: after.dataset.selected,
        chip: after.querySelector(".rule-row-selected").textContent.trim(),
        changedRows: document.querySelectorAll('.rule-row[data-changed="yes"]').length,
        savedPayload: window.oslLastSavedRules,
      };
    })()`);
    assert.equal(changed.selected, "always");
    assert.equal(changed.chip, "always");
    assert.equal(changed.changedRows, 1);
    assert.equal(changed.status, "1 place rule changed - not saved yet.");
    assert.equal(changed.savedPayload, null);

    const savedRun = await evaluateValue(page, `(() => {
      document.querySelector('[data-rule-action="save"]').click();
      return {
        status: document.querySelector(".rule-status").textContent.trim(),
        changedRows: document.querySelectorAll('.rule-row[data-changed="yes"]').length,
        payloadCount: window.oslLastSavedRules.length,
        thread: window.oslLastSavedRules.find((rule) => rule.ruleKey === "discord:thread"),
        keys: window.oslLastSavedRules.map((rule) => rule.ruleKey),
      };
    })()`);
    assert.equal(savedRun.status, `All ${places.length} place rules saved.`);
    assert.equal(savedRun.changedRows, 0);
    assert.equal(savedRun.payloadCount, places.length);
    assert.deepEqual(savedRun.keys, places.map((place) => place.ruleKey));
    assert.deepEqual(savedRun.thread, { ruleKey: "discord:thread", app: "discord", kind: "thread", choice: "always" });

    const resetRun = await evaluateValue(page, `(() => {
      document.querySelector('[data-rule-action="reset"]').click();
      const rows = [...document.querySelectorAll(".rule-row")];
      return {
        status: document.querySelector(".rule-status").textContent.trim(),
        selected: [...new Set(rows.map((row) => row.dataset.selected))],
        chips: [...new Set(rows.map((row) => row.querySelector(".rule-row-selected").textContent.trim()))],
        changedRows: document.querySelectorAll('.rule-row[data-changed="yes"]').length,
        payloadStillOld: window.oslLastSavedRules.filter((rule) => rule.choice !== "never").length,
      };
    })()`);
    assert.deepEqual(resetRun.selected, ["never"]);
    assert.deepEqual(resetRun.chips, ["never"]);
    assert.equal(resetRun.status, "26 place rules changed - not saved yet.");
    assert.equal(resetRun.changedRows, 26);
    // Reset alone keeps nothing: the last saved payload is still the old one.
    assert.equal(resetRun.payloadStillOld, 26);

    const afterReset = await evaluateValue(page, `(() => {
      document.querySelector('[data-rule-action="save"]').click();
      return {
        status: document.querySelector(".rule-status").textContent.trim(),
        payloadCount: window.oslLastSavedRules.length,
        notNever: window.oslLastSavedRules.filter((rule) => rule.choice !== "never").length,
      };
    })()`);
    assert.equal(afterReset.status, `All ${places.length} place rules saved.`);
    assert.equal(afterReset.payloadCount, places.length);
    assert.equal(afterReset.notNever, 0);

    console.log(`TASK0740_CHANGE_STATUS=${changed.status}`);
    console.log(`TASK0740_SAVE_STATUS=${savedRun.status}`);
    console.log(`TASK0740_SAVE_PAYLOAD_RULES=${savedRun.payloadCount}`);
    console.log(`TASK0740_RESET_STATUS=${resetRun.status}`);
    console.log(`TASK0740_RESET_SELECTED=${resetRun.selected.join("|")}`);
    console.log(`TASK0740_RESET_SAVED_RULES_NOT_NEVER=${afterReset.notNever}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
