/**
 * TASK 0792 - the fixed-size Linux screenshot of the Account screen, showing
 * all seven controls, with none of the fixture secrets anywhere in the capture.
 *
 * The seven are identity, password, recovery, lock, stealth, burn password and
 * Pro code. Each has to be findable twice: by name in the screen tree (the
 * page's own text and Chrome's accessibility tree) and as ink in the PNG. "Ink"
 * is measured, not assumed -- the check crops each control's rectangle and
 * counts the pixels that differ from that crop's own most common colour -- so a
 * control whose text failed to draw fails here even though the DOM still says
 * the word.
 *
 * The second half is the one this screen exists for. The fixture hands the
 * screen a real sign-in password, a twelve-word recovery phrase, a Pro code and
 * the two role passwords, and the capture reads them back out of the page to
 * prove the screen was given them. Then it searches everything the capture
 * produced -- the page text, the screen's own markup, the accessibility tree,
 * the artifact file and the PNG bytes -- for each of those secrets and for
 * every six-character run of them, and requires nothing back.
 *
 * A text search of a PNG is a check that cannot fail on its own, because the
 * pixels are deflate-compressed and no string survives into the bytes. So the
 * picture is checked a second way, on the pixels: every masked slot has to be a
 * run of identical ink blobs, which is what twelve bullets are and what no
 * password, phrase or code is. And the whole search is run once more against
 * `?leak=1`, a fixture-only mode that puts each secret into the slot standing
 * for it -- there the search has to find all three named secrets and the blob
 * signature has to break, which is what makes the zero above worth reading.
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
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0792-account.png");
const TREE_PATH = path.join(ARTIFACT_DIR, "task-0792-account-screen-tree.json");
const FIXTURE = "screenshots/task-0792-account-fixture.html";

/** The title and the seven named controls this task is measured against. */
const SCREEN_TITLE = "Account";
const NAMED_CONTROLS = [
  "identity",
  "password",
  "recovery",
  "lock",
  "stealth",
  "burn password",
  "Pro code",
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
      counts.set(rgb.join(","), (counts.get(rgb.join(",")) ?? 0) + 1);
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

/**
 * What the ink in a masked slot is shaped like.
 *
 * A masked slot draws the same character twelve times, so its ink falls into
 * twelve blobs of one width, one height and one top edge. Any text drawn there
 * instead -- a password, a phrase, a code -- has glyphs of different widths,
 * ascenders and descenders, and letters that run together. Reading the shape
 * rather than the bytes is how this check says something about the picture that
 * a search of a compressed PNG cannot.
 *
 * Only strong pixels count: a bullet's core is at full contrast, so the blobs
 * do not change size with the antialiasing at their edges.
 */
function maskSignature(png, rect, expectedBlobs, threshold = 90) {
  const x0 = Math.max(0, Math.floor(rect.x));
  const y0 = Math.max(0, Math.floor(rect.y));
  const x1 = Math.min(png.width, Math.ceil(rect.x + rect.width));
  const y1 = Math.min(png.height, Math.ceil(rect.y + rect.height));
  const counts = new Map();
  for (let y = y0; y < y1; y += 1) {
    for (let x = x0; x < x1; x += 1) {
      const offset = (y * png.width + x) * 4;
      const key = `${png.pixels[offset]},${png.pixels[offset + 1]},${png.pixels[offset + 2]}`;
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
  }
  let background = "0,0,0";
  let best = -1;
  for (const [key, count] of counts) {
    if (count > best) {
      best = count;
      background = key;
    }
  }
  const [br, bg, bb] = background.split(",").map(Number);
  const isInk = (x, y) => {
    const offset = (y * png.width + x) * 4;
    return (
      Math.abs(png.pixels[offset] - br) > threshold
      || Math.abs(png.pixels[offset + 1] - bg) > threshold
      || Math.abs(png.pixels[offset + 2] - bb) > threshold
    );
  };
  const blobs = [];
  let current = null;
  for (let x = x0; x < x1; x += 1) {
    let top = null;
    let bottom = null;
    for (let y = y0; y < y1; y += 1) {
      if (!isInk(x, y)) continue;
      if (top === null) top = y;
      bottom = y;
    }
    if (top === null) {
      if (current) blobs.push(current);
      current = null;
      continue;
    }
    if (!current) current = { left: x, right: x, top, bottom };
    else {
      current.right = x;
      current.top = Math.min(current.top, top);
      current.bottom = Math.max(current.bottom, bottom);
    }
  }
  if (current) blobs.push(current);
  const measured = blobs.map((blob) => ({
    width: blob.right - blob.left + 1,
    height: blob.bottom - blob.top + 1,
    top: blob.top - y0,
  }));
  const spread = (key) =>
    measured.length === 0
      ? 0
      : Math.max(...measured.map((blob) => blob[key])) - Math.min(...measured.map((blob) => blob[key]));
  const signature = {
    blobs: measured.length,
    widthSpread: spread("width"),
    heightSpread: spread("height"),
    topSpread: spread("top"),
    widths: measured.map((blob) => blob.width),
    heights: measured.map((blob) => blob.height),
  };
  signature.uniform =
    signature.blobs === expectedBlobs
    && signature.widthSpread <= 1
    && signature.heightSpread <= 1
    && signature.topSpread <= 1;
  return signature;
}

/** Every place a probe is looked for, and every hit found. */
function searchProbes(haystacks, probesBySecret) {
  const hits = [];
  for (const [secretName, probes] of Object.entries(probesBySecret)) {
    for (const probe of probes) {
      for (const [where, hay] of Object.entries(haystacks)) {
        if (hay.toLowerCase().includes(probe.toLowerCase())) {
          hits.push({ secret: secretName, probe, where });
        }
      }
    }
  }
  return hits;
}

/** What the page can say about the title and the seven named controls. */
const READ_SCREEN = `(() => {
  const box = (element) => {
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  const heading = document.querySelector(".account-screen-heading");
  const controls = [...document.querySelectorAll(".account-control")].map((control) => {
    const name = control.querySelector(".account-name");
    const value = control.querySelector(".account-value");
    const reset = control.querySelector("[data-account-reset]");
    return {
      id: control.dataset.accountControl,
      label: name.textContent.trim(),
      secret: control.dataset.secret,
      changed: control.dataset.changed,
      reset: control.dataset.reset,
      resetText: reset.querySelector(".account-reset-text").textContent.trim(),
      resetButton: reset.querySelector(".account-reset-button")?.textContent.trim() ?? null,
      noReset: reset.querySelector(".account-reset-none")?.textContent.trim() ?? null,
      detail: control.querySelector(".account-detail").textContent.trim(),
      valueTag: value.tagName.toLowerCase(),
      valueText: value.tagName.toLowerCase() === "span"
        ? value.textContent
        : (value.tagName.toLowerCase() === "select"
          ? value.options[value.selectedIndex].textContent
          : value.value),
      masked: value.dataset.accountMasked ?? null,
      valueLabel: value.getAttribute("aria-label"),
      action: control.querySelector(".account-action")?.textContent.trim() ?? null,
      box: box(control),
      nameBox: box(name),
      valueBox: box(value),
    };
  });
  const buttons = [...document.querySelectorAll(".account-action-button")].map((button) => ({
    action: button.dataset.accountAction,
    label: button.textContent.trim(),
    box: box(button),
  }));
  return {
    title: heading.textContent.trim(),
    titleBox: box(heading),
    controls,
    buttons,
    status: document.querySelector(".account-status").textContent.trim(),
    scrollHeight: document.documentElement.scrollHeight,
    scrollWidth: document.documentElement.scrollWidth,
    html: document.querySelector(".account-screen").outerHTML,
    text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
  };
})()`;

test("TASK 0792 captures the Account screen with all seven controls and no secret", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startVite();
  const screenModule = await server.ssrLoadModule("/src/account-screen.ts");
  const dataModule = await server.ssrLoadModule("/src/account-screen-data.ts");
  const window = dataModule.ACCOUNT_SCREEN_WINDOW;
  const secrets = dataModule.ACCOUNT_SCREEN_SECRETS;
  const settings = dataModule.ACCOUNT_SCREEN_SETTINGS;
  const maskLength = screenModule.ACCOUNT_MASK_LENGTH;

  // Every probe of every secret, and the three the finish line names.
  const probesBySecret = Object.fromEntries(
    Object.entries(secrets).map(([name, secret]) => [name, dataModule.accountSecretProbes(secret)]),
  );
  const namedSecrets = dataModule.ACCOUNT_SCREEN_NAMED_SECRETS;
  const namedProbes = Object.fromEntries(
    namedSecrets.map((name) => [name, probesBySecret[name]]),
  );

  const started = await (async () => {
    try {
      // The seven names are the screen's own, not a list kept beside it.
      assert.deepEqual(
        screenModule.ACCOUNT_CONTROL_IDS.map((id) =>
          screenModule.ACCOUNT_CONTROL_LABELS[id].toLowerCase(),
        ),
        NAMED_CONTROLS.map((name) => name.toLowerCase()),
      );
      // A capture of a screen holding nothing would find nothing.
      assert.deepEqual([...namedSecrets], ["password", "recoveryPhrase", "proCode"]);
      for (const name of namedSecrets) {
        assert.ok(secrets[name].length >= 12, `${name} fixture value is too short to test with`);
        assert.ok(probesBySecret[name].length >= 6, `${name} has too few probes`);
      }
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

    const ready = `new Promise((resolve, reject) => {
      const deadline = Date.now() + 10000;
      const tick = () => {
        if (document.documentElement.dataset.task0792 === "ready"
          && document.querySelectorAll(".account-control").length === ${NAMED_CONTROLS.length}) {
          requestAnimationFrame(() => requestAnimationFrame(resolve));
          return;
        }
        if (Date.now() > deadline) {
          reject(new Error("account screen did not render"));
          return;
        }
        setTimeout(tick, 25);
      };
      tick();
    })`;
    await evaluateValue(page, ready);

    // The page really was handed the secrets. Read back here and compared, and
    // deliberately not written into the artifact.
    const held = await evaluateValue(page, `JSON.stringify(window.oslAccountSecrets)`);
    assert.deepEqual(JSON.parse(held), secrets);

    const screen = await evaluateValue(page, READ_SCREEN);
    const ax = await page.send("Accessibility.getFullAXTree");
    const axText = accessibilityTreeText(ax.nodes ?? []);
    const treeText = [screen.text, axText].join("\n").toLowerCase();

    // 1. The title, on the screen and in the tree.
    assert.equal(screen.title, SCREEN_TITLE);
    assert.match(treeText, new RegExp(escapeRegExp(SCREEN_TITLE.toLowerCase()), "u"));
    assert.match(axText.toLowerCase(), new RegExp(escapeRegExp(SCREEN_TITLE.toLowerCase()), "u"));

    // 2. The seven named controls, drawn in that order, each in the tree.
    assert.deepEqual(
      screen.controls.map((control) => control.label.toLowerCase()),
      NAMED_CONTROLS.map((name) => name.toLowerCase()),
    );
    assert.deepEqual(
      screen.controls.map((control) => control.id),
      [...screenModule.ACCOUNT_CONTROL_IDS],
    );
    for (const name of NAMED_CONTROLS) {
      const pattern = new RegExp(escapeRegExp(name.toLowerCase()), "u");
      assert.match(treeText, pattern, `missing from the screen text: ${name}`);
      assert.match(axText.toLowerCase(), pattern, `missing from the accessibility tree: ${name}`);
    }
    assert.deepEqual(screen.buttons.map((button) => button.action), ["save", "reset-safe"]);

    // 3. Every control reads something, and the five secret ones read the mask.
    const masked = screen.controls.filter((control) => control.secret === "yes");
    assert.deepEqual(
      masked.map((control) => control.id),
      ["password", "recovery", "stealth", "burn-password", "pro-code"],
    );
    for (const control of masked) {
      assert.equal(control.valueText, screenModule.ACCOUNT_MASK);
      assert.equal(control.valueText.length, maskLength);
      assert.equal(control.masked, control.id);
    }
    assert.equal(screen.controls[0].valueText, settings.displayName);
    assert.equal(screen.controls[3].valueText, screenModule.lockLabel(settings.lockMinutes));

    // 4. Reset is offered on four controls and refused with a reason on three.
    const safe = screen.controls.filter((control) => control.reset === "safe");
    const unsafe = screen.controls.filter((control) => control.reset === "unsafe");
    assert.deepEqual(safe.map((control) => control.id), [...screenModule.ACCOUNT_SAFE_RESET_IDS]);
    assert.deepEqual(unsafe.map((control) => control.id), ["password", "recovery", "pro-code"]);
    for (const control of safe) assert.equal(control.resetButton, "Reset");
    for (const control of unsafe) {
      assert.equal(control.resetButton, null);
      assert.equal(control.noReset, "No reset");
      assert.ok(control.resetText.length > 40, `${control.id} gives no reason`);
      assert.match(treeText, new RegExp(escapeRegExp(control.resetText.toLowerCase()), "u"));
    }

    // 5. Everything named is inside the fixed-size capture.
    assert.equal(screen.scrollWidth, window.width);
    assert.ok(
      screen.scrollHeight <= window.height,
      `page is ${screen.scrollHeight}px tall, taller than the ${window.height}px capture`,
    );
    const boxes = {
      title: screen.titleBox,
      ...Object.fromEntries(screen.controls.flatMap((control) => [
        [`${control.label}|control`, control.box],
        [`${control.label}|name`, control.nameBox],
        [`${control.label}|value`, control.valueBox],
      ])),
      ...Object.fromEntries(screen.buttons.map((button) => [`${button.label}|button`, button.box])),
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
    const signatures = Object.fromEntries(
      masked.map((control) => [
        control.id,
        maskSignature(image, control.valueBox, maskLength),
      ]),
    );

    const artifact = {
      url: `${url}${FIXTURE}`,
      window,
      title: SCREEN_TITLE,
      namedControls: NAMED_CONTROLS,
      // The secrets themselves are not written here. Their digests are, so the
      // artifact says which values were searched for without carrying them.
      secretDigests: Object.fromEntries(
        Object.entries(secrets).map(([name, secret]) => [
          name,
          { sha256: createHash("sha256").update(secret).digest("hex"), length: secret.length },
        ]),
      ),
      screen,
      facts,
      ink,
      maskSignatures: signatures,
      wholeImage: whole,
      axNodes: ax.nodes,
    };
    const artifactText = JSON.stringify(artifact, null, 2);
    writeFileSync(TREE_PATH, artifactText);

    // 6. The picture is the right size and is not blank or nearly blank.
    assert.equal(facts.width, window.width);
    assert.equal(facts.height, window.height);
    assert.ok(png.length > 5_000, `PNG too small: ${png.length}`);
    assert.ok(facts.distinctColors > 20, `PNG nearly blank: ${facts.distinctColors} distinct colours`);
    assert.ok(
      whole.inkShare > 0.02,
      `PNG nearly blank: only ${(whole.inkShare * 100).toFixed(2)}% of pixels differ from the background`,
    );

    // 7. The title and every named control carry ink in the PNG itself.
    assert.ok(ink.title.inkShare > 0.02, `title did not draw: ${JSON.stringify(ink.title)}`);
    for (const control of screen.controls) {
      const whole0 = ink[`${control.label}|control`];
      const name = ink[`${control.label}|name`];
      const value = ink[`${control.label}|value`];
      assert.ok(whole0.inkShare > 0.02, `${control.label} did not draw: ${JSON.stringify(whole0)}`);
      assert.ok(name.inkShare > 0.05, `${control.label} name did not draw: ${JSON.stringify(name)}`);
      assert.ok(value.inkShare > 0.02, `${control.label} value did not draw: ${JSON.stringify(value)}`);
      assert.ok(
        facts.crops[`${control.label}|control`].distinctColors > 3,
        `${control.label} has no ink: ${facts.crops[`${control.label}|control`].distinctColors} colours`,
      );
    }
    // Save account is the brand-painted button; no part of the grey chrome can
    // make that colour, so this is the picture and not the markup talking.
    const saveCrop = facts.crops["Save account|button"];
    assert.ok(
      saveCrop.saturatedPixels / saveCrop.pixels > 0.5,
      `Save account is not brand-painted: ${saveCrop.saturatedPixels}/${saveCrop.pixels}`,
    );

    // 8. Every masked slot is a run of identical blobs in the pixels: twelve
    //    bullets, not a secret. This is the claim about the picture that a
    //    search of compressed bytes cannot make.
    for (const control of masked) {
      const signature = signatures[control.id];
      assert.equal(
        signature.blobs,
        maskLength,
        `${control.label} drew ${signature.blobs} blobs, not ${maskLength}: ${JSON.stringify(signature)}`,
      );
      assert.ok(
        signature.uniform,
        `${control.label} is not a run of identical marks: ${JSON.stringify(signature)}`,
      );
    }

    // 9. The search. Nothing the capture produced holds any secret, whole or in
    //    six-character pieces.
    const haystacks = {
      "page text": screen.text,
      "screen markup": screen.html,
      "accessibility tree": JSON.stringify(ax.nodes),
      "artifact file": artifactText,
      "png bytes": png.toString("latin1"),
    };
    const hits = searchProbes(haystacks, probesBySecret);
    assert.deepEqual(hits, [], `the capture carries secrets: ${JSON.stringify(hits)}`);
    const namedHits = searchProbes(haystacks, namedProbes);
    assert.equal(namedHits.length, 0);

    console.log(`TASK0792_PNG=${PNG_PATH}`);
    console.log(`TASK0792_TREE=${TREE_PATH}`);
    console.log(`TASK0792_URL=${url}${FIXTURE}`);
    console.log(`TASK0792_WINDOW=${facts.width}x${facts.height}`);
    console.log(`TASK0792_PNG_BYTES=${png.length}`);
    console.log(`TASK0792_PNG_SHA256=${createHash("sha256").update(png).digest("hex")}`);
    console.log(`TASK0792_PNG_DISTINCT_COLORS=${facts.distinctColors}`);
    console.log(`TASK0792_PNG_INK_SHARE=${(whole.inkShare * 100).toFixed(2)}%`);
    console.log(`TASK0792_TITLE=${screen.title}`);
    console.log(`TASK0792_CONTROL_COUNT=${screen.controls.length}`);
    console.log(`TASK0792_PAGE_HEIGHT=${screen.scrollHeight}`);
    console.log(`TASK0792_STATUS=${screen.status}`);
    for (const control of screen.controls) {
      const shape = ink[`${control.label}|control`];
      const signature = signatures[control.id];
      console.log(
        `TASK0792_CONTROL=${control.label}|tree=yes|ink=${shape.inkPixels}/${shape.pixels}`
        + ` (${(shape.inkShare * 100).toFixed(1)}%)`
        + `|reset=${control.reset}`
        + `|value=${control.secret === "yes" ? `masked ${signature.blobs} blobs, widths ${signature.widths.join("/")}` : control.valueText}`,
      );
    }
    console.log(`TASK0792_SEARCH_HAYSTACKS=${Object.keys(haystacks).join(", ")}`);
    for (const [name, probes] of Object.entries(probesBySecret)) {
      const found = searchProbes({ ...haystacks }, { [name]: probes }).length;
      console.log(
        `TASK0792_SECRET=${name}|sha256=${createHash("sha256").update(secrets[name]).digest("hex").slice(0, 16)}`
        + `|probes=${probes.length}|hits=${found}`,
      );
    }
    console.log(`TASK0792_TOTAL_HITS=${hits.length}`);

    // 10. Driving the screen never changes that. Asking for the recovery kit is
    //     handed to another screen; the kit is still not here.
    const driven = await evaluateValue(page, `(() => {
      document.querySelector('.account-action[data-account-control="recovery"]').click();
      document.querySelector('.account-reset-button[data-account-control="burn-password"]').click();
      return {
        request: window.oslLastAccountRequest,
        status: document.querySelector(".account-status").textContent.trim(),
        burnValue: document.querySelector('.account-control[data-account-control="burn-password"] .account-value').textContent.trim(),
        burnDetail: document.querySelector('.account-control[data-account-control="burn-password"] .account-detail').textContent.trim(),
        changed: [...document.querySelectorAll('.account-control[data-changed="yes"]')].map((row) => row.dataset.accountControl),
        text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
        html: document.querySelector(".account-screen").outerHTML,
      };
    })()`);
    assert.equal(driven.request, "recovery");
    assert.equal(driven.burnValue, "Off");
    assert.deepEqual(driven.changed, ["burn-password"]);
    assert.deepEqual(searchProbes({ text: driven.text, html: driven.html }, probesBySecret), []);

    const resetAll = await evaluateValue(page, `(() => {
      document.querySelector('[data-account-action="reset-safe"]').click();
      const value = (id, part) => document.querySelector('.account-control[data-account-control="' + id + '"] .' + part).textContent.trim();
      document.querySelector('[data-account-action="save"]').click();
      return {
        identity: document.querySelector('[data-account-input="identity"]').value,
        lock: document.querySelector('[data-account-select="lock"]').selectedOptions[0].textContent,
        stealth: value("stealth", "account-value"),
        burn: value("burn-password", "account-value"),
        password: value("password", "account-value"),
        recovery: value("recovery", "account-value"),
        proCode: value("pro-code", "account-value"),
        status: document.querySelector(".account-status").textContent.trim(),
        saved: window.oslLastSavedAccount,
      };
    })()`);
    assert.equal(resetAll.identity, settings.handle);
    assert.equal(resetAll.lock, screenModule.lockLabel(screenModule.DEFAULT_LOCK_MINUTES));
    assert.equal(resetAll.stealth, "Off");
    assert.equal(resetAll.burn, "Off");
    // The three that cannot be put back were not touched by "Reset safe settings".
    assert.equal(resetAll.password, screenModule.ACCOUNT_MASK);
    assert.equal(resetAll.recovery, screenModule.ACCOUNT_MASK);
    assert.equal(resetAll.proCode, screenModule.ACCOUNT_MASK);
    assert.equal(resetAll.status, "All 7 account controls saved.");
    assert.deepEqual(
      resetAll.saved.map((setting) => `${setting.id}=${setting.reading}`),
      [
        `identity=${settings.handle}`,
        "password=set",
        "recovery=12 words",
        "lock=After 5 minutes",
        "stealth=off",
        "burn-password=off",
        "pro-code=set",
      ],
    );
    assert.deepEqual(searchProbes({ saved: JSON.stringify(resetAll.saved) }, probesBySecret), []);
    console.log(`TASK0792_REQUEST=${driven.request}`);
    console.log(`TASK0792_BURN_AFTER_RESET=${driven.burnValue}`);
    console.log(`TASK0792_RESET_SAFE=identity=${resetAll.identity}|lock=${resetAll.lock}|stealth=${resetAll.stealth}|burn=${resetAll.burn}`);
    console.log(`TASK0792_RESET_KEPT=password=${resetAll.password.length} marks|recovery=${resetAll.recovery.length} marks|pro-code=${resetAll.proCode.length} marks`);
    console.log(`TASK0792_SAVE_STATUS=${resetAll.status}`);

    // 11. The same search against a screen that leaks. `?leak=1` is fixture-only
    //     and puts each secret into the slot standing for it: the search has to
    //     come back full and the blob signature has to break, or the zero above
    //     was measuring nothing. No artifact is written for this run.
    await page.navigate(`${url}${FIXTURE}?leak=1`, { timeoutMs: 30_000 });
    await evaluateValue(page, ready);
    const leakScreen = await evaluateValue(page, READ_SCREEN);
    const leakAx = await page.send("Accessibility.getFullAXTree");
    const leakPng = await page.screenshot({ fromSurface: true, captureBeyondViewport: false });
    const leakImage = parsePng(leakPng);
    const leakHaystacks = {
      "page text": leakScreen.text,
      "screen markup": leakScreen.html,
      "accessibility tree": JSON.stringify(leakAx.nodes ?? []),
      "png bytes": leakPng.toString("latin1"),
    };
    const leakHits = searchProbes(leakHaystacks, namedProbes);
    const leakedSecrets = new Set(leakHits.map((hit) => hit.secret));
    assert.deepEqual(
      [...leakedSecrets].sort(),
      [...namedSecrets].sort(),
      `the search missed a leak: ${JSON.stringify(leakHits)}`,
    );
    for (const name of namedSecrets) {
      assert.ok(
        leakHits.some((hit) => hit.secret === name && hit.probe === secrets[name]),
        `the whole ${name} was not found on the leaking screen`,
      );
    }
    const leakSignatures = Object.fromEntries(
      leakScreen.controls
        .filter((control) => control.masked)
        .map((control) => [control.id, maskSignature(leakImage, control.valueBox, maskLength)]),
    );
    for (const id of ["password", "recovery", "pro-code"]) {
      assert.equal(
        leakSignatures[id].uniform,
        false,
        `${id} still looked like a run of identical marks while leaking: ${JSON.stringify(leakSignatures[id])}`,
      );
    }
    const leakPngHits = searchProbes({ "png bytes": leakHaystacks["png bytes"] }, namedProbes).length;
    console.log(`TASK0792_LEAK_URL=${url}${FIXTURE}?leak=1`);
    console.log(`TASK0792_LEAK_HITS=${leakHits.length}`);
    for (const name of namedSecrets) {
      const found = leakHits.filter((hit) => hit.secret === name);
      console.log(
        `TASK0792_LEAK_SECRET=${name}|hits=${found.length}|where=${[...new Set(found.map((hit) => hit.where))].join(",")}`,
      );
    }
    for (const id of ["password", "recovery", "pro-code"]) {
      console.log(
        `TASK0792_LEAK_SIGNATURE=${id}|blobs=${leakSignatures[id].blobs}|widthSpread=${leakSignatures[id].widthSpread}`
        + `|heightSpread=${leakSignatures[id].heightSpread}|uniform=${leakSignatures[id].uniform}`,
      );
    }
    // Said plainly rather than left to be inferred: the PNG bytes are
    // compressed, so that one haystack finds nothing even when the screen is
    // shouting the password. The blob signature above is what covers the image.
    console.log(`TASK0792_LEAK_PNG_BYTE_HITS=${leakPngHits}`);
  } finally {
    await page.close();
    await chrome.close();
    await server.close();
  }
}, { timeout: 180_000 });
