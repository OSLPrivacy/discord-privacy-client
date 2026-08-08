// TASK 4760 - drive the "Anyone" consent gate in a real browser.
//
// The gate is mounted with its real bindings in headless Chrome at a fixed
// Linux window size. Everything below is read from the live DOM: the rendered
// text of the three sentences, the actual `disabled` property of the Turn on
// button, real clicks on the tick box, Back and Close, a real Escape key sent
// through CDP, and the stored setting printed before and after each way of
// leaving. A screenshot and screen tree are written next to the other evidence.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { inflateSync } from "node:zlib";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACT_DIR = join(ROOT, "screenshots", "evidence");
const BEFORE_PNG = join(ARTIFACT_DIR, "task-4760-anyone-gate-before-tick-1000x900.png");
const AFTER_PNG = join(ARTIFACT_DIR, "task-4760-anyone-gate-after-tick-1000x900.png");
const AX_PATH = join(ARTIFACT_DIR, "task-4760-anyone-gate-1000x900.ax.json");
const WINDOW = Object.freeze({ width: 1000, height: 900 });
const STARTING_SETTING = "allowed";
const TICKED_AT = "2026-08-07T18:20:00.000Z";
const WORDING_VERSION = "anyone-consent-v1+cdd02ec0";

/** The three sentences as the task wrote them, retyped here on purpose. */
const SENTENCES = [
  "Anyone who knows your handle can learn you run OSL.",
  "That includes the carrier itself, which can check its whole history.",
  "This is retroactive and permanent. Turning it off later removes your record, but it does not un-tell anyone who already looked.",
];

async function pageScript() {
  const outFile = join(tmpdir(), `osl-task-4760-fixture-${process.pid}-${Date.now()}.js`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          bindAnyoneConsentGate,
          chooseDiscoverySetting,
          createDiscoverySettingStore,
          readDiscoverySetting,
        } from "${join(ROOT, "src", "discovery-anyone-consent-gate.ts")}";
        const store = createDiscoverySettingStore({ setting: ${JSON.stringify(STARTING_SETTING)} });
        const left = [];
        let binding = null;
        // Choosing "Anyone" is what opens the gate. It writes nothing itself.
        function openGate() {
          const chosen = chooseDiscoverySetting(store, "anyone");
          // A fresh host per opening, so an old gate's listeners are gone.
          const app = document.getElementById("app");
          app.innerHTML = "";
          const host = document.createElement("div");
          app.appendChild(host);
          binding = bindAnyoneConsentGate(
            host,
            chosen.gate,
            {
              store,
              now: () => new Date(${JSON.stringify(TICKED_AT)}),
              onLeft: (how) => { left.push(how); },
            },
          );
          return chosen;
        }
        const chosen = openGate();
        window.task4760 = {
          chooseAnyoneResult: () => ({ ok: chosen.ok, refusal: chosen.ok ? null : chosen.refusal }),
          storedSetting: () => readDiscoverySetting(store),
          consentStamp: () => store.consentStamp,
          corrections: () => store.corrections.slice(),
          leftBy: () => left.slice(),
          reopen: () => { openGate(); return readDiscoverySetting(store); },
          turnOnDirect: () => binding.turnOnNow(),
        };
      `,
      resolveDir: ROOT,
      sourcefile: "task-4760-fixture-entry.js",
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
    readFileSync(join(ROOT, "src", "discovery-anyone-consent-gate.css"), "utf8"),
    "html, body { height: 100%; margin: 0; background: var(--bg); }",
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <title>Discovery: Anyone</title>
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

function readPng(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const data = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const chunk = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = chunk.readUInt32BE(0);
      height = chunk.readUInt32BE(4);
      assert.equal(chunk[8], 8, "PNG must be 8-bit for this evidence decoder");
      colorType = chunk[9];
      assert.ok(colorType === 2 || colorType === 6, `unsupported PNG color type ${colorType}`);
    } else if (type === "IDAT") {
      data.push(chunk);
    } else if (type === "IEND") {
      break;
    }
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(data));
  const pixels = Buffer.alloc(width * height * 4);
  let source = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[source];
    source += 1;
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? row[x - channels] : 0;
      const up = previous[x] ?? 0;
      const upLeft = x >= channels ? previous[x - channels] : 0;
      if (filter === 1) row[x] = (row[x] + left) & 0xff;
      else if (filter === 2) row[x] = (row[x] + up) & 0xff;
      else if (filter === 3) row[x] = (row[x] + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        row[x] = (row[x] + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft)) & 0xff;
      } else assert.equal(filter, 0, `unsupported PNG row filter ${filter}`);
    }
    for (let x = 0; x < width; x += 1) {
      const src = x * channels;
      const dst = (y * width + x) * 4;
      pixels[dst] = row[src];
      pixels[dst + 1] = row[src + 1];
      pixels[dst + 2] = row[src + 2];
      pixels[dst + 3] = channels === 4 ? row[src + 3] : 255;
    }
    previous = row;
  }
  return { width, height, pixels };
}

function pixelStats(image) {
  const colors = new Map();
  for (let index = 0; index < image.width * image.height; index += 1) {
    const offset = index * 4;
    const key = `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
    colors.set(key, (colors.get(key) ?? 0) + 1);
  }
  const dominant = Math.max(...colors.values());
  return { uniqueColors: colors.size, nonDominantPixels: image.width * image.height - dominant };
}

