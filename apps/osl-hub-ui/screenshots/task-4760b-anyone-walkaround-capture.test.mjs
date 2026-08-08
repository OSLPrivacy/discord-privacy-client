// TASK 4760b - click the old Strip cycler for real, in a real browser.
//
// Attempt 3 of the task says "click the old Strip cycler if any copy of it
// still exists". No copy survives in this codebase -- the named check searches
// for one and finds none -- so the tagged replica in
// `src/discovery-anyone-walkaround.ts` is mounted here with its real bindings
// and clicked with real `Input.dispatchMouseEvent` clicks at the button's real
// screen coordinates. Nothing below is a synthetic `element.click()`: the
// pointer is moved to the middle of the button and pressed.
//
// Everything printed is read off the live page after those clicks: the stored
// setting, the number of discovery cards the publisher published, and the
// refusal as it is painted on screen.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACT_DIR = join(ROOT, "screenshots", "evidence");
const PNG_PATH = join(ARTIFACT_DIR, "task-4760b-strip-cycler-refused-1000x900.png");
const AX_PATH = join(ARTIFACT_DIR, "task-4760b-strip-cycler-refused-1000x900.ax.json");
const WINDOW = Object.freeze({ width: 1000, height: 900 });
const AT = "2026-08-07T18:20:00.000Z";
const ANYONE_NEEDS_GATE = "discovery: anyone needs the consent gate";
const STARTING_SETTING = "never";

async function pageScript() {
  const outFile = join(tmpdir(), `osl-task-4760b-fixture-${process.pid}-${Date.now()}.js`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          bindOldStripCycler,
          createDiscoveryCardLedger,
          createDiscoverySettingStore,
          publishedCardCount,
          readDiscoverySetting,
        } from "${join(ROOT, "src", "discovery-anyone-walkaround.ts")}";
        const store = createDiscoverySettingStore({ setting: ${JSON.stringify(STARTING_SETTING)} });
        const ledger = createDiscoveryCardLedger();
        const host = document.getElementById("app");
        const binding = bindOldStripCycler(host, store, ledger, ${JSON.stringify(AT)});
        window.task4760b = {
          storedSetting: () => readDiscoverySetting(store),
          cardsPublished: () => publishedCardCount(ledger),
          publishedCards: () => ledger.published.slice(),
          clicks: () => binding.clicks(),
          consentStamp: () => store.consentStamp,
        };
      `,
      resolveDir: ROOT,
      sourcefile: "task-4760b-fixture-entry.js",
      loader: "js",
    },
    bundle: true,
    platform: "browser",
    format: "iife",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return readFileSync(outFile, "utf8");
}

function fixtureHtml(script) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "discovery-anyone-walkaround.css"), "utf8"),
    "html, body { height: 100%; margin: 0; background: var(--bg); }",
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <title>Who can find me</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="app"></div>
        <script>${script}</script>
      </body>
    </html>`;
}

async function fixtureServer(html) {
  const server = createServer((request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  const { port } = server.address();
  return { server, url: `http://127.0.0.1:${port}/` };
}

function closeServer(server) {
  return new Promise((done, fail) => server.close((error) => error ? fail(error) : done()));
}

/** Width and height straight out of the PNG's IHDR chunk. */
function pngSize(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a", "not a PNG");
  assert.equal(buffer.subarray(12, 16).toString("ascii"), "IHDR");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

async function evaluate(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) throw new Error(evaluated.exceptionDetails.text || "page evaluation threw");
  return evaluated.result.value;
}

const PROBE = `(() => {
  const cycler = document.querySelector("#old-strip-cycler");
  const value = document.querySelector("#old-strip-cycler-value");
  const refusal = document.querySelector("#old-strip-cycler-refusal");
  return {
    cyclerPresent: !!cycler,
    dataSetting: cycler ? cycler.getAttribute("data-setting") : null,
    dataNext: cycler ? cycler.getAttribute("data-next") : null,
    valueOnScreen: value ? value.textContent : null,
    refusalOnScreen: refusal ? refusal.textContent : null,
    storedSetting: window.task4760b.storedSetting(),
    cardsPublished: window.task4760b.cardsPublished(),
    consentStamp: window.task4760b.consentStamp(),
    visibleText: document.body.innerText,
  };
})()`;

/** A real pointer click in the middle of the button, not element.click(). */
async function clickButtonForReal(page) {
  const box = await evaluate(page, `(() => {
    const rect = document.querySelector("#old-strip-cycler-button").getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  })()`);
  await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: box.x, y: box.y, button: "none", buttons: 0 });
  await page.send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", buttons: 1, clickCount: 1 });
  await page.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: box.x, y: box.y, button: "left", buttons: 0, clickCount: 1 });
  return box;
}

