// TASK 1305 - capture the plain chat composer and prove it is a screen.
//
// Two captures are taken from the same shell, the same window size and the
// same stylesheet: the plain textarea-and-Send box this composer replaced,
// and the built composer. The built one has to carry all five named
// controls -- attachments, images, emoji, replies, edits -- in the screen
// tree, paint each of them, differ from the empty-state image, and it must
// carry neither a lock icon nor a pen icon (the two icons task 1305
// forbids). Comparing against a capture taken the same way is the point: a
// screenshot that only proves "some pixels exist" would pass with the
// feature absent.
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
const PNG_PATH = join(ARTIFACT_DIR, "task-1305-plain-chat-composer-900x420.png");
const EMPTY_PNG_PATH = join(ARTIFACT_DIR, "task-1305-plain-chat-composer-empty-900x420.png");
const AX_PATH = join(ARTIFACT_DIR, "task-1305-plain-chat-composer-900x420.ax.json");
const EMPTY_AX_PATH = join(ARTIFACT_DIR, "task-1305-plain-chat-composer-empty-900x420.ax.json");
const WINDOW = Object.freeze({ width: 900, height: 420 });
const NAMED_ELEMENTS = ["attachments", "images", "emoji", "replies", "edits"];
const FORBIDDEN_ICON_NAMES = ["lock", "pen", "pencil"];

async function screenFixture() {
  const outFile = join(tmpdir(), `osl-task-1305-fixture-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          PLAIN_CHAT_COMPOSER_TITLE,
          emptyPlainChatComposerModel,
          plainChatComposerEmptyStateMarkup,
          plainChatComposerMarkup,
          startPlainChatReply,
        } from "${join(ROOT, "src", "plain-chat-composer.ts")}";
        // A reply already in progress, so the capture shows a composer
        // someone has actually used rather than a fresh, empty box.
        const model = startPlainChatReply(
          emptyPlainChatComposerModel("Ben"),
          { authorName: "Ben", excerpt: "see you at the reading club" },
        );
        export const routeTitle = PLAIN_CHAT_COMPOSER_TITLE;
        export const bodyMarkup = plainChatComposerMarkup(model);
        export const emptyMarkup = plainChatComposerEmptyStateMarkup();
      `,
      resolveDir: ROOT,
      sourcefile: "task-1305-fixture-entry.mjs",
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

function fixtureHtml(title, markup) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "plain-chat-composer.css"), "utf8"),
    `html, body, #app { height: 100%; margin: 0; background: var(--bg); }
     .fixture-shell { display: flex; align-items: flex-end; height: 100%; padding: 16px; box-sizing: border-box; }`,
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>${title}</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="app">
          <div class="fixture-shell">${markup}</div>
        </div>
      </body>
    </html>`;
}

async function fixtureServer(pages) {
  const server = createServer((request, response) => {
    const key = (request.url ?? "/").replace(/^\//u, "") || "screen";
    const html = pages[key];
    if (!html) {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("no such fixture");
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  const { port } = server.address();
  return { server, base: `http://127.0.0.1:${port}/` };
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
  const dominant = Math.max(...colors.values());
  return { pixels: count, uniqueColors: colors.size, nonDominantPixels: count - dominant };
}

function differingPixels(a, b) {
  assert.equal(a.width, b.width);
  assert.equal(a.height, b.height);
  let differing = 0;
  for (let index = 0; index < a.width * a.height; index += 1) {
    const offset = index * 4;
    if (a.pixels[offset] !== b.pixels[offset]
      || a.pixels[offset + 1] !== b.pixels[offset + 1]
      || a.pixels[offset + 2] !== b.pixels[offset + 2]) differing += 1;
  }
  return differing;
}

function axNames(nodes) {
  return nodes
    .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
    .filter((node) => node.name);
}

async function capture(chrome, url, axPath, pngPath, rectScript) {
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
    writeFileSync(axPath, JSON.stringify(names, null, 2));
    const probe = rectScript ? await page.evaluate(rectScript) : { visibleText: await page.evaluate("document.body.innerText") };
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(pngPath, png);
    return { names, probe, png, image: readPng(png) };
  } finally {
    await page.close();
  }
}