/** Everything the finish line needs, read from the live page. */
const PROBE = `(() => {
  const gate = document.querySelector("#discovery-anyone-gate");
  const turnOn = document.querySelector("#discovery-anyone-gate-turn-on");
  const tick = document.querySelector("#discovery-anyone-consent-tick");
  return {
    gatePresent: !!gate,
    turnOnDisabled: turnOn ? turnOn.disabled : null,
    turnOnAriaDisabled: turnOn ? turnOn.getAttribute("aria-disabled") : null,
    turnOnGreyed: turnOn ? getComputedStyle(turnOn).opacity : null,
    dataTurnOnAvailable: gate ? gate.getAttribute("data-turn-on-available") : null,
    dataWordingVersion: gate ? gate.getAttribute("data-wording-version") : null,
    tickBoxes: document.querySelectorAll("input[type=checkbox]").length,
    typedWordBoxes: document.querySelectorAll("input[type=text], input[type=password], textarea").length,
    tickChecked: tick ? tick.checked : null,
    sentenceText: Array.from(document.querySelectorAll(".dag-sentence")).map((el) => el.textContent),
    backPresent: !!document.querySelector("#discovery-anyone-gate-back"),
    closePresent: !!document.querySelector("#discovery-anyone-gate-close"),
    storedSetting: window.task4760.storedSetting(),
    consentStamp: window.task4760.consentStamp(),
    leftBy: window.task4760.leftBy(),
    chooseAnyoneResult: window.task4760.chooseAnyoneResult(),
    visibleText: document.body.innerText,
  };
})()`;

const clickScript = (selector) => `(() => { document.querySelector(${JSON.stringify(selector)}).click(); return true; })()`;
const reopenScript = `window.task4760.reopen()`;
const directTurnOn = `JSON.stringify(window.task4760.turnOnDirect())`;

async function evaluate(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) throw new Error(evaluated.exceptionDetails.text || "page evaluation threw");
  return evaluated.result.value;
}

/** A real Escape key press, delivered to whatever holds focus inside the gate. */
async function pressEscape(page) {
  await evaluate(page, `(() => { document.querySelector("#discovery-anyone-consent-tick").focus(); return true; })()`);
  for (const type of ["keyDown", "keyUp"]) {
    await page.send("Input.dispatchKeyEvent", {
      type,
      key: "Escape",
      code: "Escape",
      windowsVirtualKeyCode: 27,
      nativeVirtualKeyCode: 27,
    });
  }
}

