// TASK 0842 - fixed-size, privacy-safe OSL friend page capture.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const EVIDENCE = join(ROOT, "screenshots", "evidence");
const WINDOW = Object.freeze({ width: 1000, height: 760 });
const CONTROLS = Object.freeze(["account", "conversation", "checkmark", "new-account", "save", "remove", "cancel", "back"]);
const PNG_PATH = join(EVIDENCE, "task-0842-osl-friend-1000x760.png");
const TREE_PATH = join(EVIDENCE, "task-0842-osl-friend-screen-tree.json");

function pngDimensions(png) {
  assert.equal(png.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  return { width: png.readUInt32BE(16), height: png.readUInt32BE(20) };
}

async function renderedFriendPage() {
  const out = join(tmpdir(), `osl-task-0842-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `import { friendPageMarkup } from "${join(ROOT, "src", "friend-page.ts")}"; export default friendPageMarkup({ friendName: "OSL friend", account: "OSL Chat", conversation: "Private chat", newAccountAllowed: true });`,
      resolveDir: ROOT,
      loader: "js",
    },
    bundle: true, platform: "node", format: "esm", outfile: out,
    loader: { ".css": "empty" }, logLevel: "silent",
  });
  return (await import(`file://${out}`)).default;
}

function html(markup) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "friend-page.css"), "utf8"),
    "html,body,#app{height:100%;margin:0;background:var(--bg)}",
  ].join("\n");
  return `<!doctype html><html><head><meta charset="utf-8"><title>OSL friend</title><style>${css}</style></head><body><div id="app"><main class="content-viewport">${markup}</main></div></body></html>`;
}

async function serve(page) {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(page);
  });
  await new Promise((done) => server.listen(0, "127.0.0.1", done));
  return { server, url: `http://127.0.0.1:${server.address().port}/` };
}

test("TASK 0842 captures a confirmation-closed OSL friend page without personal data", async () => {
  mkdirSync(EVIDENCE, { recursive: true });
  const { server, url } = await serve(html(await renderedFriendPage()));
  const chrome = await launchChrome({
    args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"],
  });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, screenWidth: WINDOW.width, screenHeight: WINDOW.height, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");
    const screen = JSON.parse(await page.evaluate(`JSON.stringify((() => ({
      title: document.querySelector("h1")?.textContent.trim() ?? "",
      documentTitle: document.title,
      text: document.body.innerText.replace(/\\s+/gu, " ").trim(),
      confirmationOpen: document.querySelectorAll("dialog[open], [role=dialog][data-confirmation-open=true]").length,
      controls: ${JSON.stringify(CONTROLS)}.map((name) => {
        const element = document.querySelector('[aria-label="' + name + '"]');
        if (!element) throw new Error('missing control ' + name);
        const box = element.getBoundingClientRect();
        return { name, text: element.textContent.trim(), rect: { x: box.x, y: box.y, width: box.width, height: box.height } };
      }),
    }))())`));
    assert.equal(screen.title, "OSL friend");
    assert.equal(screen.documentTitle, "OSL friend");
    assert.equal(screen.confirmationOpen, 0, "confirmation is closed");
    assert.equal(screen.controls.length, CONTROLS.length);
    for (const control of screen.controls) {
      assert.ok(control.rect.width > 0 && control.rect.height > 0, `${control.name} is visible`);
      assert.ok(control.rect.y >= 0 && control.rect.y + control.rect.height <= WINDOW.height, `${control.name} is in the fixed window`);
    }
    for (const forbidden of ["Avery", "Chen", "@", "OSLFR", "safety number"]) assert.ok(!screen.text.toLowerCase().includes(forbidden.toLowerCase()), `no personal-data probe: ${forbidden}`);
    const ax = await page.send("Accessibility.getFullAXTree");
    const treeText = JSON.stringify(ax.nodes ?? []);
    assert.ok(treeText.includes("OSL friend"), "screen tree includes title OSL friend");
    for (const control of CONTROLS) assert.ok(treeText.includes(control), `screen tree includes ${control}`);
    const png = await page.screenshot({ fromSurface: true });
    const dimensions = pngDimensions(png);
    assert.deepEqual(dimensions, WINDOW);
    assert.ok(png.length > 10_000, "image is not nearly blank");
    assert.ok(new Set(png).size > 64, "image has varied pixels");
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, screen, axNodes: ax.nodes }, null, 2));
    console.log(`TASK0842_PNG=${PNG_PATH}`);
    console.log(`TASK0842_TREE=${TREE_PATH}`);
    console.log(`TASK0842_TITLE=${screen.title}`);
    console.log(`TASK0842_CONTROLS=${screen.controls.map((control) => control.name).join("|")}`);
    console.log(`TASK0842_CONFIRMATION_OPEN=${screen.confirmationOpen}`);
    console.log(`TASK0842_WINDOW=${dimensions.width}x${dimensions.height}`);
    console.log(`TASK0842_PNG_BYTES=${png.length}`);
    console.log(`TASK0842_PNG_UNIQUE_BYTES=${new Set(png).size}`);
    console.log("TASK0842_PERSONAL_DATA_PROBES=0");
  } finally {
    await page.close();
    await chrome.close();
    await new Promise((done, fail) => server.close((error) => error ? fail(error) : done()));
  }
}, { timeout: 60_000 });
