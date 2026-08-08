#!/usr/bin/env node

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { decodePng, inkInRect } from "./capture-task-0750-verification-warning.mjs";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const UI_ROOT = path.resolve(SCRIPT_DIR, "..");
const REPO_ROOT = path.resolve(UI_ROOT, "..", "..");
const VIEWPORT = { width: 760, height: 640 };
const DEFAULT_OUTPUT = path.join(SCRIPT_DIR, "evidence", "task-3712-remove-everything.png");
const REQUIRED_NAMES = ["Remove everything", "local data", "service data", "Remove everything", "Cancel"];

async function loadScreenModule() {
  const require = createRequire(path.join(UI_ROOT, "package.json"));
  const esbuild = require("esbuild");
  const built = await esbuild.build({
    entryPoints: [path.join(UI_ROOT, "src", "remove-everything-screen.ts")],
    bundle: true,
    format: "esm",
    platform: "node",
    write: false,
  });
  const source = built.outputFiles[0].text;
  return import(`data:text/javascript;base64,${Buffer.from(source).toString("base64")}`);
}

function captureHtml(styles) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><title>TASK 3712 Remove everything</title><style>${styles.replaceAll("</style", "<\\/style")}</style><style>
    html,body,#app{width:100%;height:100%;margin:0;overflow:hidden}
    body{background:var(--bg);color:var(--text)}
    #app{box-sizing:border-box;padding:64px 72px}
    .settings-detail{box-sizing:border-box;width:100%;max-width:616px;margin:0 auto;padding:32px;background:var(--panel);border:1px solid var(--line)}
    .remove-everything-screen>p{max-width:58ch;color:var(--muted);line-height:1.55}
    .remove-everything-summary{margin-top:14px;padding:0 16px;border:1px solid var(--line);background:var(--panel-2)}
    .remove-everything-summary summary{min-height:52px;display:flex;align-items:center;font-weight:700;cursor:pointer}
    .remove-everything-actions{display:flex;gap:12px;margin-top:24px}
  </style></head><body><main id="app"><section class="settings-detail"><h2>Account</h2><p>Review account controls.</p><button class="button danger" id="open-remove-everything" type="button">Remove everything</button></section></main></body></html>`;
}

async function main() {
  const outputIndex = process.argv.indexOf("--output");
  const output = outputIndex === -1 ? DEFAULT_OUTPUT : path.resolve(process.argv[outputIndex + 1] ?? "");
  const screen = await loadScreenModule();
  const markup = screen.removeEverythingScreenMarkup();
  const checked = screen.checkRemoveEverythingScreen(markup);
  assert.equal(checked.pass, true);
  assert.deepEqual(screen.removeEverythingScreenTree(), {
    title: "Remove everything",
    controls: ["local data", "service data", "Remove everything", "Cancel"],
  });

  const browser = await launchChrome();
  let page;
  try {
    page = await browser.openPage();
    await page.send("Emulation.setDeviceMetricsOverride", { ...VIEWPORT, deviceScaleFactor: 1, mobile: false });
    await page.send("Accessibility.enable");
    const styles = readFileSync(path.join(UI_ROOT, "src", "styles.css"), "utf8");
    await page.evaluate(`document.open();document.write(${JSON.stringify(captureHtml(styles))});document.close()`);
    await page.evaluate(`(() => {
      window.__TASK3712_CONFIRMED = false;
      document.querySelector("#open-remove-everything").addEventListener("click", () => {
        document.querySelector(".settings-detail").innerHTML = ${JSON.stringify(markup)};
        document.querySelector("#remove-everything-confirm").addEventListener("click", () => { window.__TASK3712_CONFIRMED = true; });
      });
      document.querySelector("#open-remove-everything").click();
    })()`);

    const drawn = await page.evaluate(`(() => {
      const elements = [
        document.querySelector("#remove-everything-title"),
        ...document.querySelectorAll(".remove-everything-summary > summary"),
        ...document.querySelectorAll(".remove-everything-actions > button"),
      ];
      return elements.map((element) => {
        const rect = element.getBoundingClientRect();
        return { name: element.textContent.trim(), rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height } };
      });
    })()`);
    assert.deepEqual(drawn.map(({ name }) => name), REQUIRED_NAMES);
    for (const { name, rect } of drawn) {
      assert.ok(rect.width > 20 && rect.height > 16, `${name} has a painted rectangle`);
      assert.ok(rect.x >= 0 && rect.y >= 0 && rect.x + rect.width <= VIEWPORT.width && rect.y + rect.height <= VIEWPORT.height, `${name} is inside the fixed image`);
    }

    const ax = await page.send("Accessibility.getFullAXTree");
    const axNames = ax.nodes.map((node) => String(node.name?.value ?? "").trim()).filter(Boolean);
    for (const required of new Set(REQUIRED_NAMES)) {
      assert.ok(axNames.includes(required), `screen tree is missing ${required}`);
    }

    const png = await page.screenshot({
      fromSurface: true,
      clip: { x: 0, y: 0, width: VIEWPORT.width, height: VIEWPORT.height, scale: 1 },
    });
    const image = decodePng(png);
    assert.equal(image.width, VIEWPORT.width);
    assert.equal(image.height, VIEWPORT.height);
    const whole = inkInRect(image, { x: 0, y: 0, width: image.width, height: image.height });
    assert.ok(whole.distinctColors > 50, `image has only ${whole.distinctColors} colors`);
    assert.ok(whole.inkPixels / whole.area > 0.01, "image is blank or nearly blank");
    const imageLabels = drawn.map(({ name, rect }) => ({ name, rect, ink: inkInRect(image, rect) }));
    for (const { name, ink } of imageLabels) {
      assert.ok(ink.inkPixels >= 12, `${name} is not visibly painted in its image rectangle`);
    }
    mkdirSync(path.dirname(output), { recursive: true });
    writeFileSync(output, png);

    await page.evaluate(`document.querySelector("#remove-everything-confirm").click()`);
    assert.equal(await page.evaluate("window.__TASK3712_CONFIRMED"), true);

    const relativeOutput = path.relative(REPO_ROOT, output);
    console.log("TASK3712_SCREEN_TREE_TITLE=Remove everything");
    for (const control of REQUIRED_NAMES.slice(1)) console.log(`TASK3712_SCREEN_TREE_CONTROL=${control}`);
    for (const { name, rect, ink } of imageLabels) {
      console.log(`TASK3712_IMAGE_LABEL=${name} rect=${Math.round(rect.x)},${Math.round(rect.y)},${Math.round(rect.width)},${Math.round(rect.height)} ink_pixels=${ink.inkPixels}`);
    }
    console.log(`TASK3712_IMAGE path=${relativeOutput} width=${image.width} height=${image.height} bytes=${png.length} colors=${whole.distinctColors} ink_fraction=${(whole.inkPixels / whole.area).toFixed(4)} sha256=${createHash("sha256").update(png).digest("hex")} blank=false`);
    console.log("TASK3712_UI_CONFIRM=Remove everything confirmed=true");
  } finally {
    if (page) await page.close();
    await browser.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
