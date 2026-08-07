import assert from "node:assert/strict";
import { createHash } from "node:crypto";
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
const PNG_PATH = join(ARTIFACT_DIR, "task-0732-new-friend-defaults-900x900.png");
const AX_PATH = join(ARTIFACT_DIR, "task-0732-new-friend-defaults-900x900.ax.json");
const EMPTY_PNG_PATH = join(ARTIFACT_DIR, "task-0732-new-friend-defaults-empty-state-900x900.png");

/**
 * Tall enough that the whole screen -- down to Save default and Reset -- is
 * inside the picture. At 700 the action row sat below the fold: the words were
 * in the document, the buttons were not in the screenshot, and a check that
 * only read innerText would have called that a pass.
 */
const WINDOW = Object.freeze({ width: 900, height: 900 });

/**
 * The five things TASK 0732 says the screenshot has to show. The first three
 * are the accessible names of the three choice groups AND words in the visible
 * legend above them; the last two are buttons, named by the text on their face.
 */
const REQUIRED_NAMES = ["accounts", "conversations", "checkmark", "Save default", "Reset"];

/**
 * The empty state: the Settings > Friends panel exactly as it stood before this
 * task built anything into it. This is the control the built screen is measured
 * against -- if a capture of THIS produced the same picture, the screen would
 * not have been built.
 */
const EMPTY_STATE_MARKUP = `<div class="empty-state"><strong>Nothing here yet</strong><p>Friend defaults have not been built.</p></div>`;

async function newFriendDefaultsFixture() {
  const outFile = join(tmpdir(), `osl-new-friend-defaults-fixture-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          NEW_FRIEND_DEFAULTS_TITLE,
          initialNewFriendDefaults,
          newFriendDefaultsMarkup,
        } from "${join(ROOT, "src", "new-friend-defaults.ts")}";
        export const routeTitle = NEW_FRIEND_DEFAULTS_TITLE;
        export const bodyMarkup = newFriendDefaultsMarkup(initialNewFriendDefaults());
      `,
      resolveDir: ROOT,
      sourcefile: "new-friend-defaults-fixture-entry.mjs",
      loader: "js",
    },
    bundle: true,
    platform: "node",
    format: "esm",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return import(`file://${outFile}`);
}

/**
 * The same Settings shell for both captures. Only the panel body changes, so a
 * difference between the two pictures is the screen, not the chrome around it.
 */
function fixtureHtml(bodyMarkup) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "new-friend-defaults.css"), "utf8"),
    "html, body, #app { height: 100%; }",
  ].join("\n");
  const sections = ["Account", "Apps", "Friends", "Scrub", "Cleanup", "Notifications", "Appearance", "About"];
  const nav = sections
    .map((label) => `<button class="${label === "Friends" ? "active" : ""}" ${label === "Friends" ? 'aria-current="true"' : ""} type="button">${label}</button>`)
    .join("");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>Settings</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="app">
          <div class="app-frame">
            <main class="content-viewport settings-page" aria-labelledby="route-heading">
              <nav class="settings-sidebar" aria-label="Settings">
                <h1 id="route-heading" tabindex="-1">Settings</h1>
                ${nav}
              </nav>
              <section class="settings-detail">${bodyMarkup}</section>
            </main>
          </div>
        </div>
      </body>
    </html>`;
}

async function fixtureServer(html) {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
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

function pixelStats(image, rect = { x: 0, y: 0, width: image.width, height: image.height }) {
  const colors = new Map();
  const left = Math.max(0, Math.floor(rect.x));
  const top = Math.max(0, Math.floor(rect.y));
  const right = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const bottom = Math.min(image.height, Math.ceil(rect.y + rect.height));
  let count = 0;
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const offset = (y * image.width + x) * 4;
      const key = `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
      colors.set(key, (colors.get(key) ?? 0) + 1);
      count += 1;
    }
  }
  // No pixels means the box was empty or outside the picture. Report zero, not
  // `count - (-Infinity)`, which reads as an infinitely well-painted control.
  if (count === 0) return { pixels: 0, uniqueColors: 0, nonDominantPixels: 0 };
  const dominant = Math.max(...colors.values());
  return { pixels: count, uniqueColors: colors.size, nonDominantPixels: count - dominant };
}

function differingPixels(left, right) {
  assert.equal(left.width, right.width);
  assert.equal(left.height, right.height);
  let differing = 0;
  for (let index = 0; index < left.pixels.length; index += 4) {
    if (left.pixels[index] !== right.pixels[index]
      || left.pixels[index + 1] !== right.pixels[index + 1]
      || left.pixels[index + 2] !== right.pixels[index + 2]) differing += 1;
  }
  return differing;
}

function axNames(nodes) {
  return nodes
    .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
    .filter((node) => node.name);
}

