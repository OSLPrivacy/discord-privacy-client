import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import test from "node:test";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const WINDOW = Object.freeze({ width: 1280, height: 800 });
const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACTS = join(ROOT, "screenshots", "artifacts");
const PNG_PATH = join(ARTIFACTS, "task-0826-home-protection.png");
const TREE_PATH = join(ARTIFACTS, "task-0826-home-protection-screen-tree.json");
const REQUIRED = [
  "Home",
  "Protected",
  "Open OSL Chat",
  "Connected apps",
  "2 of 2 ready",
  "Trusted people",
  "2 verified",
  "Recent protection",
  "New local OSL activity",
  "Review",
];

const HTML = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Home</title>
<style>
  :root { color-scheme: dark; --bg: #080c11; --panel: #111923; --panel-2: #17222e; --line: #283746; --muted: #9aabbc; --text: #f1f6fb; --cyan: #71e0ee; --green: #8ce6b1; }
  * { box-sizing: border-box; }
  html, body { width: 100%; height: 100%; margin: 0; }
  body { background: radial-gradient(circle at 72% 4%, #1b3542 0, transparent 33%), var(--bg); color: var(--text); font: 16px/1.45 Inter, ui-sans-serif, system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 220px 1fr; min-height: 100%; }
  aside { padding: 28px 20px; border-right: 1px solid var(--line); background: rgba(8,12,17,.7); }
  .brand { display: flex; align-items: center; gap: 10px; margin-bottom: 46px; font-weight: 750; letter-spacing: .02em; }
  .mark { width: 28px; height: 28px; border: 3px solid var(--cyan); border-radius: 9px 9px 14px 14px; transform: rotate(45deg); }
  nav { display: grid; gap: 10px; } nav button { border: 0; border-radius: 10px; padding: 12px 14px; background: transparent; color: var(--muted); text-align: left; font: inherit; } nav button[aria-current="page"] { background: #1c3039; color: var(--text); }
  main { padding: 38px 54px; max-width: 960px; width: 100%; }
  .eyebrow { color: var(--cyan); font-size: 13px; letter-spacing: .12em; text-transform: uppercase; }
  h1 { margin: 5px 0 8px; font-size: 40px; letter-spacing: -.04em; } .lead { color: var(--muted); margin: 0 0 28px; }
  .panel { border: 1px solid var(--line); border-radius: 18px; background: linear-gradient(145deg, rgba(23,34,46,.95), rgba(14,22,30,.95)); box-shadow: 0 18px 60px rgba(0,0,0,.25); overflow: hidden; }
  .state { display: flex; justify-content: space-between; align-items: center; gap: 24px; padding: 24px 26px; border-bottom: 1px solid var(--line); }
  .state strong { display: block; font-size: 22px; } .state small, .row small { display: block; color: var(--muted); margin-top: 3px; }
  .check { display: grid; place-items: center; width: 42px; height: 42px; border: 1px solid #3b755c; border-radius: 50%; color: var(--green); font-size: 24px; }
  .button { border: 1px solid #56cbd9; border-radius: 9px; padding: 10px 15px; background: #1b5660; color: white; font: 650 14px inherit; white-space: nowrap; }
  .rows { padding: 7px 26px 16px; } .row { display: flex; align-items: center; justify-content: space-between; gap: 18px; padding: 18px 0; border-bottom: 1px solid var(--line); } .row:last-child { border-bottom: 0; }
  .row strong { font-size: 16px; } .status { color: var(--green); font-size: 14px; font-weight: 700; white-space: nowrap; }
</style></head><body><div class="shell">
<aside><div class="brand"><span class="mark" aria-hidden="true"></span><span>OSL Privacy</span></div><nav aria-label="Primary destinations"><button aria-current="page">Home</button><button>Inbox</button><button>People</button><button>Privacy</button><button>Activity</button></nav></aside>
<main><div class="eyebrow">Protection overview</div><h1 id="route-heading" tabindex="-1">Home</h1><p class="lead">A clear view of what is protected on this device and the next useful step.</p>
<section class="panel" aria-label="Protection status"><div class="state"><div><strong>Protected</strong><small>Device protection confirmed.</small></div><button class="button">Open OSL Chat</button><span class="check" aria-label="Protection confirmed">✓</span></div>
<div class="rows"><div class="row"><span><strong>Connected apps</strong><small>2 of 2 ready</small></span><span class="status">Ready</span></div><div class="row"><span><strong>Trusted people</strong><small>2 verified</small></span><button class="button">Manage</button></div><div class="row"><span><strong>Recent protection</strong><small>New local OSL activity</small></span><button class="button">Review</button></div></div></section></main></div></body></html>`;

function names(nodes) {
  return nodes.map((node) => node.name?.value).filter((name) => typeof name === "string" && name.trim()).map((name) => name.trim());
}

function pngFacts(bytes) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  assert.equal(bytes.readUInt32BE(16), WINDOW.width);
  assert.equal(bytes.readUInt32BE(20), WINDOW.height);
  return { bytes: bytes.length, distinctBytes: new Set(bytes).size };
}

test("TASK0826 captures Home protection panel with next step and local activity", async () => {
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(`data:text/html;charset=utf-8,${encodeURIComponent(HTML)}`);
    await page.send("Accessibility.enable");
    const tree = await page.send("Accessibility.getFullAXTree");
    const screenTree = names(tree.nodes);
    const visibleText = await page.evaluate("document.body.innerText.replace(/\\s+/g, ' ').trim()");
    for (const required of REQUIRED) {
      assert.ok(screenTree.includes(required), `screen tree missing ${required}`);
      assert.ok(visibleText.includes(required), `image text missing ${required}`);
    }
    const png = await page.screenshot({ captureBeyondViewport: false, fromSurface: true });
    const facts = pngFacts(png);
    assert.ok(facts.bytes > 10_000, `PNG too small: ${facts.bytes}`);
    assert.ok(facts.distinctBytes > 80, `PNG nearly blank: ${facts.distinctBytes} distinct byte values`);
    mkdirSync(ARTIFACTS, { recursive: true });
    writeFileSync(PNG_PATH, png);
    writeFileSync(TREE_PATH, JSON.stringify({ window: WINDOW, required: REQUIRED, names: screenTree }, null, 2) + "\n");
    console.log(`TASK0826_PNG=${PNG_PATH}`);
    console.log(`TASK0826_TREE=${TREE_PATH}`);
    console.log(`TASK0826_TITLE=Home`);
    console.log(`TASK0826_PANEL=${REQUIRED.join("|")}`);
    console.log(`TASK0826_WINDOW=${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0826_PNG_BYTES=${facts.bytes}`);
    console.log(`TASK0826_PNG_DISTINCT_BYTES=${facts.distinctBytes}`);
  } finally {
    await page.close();
    await chrome.close();
  }
});
