// TASK 0840 - capture the completed friend page against its empty state.
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
const CONTROLS = ["account", "conversation", "checkmark", "new-account", "save", "remove", "cancel", "back"];

async function fixture() {
  const out = join(tmpdir(), `osl-task-0840-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: { contents: `import { friendPageMarkup, friendPageEmptyStateMarkup } from "${join(ROOT, "src", "friend-page.ts")}"; export const built = friendPageMarkup(); export const empty = friendPageEmptyStateMarkup();`, resolveDir: ROOT, loader: "js" },
    bundle: true, platform: "node", format: "esm", outfile: out, loader: { ".css": "empty" }, logLevel: "silent",
  });
  return import(`file://${out}`);
}

function html(markup) {
  const css = [readFileSync(join(ROOT, "src", "styles.css"), "utf8"), readFileSync(join(ROOT, "src", "friend-page.css"), "utf8"), "html,body,#app{height:100%;margin:0;background:var(--bg)}"].join("\n");
  return `<!doctype html><html><head><meta charset="utf-8"><style>${css}</style></head><body><div id="app"><main class="content-viewport">${markup}</main></div></body></html>`;
}

async function serve(pages) {
  const server = createServer((request, response) => {
    const key = (request.url ?? "/").slice(1) || "built";
    response.writeHead(pages[key] ? 200 : 404, { "content-type": "text/html; charset=utf-8" });
    response.end(pages[key] ?? "missing");
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  return { server, base: `http://127.0.0.1:${server.address().port}/` };
}

async function capture(chrome, url, path) {
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const result = await page.evaluate(`(() => {
      const controls = ${JSON.stringify(CONTROLS)};
      return controls.map((name) => { const node = document.querySelector('[aria-label="' + name + '"]'); if (!node) throw new Error('missing ' + name); const r = node.getBoundingClientRect(); return { name, width: r.width, height: r.height, text: node.textContent.trim() }; });
    })()`);
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(path, png);
    return { png, result, names: ax.nodes.map((node) => node.name?.value).filter(Boolean) };
  } finally { await page.close(); }
}

test("TASK 0840 friend page captures all eight controls and differs from empty state", async () => {
  mkdirSync(EVIDENCE, { recursive: true });
  const page = await fixture();
  const { server, base } = await serve({ built: html(page.built), empty: html(page.empty) });
  const chrome = await launchChrome();
  try {
    const built = await capture(chrome, `${base}built`, join(EVIDENCE, "task-0840-friend-page-1000x760.png"));
    const empty = await pageCapture(chrome, `${base}empty`, join(EVIDENCE, "task-0840-friend-page-empty-1000x760.png"));
    assert.equal(built.result.length, CONTROLS.length);
    for (const control of built.result) assert.ok(control.width > 0 && control.height > 0, `${control.name} is not painted`);
    for (const control of CONTROLS) assert.ok(built.names.includes(control), `screen tree missing ${control}`);
    assert.notDeepEqual(built.png, empty, "built capture is byte-identical to empty state");
    console.log(`TASK_0840_WINDOW ${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK_0840_NAMED_ELEMENTS ${built.result.map((item) => `${item.name}=${Math.round(item.width)}x${Math.round(item.height)}`).join(" | ")}`);
    console.log(`TASK_0840_DIFF_VS_EMPTY built_bytes=${built.png.length} empty_bytes=${empty.length} differs=true`);
    console.log(`TASK_0840_PNG ${join(EVIDENCE, "task-0840-friend-page-1000x760.png")}`);
  } finally {
    await chrome.close();
    await new Promise((done, fail) => server.close((error) => error ? fail(error) : done()));
  }
});

async function pageCapture(chrome, url, path) {
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(url, { timeoutMs: 10_000 });
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(path, png);
    return png;
  } finally { await page.close(); }
}
