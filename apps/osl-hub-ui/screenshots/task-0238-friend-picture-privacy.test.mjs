import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const APP_ROOT = resolve(new URL("..", import.meta.url).pathname);
const FIXTURE_DIR = join(APP_ROOT, "screenshots", "fixtures");
const ARTIFACT_DIR = join(APP_ROOT, "screenshots", "artifacts", "task-0238-friend-picture-privacy");
const WINDOW = Object.freeze({ width: 480, height: 220 });
const OWNER_IMAGE_PAYLOAD = "R0lGODlhAQABAIAAAAAAAP///ywAAAAAAQABAAACAUwAOw==";

const FIXTURES = Object.freeze([
  {
    id: "friend",
    file: "task-0238-friend-picture-friend.html",
    title: "Friends - signed in as an accepted friend",
  },
  {
    id: "non-friend",
    file: "task-0238-friend-picture-non-friend.html",
    title: "Friends - signed in as a stranger",
  },
]);

function pngDimensions(png) {
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

function startFixtureServer() {
  const server = createServer((request, response) => {
    const file = String(request.url || "").replace(/^\//u, "");
    const allowed = FIXTURES.find((fixture) => fixture.file === file);
    if (!allowed) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(readFileSync(join(FIXTURE_DIR, allowed.file)));
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") return reject(new Error("fixture server did not expose a TCP port"));
      resolve({ server, url: `http://127.0.0.1:${address.port}/` });
    });
  });
}

function closeServer(server) {
  return new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

async function captureFixture(page, url, fixture) {
  const source = readFileSync(join(FIXTURE_DIR, fixture.file), "utf8");
  await page.navigate(`${url}${fixture.file}`, { timeoutMs: 10_000 });
  const facts = await page.evaluate(`(() => {
    const image = document.querySelector(".friend-picture");
    const fallback = document.querySelector(".friend-picture-fallback");
    const rect = image?.getBoundingClientRect() ?? fallback?.getBoundingClientRect();
    return {
      title: document.title,
      ownerPicture: document.querySelector(".friend-row")?.getAttribute("data-owner-picture"),
      imageCount: document.querySelectorAll("img.friend-picture").length,
      fallbackCount: document.querySelectorAll(".friend-picture-fallback").length,
      pictureVisible: Boolean(image && getComputedStyle(image).display !== "none" && image.getBoundingClientRect().width > 0 && image.getBoundingClientRect().height > 0),
      pictureAlt: image?.getAttribute("alt") ?? null,
      pictureCell: rect ? { width: rect.width, height: rect.height } : null,
      visibleText: document.body.innerText.replace(/\\s+/gu, " ").trim(),
    };
  })()`);
  const png = await page.screenshot({ fromSurface: true });
  const output = join(ARTIFACT_DIR, `task-0238-${fixture.id}-${WINDOW.width}x${WINDOW.height}.png`);
  writeFileSync(output, png);
  return { ...facts, sourceImagePayloadCount: source.split(OWNER_IMAGE_PAYLOAD).length - 1, png: output, pngDimensions: pngDimensions(png), pngBytes: png.length };
}

test("TASK 0238 captures friend picture privacy fixtures at one fixed Linux size", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const { server, url } = await startFixtureServer();
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
      screenWidth: WINDOW.width,
      screenHeight: WINDOW.height,
    });
    // One page is deliberately reused so both captures run through the same
    // Linux browser process and fixed metrics. Navigation is sequential: two
    // simultaneous navigations could make the friend result inspect the
    // non-friend document.
    const captures = [];
    for (const fixture of FIXTURES) captures.push(await captureFixture(page, url, fixture));
    const friend = captures[0];
    const nonFriend = captures[1];

    assert.equal(friend.title, FIXTURES[0].title);
    assert.equal(friend.ownerPicture, "present");
    assert.equal(friend.imageCount, 1);
    assert.equal(friend.fallbackCount, 0);
    assert.equal(friend.pictureVisible, true);
    assert.equal(friend.pictureAlt, "Friend picture");
    assert.equal(friend.sourceImagePayloadCount, 1);
    assert.deepEqual(friend.pictureCell, { width: 40, height: 40 });

    assert.equal(nonFriend.title, FIXTURES[1].title);
    assert.equal(nonFriend.ownerPicture, "absent");
    assert.equal(nonFriend.imageCount, 0);
    assert.equal(nonFriend.fallbackCount, 1);
    assert.equal(nonFriend.pictureVisible, false);
    assert.equal(nonFriend.pictureAlt, null);
    assert.equal(nonFriend.sourceImagePayloadCount, 0);
    assert.deepEqual(nonFriend.pictureCell, { width: 38, height: 38 });

    for (const capture of captures) {
      assert.deepEqual(capture.pngDimensions, WINDOW);
      assert.ok(capture.pngBytes > 1_000, `fixture PNG too small: ${capture.pngBytes}`);
      assert.match(capture.visibleText, /Ada Owner/u);
    }

    console.log(`TASK_0238_LINUX_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK_0238_FRIEND screenshot=${friend.png} image_count=${friend.imageCount} fallback_count=${friend.fallbackCount} visible=${friend.pictureVisible} owner_image_payloads=${friend.sourceImagePayloadCount}`);
    console.log(`TASK_0238_NON_FRIEND screenshot=${nonFriend.png} image_count=${nonFriend.imageCount} fallback_count=${nonFriend.fallbackCount} visible=${nonFriend.pictureVisible} owner_image_payloads=${nonFriend.sourceImagePayloadCount}`);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
}, { timeout: 30_000 });