test("TASK 4760b the old Strip cycler cannot be clicked into Anyone", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await fixtureServer(fixtureHtml(await pageScript()));
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  const log = [];
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");

    const start = await page.evaluate(PROBE);
    assert.equal(start.cyclerPresent, true, "the old Strip cycler replica did not mount");
    assert.equal(start.storedSetting, STARTING_SETTING);
    assert.equal(start.cardsPublished, 0);
    log.push(
      `TASK4760B_CYCLER_MOUNTED present=${start.cyclerPresent} setting=${start.storedSetting}`
      + ` on_screen="${start.valueOnScreen}" next=${start.dataNext} cards_published=${start.cardsPublished}`,
    );

    // Six real clicks: round the cycle and then hammered at the fourth value.
    const rows = [];
    let atAnyone = null;
    for (let click = 1; click <= 6; click += 1) {
      const before = await page.evaluate(PROBE);
      const where = await clickButtonForReal(page);
      const after = await page.evaluate(PROBE);
      const asked = before.dataNext;
      rows.push(
        `click${click} at=${Math.round(where.x)},${Math.round(where.y)} from=${before.storedSetting}`
        + ` asked=${asked} after=${after.storedSetting} cards=${after.cardsPublished}`,
      );
      if (asked === "anyone") {
        assert.equal(after.storedSetting, before.storedSetting, `clicking into anyone moved the setting on click ${click}`);
        assert.notEqual(after.storedSetting, "anyone", `the cycler reached anyone on click ${click}`);
        assert.equal(after.refusalOnScreen, ANYONE_NEEDS_GATE, "the refusal on screen is not the words the gate promises");
        assert.equal(after.visibleText.includes(ANYONE_NEEDS_GATE), true, "the refusal is not in the rendered page text");
        assert.equal(after.consentStamp, null, "a consent stamp appeared without the gate");
        atAnyone = { before: before.storedSetting, after: after.storedSetting, refusal: after.refusalOnScreen, cards: after.cardsPublished };
      }
    }

    const settled = await page.evaluate(PROBE);
    assert.notEqual(atAnyone, null, "the cycler never got as far as asking for anyone");
    assert.equal(settled.storedSetting, "shared-room", "the cycler did not stop at the value before anyone");
    assert.equal(settled.cardsPublished, 0, `${settled.cardsPublished} discovery card(s) were published`);

    log.push(
      `TASK4760B_ATTEMPT_3_BROWSER real_clicks=${rows.length} refusal="${atAnyone.refusal}"`
      + ` exact_words=${atAnyone.refusal === ANYONE_NEEDS_GATE}`
      + ` setting_before=${atAnyone.before} setting_after=${atAnyone.after}`
      + ` equal=${atAnyone.after === atAnyone.before} cards_published=${settled.cardsPublished}`,
    );
    for (const row of rows) log.push(`TASK4760B_ATTEMPT_3_BROWSER_CLICK ${row}`);

    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axTree.nodes
      .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
      .filter((node) => node.name);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));
    const refusalInTree = names.some((node) => node.name.includes(ANYONE_NEEDS_GATE));
    assert.equal(refusalInTree, true, "the refusal is not in the screen tree");
    log.push(`TASK4760B_SCREEN_TREE named_nodes=${names.length} refusal_in_screen_tree=${refusalInTree}`);

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const size = pngSize(png);
    assert.deepEqual(size, WINDOW);
    log.push(`TASK4760B_IMAGE path=${PNG_PATH} ax=${AX_PATH} window=${size.width}x${size.height} bytes=${png.length}`);

    for (const line of log) console.log(line);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
});
