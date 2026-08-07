// TASK 1408 - drive the consent page in a real browser and prove the gate.
//
// The page is mounted with its real bindings in headless Chrome at a fixed
// Linux window size. Everything below is read from the live DOM: the actual
// `disabled` property of the Continue button, real clicks on the terms
// buttons, Back and the risk ticks, and the result of calling the continue
// action directly without ever pressing the button. A screenshot and screen
// tree are written next to the other evidence so task 1410 has something to
// read.
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
const BEFORE_PNG = join(ARTIFACT_DIR, "task-1408-consent-page-before-tick-1000x900.png");
const AFTER_PNG = join(ARTIFACT_DIR, "task-1408-consent-page-after-tick-1000x900.png");
const AX_PATH = join(ARTIFACT_DIR, "task-1408-consent-page-1000x900.ax.json");
const WINDOW = Object.freeze({ width: 1000, height: 900 });
const ACCOUNTS = [
  { accountId: "discord-account-alpha-1408", accountLabel: "Ada on Discord" },
  { accountId: "discord-account-beta-1408", accountLabel: "Ada's spare Discord" },
];
const REFUSAL_BOTH =
  "Risk agreement is required for every selected Discord account before continuing:"
  + " missing=discord-account-alpha-1408,discord-account-beta-1408";
const REFUSAL_BETA =
  "Risk agreement is required for every selected Discord account before continuing:"
  + " missing=discord-account-beta-1408";