/** Capture one panel body in the shared Settings shell. */
async function capture(chrome, bodyMarkup) {
  const { server, url } = await fixtureServer(fixtureHtml(bodyMarkup));
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");
    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axNames(axTree.nodes);
    const visibleText = await page.evaluate(`document.body.innerText`);
    const rects = await page.evaluate(`(() => {
      const rectFor = (selector) => {
        const element = document.querySelector(selector);
        if (!element) return null;
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      return {
        accounts: rectFor('[data-new-friend-group="accounts"]'),
        conversations: rectFor('[data-new-friend-group="conversations"]'),
        checkmark: rectFor('[data-new-friend-group="checkmark"]'),
        save: rectFor("#save-new-friend-default"),
        reset: rectFor("#reset-new-friend-default"),
      };
    })()`);
    const png = await page.screenshot({ fromSurface: true });
    return { names, visibleText, rects, png, image: readPng(png) };
  } finally {
    await page.close();
    await closeServer(server);
  }
}

test("TASK 0732 the new-friend default screen shows all five named elements and differs from the empty state", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const fixture = await newFriendDefaultsFixture();
  assert.equal(fixture.routeTitle, "New friend defaults");

  const chrome = await launchChrome();
  try {
    const built = await capture(chrome, fixture.bodyMarkup);
    const empty = await capture(chrome, EMPTY_STATE_MARKUP);
    writeFileSync(PNG_PATH, built.png);
    writeFileSync(EMPTY_PNG_PATH, empty.png);
    writeFileSync(AX_PATH, JSON.stringify(built.names, null, 2));

    // 1..5: every named element, in the screen tree and in the visible words.
    const screenTreeCounts = {};
    const visibleTextCounts = {};
    for (const name of REQUIRED_NAMES) {
      screenTreeCounts[name] = built.names.filter((node) => node.name === name).length;
      visibleTextCounts[name] = built.visibleText.split(name).length - 1;
      assert.ok(screenTreeCounts[name] > 0, `screen tree missing ${name}`);
      assert.ok(visibleTextCounts[name] > 0, `visible text missing ${name}`);
    }

    // Named is not the same as drawn, and drawn is not the same as drawn
    // INSIDE THE PICTURE. Each control has to have a box, that box has to fall
    // within the captured window, and it has to carry paint.
    const painted = {};
    for (const [key, rect] of Object.entries(built.rects)) {
      assert.ok(rect, `${key} has no box on the built screen`);
      assert.ok(rect.width > 0 && rect.height > 0, `${key} has an empty box: ${JSON.stringify(rect)}`);
      assert.ok(
        rect.x >= 0 && rect.y >= 0 && rect.x + rect.width <= WINDOW.width && rect.y + rect.height <= WINDOW.height,
        `${key} is outside the captured ${WINDOW.width}x${WINDOW.height} window: ${JSON.stringify(rect)}`,
      );
      painted[key] = pixelStats(built.image, rect).nonDominantPixels;
      assert.ok(painted[key] > 50, `${key} is not visibly painted: ${painted[key]} non-dominant pixels`);
    }

    assert.deepEqual({ width: built.image.width, height: built.image.height }, WINDOW);
    const whole = pixelStats(built.image);
    assert.ok(whole.uniqueColors >= 64, `PNG nearly blank: uniqueColors=${whole.uniqueColors}`);
    assert.ok(whole.nonDominantPixels >= 10_000, `PNG nearly blank: nonDominantPixels=${whole.nonDominantPixels}`);

    // The empty state is the control: none of the five are in it, and the two
    // pictures are not the same picture.
    for (const name of REQUIRED_NAMES) {
      assert.equal(empty.names.filter((node) => node.name === name).length, 0, `empty state should not name ${name}`);
    }
    const builtHash = createHash("sha256").update(built.png).digest("hex");
    const emptyHash = createHash("sha256").update(empty.png).digest("hex");
    assert.notEqual(builtHash, emptyHash, "built screenshot is byte-identical to the empty-state capture");
    const changed = differingPixels(built.image, empty.image);
    assert.ok(changed > 10_000, `built screenshot barely differs from the empty state: ${changed} pixels`);

    console.log(`TASK_0732_PNG ${PNG_PATH}`);
    console.log(`TASK_0732_EMPTY_PNG ${EMPTY_PNG_PATH}`);
    console.log(`TASK_0732_AX ${AX_PATH}`);
    console.log(`TASK_0732_WINDOW ${built.image.width}x${built.image.height}`);
    console.log(`TASK_0732_SCREEN_TREE_NAMED ${REQUIRED_NAMES.map((name) => `${name}=${screenTreeCounts[name]}`).join(" ")}`);
    console.log(`TASK_0732_VISIBLE_TEXT_NAMED ${REQUIRED_NAMES.map((name) => `${name}=${visibleTextCounts[name]}`).join(" ")}`);
    console.log(`TASK_0732_PAINTED ${Object.entries(painted).map(([key, value]) => `${key}=${value}`).join(" ")}`);
    console.log(`TASK_0732_IMAGE unique_colors=${whole.uniqueColors} non_dominant_pixels=${whole.nonDominantPixels}`);
    console.log(`TASK_0732_BUILT_SHA256 ${builtHash}`);
    console.log(`TASK_0732_EMPTY_SHA256 ${emptyHash}`);
    console.log(`TASK_0732_DIFF_VS_EMPTY_STATE ${changed} pixels of ${built.image.width * built.image.height}`);
  } finally {
    await chrome.close();
  }
});
