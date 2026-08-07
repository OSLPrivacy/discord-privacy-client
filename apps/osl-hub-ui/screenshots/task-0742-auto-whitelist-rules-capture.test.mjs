/**
 * TASK 0742 - the fixed-size Linux screenshot of the auto-whitelist rules
 * screen, with mixed rule choices, showing its title and all nine named
 * controls.
 *
 * The nine are the seven place families across the top -- direct messages,
 * groups, servers, channels, threads, email, posts -- and the two buttons,
 * Save rules and Reset. Each one has to be findable twice: by name in the
 * screen tree (the page's own text and Chrome's accessibility tree), and as
 * ink in the PNG. "Ink" here is measured, not assumed: for every named
 * control the check crops its rectangle out of the screenshot and counts the
 * pixels that differ from that crop's own most common colour, so a control
 * whose text failed to draw fails here even though the DOM still says the
 * word.
 *
 * The screen is also required to be showing mixed rules -- all four choices in
 * use across the rows, some families reading one rule and others reading
 * mixed -- and the whole image is required not to be blank.
 *
 * After the capture the family controls are driven in the same browser: one
 * pick sets every place in that family, which is what makes them controls
 * rather than headings.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { createServer } from "vite";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { imageFacts, parsePng } from "./png-facts.mjs";

const APP_ROOT = path.resolve(import.meta.dirname, "..");
const ARTIFACT_DIR = path.join(APP_ROOT, "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0742-auto-whitelist-rules-controls.png");
const TREE_PATH = path.join(
  ARTIFACT_DIR,
  "task-0742-auto-whitelist-rules-controls-screen-tree.json",
);
const FIXTURE = "screenshots/task-0742-auto-whitelist-rules-controls-fixture.html";

/** The title and the nine named controls this task is measured against. */
const SCREEN_TITLE = "Auto-whitelist rules";
const NAMED_CONTROLS = [
  "direct messages",
  "groups",
  "servers",
  "channels",
  "threads",
  "email",
  "posts",
  "Save rules",
  "Reset",
];

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
 * How much of a rectangle is drawn on rather than left as background.
 *
 * The most common colour in the crop is taken as its background; every pixel
 * more than 20 away from it in any channel is ink. A control that drew its box
 * but no text lands near zero, which is the failure this is here to catch.
 */
function inkFacts(png, rect) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  const seen = [];
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const rgb = [png.pixels[offset], png.pixels[offset + 1], png.pixels[offset + 2]];
      const key = rgb.join(",");
      counts.set(key, (counts.get(key) ?? 0) + 1);
      seen.push(rgb);
    }
  }
  let background = null;
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      background = key;
    }
  }
  const [br, bg, bb] = (background ?? "0,0,0").split(",").map(Number);
  let ink = 0;
  for (const [r, g, b] of seen) {
    if (Math.abs(r - br) > 20 || Math.abs(g - bg) > 20 || Math.abs(b - bb) > 20) ink += 1;
  }
  return {
    pixels: seen.length,
    background,
    inkPixels: ink,
    inkShare: seen.length === 0 ? 0 : ink / seen.length,
    distinctColors: counts.size,
  };
}

