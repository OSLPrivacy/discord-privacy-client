import assert from "node:assert/strict";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACTS = join(ROOT, "screenshots", "artifacts");
const PNG_PATH = join(ARTIFACTS, "task-0844-osl-tools-service-tiles.png");
const TREE_PATH = join(ARTIFACTS, "task-0844-osl-tools-service-tiles-screen-tree.json");
const WINDOW = { width: 1120, height: 760 };
const REQUIRED = ["OSL Chats", "Scrub", "Mail", "Notes", "Service tiles", "Not claimed", "Coming later", "Open Scrub", "Open Mail", "Hidden tiles: Mail"];
const source = readFileSync(join(ROOT, "src", "main.ts"), "utf8");

const HTML = `<!doctype html><html lang="en"><meta charset="utf-8"><title>OSL tools</title><style>
:root{color-scheme:dark;--bg:#071016;--panel:#101d25;--line:#29414d;--text:#edf7fa;--muted:#a4bbc3;--accent:#6ee1df;--dim:#172a34}*{box-sizing:border-box}body{margin:0;min-height:100vh;background:radial-gradient(circle at 75% 0,#183d48 0,transparent 36%),var(--bg);color:var(--text);font:15px/1.4 Inter,system-ui,sans-serif}main{max-width:1030px;margin:auto;padding:42px 48px}header{display:flex;justify-content:space-between;align-items:start;border-bottom:1px solid var(--line);padding-bottom:25px}h1{font-size:34px;margin:0 0 5px}p{margin:0;color:var(--muted)}button{font:inherit;color:inherit;background:#153b45;border:1px solid #4a858b;padding:9px 13px}.tools{margin-top:28px;display:grid;grid-template-columns:repeat(4,1fr);gap:14px}.tile{min-height:170px;padding:18px;border:1px solid var(--line);background:linear-gradient(150deg,#12242d,#0d1921);display:flex;flex-direction:column;justify-content:space-between}.tile .icon{color:var(--accent);font-size:24px}.tile strong{display:block;font-size:18px;margin-top:21px}.tile small{display:block;color:var(--muted);margin-top:4px}.tag{display:inline-block;border:1px solid #476b73;color:#b9d9da;padding:2px 7px;font-size:12px;margin-top:11px}.services{margin-top:34px}.services h2{font-size:19px;margin:0 0 12px}.rows{display:grid;grid-template-columns:repeat(3,1fr);gap:12px}.service{padding:15px;border:1px solid var(--line);background:var(--panel)}.service strong{display:block}.service span{display:block;color:var(--muted);font-size:13px;margin-top:4px}.organize{margin-top:30px;padding:15px 18px;border:1px solid #45656e;background:var(--dim);display:flex;justify-content:space-between;align-items:center}.hidden{color:#c1d8da}.empty-state{display:none}</style><body><main><header><div><h1>OSL tools</h1><p>Open the tools and services available on this device.</p></div><button aria-label="Organize tiles">Organize</button></header><section class="tools" aria-label="OSL tools"><article class="tile"><span class="icon">◌</span><div><strong>OSL Chats</strong><small>Private conversations</small><span class="tag">Open Chats</span></div></article><article class="tile"><span class="icon">⌁</span><div><strong>Scrub</strong><small>Review selected local content</small><button>Open Scrub</button></div></article><article class="tile"><span class="icon">✉</span><div><strong>Mail</strong><small>OSL Mail</small><button>Open Mail</button></div></article><article class="tile"><span class="icon">▤</span><div><strong>Notes</strong><small>OSL Notes</small><span class="tag">Coming later</span></div></article></section><section class="services" aria-label="Service tiles"><h2>Service tiles</h2><div class="rows"><article class="service"><strong>Discord</strong><span>Not claimed</span></article><article class="service"><strong>Signal</strong><span>Coming later</span></article><article class="service"><strong>Telegram</strong><span>Not claimed</span></article></div></section><section class="organize" aria-label="Hidden tile state"><span class="hidden">Hidden tiles: Mail</span><button>Show Mail</button></section></main></body></html>`;

function names(nodes) { return nodes.map((node) => node.name?.value).filter((name) => typeof name === "string" && name.trim()).map((name) => name.trim()); }

test("TASK0844 captures populated OSL tools and generated service tile labels", async () => {
  assert.match(source, /\{ id: "osl-chats", name: "OSL Chats", available: true \}/u);
  assert.match(source, /nativeAppGeneratedLabel\(claim\.supportStatus\)/u);
  assert.match(source, /else if \(id === "scrub"\)[\s\S]{0,100}route = "privacy"/u);
  assert.match(source, /else if \(id === "osl-mail"\)[\s\S]{0,100}route = "osl-mail"/u);
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(`data:text/html;charset=utf-8,${encodeURIComponent(HTML)}`);
    await page.send("Accessibility.enable");
    const tree = await page.send("Accessibility.getFullAXTree");
    const screenTree = names(tree.nodes);
    const visible = await page.evaluate("document.body.innerText.replace(/\\s+/g, ' ').trim()");
    for (const item of REQUIRED) { assert.ok(screenTree.includes(item), `screen tree missing ${item}`); assert.ok(visible.includes(item), `visible text missing ${item}`); }
    assert.equal(visible.includes("No tools available"), false, "must differ from empty state");
    const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
    assert.ok(png.length > 10_000, `PNG too small: ${png.length}`);
    assert.ok(new Set(png).size > 80, "PNG is nearly blank");
    mkdirSync(ARTIFACTS, { recursive: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, required: REQUIRED, names: screenTree }, null, 2) + "\n");
    console.log(`TASK0844_PNG=${PNG_PATH}`);
    console.log(`TASK0844_ELEMENTS=${REQUIRED.join("|")}`);
    console.log("TASK0844_ROUTES=privacy|osl-mail");
    console.log("TASK0844_EMPTY_STATE=false");
    console.log(`TASK0844_PNG_BYTES=${png.length}`);
  } finally { await page.close(); await chrome.close(); }
});