async function pageScript() {
  const outFile = join(tmpdir(), `osl-task-1408-fixture-${process.pid}-${Date.now()}.js`);
  await esbuild.build({
    stdin: {
      contents: `
        import { bindScrubConsentPage, scrubConsentPageState } from "${join(ROOT, "src", "scrub-consent-page.ts")}";
        const accounts = ${JSON.stringify(ACCOUNTS)};
        const calls = [];
        // Stands in for the hub, and keeps the same rule task 1407 enforces
        // natively, so a call that got past the page would still be refused.
        const agreed = new Set();
        const invoke = async (command, payload) => {
          calls.push(command);
          if (command === "save_discord_scrub_consent_facts") {
            agreed.add(payload.input.accountId);
            return { accountId: payload.input.accountId };
          }
          if (command === "continue_discord_scrub_after_risk_agreement") {
            const missing = accounts.map((a) => a.accountId).filter((id) => !agreed.has(id));
            if (missing.length > 0) {
              throw new Error("Risk agreement is required for every selected Discord account before continuing: missing=" + missing.join(","));
            }
            return { accountIds: accounts.map((a) => a.accountId), agreedAccountIds: [...agreed], mayContinue: true };
          }
          throw new Error("unexpected command " + command);
        };
        let backPresses = 0;
        const binding = bindScrubConsentPage(
          document.getElementById("app"),
          scrubConsentPageState(accounts),
          { invoke, onBack: () => { backPresses += 1; } },
        );
        window.task1408 = {
          binding,
          invokeCalls: () => calls.slice(),
          backPresses: () => backPresses,
          continueDirect: () => binding.continueNow(),
        };
      `,
      resolveDir: ROOT,
      sourcefile: "task-1408-fixture-entry.js",
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
    readFileSync(join(ROOT, "src", "scrub-consent-page.css"), "utf8"),
    "html, body { height: 100%; margin: 0; background: var(--bg); }",
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <title>Scrub consent</title>
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
  const continueButton = document.querySelector("#scrub-consent-continue");
  const page = document.querySelector("#scrub-consent-page");
  return {
    continueDisabled: continueButton.disabled,
    continueAriaDisabled: continueButton.getAttribute("aria-disabled"),
    dataContinueAvailable: page.getAttribute("data-continue-available"),
    missingRiskTicks: page.getAttribute("data-missing-risk-ticks"),
    accountPanels: Array.from(document.querySelectorAll(".scp-account")).map((el) => el.getAttribute("data-account-id")),
    termsButtons: Array.from(document.querySelectorAll("[data-terms-button]")).map((el) => el.getAttribute("data-terms-button")),
    riskTicks: Array.from(document.querySelectorAll("[data-risk-tick]")).map((el) => el.getAttribute("data-risk-tick") + "=" + el.checked),
    backPresent: !!document.querySelector("#scrub-consent-back"),
    anchors: document.querySelectorAll("a").length,
    notice: document.querySelector("#scrub-consent-notice") ? document.querySelector("#scrub-consent-notice").textContent : null,
    invokeCalls: window.task1408.invokeCalls(),
    backPresses: window.task1408.backPresses(),
    visibleText: document.body.innerText,
  };
})()`;

const clickScript = (selector) => `(() => { document.querySelector(${JSON.stringify(selector)}).click(); return true; })()`;
const directContinue = `window.task1408.continueDirect().then((result) => JSON.stringify(result))`;
const SETTLE = "new Promise((done) => setTimeout(() => done(true), 50))";

/** `cdp-harness`'s own evaluate() does not await promises; this one does. */
async function evaluateAsync(page, expression) {
  const evaluated = await page.send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (evaluated.exceptionDetails) throw new Error(evaluated.exceptionDetails.text || "page evaluation threw");
  return evaluated.result.value;
}

test("TASK 1408 consent page keeps Continue shut until the exact risk tick is set", async () => {
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

    // --- the page itself -------------------------------------------------
    const start = await page.evaluate(PROBE);
    assert.deepEqual(start.accountPanels, ACCOUNTS.map((account) => account.accountId), "accounts are not separate panels");
    assert.deepEqual(start.termsButtons, ACCOUNTS.map((account) => account.accountId), "one terms button per account is missing");
    assert.deepEqual(start.riskTicks, ACCOUNTS.map((account) => `${account.accountId}=false`), "risk ticks are missing or pre-set");
    assert.equal(start.backPresent, true, "Back is missing");
    assert.equal(start.anchors, 0, "the consent page carries a link that could open the terms by itself");
    log.push(`TASK1408_BROWSER_PAGE accounts=${start.accountPanels.join(",")} terms_buttons=${start.termsButtons.length} risk_ticks=${start.riskTicks.length} back=${start.backPresent} continue=1 anchors=${start.anchors}`);

    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axTree.nodes
      .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
      .filter((node) => node.name);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));

    // --- 1: Continue unavailable before the risk tick --------------------
    assert.equal(start.continueDisabled, true, "Continue was live before any risk tick");
    assert.equal(start.dataContinueAvailable, "false");
    const beforePng = await page.screenshot({ fromSurface: true });
    writeFileSync(BEFORE_PNG, beforePng);
    const beforeImage = readPng(beforePng);
    assert.deepEqual({ width: beforeImage.width, height: beforeImage.height }, WINDOW);
    log.push(`TASK1408_BEFORE_TICK continue_disabled=${start.continueDisabled} aria_disabled=${start.continueAriaDisabled} missing_risk_ticks=${start.missingRiskTicks} invoke_calls=${start.invokeCalls.length}`);

    // Pressing Continue while it is shut does nothing: no invoke, still shut.
    await page.evaluate(clickScript("#scrub-consent-continue"));
    const afterDeadClick = await page.evaluate(PROBE);
    assert.equal(afterDeadClick.continueDisabled, true);
    assert.equal(afterDeadClick.invokeCalls.length, 0, "a disabled Continue reached the hub");
    log.push(`TASK1408_DISABLED_CLICK continue_disabled=${afterDeadClick.continueDisabled} invoke_calls=${afterDeadClick.invokeCalls.length}`);

    // --- 1b: a direct call, with no button press, is refused -------------
    const refusedRaw = await evaluateAsync(page, directContinue);
    const refused = JSON.parse(refusedRaw);
    assert.equal(refused.ok, false, "a direct continue call was allowed with no risk tick");
    assert.equal(refused.refusal, REFUSAL_BOTH);
    const afterDirect = await page.evaluate(PROBE);
    assert.equal(afterDirect.invokeCalls.length, 0, "the refused direct call still reached the hub");
    assert.equal(afterDirect.notice, REFUSAL_BOTH, "the page did not say why it refused");
    assert.equal(afterDirect.continueDisabled, true);
    log.push(`TASK1408_DIRECT_INVOKE_NO_TICK ok=${refused.ok} refusal="${refused.refusal}" invoke_calls=${afterDirect.invokeCalls.length} continue_disabled=${afterDirect.continueDisabled}`);

    // --- 3: only the risk tick moves it ----------------------------------
    const untouched = [];
    for (const [name, selector] of [
      ["terms_alpha", "#scrub-consent-terms-0"],
      ["terms_beta", "#scrub-consent-terms-1"],
      ["back", "#scrub-consent-back"],
      ["account_heading_alpha", "#scrub-consent-account-0 .scp-account-name"],
    ]) {
      await page.evaluate(clickScript(selector));
      const probe = await page.evaluate(PROBE);
      assert.equal(probe.continueDisabled, true, `${name} made Continue available`);
      untouched.push(`${name}=continue_disabled:${probe.continueDisabled}`);
    }
    const afterOthers = await page.evaluate(PROBE);
    assert.ok(afterOthers.visibleText.includes("https://discord.com/terms"), "the terms button did not reveal the address");
    assert.equal(afterOthers.backPresses, 1, "Back was not wired");
    log.push(`TASK1408_ONLY_RISK_TICK ${untouched.join(" ")} back_presses=${afterOthers.backPresses} terms_address_shown=true invoke_calls=${afterOthers.invokeCalls.length}`);

    // One tick of two is still not enough.
    await page.evaluate(clickScript("#scrub-consent-risk-0"));
    const onlyOne = await page.evaluate(PROBE);
    assert.equal(onlyOne.continueDisabled, true, "one risk tick of two opened Continue");
    assert.equal(onlyOne.missingRiskTicks, ACCOUNTS[1].accountId);
    const refusedPartial = JSON.parse(await evaluateAsync(page, directContinue));
    assert.equal(refusedPartial.ok, false);
    assert.equal(refusedPartial.refusal, REFUSAL_BETA);
    log.push(`TASK1408_ONE_TICK_OF_TWO continue_disabled=${onlyOne.continueDisabled} missing_risk_ticks=${onlyOne.missingRiskTicks} direct_invoke_refusal="${refusedPartial.refusal}"`);

    // --- 2: the exact risk tick on every account opens Continue ----------
    await page.evaluate(clickScript("#scrub-consent-risk-1"));
    const opened = await page.evaluate(PROBE);
    assert.equal(opened.continueDisabled, false, "Continue stayed shut after every risk tick was set");
    assert.equal(opened.dataContinueAvailable, "true");
    assert.equal(opened.missingRiskTicks, "");
    assert.deepEqual(opened.riskTicks, ACCOUNTS.map((account) => `${account.accountId}=true`));
    const afterPng = await page.screenshot({ fromSurface: true });
    writeFileSync(AFTER_PNG, afterPng);
    const afterImage = readPng(afterPng);
    const beforeStats = pixelStats(beforeImage);
    const afterStats = pixelStats(afterImage);
    assert.ok(afterStats.nonDominantPixels > 5_000, `page barely painted: ${JSON.stringify(afterStats)}`);
    assert.notEqual(afterPng.toString("base64"), beforePng.toString("base64"), "the ticked page is byte-identical to the untouched one");
    log.push(`TASK1408_AFTER_TICK continue_disabled=${opened.continueDisabled} aria_disabled=${opened.continueAriaDisabled} risk_ticks=${opened.riskTicks.join(",")} missing_risk_ticks="${opened.missingRiskTicks}"`);

    // Continue now works, both by button and by direct call.
    await page.evaluate(clickScript("#scrub-consent-continue"));
    await evaluateAsync(page, SETTLE);
    const pressed = await page.evaluate(PROBE);
    assert.deepEqual(pressed.invokeCalls, [
      "save_discord_scrub_consent_facts",
      "save_discord_scrub_consent_facts",
      "continue_discord_scrub_after_risk_agreement",
    ], `Continue did not reach the hub: ${JSON.stringify(pressed.invokeCalls)}`);
    const allowed = JSON.parse(await evaluateAsync(page, directContinue));
    assert.equal(allowed.ok, true);
    assert.deepEqual(allowed.agreedAccountIds, ACCOUNTS.map((account) => account.accountId));
    log.push(`TASK1408_CONTINUE_WORKS button_invoke_calls=${pressed.invokeCalls.join(",")} direct_invoke_ok=${allowed.ok} agreed_ids=${allowed.agreedAccountIds.join(",")}`);

    // --- and taking the exact risk tick away shuts it again --------------
    await page.evaluate(clickScript("#scrub-consent-risk-0"));
    const reclosed = await page.evaluate(PROBE);
    assert.equal(reclosed.continueDisabled, true, "clearing the risk tick left Continue live");
    assert.equal(reclosed.missingRiskTicks, ACCOUNTS[0].accountId);
    log.push(`TASK1408_TICK_CLEARED continue_disabled=${reclosed.continueDisabled} missing_risk_ticks=${reclosed.missingRiskTicks}`);

    log.push(`TASK1408_IMAGES before=${BEFORE_PNG} after=${AFTER_PNG} ax=${AX_PATH} window=${afterImage.width}x${afterImage.height} before_non_dominant_pixels=${beforeStats.nonDominantPixels} after_non_dominant_pixels=${afterStats.nonDominantPixels}`);
    for (const line of log) console.log(line);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
});