const RECT_SCRIPT = `(() => {
  const rectFor = (selector) => {
    const element = document.querySelector(selector);
    if (!element) throw new Error("missing " + selector);
    const rect = element.getBoundingClientRect();
    return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
  };
  return {
    attachments: rectFor("#plain-composer-attachments"),
    images: rectFor("#plain-composer-images"),
    emoji: rectFor("#plain-composer-emoji"),
    replies: rectFor("#plain-composer-replies"),
    edits: rectFor("#plain-composer-edits"),
    replyBanner: rectFor("#plain-composer-reply-banner"),
    forbiddenIconElements: Array.from(document.querySelectorAll("[aria-label], [class]"))
      .filter((el) => {
        const label = (el.getAttribute("aria-label") || "").toLowerCase();
        const cls = (el.getAttribute("class") || "").toLowerCase();
        return /lock|pen|pencil/u.test(label) || /lock|pen|pencil/u.test(cls);
      })
      .map((el) => el.outerHTML.slice(0, 120)),
    composerHtml: document.querySelector("#plain-chat-composer").outerHTML,
    editsControlHasSvg: !!document.querySelector("#plain-composer-edits svg"),
    visibleText: document.body.innerText,
  };
})()`;

test("TASK 1305 plain chat composer shows attachments, images, emoji, replies, edits, and neither a lock nor a pen icon", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const fixture = await screenFixture();
  const { server, base } = await fixtureServer({
    screen: fixtureHtml(fixture.routeTitle, fixture.bodyMarkup),
    empty: fixtureHtml("Plain composer (before)", fixture.emptyMarkup),
  });
  const chrome = await launchChrome();
  try {
    const empty = await capture(chrome, `${base}empty`, EMPTY_AX_PATH, EMPTY_PNG_PATH, null);
    const built = await capture(chrome, `${base}screen`, AX_PATH, PNG_PATH, RECT_SCRIPT);

    assert.deepEqual({ width: built.image.width, height: built.image.height }, WINDOW);
    assert.deepEqual({ width: empty.image.width, height: empty.image.height }, WINDOW);

    // 1..5: every named control is in the screen tree, and in the visible text.
    const foundNames = [];
    for (const wanted of NAMED_ELEMENTS) {
      const hit = built.names.find((node) => node.name.toLowerCase() === wanted);
      assert.ok(hit, `screen tree missing ${wanted}; names=${built.names.map((node) => node.name).join(" | ")}`);
      foundNames.push(`${wanted}=>"${hit.name}"(${hit.role})`);
    }
    for (const wanted of ["Attachments", "Images", "Emoji", "Replies", "Edits"]) {
      assert.ok(built.probe.visibleText.includes(wanted) || built.names.some((node) => node.name === wanted), `screen missing ${wanted}`);
    }

    // The named controls are painted, not merely present in the tree.
    const painted = {
      attachments: pixelStats(built.image, built.probe.attachments),
      images: pixelStats(built.image, built.probe.images),
      emoji: pixelStats(built.image, built.probe.emoji),
      replies: pixelStats(built.image, built.probe.replies),
      edits: pixelStats(built.image, built.probe.edits),
    };
    for (const [name, stats] of Object.entries(painted)) {
      assert.ok(stats.nonDominantPixels > 10, `${name} not visibly painted: ${JSON.stringify(stats)}`);
    }

    // The reply the fixture started is visible: proof "replies" is a real control, not just a label.
    assert.ok(built.probe.visibleText.includes("Replying to Ben"), "reply banner text missing from the built screen");
    const replyBannerStats = pixelStats(built.image, built.probe.replyBanner);
    assert.ok(replyBannerStats.nonDominantPixels > 10, `reply banner not visibly painted: ${JSON.stringify(replyBannerStats)}`);

    // Neither forbidden icon: no AX node, no DOM element, and no visible text names one.
    for (const forbidden of FORBIDDEN_ICON_NAMES) {
      const axHit = built.names.find((node) => node.name.toLowerCase().includes(forbidden));
      assert.ok(!axHit, `screen tree named a forbidden icon: ${JSON.stringify(axHit)}`);
    }
    assert.equal(built.probe.forbiddenIconElements.length, 0, `found lock/pen-labelled elements: ${JSON.stringify(built.probe.forbiddenIconElements)}`);
    const lowerVisibleText = built.probe.visibleText.toLowerCase();
    assert.ok(!lowerVisibleText.includes("lock"), "visible text names a lock");
    assert.ok(!lowerVisibleText.includes("pencil"), "visible text names a pencil");
    // Shape check, not just label check: match the exact padlock and pencil
    // markup already used elsewhere in this app (osl-chats-view.ts `onceIcon`,
    // main.ts `signinLockIcon` and the native Discord composer lock), so a
    // reused icon is caught even if nobody bothered to label it "lock".
    const composerHtmlLower = built.probe.composerHtml.toLowerCase();
    const lockSignatures = [
      '<rect x="3" y="4" width="18" height="16" rx="3"/><circle cx="9" cy="9" r="2"/>',
      'a4 4 0 0 1 8 0 v3',
      'a4 4 0 0 1 8 0v3',
      'a4 4 0 0 1 7.7-1.5',
    ];
    for (const signature of lockSignatures) {
      assert.ok(!composerHtmlLower.includes(signature.toLowerCase()), `composer markup contains a known padlock signature: ${signature}`);
    }
    // The Edits control renders no <svg> at all, so no pencil-nib path can hide in it.
    assert.equal(built.probe.editsControlHasSvg, false, "Edits control has an <svg> icon; task 1305 forbids a pen icon there");

    const whole = pixelStats(built.image);
    assert.ok(whole.uniqueColors >= 8, `PNG nearly blank: uniqueColors=${whole.uniqueColors}`);
    assert.ok(whole.nonDominantPixels >= 2_000, `PNG nearly blank: nonDominantPixels=${whole.nonDominantPixels}`);

    // The empty-state control: same shell, same size, none of the five names, and no reply banner.
    const emptyLower = empty.names.map((node) => node.name.toLowerCase());
    for (const wanted of NAMED_ELEMENTS) {
      assert.ok(!emptyLower.includes(wanted), `empty-state capture already had ${wanted}`);
    }
    assert.ok(!empty.probe.visibleText?.includes("Replying to Ben"), "empty-state capture already had the reply banner");

    // 6: the built capture differs from the empty-state capture.
    assert.notEqual(built.png.toString("base64"), empty.png.toString("base64"), "built capture is byte-identical to the empty state");
    const changed = differingPixels(built.image, empty.image);
    assert.ok(changed > 2_000, `built capture barely differs from the empty state: differing_pixels=${changed}`);

    console.log(`TASK_1305_PNG ${PNG_PATH}`);
    console.log(`TASK_1305_EMPTY_PNG ${EMPTY_PNG_PATH}`);
    console.log(`TASK_1305_AX ${AX_PATH}`);
    console.log(`TASK_1305_WINDOW ${built.image.width}x${built.image.height}`);
    console.log(`TASK_1305_NAMED_ELEMENTS ${foundNames.join(" | ")}`);
    console.log(`TASK_1305_FORBIDDEN_ICON_ELEMENTS ${JSON.stringify(built.probe.forbiddenIconElements)}`);
    console.log(`TASK_1305_PAINTED ${Object.entries(painted).map(([name, stats]) => `${name}_non_dominant_pixels=${stats.nonDominantPixels}`).join(" ")}`);
    console.log(`TASK_1305_REPLY_BANNER_PAINTED non_dominant_pixels=${replyBannerStats.nonDominantPixels}`);
    console.log(`TASK_1305_IMAGE whole_unique_colors=${whole.uniqueColors} whole_non_dominant_pixels=${whole.nonDominantPixels}`);
    console.log(`TASK_1305_DIFF_VS_EMPTY differing_pixels=${changed} of ${built.image.width * built.image.height} built_bytes=${built.png.length} empty_bytes=${empty.png.length}`);
  } finally {
    await chrome.close();
    await closeServer(server);
  }
});
