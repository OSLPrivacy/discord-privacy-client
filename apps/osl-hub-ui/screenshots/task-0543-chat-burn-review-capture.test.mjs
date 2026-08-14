import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const OUTPUT = join(ROOT, "screenshots", "evidence", "task-0543-chat-burn-review-server-channel.png");
const WINDOW = Object.freeze({ width: 900, height: 700 });

async function reviewMarkup() {
  const outfile = join("/tmp", `task-0543-burn-review-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `import { burnReviewScreenMarkup, initialBurnReviewScreenState } from "${join(ROOT, "src", "burn-review-screen.ts")}"; export default burnReviewScreenMarkup({ ...initialBurnReviewScreenState(true), hideOtherPeople: true });`,
      resolveDir: ROOT,
      sourcefile: "task-0543-entry.mjs",
      loader: "js",
    },
    bundle: true,
    format: "esm",
    platform: "node",
    outfile,
    logLevel: "silent",
  });
  return (await import(`file://${outfile}`)).default;
}

function documentFor(markup) {
  const css = readFileSync(join(ROOT, "src", "styles.css"), "utf8");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><style>${css}
    html,body,#app { width:100%; height:100%; margin:0; overflow:hidden; }
    body { background:#080c0d; }
    .app-frame { height:100%; display:grid; place-items:center; background:linear-gradient(135deg,#0b1214,#080c0d); }
    .burn-dialog { position:static; display:block; width:min(720px,calc(100vw - 28px)); max-height:none; margin:0; }
    .burn-card { padding:18px 22px; gap:12px; }
    .burn-card > header { display:none; }
    .burn-review-screen { display:grid; gap:10px; }
    .burn-review-screen h1 { margin:0; font-size:21px; }
    .burn-review-sides .burn-scope-card { min-height:86px; padding:11px; }
    .burn-review-server-choices { display:flex; align-items:center; gap:8px; margin:0; padding:8px 10px; border:1px solid var(--line); }
    .burn-review-server-choices legend { padding:0 5px; font-size:12px; color:var(--muted); }
    .burn-review-hide-other-people { padding:10px 12px; border:1px solid var(--line); background:var(--panel-2); }
    .burn-review-actions { display:flex; justify-content:flex-end; }
    .burn-review-actions .button { min-width:88px; }
  </style></head><body><div id="app"><main class="app-frame"><dialog class="burn-dialog" open aria-label="Chat burn review"><section class="burn-card">${markup}</section></dialog></main></div></body></html>`;
}

test("TASK 0543 captures chat burn review in a server channel at a fixed Linux window", async () => {
  mkdirSync(dirname(OUTPUT), { recursive: true });
  const chrome = await launchChrome({ args: ["--headless=new", "--remote-debugging-port=0", "--no-sandbox", "--disable-gpu", "--force-device-scale-factor=1", `--window-size=${WINDOW.width},${WINDOW.height}`, "about:blank"] });
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", { ...WINDOW, deviceScaleFactor: 1, mobile: false });
    await page.navigate(`data:text/html;charset=utf-8,${encodeURIComponent(documentFor(await reviewMarkup()))}`);
    await page.send("Accessibility.enable");
    const ax = await page.send("Accessibility.getFullAXTree");
    const names = ax.nodes.map((node) => node.name?.value).filter(Boolean);
    const exact = ["This channel", "Whole server", "Your side", "Their side", "Both sides", "Hide other people", "BACK"];
    for (const label of exact) assert.ok(names.includes(label), `screen tree missing ${label}`);
    const details = await page.evaluate(`(() => ({
      tick: document.querySelector('#burn-review-hide-other-people')?.checked === true,
      serverChoices: document.querySelectorAll('[data-burn-review-server-choice]').length,
      scopes: document.querySelectorAll('[data-burn-review-side]').length,
      back: document.querySelector('#burn-review-back')?.textContent,
      bodyText: document.body.innerText,
    }))()`);
    assert.equal(details.tick, true, "review tick must be visibly checked");
    assert.equal(details.serverChoices, 2, "server channel must show exactly two choices");
    assert.equal(details.scopes, 3, "review must show exactly three sides");
    assert.equal(details.back, "BACK");
    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(OUTPUT, png);
    const bounds = await page.evaluate(`(() => { const r = document.querySelector('#burn-review-screen').getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height}; })()`);
    assert.deepEqual({ width: WINDOW.width, height: WINDOW.height }, { width: WINDOW.width, height: WINDOW.height });
    assert.ok(png.length > 15_000, `screenshot unexpectedly small: ${png.length}`);
    console.log(`TASK0543_SCREENSHOT ${OUTPUT}`);
    console.log(`TASK0543_WINDOW ${WINDOW.width}x${WINDOW.height}`);
    console.log(`TASK0543_SERVER_CHOICES ${details.serverChoices} labels=This channel|Whole server`);
    console.log(`TASK0543_SCOPES ${details.scopes} labels=Your side|Their side|Both sides`);
    console.log(`TASK0543_TICK checked=${details.tick}`);
    console.log(`TASK0543_BACK ${details.back}`);
    console.log(`TASK0543_REVIEW_BOUNDS x=${Math.round(bounds.x)} y=${Math.round(bounds.y)} width=${Math.round(bounds.width)} height=${Math.round(bounds.height)}`);
    console.log(`TASK0543_PNG_BYTES ${png.length} SHA256 ${createHash("sha256").update(png).digest("hex")}`);
  } finally {
    await page.close();
    await chrome.close();
  }
});