test("TASK 4760 the Anyone consent gate carries the three sentences and holds Turn on shut", async () => {
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

    // --- choosing Anyone opens the gate and writes nothing -----------------
    const start = await page.evaluate(PROBE);
    assert.equal(start.gatePresent, true, "choosing Anyone did not open the gate");
    assert.equal(start.chooseAnyoneResult.ok, false, "choosing Anyone wrote the setting straight away");
    assert.equal(start.chooseAnyoneResult.refusal, "discovery: anyone needs the consent gate");
    assert.equal(start.storedSetting, STARTING_SETTING, "the setting moved before the gate was answered");
    log.push(
      `TASK4760_GATE_OPENED gate_present=${start.gatePresent} choose_anyone_ok=${start.chooseAnyoneResult.ok}`
      + ` refusal="${start.chooseAnyoneResult.refusal}" stored_setting=${start.storedSetting}`,
    );

    // --- 1: all three sentences, character for character -------------------
    let found = 0;
    const perSentence = [];
    for (const [index, sentence] of SENTENCES.entries()) {
      const onScreen = start.sentenceText[index];
      const exact = onScreen === sentence;
      const inPageText = start.visibleText.includes(sentence);
      assert.equal(exact, true, `sentence ${index + 1} is not on screen character for character: ${JSON.stringify(onScreen)}`);
      assert.equal(inPageText, true, `sentence ${index + 1} is not in the rendered page text`);
      if (exact && inPageText) found += 1;
      perSentence.push(`s${index + 1}=exact:${exact},chars:${sentence.length}`);
    }
    assert.equal(found, SENTENCES.length);
    log.push(`TASK4760_SENTENCES ${found} of ${SENTENCES.length} found ${perSentence.join(" ")}`);

    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axTree.nodes
      .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
      .filter((node) => node.name);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));
    const axSentences = SENTENCES.filter((sentence) => names.some((node) => node.name === sentence)).length;
    log.push(`TASK4760_SCREEN_TREE named_nodes=${names.length} sentences_in_screen_tree=${axSentences} of ${SENTENCES.length}`);

    // --- 2: Turn on greyed while the box is unticked -----------------------
    assert.equal(start.tickBoxes, 1, "the gate does not carry exactly one tick box");
    assert.equal(start.typedWordBoxes, 0, "the gate asks for a typed word (ruling 5 keeps typing for ERASE)");
    assert.equal(start.tickChecked, false, "the tick box did not start unticked");
    assert.equal(start.turnOnDisabled, true, "Turn on was live before the box was ticked");
    assert.equal(start.dataTurnOnAvailable, "false");
    assert.ok(Number(start.turnOnGreyed) < 1, `Turn on is not visibly greyed: opacity ${start.turnOnGreyed}`);
    const beforePng = await page.screenshot({ fromSurface: true });
    writeFileSync(BEFORE_PNG, beforePng);
    const beforeImage = readPng(beforePng);
    assert.deepEqual({ width: beforeImage.width, height: beforeImage.height }, WINDOW);
    log.push(
      `TASK4760_BEFORE_TICK tick_boxes=${start.tickBoxes} typed_word_boxes=${start.typedWordBoxes}`
      + ` tick_checked=${start.tickChecked} turn_on_disabled=${start.turnOnDisabled}`
      + ` aria_disabled=${start.turnOnAriaDisabled} turn_on_opacity=${start.turnOnGreyed}`,
    );

    // Pressing a greyed Turn on does nothing at all.
    await page.evaluate(clickScript("#discovery-anyone-gate-turn-on"));
    const deadClick = await page.evaluate(PROBE);
    assert.equal(deadClick.storedSetting, STARTING_SETTING, "a greyed Turn on moved the setting");
    assert.equal(deadClick.consentStamp, null);
    log.push(
      `TASK4760_GREYED_CLICK stored_setting=${deadClick.storedSetting} consent_stamp=${deadClick.consentStamp}`
      + ` turn_on_disabled=${deadClick.turnOnDisabled}`,
    );

    // And a direct call, with no button press, is refused the same way.
    const refusedRaw = JSON.parse(await evaluate(page, directTurnOn));
    const afterDirect = await page.evaluate(PROBE);
    assert.equal(refusedRaw.ok, false, "a direct turn-on was allowed with the box unticked");
    assert.equal(refusedRaw.refusal, "discovery: tick the box before turning Anyone on");
    assert.equal(afterDirect.storedSetting, STARTING_SETTING);
    log.push(
      `TASK4760_DIRECT_TURN_ON_NO_TICK ok=${refusedRaw.ok} refusal="${refusedRaw.refusal}"`
      + ` stored_setting=${afterDirect.storedSetting}`,
    );

    // --- 3: leaving any other way leaves the setting where it was ----------
    const leaves = [];
    for (const [how, act] of [
      ["back", async () => { await page.evaluate(clickScript("#discovery-anyone-gate-back")); }],
      ["close", async () => { await page.evaluate(clickScript("#discovery-anyone-gate-close")); }],
      ["escape", async () => { await pressEscape(page); }],
      ["ticked-then-back", async () => {
        await page.evaluate(clickScript("#discovery-anyone-consent-tick"));
        await page.evaluate(clickScript("#discovery-anyone-gate-back"));
      }],
      ["ticked-then-escape", async () => {
        await page.evaluate(clickScript("#discovery-anyone-consent-tick"));
        await pressEscape(page);
      }],
    ]) {
      const before = await evaluate(page, `window.task4760.storedSetting()`);
      await act();
      const after = await evaluate(page, `window.task4760.storedSetting()`);
      // A gate that was left cannot be cashed in afterwards either.
      const afterRetry = JSON.parse(await evaluate(page, directTurnOn));
      const settled = await evaluate(page, `window.task4760.storedSetting()`);
      assert.equal(after, before, `leaving by ${how} moved the stored setting`);
      assert.equal(settled, before, `turning on from a gate left by ${how} moved the stored setting`);
      assert.equal(afterRetry.ok, false);
      leaves.push(`${how} before=${before} after=${after} equal=${after === before} retry_ok=${afterRetry.ok}`);
      await evaluate(page, reopenScript);
    }
    const leftBy = await evaluate(page, `window.task4760.leftBy()`);
    log.push(`TASK4760_LEFT_ANY_OTHER_WAY ${leaves.join(" | ")}`);
    log.push(`TASK4760_LEAVE_ROUTES_WIRED ${leftBy.join(",")}`);

    // --- 4: ticking the box makes Turn on live and writes the stamp --------
    const beforeTurnOn = await evaluate(page, `window.task4760.storedSetting()`);
    await page.evaluate(clickScript("#discovery-anyone-consent-tick"));
    const ticked = await page.evaluate(PROBE);
    assert.equal(ticked.tickChecked, true);
    assert.equal(ticked.turnOnDisabled, false, "Turn on stayed greyed after the box was ticked");
    assert.equal(ticked.dataTurnOnAvailable, "true");
    assert.equal(ticked.storedSetting, STARTING_SETTING, "ticking the box wrote the setting on its own");
    log.push(
      `TASK4760_AFTER_TICK tick_checked=${ticked.tickChecked} turn_on_disabled=${ticked.turnOnDisabled}`
      + ` aria_disabled=${ticked.turnOnAriaDisabled} turn_on_opacity=${ticked.turnOnGreyed}`
      + ` stored_setting_still=${ticked.storedSetting}`,
    );

    await page.evaluate(clickScript("#discovery-anyone-gate-turn-on"));
    const turnedOn = await page.evaluate(PROBE);
    assert.equal(turnedOn.storedSetting, "anyone", "Turn on did not move the setting");
    assert.ok(turnedOn.consentStamp, "no consent stamp was written");
    assert.equal(turnedOn.consentStamp.recordedDate, TICKED_AT);
    assert.equal(turnedOn.consentStamp.wordingVersion, WORDING_VERSION);
    assert.equal(turnedOn.dataWordingVersion, WORDING_VERSION);
    log.push(
      `TASK4760_TURNED_ON stored_setting_before=${beforeTurnOn} stored_setting_after=${turnedOn.storedSetting}`
      + ` stamp_date=${turnedOn.consentStamp.recordedDate} stamp_wording_version=${turnedOn.consentStamp.wordingVersion}`
      + ` stamp_seal=${turnedOn.consentStamp.seal}`,
    );

    const afterPng = await page.screenshot({ fromSurface: true });
    writeFileSync(AFTER_PNG, afterPng);
    const afterImage = readPng(afterPng);
    const beforeStats = pixelStats(beforeImage);
    const afterStats = pixelStats(afterImage);
    assert.ok(afterStats.nonDominantPixels > 5_000, `page barely painted: ${JSON.stringify(afterStats)}`);
    assert.notEqual(afterPng.toString("base64"), beforePng.toString("base64"), "the ticked page is byte-identical to the untouched one");
    log.push(
      `TASK4760_IMAGES before=${BEFORE_PNG} after=${AFTER_PNG} ax=${AX_PATH}`
      + ` window=${afterImage.width}x${afterImage.height}`
      + ` before_non_dominant_pixels=${beforeStats.nonDominantPixels} after_non_dominant_pixels=${afterStats.nonDominantPixels}`,
    );

    for (const line of log) console.log(line);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
});