/** What the page can say about the title and the nine named controls. */
const READ_SCREEN = `(() => {
  const box = (element) => {
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const heading = document.querySelector(".rules-screen-heading");
  const families = [...document.querySelectorAll(".rule-family")].map((family) => {
    const select = family.querySelector(".rule-family-select");
    const name = family.querySelector(".rule-family-name");
    return {
      id: family.dataset.ruleFamily,
      label: name.textContent.trim(),
      places: Number(family.dataset.familyPlaces),
      reading: family.dataset.familyChoice,
      value: select.value,
      selectLabel: select.getAttribute("aria-label"),
      options: [...select.options].map((option) => option.value),
      count: family.querySelector(".rule-family-count").textContent.trim(),
      box: box(family),
      nameBox: box(name),
      selectBox: box(select),
    };
  });
  const buttons = [...document.querySelectorAll("[data-rule-action]")].map((button) => ({
    action: button.dataset.ruleAction,
    label: button.textContent.trim(),
    box: box(button),
  }));
  const rows = [...document.querySelectorAll(".rule-row")].map((row) => ({
    ruleKey: row.dataset.ruleKey,
    kind: row.dataset.kind,
    selected: row.dataset.selected,
  }));
  return {
    title: heading.textContent.trim(),
    titleBox: box(heading),
    families,
    buttons,
    rows,
    status: document.querySelector(".rule-status").textContent.trim(),
    scrollHeight: document.documentElement.scrollHeight,
    scrollWidth: document.documentElement.scrollWidth,
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

test("TASK 0742 captures the rules screen title and all nine named controls", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const screenData = await server.ssrLoadModule("/src/auto-whitelist-rules-screen-data.ts");
  const screenModule = await server.ssrLoadModule("/src/auto-whitelist-rules-screen.ts");
  const window = screenData.AUTO_WHITELIST_RULES_SCREEN_WINDOW;
  const saved = screenData.AUTO_WHITELIST_RULES_SCREEN_SAVED;
  const families = screenModule.AUTO_WHITELIST_FAMILIES;
  const places = screenModule.AUTO_WHITELIST_PLACE_KINDS;
  const choices = screenModule.AUTO_WHITELIST_CHOICES;

  const started = await (async () => {
    try {
      // The nine names this task is judged on are the screen's own, not a list
      // kept beside it: the seven families it draws plus its two buttons.
      assert.deepEqual(
        [...families.map((family) => family.label), "Save rules", "Reset"],
        NAMED_CONTROLS,
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
        if (document.documentElement.dataset.task0742 === "ready"
          && document.querySelectorAll(".rule-family").length === ${families.length}
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
    const axText = accessibilityTreeText(ax.nodes ?? []);
    const treeText = [screen.text, axText].join("\n").toLowerCase();

    // 1. The title, on the screen and in the tree.
    assert.equal(screen.title, SCREEN_TITLE);
    assert.match(treeText, new RegExp(escapeRegExp(SCREEN_TITLE.toLowerCase()), "u"));
    assert.match(axText.toLowerCase(), new RegExp(escapeRegExp(SCREEN_TITLE.toLowerCase()), "u"));

    // 2. The nine named controls, drawn in that order, each in the tree.
    const drawnNames = [
      ...screen.families.map((family) => family.label),
      ...screen.buttons.map((button) => button.label),
    ];
    assert.deepEqual(drawnNames, NAMED_CONTROLS);
    assert.deepEqual(screen.buttons.map((button) => button.action), ["save", "reset"]);
    for (const name of NAMED_CONTROLS) {
      const pattern = new RegExp(escapeRegExp(name.toLowerCase()), "u");
      assert.match(treeText, pattern, `missing from the screen text: ${name}`);
      assert.match(axText.toLowerCase(), pattern, `missing from the accessibility tree: ${name}`);
    }

    // 3. Mixed rule choices: all four in use across the rows, and the families
    //    are not all reading the same thing either.
    assert.equal(screen.rows.length, places.length);
    assert.deepEqual(screen.rows.map((row) => row.selected), places.map((place) => saved[place.ruleKey]));
    assert.deepEqual(
      [...new Set(screen.rows.map((row) => row.selected))].sort(),
      choices.map((choice) => choice.id).sort(),
    );
    const readings = screen.families.map((family) => family.reading);
    assert.ok(readings.includes("mixed"), `no family reads mixed: ${readings.join("|")}`);
    assert.ok(
      readings.some((reading) => reading !== "mixed"),
      `every family reads mixed: ${readings.join("|")}`,
    );
    // Each family covers the place kinds it claims, and its reading is the one
    // its rows actually have.
    const byKind = new Map(places.map((place) => [place.ruleKey, place]));
    for (const family of screen.families) {
      const mine = screenModule.familyPlaceKinds(family.id);
      assert.equal(mine.length, family.places, `${family.id} claims ${family.places} places`);
      assert.equal(family.count, mine.length === 1 ? "1 place" : `${mine.length} places`);
      const rowChoices = [...new Set(mine.map((place) => saved[byKind.get(place.ruleKey).ruleKey]))];
      assert.equal(family.reading, rowChoices.length === 1 ? rowChoices[0] : "mixed");
      assert.equal(family.value, family.reading);
      assert.deepEqual(
        family.options.filter((option) => option !== "mixed"),
        choices.map((choice) => choice.id),
      );
      assert.equal(family.selectLabel, `Set every ${family.label} rule`);
    }
    assert.equal(
      screenModule.AUTO_WHITELIST_FAMILIES.flatMap((family) =>
        screenModule.familyPlaceKinds(family.id),
      ).length,
      places.length,
    );

    // 4. Everything named is inside the fixed-size capture.
    assert.equal(screen.scrollWidth, window.width);
    assert.ok(
      screen.scrollHeight <= window.height,
      `page is ${screen.scrollHeight}px tall, taller than the ${window.height}px capture`,
    );
    const boxes = {
      title: screen.titleBox,
      ...Object.fromEntries(screen.families.flatMap((family) => [
        [`${family.label}|control`, family.box],
        [`${family.label}|name`, family.nameBox],
        [`${family.label}|value`, family.selectBox],
      ])),
      ...Object.fromEntries(screen.buttons.map((button) => [`${button.label}|control`, button.box])),
    };
    for (const [name, box] of Object.entries(boxes)) {
      assert.ok(box.width > 0 && box.height > 0, `${name} has no size: ${JSON.stringify(box)}`);
      assert.ok(
        box.x >= 0 && box.y >= 0
          && box.x + box.width <= window.width
          && box.y + box.height <= window.height,
        `${name} is outside the capture: ${JSON.stringify(box)}`,
      );
    }

    const png = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    writeFileSync(PNG_PATH, png);

    const facts = imageFacts(png, boxes);
    const image = parsePng(png);
    const ink = Object.fromEntries(
      Object.entries(boxes).map(([name, box]) => [name, inkFacts(image, box)]),
    );
    const whole = inkFacts(image, { x: 0, y: 0, width: image.width, height: image.height });

    writeFileSync(
      TREE_PATH,
      JSON.stringify(
        {
          url: `${url}${FIXTURE}`,
          window,
          title: SCREEN_TITLE,
          namedControls: NAMED_CONTROLS,
          screen,
          facts,
          ink,
          wholeImage: whole,
          axNodes: ax.nodes,
        },
        null,
        2,
      ),
    );

    // 5. The picture is the right size and is not blank or nearly blank.
    assert.equal(facts.width, window.width);
    assert.equal(facts.height, window.height);
    assert.ok(png.length > 5_000, `PNG too small: ${png.length}`);
    assert.ok(facts.distinctColors > 20, `PNG nearly blank: ${facts.distinctColors} distinct colours`);
    assert.ok(
      whole.inkShare > 0.05,
      `PNG nearly blank: only ${(whole.inkShare * 100).toFixed(2)}% of pixels differ from the background`,
    );

    // 6. The title and every named control carry ink in the PNG itself.
    assert.ok(ink.title.inkShare > 0.02, `title did not draw: ${JSON.stringify(ink.title)}`);
    assert.ok(facts.crops.title.distinctColors > 3, `title has no ink: ${facts.crops.title.distinctColors} colours`);
    for (const name of NAMED_CONTROLS) {
      const control = ink[`${name}|control`];
      assert.ok(control, `no crop for ${name}`);
      assert.ok(
        control.inkShare > 0.02,
        `${name} did not draw in the image: ${JSON.stringify(control)}`,
      );
      assert.ok(
        facts.crops[`${name}|control`].distinctColors > 3,
        `${name} has no ink: ${facts.crops[`${name}|control`].distinctColors} colours`,
      );
    }
    // The family names and the rule each family is reading are separately
    // legible, so a band that drew only its boxes is not enough.
    for (const family of screen.families) {
      const name = ink[`${family.label}|name`];
      const value = ink[`${family.label}|value`];
      assert.ok(name.inkShare > 0.05, `${family.label} name did not draw: ${JSON.stringify(name)}`);
      assert.ok(value.inkShare > 0.02, `${family.label} value did not draw: ${JSON.stringify(value)}`);
    }
    // Save rules is the brand-painted button; no part of the grey chrome can
    // make that colour, so this is the picture and not the markup talking.
    const saveCrop = facts.crops["Save rules|control"];
    assert.ok(
      saveCrop.saturatedPixels / saveCrop.pixels > 0.5,
      `Save rules is not brand-painted: ${saveCrop.saturatedPixels}/${saveCrop.pixels}`,
    );

    console.log(`TASK0742_PNG=${PNG_PATH}`);
    console.log(`TASK0742_TREE=${TREE_PATH}`);
    console.log(`TASK0742_URL=${url}${FIXTURE}`);
    console.log(`TASK0742_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0742_PNG_BYTES=${png.length}`);
    console.log(`TASK0742_PNG_SHA256=${createHash("sha256").update(png).digest("hex")}`);
    console.log(`TASK0742_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0742_PNG_INK_SHARE=${(whole.inkShare * 100).toFixed(2)}%`);
    console.log(`TASK0742_TITLE=${screen.title}`);
    console.log(`TASK0742_TITLE_INK=${ink.title.inkPixels}/${ink.title.pixels}`);
    console.log(`TASK0742_NAMED_CONTROL_COUNT=${NAMED_CONTROLS.length}`);
    console.log(`TASK0742_ROW_COUNT=${screen.rows.length}`);
    console.log(`TASK0742_STATUS=${screen.status}`);
    for (const name of NAMED_CONTROLS) {
      const control = ink[`${name}|control`];
      const family = screen.families.find((candidate) => candidate.label === name);
      console.log(
        `TASK0742_CONTROL=${name}|tree=yes|ink=${control.inkPixels}/${control.pixels}`
        + ` (${(control.inkShare * 100).toFixed(1)}%)`
        + `|${family ? `reads=${family.reading}, ${family.count}` : "button"}`,
      );
    }
    const counts = {};
    for (const row of screen.rows) counts[row.selected] = (counts[row.selected] ?? 0) + 1;
    for (const choice of choices) console.log(`TASK0742_ROWS_${choice.id}=${counts[choice.id] ?? 0}`);

    // A family control sets every place in its family: the seven names across
    // the top are controls, not labels over a static band.
    const drivenFamily = "channels";
    const driven = await evaluateValue(page, `(() => {
      const select = document.querySelector('.rule-family-select[data-rule-family="${drivenFamily}"]');
      select.value = "always";
      select.dispatchEvent(new Event("change", { bubbles: true }));
      const family = document.querySelector('.rule-family[data-rule-family="${drivenFamily}"]');
      const rows = [...document.querySelectorAll(".rule-row")];
      return {
        reading: family.dataset.familyChoice,
        value: family.querySelector(".rule-family-select").value,
        status: document.querySelector(".rule-status").textContent.trim(),
        changedRows: [...document.querySelectorAll('.rule-row[data-changed="yes"]')].map((row) => row.dataset.ruleKey),
        familyRows: rows
          .filter((row) => ["channel", "server_channel"].includes(row.dataset.kind))
          .map((row) => row.dataset.ruleKey + "=" + row.dataset.selected),
        otherRows: rows
          .filter((row) => !["channel", "server_channel"].includes(row.dataset.kind))
          .filter((row) => row.dataset.changed === "yes").length,
      };
    })()`);
    const channelKeys = screenModule
      .familyPlaceKinds(drivenFamily)
      .map((place) => place.ruleKey);
    assert.equal(driven.reading, "always");
    assert.equal(driven.value, "always");
    assert.deepEqual(driven.familyRows, channelKeys.map((key) => `${key}=always`));
    assert.equal(driven.otherRows, 0);
    const expectedChanged = channelKeys.filter((key) => saved[key] !== "always");
    assert.deepEqual(driven.changedRows, expectedChanged);
    assert.equal(
      driven.status,
      `${expectedChanged.length} place rules changed - not saved yet.`,
    );

    const savedRun = await evaluateValue(page, `(() => {
      document.querySelector('[data-rule-action="save"]').click();
      return {
        status: document.querySelector(".rule-status").textContent.trim(),
        payloadCount: window.oslLastSavedRules.length,
        channels: window.oslLastSavedRules
          .filter((rule) => ["channel", "server_channel"].includes(rule.kind))
          .map((rule) => rule.ruleKey + "=" + rule.choice),
      };
    })()`);
    assert.equal(savedRun.status, `All ${places.length} place rules saved.`);
    assert.equal(savedRun.payloadCount, places.length);
    assert.deepEqual(savedRun.channels, channelKeys.map((key) => `${key}=always`));

    console.log(`TASK0742_FAMILY_DRIVEN=${drivenFamily}`);
    console.log(`TASK0742_FAMILY_ROWS_AFTER=${driven.familyRows.join("|")}`);
    console.log(`TASK0742_FAMILY_OTHER_ROWS_CHANGED=${driven.otherRows}`);
    console.log(`TASK0742_FAMILY_STATUS=${driven.status}`);
    console.log(`TASK0742_FAMILY_SAVE_STATUS=${savedRun.status}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
